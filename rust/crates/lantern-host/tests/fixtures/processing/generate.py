"""Freeze real Python base-processor timers, rates and interruption accounting."""
import argparse
import copy
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
import progression as p
import logistics as l


def scripts():
    cases = []
    for kind in p.RECIPES:
        if kind == 'workshop':
            continue
        for skill, talent in [(0, 0), (20, 1)]:
            cases.append(dict(kind=kind, skill=skill, talent=talent))
    for kind, equipment in [('mill', key) for key in ('improved_millstones', 'geared_millstones', 'masterwork_millstones')]+[('bakery', key) for key in ('larger_oven', 'double_hearth_oven', 'masterwork_oven')]+[('mine', 'steel_pickaxe')]:
        cases.append(dict(kind=kind, equipment=[equipment]))
    cases.append(dict(kind='sawpit', tool='crafted_saw'))
    for fish in p.FISH:
        cases.append(dict(kind='smokehouse', inputs={fish: 3, 'wood': 3}))
    cases.append(dict(kind='smokehouse', inputs={'fish': .2, 'trout': .3, 'salmon': .4, 'sturgeon': 3, 'wood': 3}))
    cases += [dict(kind='mill', inputs={'wheat': 2}, actions=[dict(op='advance', seconds=1), dict(op='cap', key='flour', amount=1), dict(op='advance', seconds=100), dict(op='cap', key='flour', amount=None), dict(op='advance', seconds=2), dict(op='advance', seconds=100), dict(op='supply', goods={'wheat': 2}), dict(op='advance', seconds=3), dict(op='dropoff')]),
              dict(kind='bakery', inputs={'flour': 2, 'wood': .5-5e-10}),
              dict(kind='mine', ore=[dict(x=6, y=0, supply=1), dict(x=8, y=0, supply=60)]),
              dict(kind='mine', ore=[dict(x=6, y=0, supply=1)]),
              dict(kind='mine', ore=[]),
              dict(kind='mine', ore=[dict(x=20, y=0, supply=60)]),
              dict(kind='sawpit', output={'planks': 49.5}),
              dict(kind='tannery', actions=[dict(op='advance', seconds=9),dict(op='cancel'),dict(op='cancel')])]
    for index, case in enumerate(cases):
        case['name'] = f"{index}_{case['kind']}"
        building = dict(kind=case['kind'], x=0, y=0,
                        profession_upgrades=case.get('equipment', []))
        job = p.BUILDINGS[case['kind']]['job']
        worker = dict(job=job, sandbox=True, talents={p.TALENT_ROOTS.get(job, job): case.get('talent', 0)},
                      worker_tool=case.get('tool'))
        worker[job] = case.get('skill', 0)
        speed = p.processing_efficiency(worker, building)
        needs = p.production_costs(building, speed, {})
        case.setdefault('inputs', {key: n*4 for key, n in needs.items()})
        case.setdefault('ore', [dict(x=6, y=0, supply=60)])
        case.setdefault('output', {})
        case.setdefault('actions', [dict(op='advance', seconds=1), dict(op='advance', seconds=p.production_seconds(building)-1), dict(op='dropoff'), dict(op='advance', seconds=100)])
        building['input_buffer'] = copy.deepcopy(case['inputs'])
        building['output_buffer'] = copy.deepcopy(case['output'])
        source = dict(kind='storehouse', x=3, y=0, stored_goods={key: 100 for key in case['inputs']})
        town = dict(buildings=[building, source], villagers=[worker], coins=0, wood=0, food=0, goods={}, harvested=0, inventory_version=1,
                    map_resources=[dict(kind='rock', ore='iron', **v) for v in copy.deepcopy(case['ore'])])
        town.update(food_balance_version=2, market_storage_version=1)
        for b in town['buildings']:
            for key, n in p.inventory(b).items():
                p.add_stock(town, key, n)
        case['states'] = []
        for action in case['actions']:
            if action['op'] == 'advance':
                status = l.produce(town, building, worker, action['seconds'])
            elif action['op'] == 'cap':
                l.configure_rules(town, dict(resource=action['key'], cap=action['amount']))
                status = None
            elif action['op'] == 'supply':
                for key, n in action['goods'].items():
                    p.withdraw(town, key, n, source=source)
                    p.add_stock(town, key, -n)
                    building['input_buffer'][key] = building['input_buffer'].get(key, 0)+n
                status = None
            elif action['op'] == 'cancel':
                l.cancel_output(town, worker)
                status = None
            else:
                l.drop_off(town, building, worker)
                status = None
            flows = dict(incoming={}, outgoing={})
            for bucket in town.get('resource_activity', {}).get('buckets', []):
                for direction, target in [('in', 'incoming'), ('out', 'outgoing')]:
                    for key, n in bucket[direction].items():
                        flows[target][key] = flows[target].get(key, 0)+n
            case['states'].append(copy.deepcopy(dict(status=status, flows=flows,
                inputs=building.get('input_buffer', {}), output=building.get('output_buffer', {}),
                pending=worker.get('pending_output', {}), orphan=building.get('orphaned_output', {}),
                produced=town.get('produced', {}), practice=worker[job],
                elapsed=worker.get('production_work', {}).get('elapsed', 0),
                ore=[r['supply'] for r in town['map_resources']])))
        yield case


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = Path(__file__).with_name('python.json')
    data = json.dumps(list(scripts()), indent=2) + '\n'
    if args.check:
        if output.read_text() != data:
            raise SystemExit('base processor fixture drift')
        print('base processor fixture matches Python')
    else:
        output.write_text(data)
