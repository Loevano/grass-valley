"""Frozen actual Python routing contracts plus independently scanned Dijkstra.

--check recomputes every Python route and oracle; it never updates the fixture.
Synthetic isolated geometry, not generated terrain/world performance evidence.
"""
import argparse
import copy
import json
import itertools
import math
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[6] / 'server'))
import barns
import spatial
import work_cycles
from navigation import RouteCache


def points(items):
    return [list(p) for p in sorted(items)]


def edge_cost(a, b, roads, mode):
    departure, arrival = (4 if a in roads else 6), (4 if b in roads else 6)
    weight = departure if mode == 'departure' else arrival if mode == 'arrival' else (departure + arrival) / 2
    return weight * (math.sqrt(2) if a[0] != b[0] and a[1] != b[1] else 1)


def oracle(start, target, adjacent, blocked, size, terrain, bounds, roads, mode, diagonal, fences):
    """Dense unsettled minimum scan; no heap, heuristic, production search or edge costs."""
    def reached(p):
        if isinstance(target, set):
            return p in target
        q = target['x'], target['y']
        return abs(p[0]-q[0])+abs(p[1]-q[1]) == int(adjacent) and (not adjacent or not barns.crosses(p, q, fences))
    def opened(p):
        x, y = p
        return (0 <= x < size and 0 <= y < size and p not in blocked
                and (bounds is None or bounds[0] <= x <= bounds[1] and bounds[2] <= y <= bounds[3])
                and (not terrain or terrain[y][x] != 'v') and (bounds is None or not terrain or terrain[y][x]))
    distance, settled = {start: 0.0}, set()
    while remaining := [(cost, p) for p, cost in distance.items() if p not in settled]:
        cost, p = min(remaining)
        if reached(p):
            return cost
        settled.add(p)
        for dx, dy in ((1,0),(0,1),(-1,0),(0,-1),(1,1),(-1,1),(-1,-1),(1,-1)):
            if dx and dy and not diagonal:
                continue
            q = p[0]+dx, p[1]+dy
            if (not opened(q) or barns.crosses(p,q,fences)
                    or dx and dy and (not opened((q[0],p[1])) or not opened((p[0],q[1])))):
                continue
            candidate = cost + edge_cost(p,q,roads,mode)
            if candidate < distance.get(q, math.inf):
                distance[q] = candidate
    return None


def make(name, source, start, target, kind='exact', profile='ordinary', exemptions=(), only=None,
         terrain=None, bounds=None, diagonal=False, mode='smooth'):
    size = source['map_size']
    bounds = None if bounds is None else tuple(bounds)
    blocked = spatial.SpatialIndex(source['map_resources']).blocked(source)
    if profile != 'ordinary':
        blocked = blocked - {(b['x'],b['y']) for b in source['buildings'] if b['kind']=='acre' and 'construction' not in b}
    blocked = blocked - set(exemptions)
    if only is not None:
        blocked |= {(x,y) for x in range(size) for y in range(size)} - set(only)
    roads = {(r['x'],r['y']) for r in source['roads'] if 'construction' not in r}
    fences = barns.fences(source['buildings'], size)
    adjacent = kind in ('anchor','perimeter')
    actual_target = set(map(tuple,target)) if kind == 'multiple' else target
    cache = RouteCache()
    path = cache.route(start, actual_target, blocked, size, terrain or [], adjacent, bounds, roads, mode, diagonal, fences)
    goals = actual_target
    if kind == 'perimeter':
        goals = {p for p in spatial.perimeter(target)-blocked
                 if any(abs(p[0]-q[0])+abs(p[1]-q[1]) == 1 and not barns.crosses(p,q,fences)
                        for q in spatial.footprint_tiles(target))}
    expected = oracle(start,goals,adjacent,blocked,size,terrain or [],bounds,roads,mode,diagonal,fences)
    route = None if path is None else [(p['x'],p['y']) for p in path]
    at, cost = start, 0.0
    for q in route or []:
        dx,dy=abs(at[0]-q[0]),abs(at[1]-q[1])
        assert max(dx,dy)==1 and (diagonal or dx+dy==1), (name,'neighbor')
        assert q not in blocked and 0<=q[0]<size and 0<=q[1]<size, (name,'access')
        assert not barns.crosses(at,q,fences), (name,'fence')
        if dx and dy:
            for side in ((at[0],q[1]),(q[0],at[1])):
                assert side not in blocked and (not terrain or terrain[side[1]][side[0]]!='v'), (name,'corner')
        assert not terrain or terrain[q[1]][q[0]]!='v', (name,'terrain')
        if bounds:
            assert bounds[0]<=q[0]<=bounds[1] and bounds[2]<=q[1]<=bounds[3], (name,'bounds')
            assert not terrain or terrain[q[1]][q[0]], (name,'missing')
        cost += edge_cost(at,q,roads,mode)
        at = q
    assert (expected is None) == (route is None), name
    if expected is not None:
        assert abs(expected-cost) < 1e-9, (name,expected,cost)
    return dict(name=name,source=source,start=start,target=target,kind=kind,profile=profile,
                exemptions=points(exemptions),only=None if only is None else points(only),terrain=None if not terrain else [[terrain[y][x] for x in range(size)] for y in range(size)],
                bounds=bounds,diagonal=diagonal,mode=mode,path=route,cost=None if route is None else cost,
                oracle_cost=expected,state='unreachable' if route is None else 'arrived' if not route else 'completed')


def vectors():
    source = dict(map_size=12,buildings=[
        dict(kind='workshop',x=7,y=6,footprint=[2,3]),
        dict(kind='acre',x=3,y=3,footprint=[2,2]),
        dict(kind='acre',x=4,y=3,construction={}),
        dict(kind='acre',x=5,y=3,construction={'upgrade':{}}),
        dict(kind='barn',x=9,y=2,fence_orientation=2)],
        map_resources=[dict(kind='tree',x=3,y=3),dict(kind='tree',x=6,y=5)],
        npcs=[dict(kind='trader',x=4,y=7,footprint=[2,1])],roads=[],villagers=[dict(x=1,y=1)])
    cases=[]
    for seed, (diagonal, mode, profile, kind, ground) in enumerate(itertools.product(
            (False, True), ('smooth','departure','arrival'), ('ordinary','farmer','construction'),
            ('exact','anchor','perimeter','multiple'), range(4))):
        g=copy.deepcopy(source)
        if seed%4:
            g['roads']=[dict(x=x,y=1) for x in range(1,10)]+[dict(x=10,y=1,construction={}),dict(x=11,y=1,construction={'upgrade':{}})]
        target=([[10,9],[9,10]] if kind=='multiple' else dict(x=7,y=6,**({'footprint':[2,3]} if kind=='perimeter' else {})))
        terrain=None if ground==0 else ['g'*12 for _ in range(12)]
        bounds=None if ground < 2 else [0,10,0,10]
        if terrain and ground==3:
            terrain[5]='gggggvvggggg'
        cases.append(make(f'matrix-{seed}',g,(1,1),target,kind,profile,[(6,5)] if seed%5==0 else (),terrain=terrain,bounds=bounds,diagonal=diagonal,mode=mode))
    empty=dict(map_size=5,buildings=[],map_resources=[],npcs=[],roads=[],villagers=[])
    specials=[('arrived-blocked',(2,2),dict(x=2,y=2),'exact'),('leave-blocked',(2,2),dict(x=3,y=2),'exact'),
              ('raw-start-anchor',(2,2),dict(x=2,y=2),'anchor'),('out-of-map-arrived',(-1,2),dict(x=-1,y=2),'exact'),
              ('out-of-map-anchor',(0,2),dict(x=-1,y=2),'anchor'),('out-of-map-goal',(0,2),dict(x=-1,y=2),'exact'),
              ('empty',(0,0),[],'multiple'),('multiple-arrived',(0,0),[[0,0],[4,4]],'multiple')]
    for name,start,target,kind in specials:
        g=copy.deepcopy(empty)
        g['map_resources']=[dict(kind='tree',x=2,y=2)]
        cases.append(make(name,g,start,target,kind))
    for kind in ('anchor','perimeter'):
        g=copy.deepcopy(empty)
        g['map_size']=8
        g['buildings']=[dict(kind='barn',x=4,y=4)]
        target=dict(x=1,y=4,**({'footprint':[1,1]} if kind=='perimeter' else {}))
        cases.append(make(f'final-fence-{kind}',g,(0,4),target,kind,bounds=[0,1,4,4]))
    for profile in ('ordinary','farmer','construction'):
        for exempt in (False,True):
            g=copy.deepcopy(empty)
            g['buildings']=[dict(kind='acre',x=1,y=2),dict(kind='workshop',x=2,y=2,footprint=[2,2])]
            g['map_resources']=[dict(kind='tree',x=1,y=2)]
            cases.append(make(f'perimeter-admission-{profile}-{exempt}',g,(1,2),dict(x=2,y=2,footprint=[2,2]),'perimeter',profile,[(1,2)] if exempt else ()))
    for name,rows,bounds in [('missing-unbounded',['']*5,None),('missing-bounded',['']*5,[0,4,0,4]),
                             ('unloaded',['gvggg']*5,[0,4,0,4])]:
        if name.startswith('missing'):
            rows=tuple(tuple('' for _ in range(5)) for _ in range(5))
        cases.append(make(name,empty,(0,0),dict(x=4,y=0),terrain=rows,bounds=bounds))
    for dx,dy in [(1,0),(0,1),(1,1)]:
        g=copy.deepcopy(empty)
        g['map_resources']=[dict(kind='tree',x=1,y=0),dict(kind='tree',x=0,y=1)]
        cases.append(make(f'corner-{dx}-{dy}',g,(0,0),dict(x=dx,y=dy),diagonal=True))
    # Actual pier helper selects the deck; compare its route through the same overlay.
    g=copy.deepcopy(empty)
    hut=dict(kind='fishing_hut',x=1,y=2,boat_level=1)
    g['buildings']=[hut]
    g['map_resources']=[dict(kind='water',x=x,y=y) for x in range(2,5) for y in range(1,4)]
    blocked=spatial.SpatialIndex(g['map_resources']).blocked(g)
    pier=work_cycles.fishing_pier(hut,g,blocked,5,[],None)
    assert len(pier)>1
    permitted={(p['x'],p['y']) for p in g['map_resources']}
    exemptions=permitted|{(1,2)}
    c=make('actual-pier',g,(1,2),pier[-1],exemptions=exemptions)
    assert c['path']==[(p['x'],p['y']) for p in pier[1:]]
    cases.append(c)
    water=work_cycles.fishing_water(hut,g['map_resources'],spatial.occupied_tiles(g['buildings']),6)
    sea={(p['x'],p['y']) for p in water}
    assert sea
    cases.append(make('actual-permitted-sailing',g,min(sea),dict(zip(('x','y'),max(sea))),exemptions=sea,only=sea,bounds=[0,4,0,4]))
    cases.append(make('empty-ordinary-terrain',empty,(0,0),dict(x=4,y=4),terrain=[],bounds=[0,4,0,4]))
    # Actual sparse ChunkTerrain records: loaded empty cells differ from unloaded 'v'.
    from chunk_world import ChunkTerrain, CHUNK_SIZE
    for loaded in (False, True):
        for missing in (False, True):
            size=CHUNK_SIZE+2
            g=copy.deepcopy(empty)
            g['map_size']=size
            rows=[['g']*CHUNK_SIZE for _ in range(CHUNK_SIZE)]
            if missing:
                rows[1][-1]=''
            records={'0,0': {'terrain':tuple(tuple(row) for row in rows)}}
            if loaded:
                records['1,0']={'terrain':tuple('g'*CHUNK_SIZE for _ in range(CHUNK_SIZE))}
            terrain=ChunkTerrain(records,size)
            for bounded in (False, True):
                cases.append(make(f'chunk-{loaded}-{missing}-{bounded}',g,(CHUNK_SIZE-2,1),dict(x=CHUNK_SIZE,y=1),terrain=terrain,
                                  bounds=[CHUNK_SIZE-2,CHUNK_SIZE,1,1] if bounded else None))
    # Normal shared walking_map assembly includes every town's geometry/roads.
    from shared_world import walking_map
    town_a=copy.deepcopy(source)
    town_b=copy.deepcopy(empty)
    town_b['buildings']=[dict(kind='hall',x=2,y=8,footprint=[3,2])]
    town_b['roads']=[dict(x=1,y=y) for y in range(3,10)]
    world={'towns':{'a':town_a,'b':town_b}}
    assembled=walking_map(world,town_a)
    cases.append(make('actual-multi-town-walking-map',assembled,(1,1),dict(x=2,y=8,footprint=[3,2]),'perimeter',diagonal=True))
    # Deterministically varied topology, targets, road placement and fence orientation.
    for seed in range(96):
        g=copy.deepcopy(source)
        g['buildings'][-1]['fence_orientation']=seed%4
        bits=seed+23
        for y in range(12):
            for x in range(12):
                bits=(bits*1664525+1013904223) & 0xffffffff
                if bits%9==0:
                    g['map_resources'].append(dict(kind='tree',x=x,y=y))
                if bits%13==0:
                    g['roads'].append(dict(x=x,y=y))
        kind=('exact','anchor','perimeter','multiple')[seed%4]
        target=([[10,9],[9,10]] if kind=='multiple' else dict(x=7,y=6,**({'footprint':[2,3]} if kind=='perimeter' else {})))
        cases.append(make(f'varied-{seed}',g,(seed%3,seed%5),target,kind,('ordinary','farmer','construction')[seed%3],diagonal=bool(seed%2),mode=('smooth','departure','arrival')[seed//2%3]))
    return cases


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    output=json.dumps(vectors(),sort_keys=True,separators=(',',':'))+'\n'
    path=Path(__file__).with_name('python.json')
    if args.check:
        assert path.read_text()==output, 'Python routing fixtures differ; inspect source contracts before regeneration'
        print('Python routing fixtures and independent oracle agree')
    else:
        path.write_text(output)

if __name__=='__main__':
    main()
