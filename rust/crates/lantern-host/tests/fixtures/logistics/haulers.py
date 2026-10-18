"""Real Python hauler planning/pickup/deposit on disposable ordered buffers."""
import argparse
import copy
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
import logistics as l
import progression as p


def ordered(stock):
    return list(stock.items())


def cases():
    result = []
    variants = [
        dict(name='buffers', output={'wood': 40}, stored={'wood': 40}),
        dict(name='resource_order', output={'planks': 35, 'wood': 25}),
        dict(name='mixed_food', output={'bread': 2, 'eggs': 4, 'wood': 40}, destination={'eggs': 47}),
        dict(name='orphan', output={'wood': 5}, stored={'wood': 7}, orphan={'wood': 9}, owned={'wood': 70}),
        dict(name='tier20', output={'wood': 40}, stored={'wood': 40}, hauling=20),
        dict(name='below20', output={'wood': 40}, stored={'wood': 40}, hauling=19.999),
        dict(name='talent', output={'wood': 80}, talent=-1),
        dict(name='disabled', output={'wood': 20}, enabled={}),
    ]
    variants += [dict(name=f'supply_{kind}', operation='supply', kind=kind, talent=-1)
                 for kind in ('sawpit', 'mill', 'bakery', 'workshop', 'smokehouse')]
    for spec in variants:
        operation = spec.get('operation', 'collect')
        kind = spec.get('kind', 'woodcutter')
        building = dict(kind=kind, x=8, y=0, output_buffer=copy.deepcopy(spec.get('output', {})),
                        stored_goods=copy.deepcopy(spec.get('stored', {})),
                        orphaned_output=copy.deepcopy(spec.get('orphan', {})),
                        input_buffer={})
        if kind == 'workshop':
            building['craft_queue'] = [dict(id=1, recipe='tools', remaining=100)]
        owned = spec.get('owned', {})
        building['pending_output'] = {key: spec.get('orphan', {}).get(key, 0) + owned.get(key, 0)
                                     for key in set(spec.get('orphan', {})) | set(owned)}
        storage = dict(kind='storehouse', x=2, y=0, level=3,
                       storage_enabled=spec.get('enabled', {'wood': True, 'planks': True, 'food': True}),
                       stored_goods=copy.deepcopy(spec.get('destination', {})))
        producer = dict(job=p.public_catalog()['buildings'].get(kind, {}).get('job', 'woodcutting'),
                        building_index=2, talents={}, sandbox=True)
        if operation == 'supply':
            storage['stored_goods'] = {key: 100 for key in ('wood', 'wheat', 'flour', 'planks', *p.FISH)}
        worker = dict(job='hauling', building_index=1, talents={'hauling': spec.get('talent', 0)},
                      hauling=spec.get('hauling', 0))
        owner = dict(job='woodcutting', building_index=2, pending_output=copy.deepcopy(owned),
                     pending_workplace={key: building[key] for key in ('kind', 'x', 'y')})
        town = dict(buildings=[dict(kind='hall', x=0, y=0, level=1), storage, building],
                    villagers=[worker, producer, owner], coins=0, wood=0, food=0, goods={}, harvested=0)
        p.ensure_inventory(town)
        initial = dict(output=ordered(building.get('output_buffer', {})), stored=ordered(building.get('stored_goods', {})),
                       orphan=ordered(building.get('orphaned_output', {})), owned=ordered(owned),
                       destination=ordered(storage.get('stored_goods', {})))
        planned = l.planned_cargo(town, building, storage, operation, worker)
        picked = l.pickup_cargo(town, building, storage, operation, worker)
        # Fill/disable may change after pickup in real trips. Hold all goods when
        # disabled, then accept them once filters are restored (Rust lifecycle tests).
        moved, held = l.deposit_cargo(town, building, storage, operation, picked)
        result.append(dict(name=spec['name'], kind=kind, operation=operation, initial=initial,
                           enabled=storage['storage_enabled'], hauling=worker['hauling'],
                           talent=spec.get('talent', 0), planned=ordered(planned), picked=ordered(picked),
                           moved=ordered(moved), held=ordered(held)))
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    path = Path(__file__).with_name('haulers.json')
    text = json.dumps(cases(), indent=2, sort_keys=True) + '\n'
    if args.check:
        assert path.read_text() == text, 'hauler fixture drift'
    else:
        path.write_text(text)
