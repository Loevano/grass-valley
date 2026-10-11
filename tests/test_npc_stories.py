import copy
import json
import unittest

import progression as economy
from npcs import STORIES, local_key
from server import command, ensure_resources, new_town
from shared_world import bind_map, export_world, new_world


class LocalStories(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Stories')
        self.town['npcs'] = [{'name': name, 'x': 5, 'y': 8}
                             for name in ('Moss', 'Tobin', 'Flint', 'Reed')]
        self.town['map_resources'].extend(
            {'kind': 'rock', 'x': x, 'y': 10} for x in (3, 5, 7))

    def advance(self, index):
        command(self.town, {'action': 'advance_local_story', 'npc_index': index})

    def reject(self, **data):
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            command(self.town, data)
        self.assertEqual(self.town, before)

    def reload(self):
        self.town = json.loads(json.dumps(self.town))
        ensure_resources(self.town)

    def test_moss_supplies_are_consumed_and_reward_is_once_after_reload(self):
        self.advance(0)
        self.reject(action='advance_local_story', npc_index=0)
        self.town['buildings'].append({'kind': 'storehouse', 'x': 3, 'y': 3,
                                       'storage_allocations': {'herbs': 10}})
        economy.deliver(self.town, 'herbs', 5, 0)
        self.advance(0)
        self.assertEqual(economy.quantity(self.town, 'herbs'), 2)
        self.reload()
        coins, food = self.town['coins'], self.town['food']
        self.advance(0)
        self.assertEqual(self.town['food'], food - 4)
        self.assertEqual(economy.quantity(self.town, 'herbs'), 0)
        self.assertEqual(self.town['coins'], coins + STORIES['Moss']['reward'])
        self.assertEqual(self.town['player']['keepsakes'], [STORIES['Moss']['keepsake']])
        self.reload()
        self.reject(action='advance_local_story', npc_index=0)

    def test_tobin_needs_completed_roads_and_preserves_other_local_offers(self):
        self.advance(1)
        food = self.town['food']
        self.advance(1)
        self.assertEqual(self.town['food'], food - 4)
        self.town['roads'] = [{'x': x, 'y': 6} for x in (3, 4)]
        self.town['roads'].append({'x': 5, 'y': 6, 'construction': {'progress': 0, 'total': 10}})
        self.reject(action='advance_local_story', npc_index=1)
        del self.town['roads'][-1]['construction']
        wood = self.town['wood']
        self.advance(1)
        self.assertEqual(self.town['wood'], wood - 6)
        command(self.town, {'action': 'trade_local', 'npc_index': 1})
        progress = self.town['local_visits'][local_key(self.town['npcs'][1])]
        self.assertEqual(progress['story_stage'], 3)
        self.assertEqual(progress['trade_remaining'], 300)

    def test_flint_requires_real_nearby_unique_rocks_during_survey(self):
        self.reject(action='inspect_story_rock', x=5, y=10)
        self.advance(2)
        self.reject(action='advance_local_story', npc_index=2)
        self.reject(action='inspect_story_rock', x=True, y=10)
        self.reject(action='inspect_story_rock', x=5, y=9)
        self.reject(action='inspect_story_rock', x=5, y=10)
        for x in (3, 5, 7):
            self.town['player'].update(x=x, y=9)
            command(self.town, {'action': 'inspect_story_rock', 'x': x, 'y': 10})
            self.reject(action='inspect_story_rock', x=x, y=10)
        self.reload()
        self.town['player'].update(x=5, y=8)
        self.advance(2)
        self.town['player'].update(x=5, y=9)
        self.reject(action='inspect_story_rock', x=5, y=10)
        self.town['player'].update(x=5, y=8)
        self.advance(2)
        self.assertIn(STORIES['Flint']['keepsake'], self.town['player']['keepsakes'])

    def test_requests_require_valid_local_and_proximity(self):
        for index in (True, -1, 4, 3):
            self.reject(action='advance_local_story', npc_index=index)
        self.town['player']['x'] = 20
        self.reject(action='advance_local_story', npc_index=0)

    def test_shared_world_progress_is_personal_and_survives_export(self):
        world = new_world(seed=42)
        world['towns'] = {name: new_town(name, world) for name in ('one', 'two')}
        bind_map(world)
        index = next(i for i, n in enumerate(world['shared_map']['npcs']) if n['name'] == 'Moss')
        npc = world['shared_map']['npcs'][index]
        town = world['towns']['one']
        town['player'].update(x=npc['x'], y=npc['y'])
        command(town, {'action': 'advance_local_story', 'npc_index': index}, world)
        self.assertNotIn('local_visits', world['towns']['two'])
        restored = json.loads(json.dumps(export_world(world)))
        bind_map(restored)
        self.assertEqual(restored['towns']['one']['local_visits'][local_key(npc)]['story_stage'], 1)

    def test_survey_records_rocks_in_streamed_world_chunks(self):
        world = new_world(seed=42, explorable=True)
        town = new_town('Survey', world)
        world['towns']['one'] = town
        index = next(i for i, n in enumerate(town['npcs']) if n['name'] == 'Flint')
        npc = town['npcs'][index]
        town['player'].update(x=npc['x'], y=npc['y'])
        command(town, {'action': 'advance_local_story', 'npc_index': index}, world)
        rock = next(r for r in town['map_resources'] if r['kind'] == 'rock')
        town['player'].update(x=rock['x'] + 1, y=rock['y'])
        command(town, {'action': 'inspect_story_rock', 'x': rock['x'], 'y': rock['y']}, world)
        self.assertEqual(town['local_visits'][local_key(npc)]['story_rocks'],
                         [f"{rock['x']}:{rock['y']}"])


if __name__ == '__main__':
    unittest.main()
