"""Freeze real Python merchant transactions, resource activity and personal practice."""
import argparse
import copy
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
import logistics as l
import progression as p


def snapshot(town, worker):
    history = town.get('resource_activity', {'buckets': []})
    incoming, outgoing = {}, {}
    for bucket in history['buckets']:
        for direction, values in [('in', incoming), ('out', outgoing)]:
            for key, n in bucket[direction].items():
                values[key] = values.get(key, 0) + n
    return dict(free={key: n for key in p.STORAGE_KEYS if key != 'food' and (n := p.quantity(town, key)) != 0},
                coins=town['coins'], food=town['food'], sales=town.get('market_sales', 0),
                incoming=incoming, outgoing=outgoing,
                trading=worker.get('trading', 0), hauling=worker.get('hauling', 0),
                held=worker.get('logistics_trip', {}).get('cargo', {}))


def cases():
    scripts = []
    for key in ('planks', 'vegetables', 'smoked_sturgeon', 'wood', 'trade_crates'):
        for strict in (False, True):
            configure = dict(op='trade', key=key, buy=None, sell=10) if strict else dict(op='sale', key=key, cap=10, enabled=True, amount=4)
            scripts.append(dict(name=f'{key}_{strict}', goods={key: 13}, actions=[configure, dict(op='pickup'), dict(op='sell')]))
    for enabled, cap, amount in [(True, 12, 10), (False, 10, 10), (True, 10, 1)]:
        scripts.append(dict(name=f'changed_{enabled}_{cap}_{amount}', goods={'planks': 13}, actions=[dict(op='trade', key='planks', buy=None, sell=10), dict(op='pickup'), dict(op='sale', key='planks', enabled=enabled, cap=cap, amount=amount), dict(op='sell')]))
    scripts += [dict(name='purchases_rotation', goods={}, actions=[dict(op='trade', key='wood', buy=25, sell=None), dict(op='trade', key='vegetables', buy=25, sell=None), *[dict(op='purchase', elapsed=n) for n in (1, 2, 100, 3, 3, 3, 3, 3)]]),
                dict(name='purchase_price_and_target', goods={}, actions=[dict(op='trade', key='planks', buy=7, sell=None), dict(op='purchase', elapsed=3), dict(op='purchase', elapsed=3)]),
                dict(name='quick_sale', goods={'planks': 13}, actions=[dict(op='quick', key='planks', amount=5), dict(op='quick', key='planks', amount=8)])]
    for script in scripts:
        worker = dict(job='trading', talents={})
        source = dict(kind='sawpit', x=8, y=0, stored_goods=copy.deepcopy(script['goods']))
        market = dict(kind='market', x=10, y=0)
        town = dict(buildings=[dict(kind='hall', x=0, y=0, level=1), dict(kind='storehouse', x=2, y=0, storage_enabled={'wood': True, 'planks': True, 'food': True}), source, market], villagers=[worker], coins=1000, wood=0, food=0, goods={}, harvested=0)
        town.update(inventory_version=1, food_balance_version=2,
                    wood=script['goods'].get('wood', 0),
                    goods={key: n for key, n in script['goods'].items() if key != 'wood'},
                    food=sum(n * p.EDIBLE.get(key, 0) for key, n in script['goods'].items()))
        p.ensure_inventory(town)
        states = []
        for action in script['actions']:
            op = action['op']
            if op == 'trade':
                l.configure_resource_trade(town, dict(resource=action['key'], buy_below=action['buy'], sell_above=action['sell']))
            elif op == 'sale':
                l.configure_market_sale(town, dict(resource=action['key'], cap=action['cap'], enabled=action['enabled'], amount=action['amount']))
            elif op == 'pickup':
                held = l.pickup_sale(town, source, worker)
                worker['logistics_trip'] = dict(operation='sell', picked_up=True, delivered=False, cargo=held, workplace={key: source[key] for key in ('kind', 'x', 'y')})
            elif op == 'sell':
                l.transfer(town, source, market, 'sell', worker, worker['logistics_trip']['cargo'])
                worker.pop('logistics_trip')
            elif op == 'purchase':
                l.purchase_batch(town, worker, action['elapsed'])
            elif op == 'quick':
                p.spend(town, {action['key']: action['amount']})
                p.record_sale(town, action['key'], action['amount'])
            states.append(snapshot(town, worker))
        yield {**script, 'states': states}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    path = Path(__file__).with_name('markets.json')
    text = json.dumps(list(cases()), indent=2, sort_keys=True) + '\n'
    if args.check:
        assert path.read_text() == text, 'market fixture drift'
    else:
        path.write_text(text)
