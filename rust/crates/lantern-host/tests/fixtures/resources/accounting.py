"""Real Python accounting helpers and browser summaries on disposable facts.

Recipe/rate/qualification evaluation belongs to slice 3; batches are explicit.
"""
import copy
import json
from pathlib import Path
import subprocess
import progression as p
import logistics as l
import server


def snapshot(town):
    return dict(town=copy.deepcopy(town),
                rooms={k: p.production_room(town, k) for k in town.get('rules', {}).get('production_caps', {})},
                storage_room={k: p.storage_room(town, k) for k in set(p.STORAGE_KEYS)},
                storage_limit={k: p.storage_limit(town, k) for k in set(p.STORAGE_KEYS)})


def apply(town, a):
    op = a['op']
    b = town['buildings'][a.get('building', 0)]
    v = town['villagers'][a.get('actor', 0)]
    if op == 'cap':
        l.configure_rules(town, {'resource': a['key'], 'cap': a['cap']})
    elif op == 'advance':
        p.resource_activity.advance(town, a['seconds'])
    elif op == 'spend':
        assert p.afford(town, a['goods'])
        p.spend(town, a['goods'])
    elif op == 'eat':
        p.eat(town, a['amount'])
    elif op == 'reserve':
        source = town['buildings'][a['source']]
        for k, n in a['goods'].items():
            p.withdraw(town, k, n, source)
            p.add_stock(town, k, -n)
            b.setdefault('input_buffer', {})[k] = b.get('input_buffer', {}).get(k, 0) + n
    elif op == 'release':
        for k, n in a['goods'].items():
            b['input_buffer'][k] -= n
            b.setdefault('output_buffer', {})[k] = b.get('output_buffer', {}).get(k, 0) + n
            p.add_stock(town, k, n)
    elif op == 'produce':
        for k, n in a['costs'].items():
            assert b.get('input_buffer', {}).get(k, 0) + 1e-9 >= n
        assert l.output_fits(b, a['goods'])
        for k, n in a['goods'].items():
            assert n <= p.production_room(town, k) + 1e-9
        for k, n in a['costs'].items():
            b['input_buffer'][k] = max(0, b['input_buffer'][k] - n)
            p.record_activity(town, k, n, 'out')
        for k, n in a['goods'].items():
            l.record_output(town, b, k, n, v if a.get('pending', True) else None)
    elif op == 'drop':
        l.drop_off(town, b, v)
    elif op == 'orphan':
        l.cancel_output(town, v)
    elif op == 'pickup':
        source = town['buildings'][a['source']]
        operation = a['kind']
        workplace, storage = (source, b) if operation in {'collect', 'sell'} else (b, source)
        if operation == 'sell':
            town.setdefault('rules', {})['market_sales'] = {k: {'enabled': True, 'cap': 1, 'amount': 10} for k in p.SELLABLE_GOODS}
        cargo = l.pickup_cargo(town, workplace, storage, operation, v)
        a['goods'] = cargo
        v['logistics_trip'] = dict(operation=operation, picked_up=True, delivered=False, cargo=cargo,
                                  workplace={k: workplace.get(k) for k in ('kind','x','y')},
                                  storage={k: storage.get(k) for k in ('kind','x','y')})
    elif op == 'deposit':
        trip = v['logistics_trip']
        source = town['buildings'][a['source']]
        workplace, storage = (source, b) if trip['operation'] == 'collect' else (b, source)
        _, remaining = l.deposit_cargo(town, workplace, storage, trip['operation'], trip['cargo'])
        trip.update(cargo=remaining, delivered=not remaining)
    elif op == 'cancel':
        l.cancel_trip(town, v)
    elif op == 'storage':
        server.command(town, dict(action='configure_storage', index=a['building'], enabled=a['enabled']))
    elif op == 'policy':
        b.update(a['fields'])
    elif op == 'deliver':
        a['received'] = p.deliver(town, a['key'], a['amount'], 100, record=a['production'])
    else:
        raise AssertionError(op)


def cases():
    capacities = []
    for kind in ('hall', *server.COSTS):
        for level in ([1, 2, 3, 8] if kind == 'storehouse' else [1, 2, 3]):
            b = dict(kind=kind, level=level, stored_goods={'wood': 7, 'bread': 3, 'food': 2},
                     output_buffer={'wood': .125}, input_buffer={'wood': 50}, pending_output={'wood': .375, 'bread': 2},
                     storage_enabled={'food': True, 'bread': False, 'wood': True},
                     profession_upgrades=[*p.PROFESSION_UPGRADES, 'unknown_saved_equipment'])
            capacities.append(dict(building=b, size=p.storage_size(b), allocations=p.storage_allocations(b),
                                   rooms={k:p.local_room(b,k) for k in set(p.STORAGE_KEYS)}))
    for upgrade, spec in p.PROFESSION_UPGRADES.items():
        if 'throughput' in spec:
            b = dict(kind=spec['building'], level=2, profession_upgrades=[upgrade])
            capacities.append(dict(building=b,size=p.storage_size(b),allocations=p.storage_allocations(b),
                                   rooms={k:p.local_room(b,k) for k in set(p.STORAGE_KEYS)}))
    for filters in [dict(storage_allocations={'wood': 2, 'food': 5}), {},
                    dict(storage_enabled={'food':True, 'salmon':False}), dict(storage_enabled={})]:
        b = dict(kind='storehouse', level=2, **filters)
        capacities.append(dict(building=b,size=p.storage_size(b),allocations=p.storage_allocations(b),
                               rooms={k:p.local_room(b,k) for k in set(p.STORAGE_KEYS)}))
    initial = dict(inventory_version=1, food_balance_version=2, market_storage_version=1,
        town_origin=[0,0], map_resources=[], terrain=["g"*8]*8, map_size=8, npcs=[], wood_chain_version=1, grain_chain_version=1, acre_upgrade_version=1, worker_capacity_version=1,
        harvested=0, coins=100, wood=20, food=6, goods={'wheat':30,'flour':30,'bread':2}, buildings=[
            dict(kind='hall', x=0,y=0,stored_goods={'wood':20, 'wheat':30, 'flour':30, 'bread':2, 'food':3}),
            dict(kind='bakery',x=2,y=0),
            dict(kind='storehouse',x=4,y=0,storage_enabled={'bread':True,'wood':True}),
            dict(kind='sawpit',x=6,y=0)],
        villagers=[dict(job='baking',building_index=1,baking=0,talents={}),dict(job='hauling',talents={})])
    actions = [dict(op='cap',key='bread',cap=10),dict(op='cap',key='wood',cap=20),
        dict(op='reserve',source=0,building=1,goods={'flour':4, 'wood':1}),dict(op='advance',seconds=3),
        dict(op='produce',building=1,actor=0,costs={'flour':2,'wood':.5},goods={'bread':2}),
        dict(op='drop',building=1,actor=0),dict(op='pickup',source=1,building=2,actor=1,kind='collect'),
        dict(op='storage',building=2,enabled={'wood':True}),dict(op='deposit',source=1,building=2,actor=1),
        dict(op='storage',building=2,enabled={'bread':True}),dict(op='deposit',source=1,building=2,actor=1),
        dict(op='produce',building=1,actor=0,costs={'flour':2,'wood':.5},goods={'bread':2}),
        dict(op='orphan',building=1,actor=0),dict(op='pickup',source=1,building=2,actor=1,kind='collect'),
        dict(op='cancel',actor=1),dict(op='pickup',source=0,building=1,actor=1,kind='supply'),
        dict(op='deposit',source=0,building=1,actor=1),dict(op='release',building=1,goods={'flour':2,'wood':.5}),
        dict(op='pickup',source=1,building=2,actor=1,kind='sell'),dict(op='cancel',actor=1),
        dict(op='spend',goods={'bread':1,'coins':.125,'wood':.5}),dict(op='eat',amount=4.25),
        dict(op='deliver',key='salmon',amount=1.125,production=False),
        dict(op='deliver',key='wood',amount=.75,production=True),
        dict(op='policy',building=0,fields={'construction':{'remaining':1}}),
        dict(op='deliver',key='trout',amount=5,production=True),
        dict(op='advance',seconds=57),dict(op='advance',seconds=3),dict(op='cap',key='wood',cap=None),
        dict(op='produce',building=3,pending=False,costs={},goods={'planks':.75})]
    town = copy.deepcopy(initial)
    steps = []
    all_snapshots = [snapshot(town)]
    for action in actions:
        apply(town, action)
        state = snapshot(town)
        all_snapshots.append(state)
        steps.append(dict(action=action,state=state))
    browser = subprocess.run(['node',str(Path(__file__).with_name('overview.js'))],
        input=json.dumps(dict(catalog=p.public_catalog(),towns=[s['town'] for s in all_snapshots])),
        text=True,capture_output=True,check=True)
    for state, rows in zip(all_snapshots,json.loads(browser.stdout)):
        state['overview'] = rows
    return dict(storage=capacities,traces=[dict(initial=all_snapshots[0],steps=steps)])
