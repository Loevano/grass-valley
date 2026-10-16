"""Full actual command batch including attach/activate and per-command windows."""
from collections import OrderedDict
from copy import deepcopy
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[6] / 'server'))
from chunk_world import ChunkMap, ChunkTerrain
from server import command
from shared_world import bind_map


def generate():
    cases=[]
    for position,inputs in [((39.9,20.),[(1,0)]*5),((39.9,20.),[(1,0),(-1,0),(1,0)]),((20.,20.),[(0,0)])]:
        world=dict(towns={'synthetic-town':dict(buildings=[],roads=[],villagers=[],town_origin=dict(x=0,y=0),player=dict(x=position[0],y=position[1],facing=-1,extension={'private':'preserve'}))},shared_map=dict(chunked=1,chunk_size=40,map_size=480,terrain_version=1,terrain_seed='00042',starter_offset=160,chunk_clock=0,npcs=[],villages=[]))
        saved={f'{x},{y}':dict(terrain=['g'*40]*40,map_resources=[],tree_stumps=[],village_version=3,industry_version=1,mineral_version=1) for x in range(4) for y in range(4)}
        saved['2,0']['clock']=.5
        saved['1,0']['map_resources']=[dict(kind='rock',x=42,y=20)]
        saved['3,3']['map_resources']=[dict(kind='rock',x=120,y=120)]
        active={k:deepcopy(saved[k]) for k in ['0,0','0,1','1,0','1,1']}
        camera={'3,3':deepcopy(saved['3,3'])}
        runtime=deepcopy(world);m=ChunkMap(runtime);m.memory={k:json.dumps(v,separators=(',',':')) for k,v in saved.items()};m.records=deepcopy(active);m.views=OrderedDict(deepcopy(camera));m.terrain=ChunkTerrain(m.records,m.size);runtime['_chunks']=m
        runtime['shared_map'].update(terrain=m.terrain,map_resources=[r for k in sorted(m.records) for r in m.records[k]['map_resources']],tree_stumps=[])
        bind_map(runtime)
        # Real command recursively attaches before every ordered batch element.
        expected=command(runtime['towns']['synthetic-town'],dict(action='walk',steps=[dict(dx=x,dy=y) for x,y in inputs]),runtime)['player']
        cases.append(dict(world=world,saved=saved,active=active,camera=camera,inputs=inputs,expected=expected,final_active=sorted(m.records)))
    return dict(version=1,cases=cases)

if __name__=='__main__':
    target=Path(__file__).with_name('lifecycle_python.json');actual=json.dumps(generate(),separators=(',',':'))+'\n'
    if '--check' in sys.argv:
        assert target.read_text()==actual,'Frozen founder lifecycle vectors differ.'
        print('Full command lifecycle Python vectors recomputed and match.')
    else:target.write_text(actual)
