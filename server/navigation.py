"""Bounded, derived travel-time route caches; never stored in world snapshots."""
from collections import OrderedDict, deque
from contextlib import contextmanager
import heapq
from math import sqrt
from spatial import perimeter, footprint_tiles
import barns


class RoutePending(Exception):
    """The update budget or queue capacity was reached; this is not failure."""


class RouteCache:
    def __init__(self, graphs=8, routes=256, steps=32768, budget=2048, quantum=128, pending=128):
        self.graph_limit, self.route_limit, self.step_limit = graphs, routes, steps
        self.graphs = OrderedDict()
        self.budget, self.quantum, self.pending_limit = budget, quantum, pending
        self.pending = OrderedDict()
        self.reservation = None
        self.reservation_seen = False
        self.world = None
        self.active = False
        self.owner = None
        self.priority = 2
        self.prefix = {}
        self.turn = 0
        self.sequence = 0
        self.work = 0
        self.seen = set()
        self.waiting = set()

    @contextmanager
    def update(self, world, budget=None, prune=True):
        """One shared search allowance for every town in a simulation update."""
        previous_budget = self.budget
        if budget is not None:
            self.budget = min(self.budget, budget)
        if world is not self.world:
            self.pending.clear()
            self.reservation = None
            self.world = world
        self.active, self.work = True, 0
        self.seen, self.waiting = set(), set()
        self.reservation_seen = False
        try:
            yield
            # Only requests revalidated against this update's task/navigation
            # can run. Removed actors and changed tasks release their jobs.
            if prune:
                for owner in list(self.pending):
                    if id(self.pending[owner]) not in self.seen:
                        del self.pending[owner]
                if not self.reservation_seen:
                    self.reservation = None
            self.drain(self.budget - self.work)
        finally:
            self.active, self.owner = False, None
            self.budget = previous_budget

    @contextmanager
    def worker(self, worker, purpose='work', priority=None):
        previous = self.owner, self.priority, self.prefix
        self.prefix = {}
        self.owner = (purpose, id(worker)) if self.active else None
        animation = worker.get('work_animation', {})
        blocked = ((animation.get('phase') == 'waiting' and not animation.get('route_pending'))
                   or 'blocked' in worker.get('production_status', '').lower())
        urgent = bool(worker.get('pending_output') or worker.get('logistics_trip', {}).get('picked_up'))
        self.priority = priority if priority is not None else 0 if blocked else 1 if urgent else 2
        try:
            yield
        except RoutePending:
            self.waiting.add(id(worker))
            worker['production_status'] = 'Waiting for a route'
        finally:
            if self.is_waiting(worker):
                worker['route_waiting'] = True
            else:
                worker.pop('route_waiting', None)
            self.owner, self.priority, self.prefix = previous

    def is_waiting(self, worker):
        return self.active and id(worker) in self.waiting

    def drain(self, allowance):
        """Weighted round robin: blocked 4, urgent 2, ordinary 1 quantum.

        FIFO rotation within each class guarantees service even under a
        continuous stream of higher priority requests. Count stale heap pops
        and path reconstruction too, not just successful node expansions.
        """
        used = 0
        lanes = (0, 0, 0, 0, 1, 1, 2)
        while used < allowance and self.work < self.budget:
            ready = [job for job in self.pending.values()
                     if id(job) in self.seen and 'result' not in job]
            if not ready:
                break
            for _ in lanes:
                lane = lanes[self.turn % len(lanes)]
                self.turn += 1
                eligible = [job for job in ready if job['priority'] == lane]
                if eligible:
                    break
            job = min(eligible, key=lambda job: job['order'])
            for _ in range(min(self.quantum, allowance - used, self.budget - self.work)):
                used += 1
                self.work += 1
                try:
                    next(job['search'])
                except StopIteration as done:
                    job['result'] = done.value
                    self.remember(job['paths'], job['key'], done.value)
                    break
            self.sequence += 1
            job['order'] = self.sequence
        return used

    def remember(self, paths, key, path):
        cost = len(path) if path is not None else 1
        if key not in paths[0] and cost <= self.step_limit:
            paths[0][key] = path
            paths[1] += cost
            while len(paths[0]) > self.route_limit or paths[1] > self.step_limit:
                _, old = paths[0].popitem(last=False)
                paths[1] -= len(old) if old is not None else 1

    def route(self, start, target, blocked, size, terrain, adjacent=True, bounds=None,
              roads=(), road_mode='smooth', diagonal=False, fences=()):
        if self.active and self.owner is None:
            raise RuntimeError('Simulation searches require a worker scope')
        if adjacent and isinstance(target, dict) and target.get('footprint'):
            tiles = footprint_tiles(target)
            target = {p for p in perimeter(target) - set(blocked)
                      if any(abs(p[0] - q[0]) + abs(p[1] - q[1]) == 1
                             and not barns.crosses(p, q, fences) for q in tiles)}
        # Chunk records are sparse. Include loaded chunk keys as well as terrain
        # rows so loading/evicting terrain cannot reuse an obsolete route.
        if hasattr(terrain, 'records'):
            ground = tuple(sorted((key, tuple(record['terrain']))
                                  for key, record in terrain.records.items()))
        else:
            ground = tuple(terrain)
        roads = frozenset(roads)
        graph = size, frozenset(blocked), ground, bounds, roads, fences
        if graph not in self.graphs:
            self.graphs[graph] = [OrderedDict(), 0]
            if len(self.graphs) > self.graph_limit:
                self.graphs.popitem(last=False)
        self.graphs.move_to_end(graph)
        paths = self.graphs[graph]
        goals = frozenset(target) if isinstance(target, (set, frozenset)) else (target['x'], target['y'])
        key = start, goals, adjacent, road_mode if roads else None, diagonal
        if self.active and self.owner is not None:
            identity = graph, key
            job = self.pending.get(self.owner)
            if (job and job['key'] == key and job['identity'][0] != graph
                    and job['identity'][0][0] == graph[0]
                    and job['identity'][0][2:] == graph[2:]
                    and (job['identity'][0][1] ^ graph[1]).isdisjoint(job['examined'])):
                # Unexamined obstacle changes cannot affect earlier search
                # decisions. Apply them to future expansions without throwing
                # away progress whenever a distant tree or NPC moves.
                job['blocked'].clear()
                job['blocked'].update(graph[1])
                job.update(identity=identity, paths=paths)
                if 'result' in job:
                    self.remember(paths, key, job['result'])
            if job and identity in job['prefix']:
                path = job['prefix'][identity]
            elif job and job['identity'] == identity and 'result' in job:
                path = job['result']
                del self.pending[self.owner]
            elif key in paths[0]:
                paths[0].move_to_end(key)
                path = paths[0][key]
            else:
                if job and job['identity'] != identity:
                    del self.pending[self.owner]
                    job = None
                if self.reservation == self.owner:
                    self.reservation_seen = True
                if job is None and (len(self.pending) >= self.pending_limit
                                    or self.reservation not in (None, self.owner)):
                    # Reserve the next free slot for the first rejected actor.
                    # Ignore later admission attempts until that actor enters;
                    # this rotates admission through retrying actors without an
                    # unbounded overflow queue, even under continuous urgency.
                    if self.reservation is None:
                        self.reservation = self.owner
                        self.reservation_seen = True
                    self.waiting.add(self.owner[1])
                    raise RoutePending()
                if job is None:
                    if self.reservation == self.owner:
                        self.reservation = None
                    # Freeze terrain as well as obstacles. A suspended search
                    # never reads mutable authoritative chunk records.
                    if hasattr(terrain, 'records'):
                        from chunk_world import ChunkTerrain
                        frozen = ChunkTerrain({k: {'terrain': rows} for k, rows in ground}, size)
                    else:
                        frozen = ground
                    self.sequence += 1
                    search_blocked, examined = set(graph[1]), set()
                    job = {'identity': identity, 'key': key, 'paths': paths,
                           'priority': self.priority, 'order': self.sequence,
                           'blocked': search_blocked, 'examined': examined,
                           'prefix': dict(self.prefix),
                           'search': self.search_steps(start, goals if isinstance(target, (set, frozenset)) else dict(target),
                               search_blocked, size, frozen, adjacent, bounds, roads, road_mode, diagonal, fences,
                               examined)}
                    self.pending[self.owner] = job
                if job:
                    self.seen.add(id(job))
                    job['priority'] = min(job['priority'], self.priority)
                    # Small plans can finish immediately, but all calls and
                    # resumed jobs share the same hard per-update allowance.
                    # Reserve half for the end of the update, when even late
                    # actors have revalidated their jobs. Early high-priority
                    # actors must not spend the ordinary class's whole share.
                    self.drain(max(0, min(self.quantum, self.budget // 2 - self.work)))
                if job is None or 'result' not in job:
                    self.waiting.add(self.owner[1])
                    raise RoutePending()
                path = job['result']
                del self.pending[self.owner]
            # Keep a bounded prefix for multi-leg plans whose earlier graph
            # may leave the shared LRU while a later leg is still queued.
            prefix_cost = sum(len(p) if p else 1 for p in self.prefix.values())
            if len(self.prefix) < 16 and prefix_cost + (len(path) if path else 1) <= self.step_limit:
                self.prefix[identity] = path
        elif key in paths[0]:
            paths[0].move_to_end(key)
            path = paths[0][key]
        else:
            path = self.search(start, target, blocked, size, terrain, adjacent, bounds, roads, road_mode, diagonal, fences)
            self.remember(paths, key, path)
        # Callers consume and edit paths. Cache only immutable coordinates.
        return None if path is None else [{'x': x, 'y': y} for x, y in path]

    @staticmethod
    def search(*args, **kwargs):
        search = RouteCache.search_steps(*args, **kwargs)
        while True:
            try:
                next(search)
            except StopIteration as done:
                return done.value

    @staticmethod
    def search_steps(start, target, blocked, size, terrain, adjacent, bounds, roads=(), road_mode='smooth', diagonal=False, fences=(), examined=None):
        queue, previous = deque([start]), {start: None}
        costs, order = {start: 0}, 0
        weighted = bool(roads) or diagonal
        goal_bounds = None
        if isinstance(target, (set, frozenset)):
            if not target:
                return None
            goal_bounds = (min(x for x, y in target), max(x for x, y in target),
                           min(y for x, y in target), max(y for x, y in target))
        # A road step costs 4 units versus 6 on open ground (50% faster).
        # Smooth walkers spend half an edge on each tile. Builders time the
        # departure tile; haulers time the arrival tile. Keep those host rules.
        def heuristic(tile):
            if goal_bounds is not None:
                # A constant-cost lower bound for any goal, including a shop's
                # perimeter. A zero heuristic floods the map before returning
                # to nearby workplaces and adds whole simulation ticks of wait.
                dx = max(goal_bounds[0] - tile[0], 0, tile[0] - goal_bounds[1])
                dy = max(goal_bounds[2] - tile[1], 0, tile[1] - goal_bounds[3])
            else:
                dx, dy = abs(tile[0]-target['x']), abs(tile[1]-target['y'])
            distance = max(dx, dy)+(sqrt(2)-1)*min(dx, dy) if diagonal else dx+dy
            return (4 if roads else 6)*max(0, distance-int(adjacent and goal_bounds is None))

        def open_tile(tile):
            if examined is not None:
                examined.add(tile)
            x, y = tile
            return (0 <= x < size and 0 <= y < size and tile not in blocked
                    and (not bounds or bounds[0] <= x <= bounds[1] and bounds[2] <= y <= bounds[3])
                    and (not terrain or terrain[y][x] != 'v')
                    and (not bounds or not terrain or terrain[y][x]))

        heap = [(heuristic(start), 0, 0, start)] if weighted else []
        while (heap if weighted else queue):
            yield
            if weighted:
                _, _, cost, tile = heapq.heappop(heap)
                if costs[tile] != cost:
                    continue
            else:
                tile = queue.popleft()
            reached = tile in target if isinstance(target, (set, frozenset)) else abs(tile[0] - target['x']) + abs(tile[1] - target['y']) == int(adjacent)
            if reached and adjacent and not isinstance(target, (set, frozenset)):
                reached = not barns.crosses(tile, (target['x'], target['y']), fences)
            if reached:
                path = []
                while previous[tile] is not None:
                    yield
                    path.append(tile)
                    tile = previous[tile]
                return tuple(reversed(path))
            x, y = tile
            neighbors = ((x+1, y), (x, y+1), (x-1, y), (x, y-1))
            if diagonal:
                neighbors += ((x+1, y+1), (x-1, y+1), (x-1, y-1), (x+1, y-1))
            for next_tile in neighbors:
                if barns.crosses(tile, next_tile, fences):
                    continue
                nx, ny = next_tile
                slant = nx != x and ny != y
                if slant and (not open_tile((nx, y)) or not open_tile((x, ny))):
                    continue
                if open_tile(next_tile) and (weighted or next_tile not in previous):
                    if not weighted:
                        previous[next_tile] = tile
                        queue.append(next_tile)
                        continue
                    departure, arrival = (2 if tile in roads else 3), (2 if next_tile in roads else 3)
                    weight = 2 * departure if road_mode == 'departure' else 2 * arrival if road_mode == 'arrival' else departure + arrival
                    if slant:
                        weight *= sqrt(2)
                    next_cost = cost + weight
                    if next_cost >= costs.get(next_tile, float('inf')):
                        continue
                    previous[next_tile] = tile
                    costs[next_tile] = next_cost
                    order += 1
                    heapq.heappush(heap, (next_cost + heuristic(next_tile), order, next_cost, next_tile))
        return None


routes = RouteCache()
