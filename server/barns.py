"""Barn pen geometry shared by host collision and route planning."""
from functools import lru_cache

NORMALS = ((1, 0), (0, 1), (-1, 0), (0, -1))


def active(building):
    return building['kind'] == 'barn' and (not building.get('construction')
                                           or 'upgrade' in building['construction'])


@lru_cache(maxsize=256)
def layout(x, y, size, orientation=0):
    left, right = max(-.5, x - 3.5), min(size - .5, x + 3.5)
    back, front = max(-.5, y - 3.5), min(size - .5, y + 3.5)
    sides = ((right, back, right, front), (right, front, left, front),
             (left, front, left, back), (left, back, right, back))
    available = (right < size - .5, front < size - .5, left > -.5, back > -.5)
    if orientation not in range(4) or not available[orientation]:
        orientation = next((i for i, valid in enumerate(available) if valid), 0)
    nx, ny = NORMALS[orientation]
    tx, ty = -ny, nx
    edge = sides[orientation]
    lo, hi = sorted(((edge[0] - x) * tx + (edge[1] - y) * ty,
                     (edge[2] - x) * tx + (edge[3] - y) * ty))
    start = max(lo + 2, min(.5, hi - 2))
    distance = (edge[0] - x) * nx + (edge[1] - y) * ny
    a = x + distance * nx + start * tx, y + distance * ny + start * ty
    b = a[0] + 2 * tx, a[1] + 2 * ty
    segments = tuple(side for i, side in enumerate(sides) if i != orientation)
    segments += ((edge[0], edge[1], *a), (*b, edge[2], edge[3]))
    return orientation, a, b, segments


def pen(building, size=120):
    return layout(building['x'], building['y'], size, building.get('fence_orientation', 0))


def fences(buildings, size=120):
    return tuple(segment for b in buildings if active(b) for segment in pen(b, size)[3])


def blocked(x, y, segments, radius=.18):
    for x1, y1, x2, y2 in segments:
        if x1 == x2:
            if abs(x - x1) < radius and min(y1, y2) - radius < y < max(y1, y2) + radius:
                return True
        elif abs(y - y1) < radius and min(x1, x2) - radius < x < max(x1, x2) + radius:
            return True
    return False


def crosses(start, end, segments):
    """Test a movement segment, including diagonal steps and corner contacts."""
    ax, ay = start
    bx, by = end
    for x1, y1, x2, y2 in segments:
        if x1 == x2 and abs(bx - ax) > 1e-9:
            t = (x1 - ax) / (bx - ax)
            if 0 <= t <= 1 and min(y1, y2) - 1e-9 <= ay + t * (by - ay) <= max(y1, y2) + 1e-9:
                return True
        elif y1 == y2 and abs(by - ay) > 1e-9:
            t = (y1 - ay) / (by - ay)
            if 0 <= t <= 1 and min(x1, x2) - 1e-9 <= ax + t * (bx - ax) <= max(x1, x2) + 1e-9:
                return True
    return False
