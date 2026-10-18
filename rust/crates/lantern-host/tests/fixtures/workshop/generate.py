"""Real Python advanced recipes and workshop order lifecycle on synthetic towns."""
import argparse
import copy
import json
from pathlib import Path
import sys
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[6]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT / 'tests')]
import progression as p
import logistics as l
import server
from fixtures import flat_town


def scenarios():
    result = []
    for kind in ('bakery', 'butcher', 'workshop'):
        for spec in p.PROFESSION_RECIPES[kind]:
            if kind != 'workshop' and spec['level'] == 1:
                continue
            for skill, talent in [(0, 0), (40, 1)]:
                result.append(dict(kind=kind, recipe=spec['output'], skill=skill, talent=talent))
    result += [dict(kind='workshop', recipe='lanterns', skill=40, talent=1,
                    orders=[dict(recipe='lanterns', amount=5), dict(recipe='furniture', amount=2)],
                    actions=[dict(op='advance', seconds=9), dict(op='advance', seconds=3), dict(op='cancel', id=2), dict(op='cancel', id=1), dict(op='dropoff'), dict(op='advance', seconds=100)]),
               dict(kind='workshop', recipe='furniture', skill=40, talent=1,
                    orders=[dict(recipe='furniture', amount=5), dict(recipe='lanterns', amount=2)],
                    inputs={'planks':20,'wool':5,'wood':2},
                    actions=[dict(op='advance', seconds=12),dict(op='dropoff'),dict(op='advance', seconds=12),dict(op='dropoff'),dict(op='advance',seconds=3)]),
               dict(kind='workshop', recipe='lanterns', orders=[dict(recipe='lanterns',amount=2)], cap=0,
                    actions=[dict(op='advance',seconds=100),dict(op='uncap'),dict(op='advance',seconds=3),dict(op='cancel',id=1),dict(op='dropoff')])]
    return result


def cases():
    for index, case in enumerate(scenarios()):
        case['name'] = f"{index}_{case['kind']}_{case['recipe']}"
        kind = case['kind']
        building = dict(kind=kind, x=0, y=0, recipe=case['recipe'])
        job = p.BUILDINGS[kind]['job']
        worker = dict(job=job, sandbox=True, talents={p.TALENT_ROOTS.get(job,job):case.get('talent',0)})
        worker[worker['job']] = case.get('skill',0)
        town = flat_town('Advanced recipes')
        town.update(sandbox=True, level=10, wood=0, food=0, coins=0, goods={}, harvested=0, inventory_version=1,
                    food_balance_version=2, market_storage_version=1)
        hall = dict(kind='hall', x=20, y=20)
        town['buildings'] = [building,hall]
        town['villagers'] = [worker]
        worker['building_index'] = 0
        if kind == 'workshop':
            case.setdefault('orders',[dict(recipe=case['recipe'],amount=2)])
            for serial, order in enumerate(case['orders'],1):
                with patch.object(server.secrets, 'token_hex', return_value=f'{serial:016x}'):
                    server.command(town,dict(action='queue_workshop',index=0,**order))
        speed = p.processing_efficiency(worker,building)
        case.setdefault('inputs',{k:n*2 for k,n in p.production_costs(building,speed,{}).items()})
        building['input_buffer'] = copy.deepcopy(case['inputs'])
        if 'cap' in case:
            l.configure_rules(town,dict(resource=case['recipe'],cap=case['cap']))
        case.setdefault('actions',[dict(op='advance',seconds=1),dict(op='advance',seconds=p.production_seconds(building)-1),dict(op='dropoff'),dict(op='advance',seconds=p.production_seconds(building)),dict(op='dropoff'),dict(op='advance',seconds=100)])
        case['states'] = []
        for action in case['actions']:
            status = None
            if action['op'] == 'advance':
                status = l.produce(town,building,worker,action['seconds'])
            elif action['op'] == 'cancel':
                server.command(town,dict(action='cancel_workshop',index=0,request_id=f"{action['id']:016x}"))
            elif action['op'] == 'uncap':
                l.configure_rules(town,dict(resource=case['recipe'],cap=None))
            else:
                l.drop_off(town,building,worker)
            flows = dict(incoming={},outgoing={})
            for bucket in town.get('resource_activity',{}).get('buckets',[]):
                for direction,target in [('in','incoming'),('out','outgoing')]:
                    for k,n in bucket[direction].items():
                        flows[target][k] = flows[target].get(k,0)+n
            queue = [dict(id=int(o['id'],16),recipe=o['recipe'],amount=o['amount'],remaining=o['remaining']) for o in building.get('craft_queue',[])]
            case['states'].append(copy.deepcopy(dict(status=status,inputs=building.get('input_buffer',{}),
                output=building.get('output_buffer',{}),pending=worker.get('pending_output',{}),
                produced=town.get('produced',{}),elapsed=worker.get('production_work',{}).get('elapsed',0),
                practice=worker[worker['job']],queue=queue,flows=flows)))
        yield case


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args = parser.parse_args()
    path = Path(__file__).with_name('python.json')
    text = json.dumps(list(cases()),indent=2)+'\n'
    if args.check:
        if path.read_text() != text:
            raise SystemExit('advanced/workshop fixture drift')
        print('advanced/workshop fixture matches Python')
    else:
        path.write_text(text)
