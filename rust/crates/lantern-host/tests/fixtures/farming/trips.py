"""Real Python water-round and multi-acre harvest semantics on synthetic towns.

Routes remain real Python farmer routes. Compare committed field/crop/accounting
facts at resting boundaries; Rust route timing is separately tested with explicitly
admitted paths, so differing route waypoints are not blanket-ignored snapshots.
"""
import argparse
import copy
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT / 'tests')]
from fixtures import flat_town
import farming
import progression as p
import server


def cases():
    result = []
    specs = [
        dict(name='water_round', crops=['wheat', 'wheat'], stages=['thirsty'] * 2),
        dict(name='next_round', crops=['wheat', 'wheat'], stages=['thirsty'] * 2,
             actions=['work', 'grow', 'work', 'grow', 'work']),
        dict(name='mixed_harvest', crops=['wheat', 'vegetables', 'fodder'], stages=['ripe'] * 3),
        dict(name='cap', crops=['wheat', 'wheat'], stages=['ripe'] * 2, cap=15),
        dict(name='buffer', crops=['wheat', 'wheat'], stages=['ripe'] * 2, stored=465),
        dict(name='carried_cross_pass', crops=['wheat', 'vegetables'], stages=['ripe', 'unplowed'],
             cycles=[2, 0], pending=1),
    ]
    for spec in specs:
        town = flat_town('water trips')
        farm = dict(kind='farm', x=3, y=3)
        acres = [dict(kind='acre', x=3 + i % 2, y=4 + i // 2, farm_index=1, crop=crop,
                      field=dict(stage=stage, elapsed=0, progress=0,
                                 cycle=spec.get('cycles', [0] * len(spec['crops']))[i], crop=crop))
                 for i, (crop, stage) in enumerate(zip(spec['crops'], spec['stages']))]
        town.update(buildings=[dict(kind='hall', x=0, y=0), farm, *acres],
                    terrain=['g' * 16] * 16, map_size=16,
                    map_resources=[dict(kind='water', x=8, y=4)], npcs=[],
                    wood=0, food=0, goods={}, inventory_version=1,
                    rules={'production_caps': {'wheat': spec['cap']} if 'cap' in spec else {}})
        worker = town['villagers'][0]
        worker.update(job='farming', farming=0, building_index=1, talents={},
                      construction_position=dict(x=3, y=4))
        if spec.get('stored'):
            farm['output_buffer'] = {'wheat': spec['stored']}
            p.add_stock(town, 'wheat', spec['stored'])
        if spec.get('pending'):
            server.logistics.record_output(town, farm, 'wheat', spec['pending'], worker)
        snapshots = []
        for action in spec.get('actions', ['work']):
            collections = 0
            if action == 'grow':
                farming.grow(town, 15)
            else:
                source = server.work_source(town, 'farming', 0)
                server.advance_farmer(town, town, worker, farm, source, 0, 30)
                previous = None
                for sample in worker.get('work_animation', {}).get('playback', []):
                    phase = sample['phase']
                    if phase == 'collecting_water' and previous != phase:
                        collections += 1
                    previous = phase
            snapshots.append(dict(action=action, collections=collections,
                                  fields=[copy.deepcopy(farming.field_state(a)) for a in acres],
                                  free={k: p.quantity(town, k) for k in spec['crops']},
                                  produced={k: town.get('produced', {}).get(k, 0) for k in spec['crops']},
                                  pending=copy.deepcopy(worker.get('pending_output', {})),
                                  round=worker.get('watering_round')))
        result.append(dict(spec=spec, snapshots=snapshots))
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = Path(__file__).with_name('trips.json')
    data = json.dumps(cases(), indent=2) + '\n'
    if args.check:
        if output.read_text() != data:
            raise SystemExit('farming trips fixture drift')
        print('farming trips fixture matches Python')
    else:
        output.write_text(data)
