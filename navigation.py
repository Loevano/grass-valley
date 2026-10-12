"""Bounded, derived shortest-path caches; never stored in world snapshots."""
from collections import OrderedDict, deque


class RouteCache:
    def __init__(self, graphs=8, routes=256, steps=32768):
        self.graph_limit, self.route_limit, self.step_limit = graphs, routes, steps
        self.graphs = OrderedDict()

    def route(self, start, target, blocked, size, terrain, adjacent=True, bounds=None):
        # Chunk records are sparse. Include loaded chunk keys as well as terrain
        # rows so loading/evicting terrain cannot reuse an obsolete route.
        if hasattr(terrain, 'records'):
            ground = tuple(sorted((key, tuple(record['terrain']))
                                  for key, record in terrain.records.items()))
        else:
            ground = tuple(terrain)
        graph = size, frozenset(blocked), ground, bounds
        if graph not in self.graphs:
            self.graphs[graph] = [OrderedDict(), 0]
            if len(self.graphs) > self.graph_limit:
                self.graphs.popitem(last=False)
        self.graphs.move_to_end(graph)
        paths = self.graphs[graph]
        goals = frozenset(target) if isinstance(target, set) else (target['x'], target['y'])
        key = start, goals, adjacent
        if key in paths[0]:
            paths[0].move_to_end(key)
            path = paths[0][key]
        else:
            path = self.search(start, target, blocked, size, terrain, adjacent, bounds)
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
    def search(start, target, blocked, size, terrain, adjacent, bounds):
        queue, previous = deque([start]), {start: None}
        while queue:
            tile = queue.popleft()
            reached = tile in target if isinstance(target, set) else abs(tile[0] - target['x']) + abs(tile[1] - target['y']) == int(adjacent)
            if reached:
                path = []
                while previous[tile] is not None:
                    path.append(tile)
                    tile = previous[tile]
                return tuple(reversed(path))
            x, y = tile
            for next_tile in ((x + 1, y), (x, y + 1), (x - 1, y), (x, y - 1)):
                nx, ny = next_tile
                if bounds and not (bounds[0] <= nx <= bounds[1] and bounds[2] <= ny <= bounds[3]):
                    continue
                if (0 <= nx < size and 0 <= ny < size and next_tile not in blocked
                        and next_tile not in previous and (not terrain or terrain[ny][nx] != 'v')
                        and (not bounds or not terrain or terrain[ny][nx])):
                    previous[next_tile] = tile
                    queue.append(next_tile)
        return None


routes = RouteCache()
