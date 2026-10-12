import copy
import json
import unittest

import quests
from server import command, new_town
from shared_world import export_world


class QuarryPuzzle(unittest.TestCase):
    def setUp(self):
        self.world = {'towns': {key: new_town(key) for key in ('one', 'two', 'three')}}

    def act(self, key, action, **data):
        return command(self.world['towns'][key], dict(action=action, **data), self.world)

    def walk(self, key, *steps):
        for dx, dy in steps:
            self.act(key, 'quarry_walk', dx=dx, dy=dy)

    def join_pair(self):
        self.act('one', 'join_quarry')
        self.act('two', 'join_quarry')

    def cross_gate(self):
        self.walk('one', (1, 0), (0, -1))  # Hold the plate at 2,2.
        self.walk('two', *((1, 0),) * 5)  # Cross and secure the far-side catch.

    def solve_and_collect(self, key='two'):
        self.walk(key, (1, 0), (0, -1))  # Sun rune at 7,2.
        self.act(key, 'quarry_interact', target='sun')
        self.act(key, 'quarry_interact', target='leaf')
        self.walk(key, (1, 0))
        self.act(key, 'quarry_interact', target='moon')
        self.walk(key, (0, 1), (0, 1))
        self.act(key, 'quarry_interact', target='treasure')

    def test_entry_requires_two_explicit_participants_and_stops_gathering(self):
        town = self.world['towns']['one']
        town['player']['activity'] = {'job': 'gathering'}
        self.act('one', 'join_quarry')
        self.assertNotIn('activity', town['player'])
        for action, data in [('quarry_walk', {'dx': 1, 'dy': 0}),
                             ('walk', {'dx': 1, 'dy': 0}), ('gather', {})]:
            with self.assertRaises(ValueError):
                self.act('one', action, **data)
        self.assertFalse(self.world['quarry']['started'])
        with self.assertRaises(ValueError):
            self.act('one', 'join_quarry')
        self.assertFalse(self.world['quarry']['started'])
        self.act('two', 'join_quarry')
        self.assertTrue(self.world['quarry']['started'])

    def test_plate_gate_catch_and_separate_valley_position(self):
        self.join_pair()
        valley_positions = [copy.deepcopy(t['player']) for t in self.world['towns'].values()]
        self.walk('two', *((1, 0),) * 3)
        with self.assertRaises(ValueError):
            self.walk('two', (1, 0))
        self.walk('one', (1, 0), (0, -1))
        self.walk('two', (1, 0))  # Gate tile.
        self.walk('one', (0, 1))  # Plate releases before the companion steps off.
        self.walk('two', (1, 0))
        self.assertTrue(self.world['quarry']['gate_latched'])
        self.walk('two', (-1, 0), (-1, 0))  # Both sides remain accessible.
        self.assertEqual(valley_positions, [t['player'] for t in self.world['towns'].values()])

    def test_clue_wrong_sequence_and_reward_once_for_every_member(self):
        self.join_pair()
        self.cross_gate()
        with self.assertRaises(ValueError):
            self.act('one', 'quarry_interact', target='sun')
        self.walk('two', (1, 0), (0, 1))
        self.act('two', 'quarry_interact', target='inscription')
        self.assertEqual(self.world['quarry']['message'], quests.CATALOG['clue'])
        self.walk('two', (1, 0))
        with self.assertRaises(ValueError):
            self.act('two', 'quarry_interact', target='treasure')
        self.walk('two', (0, -1), (0, -1))
        self.act('two', 'quarry_interact', target='sun')
        self.act('two', 'quarry_interact', target='moon')
        self.assertEqual(self.world['quarry']['sequence'], [])
        self.walk('two', (-1, 0), (-1, 0), (0, 1))
        self.act('one', 'leave_quarry')
        coins = {key: t['coins'] for key, t in self.world['towns'].items()}
        self.solve_and_collect()
        for key in ('one', 'two'):
            town = self.world['towns'][key]
            self.assertEqual(town['coins'], coins[key] + quests.REWARD)
            self.assertEqual(town['player']['keepsakes'], [quests.KEEPSAKE])
        self.assertEqual(self.world['towns']['three']['coins'], coins['three'])
        completed = copy.deepcopy(self.world)
        with self.assertRaises(ValueError):
            self.act('two', 'quarry_interact', target='treasure')
        with self.assertRaises(ValueError):
            self.act('three', 'join_quarry')
        self.assertEqual(self.world, completed)

    def test_departure_allows_solo_completion_and_rejoining(self):
        self.join_pair()
        self.act('one', 'leave_quarry')
        self.assertTrue(self.world['quarry']['gate_latched'])
        self.act('one', 'join_quarry')
        self.act('one', 'leave_quarry')
        self.walk('two', *((1, 0),) * 5)
        self.solve_and_collect()
        self.assertTrue(self.world['quarry']['completed'])
        self.act('two', 'leave_quarry')
        self.assertFalse(quests.inside(self.world, self.world['towns']['two']))

    def test_save_reload_preserves_party_positions_gate_and_sequence(self):
        self.join_pair()
        self.cross_gate()
        self.walk('two', (1, 0), (0, -1))
        self.act('two', 'quarry_interact', target='sun')
        self.world = json.loads(json.dumps(export_world(self.world)))
        self.assertTrue(self.world['quarry']['gate_latched'])
        self.assertEqual(self.world['quarry']['sequence'], ['sun'])
        self.assertEqual(self.world['quarry']['members']['two']['x'], 7)
        self.act('two', 'quarry_interact', target='leaf')
        self.walk('two', (1, 0))
        self.act('two', 'quarry_interact', target='moon')
        self.assertTrue(self.world['quarry']['runes_solved'])

    def test_invalid_commands_cannot_move_or_interact_for_another_founder(self):
        self.join_pair()
        original = copy.deepcopy(self.world)
        for dx, dy in [(True, 0), (1, 1), (0, 0), (2, 0), ('1', 0)]:
            with self.assertRaises(ValueError):
                self.act('one', 'quarry_walk', dx=dx, dy=dy)
        for target in [None, [], 'unknown', 'treasure']:
            with self.assertRaises(ValueError):
                self.act('one', 'quarry_interact', target=target, town_id='two')
        with self.assertRaises(ValueError):
            self.act('three', 'quarry_walk', dx=1, dy=0)
        self.walk('one', (0, -1), (0, -1))
        before_wall = copy.deepcopy(self.world)
        with self.assertRaises(ValueError):
            self.walk('one', (0, -1))
        self.assertEqual(self.world, before_wall)
        self.walk('one', (0, 1), (0, 1))
        self.assertEqual(self.world, original)


if __name__ == '__main__':
    unittest.main()
