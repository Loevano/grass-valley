#!/usr/bin/env python3
"""Real Python saved-first/ensure/overlay/water oracle, explicitly BEFORE catch-up.
--check independently recomputes and never refreshes frozen data.
"""
import argparse
from collections import OrderedDict
from copy import deepcopy
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
from chunk_world import ChunkMap, generate_chunk
from villages import ensure_villages, apply_villages


def world(version=7, size=480, seed=42, offset=160, villages=None):
    shared = dict(chunked=1, chunk_size=40, terrain_seed=seed, terrain_version=version,
                  map_size=size, chunk_clock=99999, npcs=[])
    if offset is not None: shared['starter_offset'] = offset
    if villages is not None: shared['villages'] = deepcopy(villages)
    return dict(towns={'synthetic-town': dict(buildings=[], villagers=[], local_visits={}, wood=.125)},
                shared_map=shared, private_extension={'credential': 'SYNTHETIC_SECRET', 'quantity': .0625})


def build():
    cases = []
    def add(label, w, keys, saved=None, active=None, camera=None):
        saved = saved or {}
        active = active or {}
        camera = camera or {}
        prepared = deepcopy(w)
        ensure_villages(prepared)
        runtime = deepcopy(w)
        manager = ChunkMap(runtime)
        manager.records = deepcopy(active)
        manager.views = OrderedDict(deepcopy(camera))
        # Observe the real ensure path's single-village cache calls, not a fake
        # overlay implementation. Its aggregate rebinding is lifecycle-only.
        if active or camera: runtime['_chunks'] = manager
        ensure_villages(runtime)
        prepared_active, prepared_camera = deepcopy(manager.records), deepcopy(manager.views)
        original = manager._read
        manager._read = lambda key: deepcopy(saved[key]) if key in saved else original(key)
        # Deliberate phase boundary: no ensure_industry, supply/stump catch-up or
        # elapsed simulation. Everything else uses actual ChunkMap.read code.
        manager.catch_up = lambda *args: None
        outputs = {key: deepcopy(manager.read(key)) for key in keys}
        cases.append(dict(label=label, world=w, saved=saved, active=active, camera=camera,
                          prepared_world=prepared, prepared_active=prepared_active,
                          prepared_camera=prepared_camera, outputs=outputs))

    for version in range(1, 8):
        for size, offset, seed in [(480,160,'00042'), (1920,None,-(2**130+17))]:
            start = (offset if offset is not None else 880)//40
            add(f'fresh-{version}-{size}', world(version,size,seed,offset), [f'{start},{start}', '0,0'])
    legacy_default = world(1, offset=None, villages=[])
    legacy_default['shared_map'].pop('terrain_version')
    add('absent-version-and-offset', legacy_default, ['4,4'])
    reed = dict(name='Reedhaven',biome='b',x=80,y=200,layout_version=2,life_version=1,swamp_version=1)
    for version in range(1,8):
        w=world(version,villages=[reed]); record=generate_chunk(42,'2,5',version=version)
        apply_villages(record,'2,5',[reed],40)
        record['village_version']=(version-1)%4
        record['swamp_version']=(version-1)%3
        record['clock']=.125
        record['map_resources'].append(dict(kind='berry_bush',x=80,y=200,supply=.125,supply_max=12.5,regeneration=.03125,extension={'private':'SYNTHETIC_SECRET'}))
        record['tree_stumps'].append(dict(x=80,y=200,remaining=9.125))
        record['terrain'][0]='p'+record['terrain'][0][1:]
        record['extension']={'quantity':.125,'huge':123456789012345678901234567890}
        add(f'saved-upgrade-{version}',w,['2,5','1,5','2,4'],{'2,5':record})

    def edited(w, keys, points):
        saved={}
        for key in keys:
            record=generate_chunk(w['shared_map']['terrain_seed'],key,version=w['shared_map']['terrain_version'],size=w['shared_map']['map_size'])
            record['map_resources']=[r for r in record['map_resources'] if r['kind']!='water']
            cx,cy=map(int,key.split(','))
            record['map_resources'] += [dict(kind='water',x=x,y=y,private_extension=.125) for x,y in points if x//40==cx and y//40==cy]
            record.update(village_version=3,swamp_version=2,extension='SYNTHETIC_SECRET')
            record.pop('clock')
            saved[key]=record
        return saved
    around=[f'{x},{y}' for y in range(1,4) for x in range(1,4)]
    for label,points in [
        ('singleton',[(119,119)]), ('pair',[(119,119),(120,119)]),
        ('corner-three',[(119,119),(120,119),(120,120)]),
        ('horizontal-three',[(118,100),(119,100),(120,100)]),
        ('local-three',[(90,90),(90,91),(91,91)]),
    ]:
        w=world(villages=[])
        add(f'seam-{label}',w,['2,2','3,2','3,3'],edited(w,around,points))
    w=world(6,villages=[])
    add('v6-no-repair',w,['2,2'],edited(w,around,[(119,119)]))
    w=world(villages=[])
    add('outside-world-pair',w,['0,0'],edited(w,['0,0','0,1','1,0'],[(0,0),(0,1)]))
    w=world(size=1920,offset=None,villages=[])
    add('legacy-outer-boundary',w,['47,47'],edited(w,['47,47','46,47','47,46'],[(1919,1919),(1918,1919)]))
    sun=dict(name='Sunwell',biome='d',x=112,y=126,layout_version=2,life_version=1)
    w=world(villages=[sun]); saved=edited(w,around,[(120,119),(120,120)])
    saved['2,2']['village_version']=2
    add('life-water-connects-over-corner',w,['2,2','3,2'],saved)
    sun.update(x=128,y=100)
    w=world(villages=[sun]); saved=edited(w,around,[(119,100),(120,100),(121,100)])
    saved['3,2']['village_version']=0
    add('neighbor-overlay-removes-connection',w,['2,2','3,2'],saved)

    # Actual ordered metadata effects on both resident classes. Sequential calls
    # differ from one combined call when overlapping villages share a marker.
    for reverse in (False,True):
        villages=[deepcopy(reed),dict(reed,x=81,y=201,swamp_version=0)]
        if reverse:villages.reverse()
        w=world(villages=villages)
        active=generate_chunk(42,'2,5',version=7)
        active.update(village_version=1,swamp_version=0,clock=.125,extension='resident-active')
        camera=generate_chunk(42,'1,5',version=7)
        camera.update(village_version=1,swamp_version=1,clock=.0625,extension='resident-camera')
        saved={'2,5':generate_chunk(42,'2,5',version=7)}
        add(f'ordered-resident-upgrades-{reverse}',w,['2,5','1,5','2,4'],saved,{'2,5':active},{'1,5':camera})
    w=world(villages=[]); saved=edited(w,around,[(119,100)])
    resident=deepcopy(saved['3,2']);resident['map_resources'] += [dict(kind='water',x=120,y=100),dict(kind='water',x=121,y=100)]
    add('resident-neighbor-precedence',w,['2,2','3,2'],saved,camera={'3,2':resident})
    return cases


def main():
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    cases=build();text=json.dumps(cases,ensure_ascii=False,separators=(',',':'))+'\n'
    target=Path(__file__).with_name('materialized.json')
    if args.check:
        if target.read_text()!=text:raise SystemExit('Actual Python post-overlay oracle differs; review before refreshing')
    else:target.write_text(text)
    print(f'{len(cases)} actual Python saved-first post-overlay worlds match before catch-up')
if __name__=='__main__':main()
