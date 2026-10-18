"""Reviewable R13 data export and independent affordability oracle; --check never writes."""
import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
import progression as p


def outputs():
    catalog = dict(public=p.public_catalog(),
                   resources=list(dict.fromkeys(('coins', *(k for k in p.STORAGE_KEYS if k != 'food')))))
    cases = []
    for stock, costs in [({}, {}), ({}, {'wood': .5}),
                         ({'wood': .5 - 5e-10}, {'wood': .5}),
                         ({'wood': .5 - 2e-9}, {'wood': .5}),
                         ({'wood': .7, 'flour': 2}, {'wood': .5, 'flour': 2}),
                         ({'flour': 2}, {'wood': .5, 'flour': 2}),
                         ({'coins': .125, 'trout': .25}, {'coins': .125, 'trout': .25}),
                         ({'wood': .1 + .2}, {'wood': .3})]:
        town = dict(goods={k: v for k, v in stock.items() if k not in {'wood', 'coins'}},
                    **{k: v for k, v in stock.items() if k in {'wood', 'coins'}})
        affordable = p.afford(town, costs)
        # Single-container resource debit; town withdrawal/food rules remain later slices.
        after = dict(stock)
        if affordable:
            for key, cost in costs.items():
                after[key] = max(0, p.quantity(town, key) - cost)
        cases.append(dict(stock=stock, costs=costs, affordable=affordable, after=after))
    return {'src/resources/catalog.json': catalog, 'tests/fixtures/resources/python.json': cases}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, value in outputs().items():
        path = ROOT / 'rust/crates/lantern-host' / name
        encoded = json.dumps(value, indent=2, sort_keys=True) + '\n'
        if args.check:
            if path.read_text() != encoded:
                raise SystemExit(f'Stale reviewed R13 export: {name}')
        else:
            path.write_text(encoded)
