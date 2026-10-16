"""Independent Python contract vectors; synthetic isolated geometry only."""
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[6] / 'server'))
import barns
import spatial
from navigation import RouteCache


def tiles(points):
    return [list(p) for p in sorted(points)]


def main():
    geometries = [
        {'map_size': 24, 'buildings': [
            {'kind': 'hall', 'x': 1, 'y': 1, 'orientation': 1},
            {'kind': 'sawpit', 'x': 3, 'y': 2, 'footprint': [2, 3], 'orientation': 0},
            {'kind': 'acre', 'x': 7, 'y': 7},
            {'kind': 'acre', 'x': 8, 'y': 7, 'construction': {'upgrade': {}}},
            {'kind': 'acre', 'x': 9, 'y': 7, 'construction': {}},
            {'kind': 'barn', 'x': 14, 'y': 14, 'fence_orientation': 2},
            {'kind': 'barn', 'x': 0, 'y': 0, 'fence_orientation': 2, 'construction': {}},
            {'kind': 'barn', 'x': 22, 'y': 22, 'construction': {'remaining': 3}},
            {'kind': 'unknown_extension', 'x': 20, 'y': 2, 'future': {'secret': 10**30}},
        ], 'map_resources': [{'kind': 'tree', 'x': 8, 'y': 7}, {'kind': 'water', 'x': 10, 'y': 10}],
        'npcs': [{'kind': 'resident', 'x': 5, 'y': 5, 'footprint': [2, 1]}],
        'roads': [{'x': 10, 'y': 1}, {'x': 11, 'y': 1, 'construction': {}}, {'x': 12, 'y': 1, 'construction': {'upgrade': {}}}],
        'villagers': [{'x': 6, 'y': 6}], 'private_extension': [1, 2, 3]},
    ]
    cases = []
    for g in geometries:
        blocked = spatial.SpatialIndex(g['map_resources']).blocked(g)
        completed_acres = {(b['x'], b['y']) for b in g['buildings'] if b['kind'] == 'acre' and 'construction' not in b}
        cases.append({'source': g, 'blocked': tiles(blocked), 'farmer': tiles(blocked-completed_acres),
                      'roads': tiles((r['x'],r['y']) for r in g['roads'] if 'construction' not in r),
                      'footprints': [tiles(spatial.footprint_tiles(b)) for b in g['buildings']],
                      'perimeters': [tiles(spatial.perimeter(b)) for b in g['buildings']],
                      'fences': barns.fences(g['buildings'],g['map_size'])})
    pens=[]
    for size in (5,12,24):
        for x,y in ((0,0),(size-1,size-1),(size//2,size//2)):
            for orientation in (-1,0,1,2,3,4):
                actual,a,b,segments=barns.layout(x,y,size,orientation)
                points=[(x+dx/10,y+dy/10) for dx in (-36,-35,-34,-2,0,2,34,35,36) for dy in (-36,-35,-34,-2,0,2,34,35,36)]
                edges=[((x,y),p) for p in points]
                pens.append({'origin':[x,y],'size':size,'requested':orientation,'orientation':actual,'gate':[a,b],'segments':segments,
                             'points': [[*p,barns.blocked(*p,segments)] for p in points],
                             'edges': [[list(a),list(b),barns.crosses(a,b,segments)] for a,b in edges]})
    costs=[]
    for mode in ('smooth','departure','arrival'):
        for road_start in (False,True):
            for road_end in (False,True):
                for end in ((3,2),(3,3)):
                    roads={(2,2)} if road_start else set()
                    if road_end: roads.add(end)
                    job=RouteCache.search_steps((2,2),{'x':end[0],'y':end[1]},set(),8,[],False,None,roads,mode,True)
                    while True:
                        next(job)
                        cost=job.gi_frame.f_locals['costs'].get(end)
                        if cost is not None: break
                    costs.append({'mode':mode,'roads':tiles(roads),'end':end,'cost':cost})
    output={'cases':cases,'pens':pens,'costs':costs}
    Path(__file__).with_name('python.json').write_text(json.dumps(output,separators=(',',':'))+'\n')

if __name__ == '__main__': main()
