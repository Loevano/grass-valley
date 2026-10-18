"""Freeze actual Python barn growth, named count edits and manual rewards."""
import argparse
import copy
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT / 'tests')]
import progression as p
import logistics as l
import server as s
from fixtures import flat_town


def town():
    t = flat_town('livestock migration', size=32)
    t.update(level=5, food=0, goods={}, wood=0, coins=100, npcs=[], map_resources=[],
             inventory_version=1)
    t['buildings'] = [dict(kind='hall', x=1, y=1, stored_goods={}),
                      dict(kind='barn', x=12, y=12), dict(kind='butcher', x=5, y=12)]
    t['villagers'] = [dict(job='butchery', building_index=2, husbandry=2, talents={})]
    return t


def snapshot(b):
    return dict(animals=b.get('animals', {}), ages=b.get('animal_growth', {}), names=b.get('animal_names', {}))


def cases():
    growth = []
    for animal in p.ANIMAL_MEAT:
        duration = p.animal_duration(animal)
        for keepers in (0, 1, 3):
            for age, elapsed, room in [(0, 3, 100), (duration-1, 3, 100),
                                        (0, duration*2, 100), (duration, 3, 100),
                                        (0, 3, 0), (0, 3, .1)]:
                t = town()
                b = t['buildings'][1]
                b.update(animals={animal: 1}, animal_growth={animal: [age]})
                product = p.ANIMALS[animal]['output']
                if room < 100:
                    t['rules'] = {'production_caps': {product: room, 'manure': room}}
                l.advance_barn(t, b, elapsed, keepers)
                growth.append(dict(species=animal, age=age, elapsed=elapsed, keepers=keepers,
                                   room=room, grown=b['animal_growth'][animal][0],
                                   output=b.get('output_buffer', {})))
    names = []
    t = town()
    b = t['buildings'][1]
    b.update(animals={'chicken': 2, 'sheep': 1}, animal_growth={'chicken': [10, 60], 'sheep': [40]})
    s.ensure_animal_names(t)
    names.append(dict(op='admit', state=copy.deepcopy(snapshot(b))))
    for count in (1, 3, 0, 2):
        s.command(t, dict(action='set_animal_count', index=1, animal='chicken', count=count))
        names.append(dict(op='count', count=count, state=copy.deepcopy(snapshot(b))))
    replacement = []
    for animal in p.ANIMAL_MEAT:
        t = town()
        b, shop = t['buildings'][1:]
        b.update(animals={animal: 3})
        s.ensure_animal_names(t)
        for _ in range(3):
            s.command(t, dict(action='butcher_animal', index=2, barn_index=1, animal=animal))
            selected = shop['butchering']['name']
            replacement.append(dict(species=animal, state=copy.deepcopy(snapshot(b)), name=selected))
            shop.pop('butchering')
    rewards = []
    for animal in p.ANIMAL_MEAT:
        for cap in (False, True):
            t = town()
            shop = t['buildings'][2]
            shop['butchering'] = dict(animal=animal, amount=p.ANIMAL_MEAT[animal], remaining=60)
            if cap:
                t['rules'] = {'production_caps': {'meat': 0, 'hides': 0}}
            worker = t['villagers'][0]
            worker['work_animation'] = {'phase': 'working'}
            for _ in range(20):
                l.butcher_animal(t, shop, worker, 3)
            rewards.append(dict(species=animal, cap=cap, output=shop.get('output_buffer', {}),
                                practice=worker.get('butchery', 0), pending='butchering' in shop,
                                remaining=shop.get('butchering', {}).get('remaining', 0)))
    return dict(growth=growth, names=names, replacement=replacement, rewards=rewards)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = Path(__file__).with_name('python.json')
    data = json.dumps(cases(), indent=2) + '\n'
    if args.check:
        if output.read_text() != data:
            raise SystemExit('livestock fixture drift')
        print('livestock fixture matches Python')
    else:
        output.write_text(data)
