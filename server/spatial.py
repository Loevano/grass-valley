"""Temporary spatial lookups shared by workers during one simulation tick."""

CELL_SIZE = 8

# Dimensions are saved on each building; absent dimensions preserve old layouts.
BUILDING_FOOTPRINTS = {'hall': (2, 2), 'mill': (2, 2), 'sawpit': (3, 2), 'workshop': (2, 2),
                       'butcher': (2, 2), 'bakery': (2, 2), 'smokehouse': (2, 2), 'tannery': (2, 2)}


def footprint_tiles(building):
    width, depth = building.get('footprint', (1, 1))
    return {(building['x'] + dx, building['y'] + dy)
            for dx in range(width) for dy in range(depth)}


def occupied_tiles(items):
    return {tile for item in items for tile in footprint_tiles(item)}


def perimeter(building):
    tiles = footprint_tiles(building)
    return {(x + dx, y + dy) for x, y in tiles
            for dx, dy in ((1, 0), (0, 1), (-1, 0), (0, -1))} - tiles


class SpatialIndex:
    def __init__(self, resources):
        self.resources = resources
        self.resource_count = None
        self.cells = {}
        self.obstacles = {}

    def refresh(self):
        # During a tick resources can disappear when felled. Rebuild before the
        # next lookup; supply changes remain visible through object references.
        if self.resource_count == len(self.resources):
            return
        self.cells = {}
        self.resource_tiles = set()
        for order, resource in enumerate(self.resources):
            x, y = resource['x'], resource['y']
            self.cells.setdefault((x // CELL_SIZE, y // CELL_SIZE), []).append((order, resource))
            self.resource_tiles.update(footprint_tiles(resource))
        self.resource_count = len(self.resources)
        self.depths = None
        self.obstacles.clear()

    def water_depths(self):
        """Tile distance from the nearest bank; shoreline water has depth 1."""
        self.refresh()
        if self.depths is not None:
            return self.depths
        water = {(r['x'], r['y']) for r in self.resources if r['kind'] == 'water'}
        depths = {p: 1 for p in water if any(q not in water for q in
                  ((p[0] + 1, p[1]), (p[0] - 1, p[1]), (p[0], p[1] + 1), (p[0], p[1] - 1)))}
        queue = list(depths)
        for x, y in queue:
            for p in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                if p in water and p not in depths:
                    depths[p] = depths[(x, y)] + 1
                    queue.append(p)
        self.depths = depths
        return depths

    def nearby(self, kind, x, y, reach):
        self.refresh()
        matches = []
        for cx in range((x - reach) // CELL_SIZE, (x + reach) // CELL_SIZE + 1):
            for cy in range((y - reach) // CELL_SIZE, (y + reach) // CELL_SIZE + 1):
                matches.extend((order, resource) for order, resource in self.cells.get((cx, cy), ())
                               if resource['kind'] == kind
                               and abs(resource['x'] - x) + abs(resource['y'] - y) <= reach)
        # Preserve map order, including tie-breaking between equally near sources.
        matches.sort(key=lambda item: item[0])
        return [resource for _, resource in matches]

    def blocked(self, landscape):
        self.refresh()
        buildings = tuple(sorted(occupied_tiles(landscape['buildings'])))
        npcs = tuple(sorted(occupied_tiles(landscape.get('npcs', []))))
        key = buildings, npcs
        if key not in self.obstacles:
            self.obstacles[key] = self.resource_tiles.union(buildings, npcs)
        return self.obstacles[key]
