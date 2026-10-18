"""Freeze real Python woodcutting rates and movement interruption clocks."""
import argparse
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT / 'tests')]
import progression as p
import npcs
import work_cycles as wc
import logistics as l
from fixtures import flat_town


def cases():
    rates = []
    for skill, talent in [(0, 0), (20, 1), (40, -1)]:
        for tool in [None, 'crafted_axe', 'fine_axe', 'steel_axe',
                     'masterwork_axe', 'fine_sickle']:
            for elapsed, room, supply in [(3, 50, 8), (0.2, 50, 8),
                                          (3, 0.1, 8), (3, 50, 0.05),
                                          (3, 0, 8)]:
                worker = dict(job='woodcutting', woodcutting=skill,
                              talents={'woodcutting': talent}, worker_tool=tool)
                amount = min(room, p.WOOD_PER_CYCLE * elapsed / 3
                             * (1 + npcs.tool_bonus(worker, 'woodcutting'))
                             * p.efficiency(worker, 'woodcutting'))
                landscape = flat_town('depletion')
                tree = dict(kind='tree', x=5, y=2, supply=supply)
                landscape['map_resources'] = [tree]
                amount = p.take_supply(landscape, tree, amount)
                if amount > 0:
                    p.practice(worker, 'woodcutting', elapsed)
                rates.append(dict(skill=skill, talent=talent, tool=tool,
                                  elapsed=elapsed, room=room, supply=supply,
                                  amount=amount, practice=worker['woodcutting']))
    clocks = []
    for phase, clock, elapsed in [('working', 0, 3), ('working', 3, .2),
                                   ('depositing', .4, 3),
                                   ('depositing', 0, .2)]:
        town = flat_town('wood clocks')
        camp = dict(kind='woodcutter', x=5, y=3)
        tree = dict(kind='tree', x=5, y=2)
        town['buildings'].append(camp)
        town['map_resources'] = [tree]
        worker = town['villagers'][0]
        worker.update(job='woodcutting', building_index=1, talents={})
        wc.advance(town, town, worker, camp, tree, 0, 0)
        worker['work_animation'].update(phase=phase, elapsed=clock,
                                       x=5, y=1, path=[])
        finished, remaining = wc._advance(town, town, worker, camp, tree, 0,
                                          elapsed, stop_at_delivery=True)
        state = worker['work_animation']
        clocks.append(dict(phase=phase, clock=clock, elapsed=elapsed,
                           finished=finished, remaining=remaining,
                           result_phase=state['phase'], result_clock=state['elapsed']))
    town = flat_town('output interruption')
    town.update(wood=0, food=0, goods={}, harvested=0)
    camp = dict(kind='woodcutter', x=5, y=3)
    town['buildings'] = [dict(kind='hall', x=0, y=0), camp]
    town['inventory_version'] = 1
    worker = dict(job='woodcutting', talents={})
    other = dict(job='woodcutting', talents={})
    town['villagers'] = [worker, other]
    accounting = []
    for op in ['work', 'cancel', 'replacement', 'dropoff', 'retry']:
        if op == 'work':
            l.record_output(town, camp, 'wood', .25, worker)
        elif op == 'cancel':
            l.cancel_output(town, worker)
        elif op == 'replacement':
            l.record_output(town, camp, 'wood', .25, other)
        else:
            l.drop_off(town, camp, other)
        accounting.append(dict(op=op, free=p.quantity(town, 'wood'),
                               pending=camp.get('pending_output', {}).get('wood', 0),
                               orphan=camp.get('orphaned_output', {}).get('wood', 0),
                               worker=other.get('pending_output', {}).get('wood', 0),
                               produced=town.get('produced', {}).get('wood', 0)))
    return dict(rates=rates, clocks=clocks, accounting=accounting)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = Path(__file__).with_name('python.json')
    data = json.dumps(cases(), indent=2) + '\n'
    if args.check:
        if output.read_text() != data:
            raise SystemExit('woodcutting fixture drift')
        print('woodcutting fixture matches Python')
    else:
        output.write_text(data)
