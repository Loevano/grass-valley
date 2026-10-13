"""Winding river networks and varied habitats for generation version three."""
from functools import lru_cache
import math
import random


@lru_cache(maxsize=64)
def layout(seed):
    rng = random.Random(f'{seed}:regions-v3')
    lake_x = rng.uniform(.56, .60)
    return {
        'swamp': (rng.uniform(.16, .19), .43, .22, .25),
        'desert': (rng.uniform(.79, .82), .43, .17, .21),
        'ice': (.26, .05, .29, .16),
        'lake': (lake_x, .79, .145, .115),
        'islands': ((lake_x-.055, .765, .023, .018),
                    (lake_x+.035, .815, .030, .022),
                    (lake_x+.060, .755, .016, .020)),
        'oases': ((.85, .36, .013, .010), (.79, .54, .016, .011)),
        'ice_pools': ((.20, .075, .020, .012), (.33, .045, .014, .010)),
        'phase': rng.uniform(0, math.tau),
    }


@lru_cache(maxsize=64)
def river_segments(seed, size):
    """Sample connected Catmull–Rom curves once per world, in tile units."""
    lake = layout(seed)['lake'][:2]
    junction = (.66, .56)
    west = (.13, .63)
    north = (.65, .12)
    paths = (
        (.0065, ((.78, -.03), (.70, .045), north, (.71, .24),
                  (.64, .34), (.65, .44), junction, (.60, .67), lake)),
        (.0050, ((-.03, .55), (.04, .59), west, (.23, .73),
                  (.34, .66), (.43, .74), (.51, .70), lake)),
        (.0035, ((-.03, .22), (.045, .30), (.09, .39), (.045, .49), west)),
        (.0040, ((1.03, .69), (.91, .62), (.83, .72), (.75, .63), junction)),
        (.0035, ((.08, -.03), (.13, .065), (.075, .14), (.18, .22),
                  (.30, .18), (.39, .25), (.52, .20), north)),
    )
    segments = []
    phase = layout(seed)['phase']
    for branch, (width, controls) in enumerate(paths):
        rng = random.Random(f'{seed}:river-v3:{branch}')
        # Shared junctions and endpoints remain exact. Only interior bends vary.
        fixed = {lake, junction, west, north}
        points = [point if point in fixed or index in (0, len(controls)-1)
                  else (point[0]+rng.uniform(-.008, .008), point[1]+rng.uniform(-.008, .008))
                  for index, point in enumerate(controls)]
        samples = []
        for index in range(len(points)-1):
            p0, p1 = points[max(0, index-1)], points[index]
            p2, p3 = points[index+1], points[min(len(points)-1, index+2)]
            for step in range(12):
                t = step / 12
                samples.append(tuple(.5 * (2*p1[axis] + (-p0[axis]+p2[axis])*t
                    + (2*p0[axis]-5*p1[axis]+4*p2[axis]-p3[axis])*t*t
                    + (-p0[axis]+3*p1[axis]-3*p2[axis]+p3[axis])*t*t*t) * size
                    for axis in (0, 1)))
        samples.append(tuple(value*size for value in points[-1]))
        for index, (a, b) in enumerate(zip(samples, samples[1:])):
            dx, dy = b[0]-a[0], b[1]-a[1]
            radius = width*size*(.85 + .25*index/(len(samples)-1)
                                 + .12*math.sin(index*.3+phase))
            segments.append((a[0], a[1], dx, dy, 1/(dx*dx+dy*dy), radius))
    return tuple(segments)


@lru_cache(maxsize=2048)
def local_rivers(seed, size, cx, cy):
    """Only nearby segments participate in tile sampling; no world-size scan."""
    left, top = cx*40, cy*40
    result = []
    for ax, ay, dx, dy, inverse, radius in river_segments(seed, size):
        margin = radius+3
        if (max(ax, ax+dx)+margin >= left and min(ax, ax+dx)-margin < left+40
                and max(ay, ay+dy)+margin >= top and min(ay, ay+dy)-margin < top+40):
            result.append((ax, ay, dx, dy, inverse, radius))
    return tuple(result)


def sample_tile(seed, x, y, size, field, sparse_grass=False):
    u, v = x/size, y/size
    plan = layout(seed)
    detail = field(seed, 'v3-edge', x, y, size/24)
    woods = field(seed, 'v3-woods', x, y, size/22)
    canopy = field(seed, 'v3-canopy', x, y, size/60)
    wet = field(seed, 'v3-moisture', x, y, size/9)
    soil = field(seed, 'v3-ground', x, y, size/14)

    def distance(ellipse):
        cx, cy, rx, ry = ellipse
        return ((u-cx)/rx)**2 + ((v-cy)/ry)**2

    def inside(name):
        return distance(plan[name]) < .80 + detail*.40

    biome = 'f' if woods > .62 else 'r' if soil > .80 else 'm' if wet > .48 else 'g'
    trees = max(0, woods-.36)*(.45+canopy)*.90
    if sparse_grass and biome in 'gm':
        # Open farmland between woodland patches; specialized groves below
        # still set their own density, including riverbanks.
        trees *= .5
    rock = .18 if biome == 'r' else .008
    species = None
    marsh = distance(plan['swamp'])
    if inside('ice'):
        biome = 'f' if woods > .66 and canopy > .48 else 'r' if soil > .71 else 'i'
        trees = .12 + max(0, canopy-.45)*.60 if biome == 'f' else 0
        rock = .09 if biome in 'ir' else .015
    elif inside('desert'):
        biome = 'r' if soil > .69 and detail > .42 else 'd'
        trees = 0
        rock = .15 if biome == 'r' else .018
    elif marsh < .80+detail*.40 or (marsh < 1.55 and detail > .65):
        biome = 'b'
        trees = .30 + woods*.22 + canopy*.15
        rock = 0
    elif soil < .22 and detail < .30:
        biome, trees = 's', 0

    channel, bank = False, False
    for ax, ay, dx, dy, inverse, radius in local_rivers(seed, size, x//40, y//40):
        t = max(0, min(1, ((x-ax)*dx+(y-ay)*dy)*inverse))
        squared = (x-ax-t*dx)**2 + (y-ay-t*dy)**2
        if squared < radius*radius:
            channel = True
            break
        if squared < (radius+2.5)**2:
            bank = True

    # Irregular shorelines and distinct wooded islands break up the big lake.
    lake = distance(plan['lake']) < .78 + detail*.44
    island = lake and any(distance(ellipse) < .76+detail*.38 for ellipse in plan['islands'])
    water = channel or (lake and not island)
    if island and not channel:
        biome, trees, rock = ('f' if woods > .48 else 'g'), .25+canopy*.25, .015
    if biome == 'b':
        pools = field(seed, 'v3-marsh-pools', x, y, size/40)
        ribbons = field(seed, 'v3-marsh-ribbons', x, y, size/14)
        water = water or pools < .34 or (abs(ribbons-.5) < .045 and pools < .57)
    if biome in 'dr':
        oasis = min(distance(ellipse) for ellipse in plan['oases'])
        if oasis < 1:
            water = True
        elif oasis < 4:
            biome, trees, species, rock = 'g', .22+canopy*.30, 'palm', 0
    if biome in 'ir' and any(distance(ellipse) < 1 for ellipse in plan['ice_pools']):
        water = True
    if bank and not water and biome in 'gmf':
        biome, trees = 'm', max(trees, .12+canopy*.22)
    if water:
        trees, rock = 0, 0
        if biome not in 'bdi':
            biome = 's'
    return biome, water, trees, rock, species
