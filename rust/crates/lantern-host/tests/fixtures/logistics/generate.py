"""Freeze real Python producer supply plans on synthetic inventories."""
import argparse
import copy
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
import progression as p
import logistics as l


def cases():
    result = []
    for kind in p.RECIPES:
        for stocked in (False, True):
            building = dict(kind=kind, x=20, y=0, input_buffer={'wood': .5} if stocked else {})
            if kind == 'workshop':
                building['craft_queue'] = [dict(id=1, recipe='tools', output='tools', remaining=3)]
            worker = dict(job=p.public_catalog()['buildings'][kind]['job'], sandbox=True, building_index=2)
            source = dict(kind='storehouse', x=10, y=0, stored_goods={k: 80 for k in p.STORAGE_KEYS if k != 'food'})
            hall = dict(kind='hall', x=0, y=0, level=1)
            town = dict(buildings=[hall, source, building], villagers=[worker], wood=0, coins=0, food=0, goods={}, harvested=0)
            p.ensure_inventory(town)
            request = l.supply_request(town, building, worker)
            cargo = l.planned_cargo(town, building, source, 'supply', worker)
            result.append(dict(building=copy.deepcopy(building), worker=worker, source=source, request=request, cargo=cargo))
    return result

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    order = {}
    for kind, recipe in p.RECIPES.items():
        order[kind] = {recipe[1]: list(recipe[0])}
        for spec in p.PROFESSION_RECIPES.get(kind, []):
            order[kind][spec['output']] = list(spec['inputs'])
    order_path = ROOT / 'rust/crates/lantern-host/src/logistics/input_order.json'
    order_text = json.dumps(order, indent=2, sort_keys=True) + '\n'
    if args.check:
        assert order_path.read_text() == order_text, 'ingredient order drift'
    else:
        order_path.write_text(order_text)
    path = Path(__file__).with_name('python.json')
    text = json.dumps(cases(), indent=2, sort_keys=True) + '\n'
    if args.check:
        assert path.read_text() == text, 'producer fixture drift'
    else:
        path.write_text(text)
