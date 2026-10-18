"""Synthetic paired R16 slice-1 fields, selection and authoritative harvests."""
import argparse
import copy
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT / 'tests')]
import farming as f
import progression as p
import server as s
import work_cycles as wc
from fixtures import flat_town


def cases():
    yields = []
    for crop in ['vegetables', 'wheat', 'carrots', 'fodder']:
        for level, tool, skill, talent, manure, cap in [
            (1, None, 0, 0, 0, None), (3, 'fine_sickle', 20, 1, .2, None),
            (2, 'steel_sickle', 40, -1, .1, 2),
            (3, 'masterwork_sickle', 0, 0, .1, 0),
        ]:
            town = flat_town('slice1')
            farm = dict(kind='farm', x=3, y=3, level=level)
            acre = dict(kind='acre', x=3, y=4, farm_index=1, crop=crop,
                        field=dict(stage='ripe', elapsed=0, progress=2, cycle=0, crop=crop))
            town.update(buildings=[dict(kind='hall', x=0, y=0), farm, acre],
                        inventory_version=1, wood=0, food=0, goods={'manure': manure},
                        rules={'production_caps': {} if cap is None else {crop: cap}})
            worker = dict(job='farming', farming=skill, talents={'farming': talent},
                          worker_tool=tool, work_animation={})
            success = s.harvest_crop(town, worker, farm, acre)
            yields.append(dict(crop=crop, level=level, tool=tool, skill=skill,
                               talent=talent, manure=manure, cap=cap, success=success,
                               state=copy.deepcopy(acre['field']),
                               produced=worker.get('pending_output', {}).get(crop, 0),
                               manure_left=p.quantity(town, 'manure')))
    growth = []
    for stage in ['sprouting', 'growing']:
        for parts in [[15], [2, 3, 10], [30], [.25] * 60]:
            acre = dict(kind='acre', x=3, y=4,
                        field=dict(stage=stage, elapsed=0, progress=0, cycle=0, crop='wheat'))
            for seconds in parts:
                f.grow({'buildings': [acre]}, seconds)
            growth.append(dict(stage=stage, parts=parts, state=acre['field']))
    selection = []
    for stages in [['unplowed'] * 4, ['plowed', 'unplowed', 'plowed', 'plowed'],
                   ['sprouting', 'ripe', 'thirsty', 'growing'], ['ripe'] * 4]:
        town = flat_town('selection')
        farm = dict(kind='farm', x=3, y=3)
        acres = [dict(kind='acre', x=x, y=y, farm_index=1,
                      field=dict(stage=stage, elapsed=0, progress=0, cycle=0, crop='wheat'))
                 for (x, y), stage in zip([(3, 4), (4, 4), (3, 5), (4, 5)], stages)]
        town['buildings'] = [dict(kind='hall', x=0, y=0), farm, *acres]
        town['villagers'] = [dict(job='farming', building_index=1,
                                  construction_position=dict(x=3, y=4)) for _ in range(2)]
        reservations = {}
        chosen = []
        for i in range(2):
            target = f.choose_field(town, farm, acres, i, reservations)
            # Python returns a fallback for a waiting worker; compare work-ready
            # targets only, because Rust exposes an explicit None for this case.
            cycle, stage = f.work_pass(town, farm)
            ready = target['field']['stage'] == stage and target['field']['cycle'] == cycle and not f.claimed(town, target, i, reservations)
            if ready:
                reservations[i] = target
            chosen.append(acres.index(target) + 2 if ready else None)
        selection.append(dict(stages=stages, chosen=chosen, work_pass=list(f.work_pass(town, farm))))
    actions = []
    for stage in ['unplowed', 'plowed']:
        for progress, seconds in [(0, .75), (0, 2), (.5, 3)]:
            town = flat_town('actions')
            farm = dict(kind='farm', x=3, y=3)
            acre = dict(kind='acre', x=3, y=4, farm_index=1, crop='wheat',
                        field=dict(stage=stage, elapsed=0, progress=progress, cycle=0))
            town.update(buildings=[dict(kind='hall', x=0, y=0), farm, acre],
                        terrain=['g' * 16] * 16, map_size=16, map_resources=[], npcs=[])
            worker = town['villagers'][0]
            worker.update(job='farming', farming=0, building_index=1, talents={},
                          construction_position=wc.field_work_spot(acre))
            _, remaining = wc.advance_farming(town, town, worker, farm, acre, 0, seconds)
            actions.append(dict(stage=stage, progress=progress, seconds=seconds,
                                state=copy.deepcopy(acre['field']), remaining=remaining,
                                practice=worker.get('farming', 0)))
    return dict(yields=yields, growth=growth, selection=selection, actions=actions)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = Path(__file__).with_name('python.json')
    data = json.dumps(cases(), indent=2) + '\n'
    if args.check:
        if output.read_text() != data:
            raise SystemExit('farming fixture drift')
        print('farming fixture matches Python')
    else:
        output.write_text(data)
