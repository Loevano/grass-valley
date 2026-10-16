"""Actual Python shared_world + command + ChunkMap movement/recovery vectors."""
import copy
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[6] / 'server'))
from server import move_player, walkable
from shared_world import bind_map, walking_map
from chunk_world import ChunkMap, ChunkTerrain


def generate():
    cases = []
    def case(name, world, inputs=((0, 0),), records=(), probes=()):
        source = copy.deepcopy(world)
        original_records = copy.deepcopy(records)
        if world.get('shared_map', {}).get('chunked'):
            # Install already-active copied terrain, independently of generation.
            manager = object.__new__(ChunkMap)
            manager.world = world
            manager.landscape = world['shared_map']
            manager.records = {key: rec for key, rec in copy.deepcopy(records)}
            manager.resource_index = None
            manager.terrain = ChunkTerrain(manager.records, world['shared_map']['map_size'])
            manager.landscape['terrain'] = manager.terrain
            world['_chunks'] = manager
        bind_map(world)
        town = world['towns']['a']
        opened = [walkable(walking_map(world, town) if 'shared_map' in world else town, *p) for p in probes]
        for x,y in inputs:
            scene = walking_map(world, town, local=True) if 'shared_map' in world else town
            result = move_player(scene, dict(dx=x,dy=y))
        expected = result['player']
        for key, value in source['towns']['a'].get('player', {}).items():
            if key not in ('x','y','facing','activity','activity_status'):
                assert expected[key] == value, (name,key)
        cases.append(dict(name=name,source=source,records=original_records,inputs=inputs,expected=expected,probes=probes,open=opened))
    def town(x=4.,y=4.,**kwargs):
        return dict(buildings=[], roads=[], villagers=[], player=dict(x=x,y=y,facing=-1,activity='fish',activity_status='Fishing',extension={'preserve':1234567890123456789}),**kwargs)
    def base(size=12):
        return dict(towns={'a':town(),'b':town(9.,9.)},shared_map=dict(map_size=size,terrain=['g'*size]*size,map_resources=[],npcs=[]))
    w=base(); w['towns']['b']['buildings']=[dict(kind='hall',x=4,y=4)]; case('shared-recovery-x-major-tie',w,probes=[(4,4),(3,4)])
    w=base(); w['towns']['a']['player'].update(x=4.4,y=4.2); case('fractional-valid-no-recenter',w)
    w=base(); w['towns']['b']['roads']=[dict(x=4,y=4),dict(x=5,y=4)]; case('all-town-roads',w,inputs=[(1,0)]*5)
    w=base(); w['towns']['b']['buildings']=[dict(kind='barn',x=7,y=7)]; case('other-town-fence',w,inputs=[(1,0)]*5)
    w=base(); w['shared_map']['npcs']=[dict(kind='merchant',x=4,y=4)]; w['towns']['a']['map_resources']=[dict(kind='rock',x=4,y=4)]; case('landscape-resources-npcs-not-town-stale',w)
    w=base(); w['shared_map']['map_resources']=[dict(kind='rock',x=4,y=4)]; case('shared-resource-recovery',w,inputs=[(1,0)]*3)
    w=base(2); w['shared_map']['terrain']=['vv','vv']; case('no-open-candidate',w,inputs=[(1,0),(0,0)])
    w=base(); w.pop('shared_map');
    for t in w['towns'].values(): t.update(map_size=12,terrain=['g'*12]*12,map_resources=[],npcs=[])
    w['towns']['b']['buildings']=[dict(kind='hall',x=4,y=4)]; case('independent-other-town-ignored',w)
    w=base(); w['towns']['a'].pop('player'); case('default-founder',w)
    w=base(); w['shared_map']['terrain'][4]='ggggvggggggg'; case('ordinary-void-recovery',w)
    w=base(); w['shared_map'].pop('terrain'); case('ordinary-absent-terrain',w,inputs=[(1,0)]*2)
    def chunkworld(position=(39.8,20.)):
        w=base(80); w['towns']['a']['player'].update(x=position[0],y=position[1]); w['shared_map'].update(chunked=1,chunk_size=40);w['shared_map'].pop('terrain');return w
    def record(key, biome='g'):
        return (key,dict(terrain=[biome*40]*40,map_resources=[],tree_stumps=[]))
    for records in ([record('0,0')], [record('1,0'),record('0,0')], []):
        w=chunkworld(); case('chunk-boundary-'+str(len(records)),w,records=records,inputs=[(1,0)]*5,probes=[(39.32,20),(39.321,20),(40,20)])
    # y-major vs x-major tie, and stable supplied record insertion order.
    w=chunkworld((20.,20.)); w['shared_map']['map_resources']=[dict(kind='rock',x=20,y=20)]; case('chunk-recovery-y-major',w,records=[record('0,0')])
    for order in (['1,0','0,0'],['0,0','1,0']):
        w=chunkworld((39.5,20.)); w['shared_map']['map_resources']=[dict(kind='rock',x=39,y=20),dict(kind='rock',x=40,y=20)]; case('chunk-record-order-'+order[0],w,records=[record(k) for k in order])
    w=chunkworld((38.8,20.)); w['shared_map']['map_resources']=[dict(kind='rock',x=42,y=20)]; w['towns']['a']['player'].update(mounted=True); case('chunk-window-refresh',w,records=[record('0,0'),record('1,0')],inputs=[(1,0)]*5)
    w=chunkworld((20.,20.)); w['shared_map']['map_resources']=[dict(kind='water',x=20,y=20)];w['towns']['a']['player']['boat']=True;case('chunk-water-boat',w,records=[record('0,0')],inputs=[(1,1)]*3)
    from navigation import RouteCache
    search=[]
    for cell in ('absent',None,'g','v'):
        for bounds in (None,(0,3,0,3),(2,3,0,3)):
            terrain=None if cell=='absent' else [['g']*4 for _ in range(4)]
            if terrain is not None: terrain[1][1]=cell
            blocked={(x,y) for x in range(4) for y in range(4)}-{(0,1),(1,1)}
            iterator=RouteCache.search_steps((0,1),dict(x=1,y=1),blocked,4,terrain,False,bounds)
            while True:
                try: next(iterator)
                except StopIteration as result:
                    opened=result.value is not None
                    break
            search.append(dict(cell=cell,bounds=bounds,open=opened))
    return dict(version=1,tolerance=1e-12,cases=cases,search=search)

if __name__ == '__main__':
    target=Path(__file__).with_name('world_python.json')
    actual=json.dumps(generate(),separators=(',',':'))+'\n'
    if '--check' in sys.argv:
        if target.read_text()!=actual: raise SystemExit('Frozen world founder vectors differ.')
        print('World founder frozen Python vectors recomputed and match.')
    else: target.write_text(actual)
