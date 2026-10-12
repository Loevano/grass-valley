import copy
import json
import math
import unittest

import equipment
import npcs
import server
import village_life
from shared_world import export_world, new_world


class VillageLife(unittest.TestCase):
    def setUp(self):
        self.world = new_world(42, explorable=True)
        self.town = server.new_town('Visitor', self.world)
        self.world['towns']['own'] = self.town
        self.locals = [n for n in self.world['shared_map']['npcs'] if n.get('village')]

    def test_routes_walk_work_and_preserve_town_resources(self):
        before = {key: copy.deepcopy(self.town[key]) for key in ('wood', 'food', 'coins', 'player')}
        anchors = {n['name']: npcs.local_key(n) for n in self.locals}
        moved, jobs = set(), set()
        for _ in range(240):
            village_life.advance(self.world, .5)
            for npc in self.locals:
                state = npc['routine']
                if npcs.position(npc) != (npc['x'], npc['y']):
                    moved.add(npc['name'])
                if state['phase'] == 'working':
                    jobs.add(state['job'])
                village = next(v for v in self.world['shared_map']['villages'] if v['name'] == npc['village'])
                blocked = village_life.blocked_tiles(self.world, village)
                self.assertNotIn((round(state['x']), round(state['y'])), blocked)
                self.assertEqual(npcs.local_key(npc), anchors[npc['name']])
        self.assertEqual(moved, {n['name'] for n in self.locals})
        self.assertTrue({'woodcutting', 'fishing', 'gathering', 'husbandry', 'crafting', 'baking'} <= jobs)
        self.assertEqual({key: self.town[key] for key in before}, before)

    def test_visitors_pause_locals_and_trade_at_their_current_position(self):
        for _ in range(20):
            village_life.advance(self.world, .5)
        trader = next(n for n in self.locals if n['name'] == 'Fen')
        self.assertGreater(math.dist(npcs.position(trader), (trader['x'], trader['y'])), 1.8)
        self.town['player'].update(x=trader['x'], y=trader['y'])
        index = self.town['npcs'].index(trader)
        with self.assertRaisesRegex(ValueError, 'beside'):
            npcs.change(self.town, {'action': 'interact_local', 'npc_index': index})
        x, y = npcs.position(trader)
        self.town['player'].update(x=x, y=y)
        village_life.advance(self.world, 1)
        self.assertEqual(npcs.position(trader), (x, y))
        self.assertTrue(trader['routine']['paused'])
        npcs.change(self.town, {'action': 'interact_local', 'npc_index': index})
        self.assertTrue(self.town['local_visits'][npcs.local_key(trader)]['met'])
        self.town['coins'] = 20
        equipment.change(self.town, {'action': 'buy_equipment', 'item': 'swamp_boots', 'npc_index': index})
        self.assertEqual(self.town['player']['items']['swamp_boots'], 1)

    def test_routines_survive_save_and_resume_without_changing_visit_keys(self):
        village_life.advance(self.world, 1)
        for _ in range(10):
            village_life.advance(self.world, .5)
        restored = json.loads(json.dumps(export_world(self.world)))
        village_life.advance(self.world, .5)
        village_life.advance(restored, .5)
        self.assertEqual(self.world['shared_map']['npcs'], restored['shared_map']['npcs'])

    def test_new_building_on_a_route_is_avoided(self):
        npc = next(n for n in self.locals if n['name'] == 'Hakon')
        for _ in range(8):
            village_life.advance(self.world, .5)
        target = npc['routine']['path'][0]
        self.town['buildings'].append(dict(kind='home', x=target[0], y=target[1]))
        for _ in range(80):
            village_life.advance(self.world, .5)
            self.assertNotIn(target, npc['routine']['path'])
            self.assertNotEqual(npcs.position(npc), tuple(target))
