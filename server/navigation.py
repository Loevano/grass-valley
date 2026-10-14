"""Bounded, derived travel-time route caches; never stored in world snapshots."""
from collections import OrderedDict, deque
import heapq
from math import sqrt
from spatial import perimeter


class RouteCache:
    def __init__(self, graphs=8, routes=256, steps=32768):
        self.graph_limit, self.route_limit, self.step_limit = graphs, routes, steps
        self.graphs = OrderedDict()

    def route(self, start, target, blocked, size, terrain, adjacent=True, bounds=None,
              roads=(), road_mode='smooth', diagonal=False):
        if adjacent and isinstance(target, dict) and target.get('footprint'):
            target = perimeter(target) - set(blocked)
        # Chunk records are sparse. Include loaded chunk keys as well as terrain
        # rows so loading/evicting terrain cannot reuse an obsolete route.
        if hasattr(terrain, 'records'):
            ground = tuple(sorted((key, tuple(record['terrain']))
                                  for key, record in terrain.records.items()))
        else:
            ground = tuple(terrain)
        roads = frozenset(roads)
        graph = size, frozenset(blocked), ground, bounds, roads
        if graph not in self.graphs:
            self.graphs[graph] = [OrderedDict(), 0]
            if len(self.graphs) > self.graph_limit:
                self.graphs.popitem(last=False)
        self.graphs.move_to_end(graph)
        paths = self.graphs[graph]
        goals = frozenset(target) if isinstance(target, set) else (target['x'], target['y'])
        key = start, goals, adjacent, road_mode if roads else None, diagonal
        if key in paths[0]:
            paths[0].move_to_end(key)
            path = paths[0][key]
        else:
            path = self.search(start, target, blocked, size, terrain, adjacent, bounds, roads, road_mode, diagonal)
            cost = len(path) if path is not None else 1
            if cost <= self.step_limit:
                paths[0][key] = path
                paths[1] += cost
                while len(paths[0]) > self.route_limit or paths[1] > self.step_limit:
                    _, old = paths[0].popitem(last=False)
                    paths[1] -= len(old) if old is not None else 1
        # Callers consume and edit paths. Cache only immutable coordinates.
        return None if path is None else [{'x': x, 'y': y} for x, y in path]

    @staticmethod
    def search(start, target, blocked, size, terrain, adjacent, bounds, roads=(), road_mode='smooth', diagonal=False):
        queue, previous = deque([start]), {start: None}
        costs, order = {start: 0}, 0
        weighted = bool(roads) or diagonal
        # A road step costs 4 units versus 6 on open ground (50% faster).
        # Smooth walkers spend half an edge on each tile. Builders time the
        # departure tile; haulers time the arrival tile. Keep those host rules.
        def heuristic(tile):
            if isinstance(target, set):
                return 0
            dx, dy = abs(tile[0]-target['x']), abs(tile[1]-target['y'])
            distance = max(dx, dy)+(sqrt(2)-1)*min(dx, dy) if diagonal else dx+dy
            return (4 if roads else 6)*max(0, distance-int(adjacent))

        def open_tile(tile):
            x, y = tile
            return (0 <= x < size and 0 <= y < size and tile not in blocked
                    and (not bounds or bounds[0] <= x <= bounds[1] and bounds[2] <= y <= bounds[3])
                    and (not terrain or terrain[y][x] != 'v')
                    and (not bounds or not terrain or terrain[y][x]))

        heap = [(heuristic(start), 0, 0, start)] if weighted else []
        while (heap if weighted else queue):
            if weighted:
                _, _, cost, tile = heapq.heappop(heap)
                if costs[tile] != cost:
                    continue
            else:
                tile = queue.popleft()
            reached = tile in target if isinstance(target, set) else abs(tile[0] - target['x']) + abs(tile[1] - target['y']) == int(adjacent)
            if reached:
                path = []
                while previous[tile] is not None:
                    path.append(tile)
                    tile = previous[tile]
                return tuple(reversed(path))
            x, y = tile
            neighbors = ((x+1, y), (x, y+1), (x-1, y), (x, y-1))
            if diagonal:
                neighbors += ((x+1, y+1), (x-1, y+1), (x-1, y-1), (x+1, y-1))
            for next_tile in neighbors:
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
