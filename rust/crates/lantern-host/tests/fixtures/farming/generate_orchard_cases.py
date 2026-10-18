"""Synthetic real Python orchard growth, pick clocks and crop-edit contracts."""
import argparse
import copy
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT / 'tests')]
import orchards as o
import progression as p
import work_cycles as wc
import server
from fixtures import flat_town


def cases():
    growth = []
    for stage in ['sapling', 'blossom', 'fruit', 'harvested', 'ripe']:
        for parts in [[10], [30, 30, 90], [150], [1000], [.25] * 600]:
            tree = dict(kind='orchard_tree', x=4, y=4,
                        tree=dict(stage=stage, elapsed=0, fruit=0 if stage == 'harvested' else 8))
            for n in parts:
                o.grow({'buildings': [tree]}, n)
            growth.append(dict(stage=stage, parts=parts, state=tree['tree']))
    picks = []
    for level in [1, 2, 3]:
        for skill, talent in [(0, 0), (20, 1), (40, -1)]:
            for cap in [None, .5, 0]:
                town = flat_town('orchard picks')
                orchard = dict(kind='orchard', x=3, y=3, level=level)
                tree = dict(kind='orchard_tree', x=4, y=4, orchard_index=1,
                            tree=dict(stage='ripe', elapsed=0, fruit=8))
                town.update(buildings=[dict(kind='hall', x=0, y=0), orchard, tree],
                            terrain=['g' * 16] * 16, map_size=16, map_resources=[], npcs=[],
                            inventory_version=1, goods={}, wood=0, food=0,
                            rules={'production_caps': {} if cap is None else {'fruit': cap}})
                worker = town['villagers'][0]
                worker.update(job='orcharding', building_index=1, farming=20, orcharding=skill,
                              talents={'farming': talent}, construction_position=dict(x=4, y=3))
                duration = o.PICK_SECONDS / (p.efficiency(worker, 'orcharding') * (1 + .25 * (level - 1)))
                wc.advance(town, town, worker, orchard, tree, 0, 0)
                worker['work_animation'].update(phase='working', path=[], elapsed=0, x=4, y=3)
                if cap == 0:
                    o.advance(town, town, worker, orchard, tree, 0, duration)
                else:
                    wc.advance(town, town, worker, orchard, tree, 0, duration)
                picks.append(dict(level=level, skill=skill, talent=talent, cap=cap, duration=duration,
                                  state=copy.deepcopy(tree['tree']),
                                  produced=town.get('produced', {}).get('fruit', 0),
                                  pending=worker.get('pending_output', {}).get('fruit', 0),
                                  practice=worker.get('orcharding', 0)))
    edits = []
    for target in [1, 2]:
        town = flat_town('crop edits')
        town['level'] = 10
        town['buildings'] = [dict(kind='hall', x=0, y=0), dict(kind='farm', x=3, y=3),
                             dict(kind='acre', x=3, y=4, farm_index=1, crop='wheat',
                                  field=dict(stage='ripe', elapsed=0, progress=2, cycle=5, crop='wheat', yield_remaining=3)),
                             dict(kind='acre', x=4, y=4, farm_index=1, crop='vegetables')]
        server.command(town, dict(action='set_crop', index=target, crop='carrots'))
        edits.append(dict(target=target, crops=[b.get('crop', 'vegetables') for b in town['buildings'][1:]],
                          field=town['buildings'][2]['field']))
    return dict(growth=growth, picks=picks, edits=edits)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = Path(__file__).with_name('orchard_cases.json')
    data = json.dumps(cases(), indent=2) + '\n'
    if args.check:
        if output.read_text() != data:
            raise SystemExit('orchard fixture drift')
        print('orchard fixture matches Python')
    else:
        output.write_text(data)
