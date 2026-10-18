"""Freeze land catch accounting, piers, water access and clocks from Python."""
import argparse
import json
from pathlib import Path
from unittest.mock import patch
import sys
ROOT = Path(__file__).resolve().parents[6]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT / 'tests')]
import progression as p
import npcs
import work_cycles as wc
from spatial import SpatialIndex
from fixtures import flat_town


def cases():
    rates = []
    for skill, talent in [(0, 0), (20, 1), (40, -1)]:
        for level, upgrades in [(1, []), (3, ['nets']), (5, ['nets', 'wool_nets'])]:
            for tool in [None, 'fine_fishing_rod', 'steel_fishing_rod', 'masterwork_fishing_rod']:
                for roll, room, supply in [(0, 50, 80), (1, 50, 80), (9, .1, 80), (1, 50, .05), (1, 50, 0)]:
                    worker = dict(job='fishing', fishing=skill, talents={'fishing': talent}, worker_tool=tool)
                    landscape = flat_town('catch supply')
                    landscape['map_resources'] = [dict(kind='water', x=x, y=2, supply=supply) for x in (5, 6)]
                    amount = min(room, (1 + .25 * (level - 1) + .25 * len(upgrades)
                                       + npcs.tool_bonus(worker, 'fishing')) * p.efficiency(worker, 'fishing'))
                    amount = p.take_supply(landscape, landscape['map_resources'][0], amount)
                    if amount > 0:
                        p.practice(worker, 'fishing', 3)
                    rates.append(dict(skill=skill, talent=talent, level=level, upgrades=upgrades,
                                      tool=tool, roll=roll, room=room, supply=supply,
                                      amount=amount if roll else 0, consumed=amount,
                                      practice=worker['fishing']))
    clocks = []
    for phase, clock, elapsed, pending in [('working', 0, 3, False), ('working', 3, 3, False),
                                          ('working', 3, .2, True), ('depositing', .4, 3, True),
                                          ('depositing', 0, .2, True)]:
        town = flat_town('fisher clocks')
        hut = dict(kind='fishing_hut', x=5, y=3)
        source = dict(kind='water', x=5, y=2)
        town['buildings'].append(hut)
        town['map_resources'] = [source]
        worker = town['villagers'][0]
        worker.update(job='fishing', building_index=1, talents={})
        wc.advance(town, town, worker, hut, source, 0, 0)
        worker['work_animation'].update(phase=phase, elapsed=clock, x=5, y=2, path=[])
        if pending:
            worker['pending_output'] = {'fish': 1}
        finished, remaining = wc._advance(town, town, worker, hut, source, 0, elapsed, stop_at_delivery=True)
        state = worker['work_animation']
        clocks.append(dict(phase=phase, clock=clock, elapsed=elapsed, pending=pending,
                           finished=finished, remaining=remaining, result_phase=state['phase'],
                           result_clock=state['elapsed'], pier=state['pier']))
    access = []
    for origin, points in [((1, 2), [(x, y) for y in range(1, 6) for x in range(3, 8)] + [(2, 8)]),
                            ((1, 2), [(3, 2), (4, 2), (4, 3), (8, 2)])]:
        water = [dict(kind='water', x=x, y=y) for x, y in points]
        depth = SpatialIndex(water).water_depths()
        for reach in (3, 6, 10):
            for tier in (0, 1, 3):
                hut = dict(kind='fishing_hut', x=origin[0], y=origin[1],
                           profession_upgrades=[] if not tier else [['boat'], ['boat', 'improved_boat'], ['boat', 'improved_boat', 'master_boat']][tier-1])
                occupied = {(4, 3)}
                selected = wc.fishing_water(hut, water, occupied, reach, depths=depth)
                access.append(dict(origin=origin, points=points, depths=[[x, y, depth[(x, y)]] for x,y in points],
                                   tier=tier, reach=reach, occupied=list(occupied), selected=[[r['x'], r['y']] for r in selected]))
    founder = []
    for skill in (0, 18, 20, 40):
        for roll in range(5):
            town = flat_town('founder land fishing')
            town.update(food=0, goods={}, wood=0, inventory_version=1)
            town['buildings'] = [dict(kind='hall', x=0, y=0, stored_goods={})]
            town['map_resources'] = [dict(kind='water', x=5, y=2, supply=80)]
            town['player'] = dict(x=4, y=2, fishing=skill,
                                  activity=dict(job='fishing', kind='water', x=5, y=2))
            with patch('progression.secrets.randbelow', return_value=roll):
                p.player_tick(town, town, 1000)
            founder.append(dict(skill=skill, roll=roll, amount=p.quantity(town, 'fish'),
                                practice=town['player']['fishing'],
                                supply=town['map_resources'][0]['supply']))
    return dict(rates=rates, clocks=clocks, access=access, founder=founder)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = Path(__file__).with_name('python.json')
    data = json.dumps(cases(), indent=2) + '\n'
    if args.check:
        if output.read_text() != data:
            raise SystemExit('land fishing fixture drift')
        print('land fishing fixture matches Python')
    else:
        output.write_text(data)
