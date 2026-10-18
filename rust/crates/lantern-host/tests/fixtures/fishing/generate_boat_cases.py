"""Freeze Python boat selection, species/catch rates, crew and exploration costs."""
import argparse
import json
from pathlib import Path
import sys
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[6]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT / 'tests')]
import progression as p
import npcs
import work_cycles as wc
from server import command, work_assignment
from spatial import SpatialIndex
from fixtures import flat_town


def cases():
    upgrades = ['boat', 'improved_boat', 'master_boat']
    selection = []
    for tier in range(1, 4):
        for island in (False, True):
            town = flat_town('boat selection', size=32)
            town.update(npcs=[], terrain=['g' * 32] * 32)
            hut = dict(kind='fishing_hut', x=2, y=8, profession_upgrades=upgrades[:tier])
            town['buildings'] = [dict(kind='hall', x=0, y=0), hut]
            town['villagers'] = [dict(job='fishing', building_index=1)]
            town['map_resources'] = [dict(kind='water', x=x, y=y, supply=80)
                                     for x in range(3, 18) for y in range(1, 16)
                                     if not island or (x, y) != (7, 8)]
            source = work_assignment(town, 'fishing', 0)[1]
            depths = SpatialIndex(town['map_resources']).water_depths()
            selection.append(dict(tier=tier, points=[[w['x'], w['y']] for w in town['map_resources']],
                                  source=[source['x'], source['y']], depth=depths[(source['x'], source['y'])],
                                  species=p.fishing_catch(hut, depths[(source['x'], source['y'])])))
    rates = []
    for tier in range(1, 4):
        for level in (1, 3):
            for skill, talent in [(0, 0), (20, 1)]:
                for tool in (None, 'masterwork_fishing_rod'):
                    for roll, room, supply in [(0, 50, 80), (1, 50, 80), (1, .1, 80), (1, 50, .05)]:
                        hut = dict(kind='fishing_hut', level=level,
                                   profession_upgrades=upgrades[:tier] + ['nets', 'wool_nets'])
                        worker = dict(job='fishing', fishing=skill, talents={'fishing': talent}, worker_tool=tool)
                        amount = min(room, p.fishing_boat_capacity(hut), (1 + .25 * (level - 1) + .5 + .25 * (tier - 1)
                                            + npcs.tool_bonus(worker, 'fishing')) * p.efficiency(worker, 'fishing'))
                        town = flat_town('boat supply')
                        source = dict(kind='water', x=3, y=2, supply=supply)
                        town['map_resources'] = [source]
                        amount = p.take_supply(town, source, amount)
                        p.practice(worker, 'fishing', 3)
                        rates.append(dict(tier=tier, level=level, skill=skill, talent=talent,
                                          tool=tool, roll=roll, room=room, supply=supply,
                                          amount=amount if roll else 0, consumed=amount, practice=worker['fishing']))
    crews = []
    for tier in range(1, 4):
        town = flat_town('crews', size=32)
        hut = dict(kind='fishing_hut', x=2, y=2, profession_upgrades=upgrades[:tier])
        town['buildings'] = [dict(kind='hall', x=0, y=0), hut]
        town['map_resources'] = [dict(kind='water', x=x, y=2) for x in range(3, 15)]
        town['villagers'] = [dict(job='fishing', building_index=1) for _ in range(4)]
        crews.append(dict(tier=tier, crew=wc.fishing_crew(town, hut), capacity=p.fishing_boat_capacity(hut)))
    exploration = []
    for build in (False, True):
        town = flat_town('exploration boats')
        town.update(level=5, coins=200, wood=200, goods={'planks': 100})
        town['player'].update(x=20., y=20., fishing=40)
        command(town, dict(action='build_boat' if build else 'buy_boat'))
        for tier in range(1, 4):
            if tier > 1:
                town['player']['fishing'] = 40
                command(town, dict(action='upgrade_boat'))
            for afloat in (False, True):
                town['player'].update(x=20., y=20. if afloat else 21., fishing=0,
                                      activity=dict(job='fishing', kind='water', x=20, y=20))
                town['map_resources'] = [dict(kind='water', x=20, y=20, supply=80)]
                before = p.quantity(town, 'fish')
                with patch('progression.secrets.randbelow', return_value=1):
                    p.player_tick(town, town, 1000)
                exploration.append(dict(build=build, tier=tier, afloat=afloat,
                                        coins=town['coins'], wood=town['wood'], planks=town['goods']['planks'],
                                        catch=p.quantity(town, 'fish') - before))
    clocks = []
    for clock, elapsed, amount, supply in [(0, 3, 2, 10), (3, 1, 2, 10),
                                           (3, 1, 5, 10), (3, 1, 2, 0)]:
        town = flat_town('boat clocks', size=32)
        hut = dict(kind='fishing_hut', x=2, y=2, profession_upgrades=['boat'])
        town['buildings'] = [dict(kind='hall', x=0, y=0), hut]
        town['map_resources'] = [dict(kind='water', x=x, y=2, supply=supply) for x in range(3, 9)]
        source = town['map_resources'][-1]
        worker = dict(job='fishing', building_index=1, pending_output={'fish': amount},
                      logistics_position={'x': 8, 'y': 2})
        town['villagers'] = [worker]
        wc.advance(town, town, worker, hut, source, 0, 0)
        worker['work_animation'].update(phase='working', elapsed=clock, path=[])
        wc._advance(town, town, worker, hut, source, 0, elapsed, stop_at_delivery=True)
        state = worker['work_animation']
        clocks.append(dict(clock=clock, elapsed=elapsed, amount=amount, supply=supply,
                           phase=state['phase'], result_clock=state['elapsed'], x=state['x'], y=state['y']))
    return dict(selection=selection, rates=rates, crews=crews, exploration=exploration, clocks=clocks)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = Path(__file__).with_name('boats.json')
    data = json.dumps(cases(), indent=2) + '\n'
    if args.check:
        if output.read_text() != data:
            raise SystemExit('boat fixture drift')
        print('boat fixture matches Python')
    else:
        output.write_text(data)
