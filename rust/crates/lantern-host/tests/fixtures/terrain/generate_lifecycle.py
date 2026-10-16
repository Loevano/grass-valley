#!/usr/bin/env python3
"""Actual Python private lifecycle traces. --check recomputes, never refreshes."""
import argparse
from collections import OrderedDict
from copy import deepcopy
import json
from pathlib import Path
import sys
ROOT=Path(__file__).resolve().parents[6]
sys.path.insert(0,str(ROOT/'server'))
from chunk_world import ChunkMap, generate_chunk
from villages import ensure_villages
from spatial import occupied_tiles


def build():
    cases=[]
    def add(label,w,actions,saved=None,active=None,camera=None):
        saved=saved or {};active=active or {};camera=camera or {}
        runtime=deepcopy(w);m=ChunkMap(runtime)
        m.memory={k:json.dumps(v,separators=(',',':')) for k,v in saved.items()}
        m.records=deepcopy(active);m.views=OrderedDict(deepcopy(camera))
        # Establish a live manager's active resource bindings before ensure.
        runtime['shared_map'].update(map_resources=[r for k in sorted(m.records) for r in m.records[k]['map_resources']],
                                     tree_stumps=[r for k in sorted(m.records) for r in m.records[k]['tree_stumps']])
        runtime['_chunks']=m
        ensure_villages(runtime)
        def world_copy():
            value={k:deepcopy(v) for k,v in runtime.items() if k!='_chunks' and k!='shared_map'}
            value['shared_map']={k:deepcopy(v) for k,v in runtime['shared_map'].items() if k!='terrain'}
            return value
        def state():
            obstacles=[b for town in runtime['towns'].values() for b in town['buildings']+town.get('roads',[])]
            obstacles+=runtime['shared_map'].get('npcs',[])
            obstacles += [dict(x=round(t['player']['x']),y=round(t['player']['y'])) for t in runtime['towns'].values() if 'player' in t]
            occupied=[list(p) for p in sorted(occupied_tiles(obstacles)) if 0<=p[0]<m.size and 0<=p[1]<m.size]
            return dict(occupied=occupied,world=world_copy(),active=sorted(m.records),camera=list(m.views),
                        desired=sorted(m.desired()),records=deepcopy(m.records))
        initial=state();outputs=[]
        for action in actions:
            kind=action['op'];record=None
            if kind=='read':record=deepcopy(m.read(action['key']))
            elif kind=='activate':m.activate()
            elif kind=='clock':runtime['shared_map']['chunk_clock']=action['clock']
            elif kind=='move':runtime['towns']['synthetic-town']['player'].update(x=action['x'],y=action['y'])
            elif kind=='edit':
                r=m.records[action['key']]
                # Exercise resource replacement, quantity edits, stumps, ground,
                # exact private extension retention, membership and aggregate rebind.
                r['map_resources']=[deepcopy(action['resource'])]
                r['tree_stumps']=[deepcopy(action['stump'])]
                rows=r['terrain'];idx=action['resource']['y']%40
                x=action['resource']['x']%40
                rows[idx]=rows[idx][:x]+'f'+rows[idx][x+1:]
                r['private_edit']={'quantity':123456789012345678901234567890,'opaque':'.0625'}
                runtime['shared_map']['map_resources']=[r for k in sorted(m.records) for r in m.records[k]['map_resources']]
                runtime['shared_map']['tree_stumps']=[r for k in sorted(m.records) for r in m.records[k]['tree_stumps']]
                m.membership=None;m.resource_index=None
            elif kind=='flush':m.flush()
            elif kind=='snapshot':
                # Real snapshot side effects, retaining private record copies for
                # comparison; wire versions/filtering are later protocol work.
                m.snapshot(action['keys']);record={k:deepcopy(m.read(k)) for k in sorted(set(m.records)|set(action['keys']))}
            else:raise AssertionError(kind)
            # Every step checks aggregates and source ownership; active full
            # records need only be frozen when activation/edit can change them.
            output=state()
            if kind not in ('activate','edit'):output.pop('records')
            if record is not None:output['read']=record
            output['memory']={k:json.loads(v) for k,v in m.memory.items() if k not in saved or json.loads(v)!=saved[k]}
            outputs.append(output)
        cases.append(dict(label=label,world=w,saved=saved,active=active,camera=camera,active_order=list(active),initial=initial,actions=actions,outputs=outputs))
    def world(version=7,size=480,seed='00042',offset=160):
        shared=dict(chunked=1,chunk_size=40,terrain_seed=seed,terrain_version=version,map_size=size,
                    chunk_clock=0,npcs=[],villages=[])
        if offset is not None:shared['starter_offset']=offset
        return dict(towns={'synthetic-town':dict(buildings=[dict(kind='hall',x=2,y=3,footprint=[2,3])],
            roads=[dict(x=4,y=4)],villagers=[],local_visits={},wood=.125,
            town_origin=dict(x=0,y=0),player=dict(x=1.5,y=2.5))},shared_map=shared,
            private_extension={'quantity':123456789012345678901234567890,'private':'SYNTHETIC_SECRET'})
    for version in range(1,8):
        for size,offset,seed in [(480,160,'00042'),(1920,None,-(2**130+17))]:
            w=world(version,size,seed,offset);far=size//40-1;key=f'{far},{far}';p=size-1
            actions=[dict(op='read',key=key),dict(op='activate'),dict(op='clock',clock=6.125),
                dict(op='activate'),dict(op='read',key='0,0'), # active bypasses catchup
                dict(op='move',x=float(p),y=float(p)),dict(op='activate'),
                dict(op='edit',key=key,resource=dict(kind='berry_bush',x=p,y=p,supply=.125,supply_max=6.0,regeneration=.3,extension='.125'),
                     stump=dict(kind='tree',x=p,y=p,remaining=12.125,extension='.0625')),
                dict(op='clock',clock=9.125),dict(op='flush'),dict(op='flush'),
                dict(op='move',x=1.5,y=2.5),dict(op='activate'),dict(op='clock',clock=30.125),
                dict(op='read',key=key),dict(op='read',key=key),dict(op='snapshot',keys=[key]),dict(op='snapshot',keys=[])]
            add(f'lifecycle-{version}-{size}',w,actions)
    # Ensure actual deferred swamp effects consume every ordinal and rebind active
    # resources before any activation. Mixed overlap/order is separately frozen.
    for reverse in (False,True):
        w=world();villages=[dict(name='Reedhaven',biome='b',x=80,y=200,layout_version=2,life_version=1,swamp_version=1),
            dict(name='Reedhaven',biome='b',x=90,y=200,layout_version=2,life_version=1,swamp_version=1)]
        w['shared_map']['villages']=villages[::-1] if reverse else villages
        active={'2,5':generate_chunk('00042','2,5',7),'10,0':generate_chunk('00042','10,0',7)};camera={'1,5':generate_chunk('00042','1,5',7)}
        add(f'ordered-swamp-{reverse}',w,[dict(op='read',key='2,5'),dict(op='read',key='1,5'),dict(op='activate')],active=active,camera=camera)
    w=world()  # R06 archives omit runtime aggregates; bootstrap from resident ownership.
    add('active-no-plan-bootstrap',w,[dict(op='read',key='2,5'),dict(op='activate')],active={'2,5':generate_chunk('00042','2,5',7)})
    # Empty towns use sites; edges clamp correctly and lexical key sorting differs
    # from numeric coordinate sorting. Actual generation remains in every trace.
    w=world();w['towns']={};w['town_sites']=[dict(x=-.5,y=-.5),dict(x=479.5,y=479.5)]
    add('sites-boundaries',w,[dict(op='activate'),dict(op='snapshot',keys=['10,0','2,0'])])
    # Supplied camera with absent clock must catch up at zero (None != 0).
    w=world();r=generate_chunk('00042','2,5',7);r.pop('clock')
    add('camera-no-clock',w,[dict(op='read',key='2,5')],camera={'2,5':r})
    # Deduplicate repeated private record/aggregate payloads; source worlds and
    # resident inputs stay literal. References are fixture encoding, never game JSON.
    payloads=[];index={}
    def pack(value):
        if not isinstance(value,(dict,list)):return value
        encoded=json.dumps(value,separators=(',',':'))
        if len(encoded)<=512:return value
        if encoded in index:return {'fixture_ref':index[encoded]}
        packed={k:pack(v) for k,v in value.items()} if isinstance(value,dict) else [pack(v) for v in value]
        ordinal=len(payloads);index[encoded]=ordinal;payloads.append(packed)
        return {'fixture_ref':ordinal}
    for case in cases:
        case['initial']=pack(case['initial'])
        case['outputs']=[pack(output) for output in case['outputs']]
    return dict(payloads=payloads,cases=cases)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    target=Path(__file__).with_name('lifecycle.json');result=build()
    if args.check:
        assert json.loads(target.read_text())==result,'Python lifecycle differs from frozen oracle'
        print(f'Checked {len(result["cases"])} actual Python lifecycle traces.')
    else:
        target.write_text(json.dumps(result,separators=(',',':'))+'\n');print(f'Wrote {len(result["cases"])} actual Python lifecycle traces.')
if __name__=='__main__':main()
