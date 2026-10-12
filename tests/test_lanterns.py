import copy
import json
import unittest

import lanterns
from server import command, new_town
from shared_world import export_world


class FallingLanterns(unittest.TestCase):
    def setUp(self):
        self.world = {'towns': {k: new_town(k) for k in ('one', 'two', 'three')}}

    def act(self, key, action, **data):
        return command(self.world['towns'][key], dict(action=action, **data), self.world)

    def pair(self):
        for key in ('one', 'two'):
            self.act(key, 'join_lanterns')
        return self.world['lanterns']

    def test_two_players_required_and_expeditions_exclusive(self):
        self.act('one', 'join_lanterns')
        original = copy.deepcopy(self.world)
        lanterns.advance(self.world, .25)
        self.assertEqual(self.world, original)
        for action, data in [('lanterns_walk', dict(dx=1, dy=0)), ('join_quarry', {}),
                             ('walk', dict(dx=1, dy=0)), ('gather', {})]:
            with self.assertRaises(ValueError):
                self.act('one', action, **data)
        self.act('two', 'join_lanterns')
        self.assertTrue(lanterns.snapshot(self.world)['playing'])
        self.act('two', 'leave_lanterns')
        self.assertFalse(lanterns.snapshot(self.world)['playing'])
        self.act('two', 'join_quarry')
        with self.assertRaises(ValueError):
            self.act('two', 'join_lanterns')

    def test_timed_warning_strike_checkpoint_and_safe_row(self):
        quest = self.pair()
        valley = copy.deepcopy(self.world['towns'])
        member = quest['members']['one']
        member.update(x=4, y=2)
        self.assertTrue(all(h['state'] == 'warning' for h in lanterns.hazards(quest) if h['x'] == 4))
        for _ in range(8):
            lanterns.advance(self.world, .25)
        self.assertEqual((member['x'], member['y'], member['hits']), (1, 2, 1))
        member.update(x=4, y=1)
        lanterns.advance(self.world, .2)
        self.assertEqual(member['hits'], 1)
        member.update(x=5, y=1)
        self.act('one', 'lanterns_walk', dx=1, dy=0)
        self.assertEqual(member['checkpoint'], 6)
        member.update(x=8, y=3)
        quest['elapsed'] = 4.2  # Second section strikes with row 2 safe.
        lanterns.advance(self.world, .1)
        self.assertEqual((member['x'], member['y'], member['hits']), (6, 2, 2))
        self.assertEqual(self.world['towns'], valley)

    def test_invalid_moves_rate_limit_and_actor_identity(self):
        quest = self.pair()
        original = copy.deepcopy(quest)
        for dx, dy in [(True, 0), (1, 1), (0, 0), (2, 0), ('1', 0), (-1, 0)]:
            with self.assertRaises(ValueError):
                self.act('one', 'lanterns_walk', dx=dx, dy=dy)
        self.assertEqual(quest, original)
        self.act('one', 'lanterns_walk', dx=1, dy=0, town_id='two')
        self.assertEqual(quest['members']['two']['x'], 1)
        with self.assertRaises(ValueError):
            self.act('one', 'lanterns_walk', dx=1, dy=0)
        with self.assertRaises(ValueError):
            self.act('three', 'lanterns_collect')

    def test_save_reload_rejoin_and_rewards_once(self):
        quest = self.pair()
        quest['members']['one'].update(x=11, checkpoint=6)
        with self.assertRaises(ValueError):
            self.act('one', 'lanterns_collect')
        self.act('two', 'leave_lanterns')
        self.world = json.loads(json.dumps(export_world(self.world)))
        self.act('two', 'join_lanterns')
        quest = self.world['lanterns']
        self.assertEqual(quest['members']['one']['checkpoint'], 6)
        quest['members']['two']['x'] = 11
        self.act('three', 'join_lanterns')
        self.act('three', 'leave_lanterns')
        coins = {k: t['coins'] for k, t in self.world['towns'].items()}
        self.act('one', 'lanterns_collect')
        for k, town in self.world['towns'].items():
            self.assertEqual(town['coins'], coins[k] + lanterns.REWARD)
            self.assertEqual(town['player']['keepsakes'], [lanterns.KEEPSAKE])
        completed = copy.deepcopy(self.world)
        with self.assertRaises(ValueError):
            self.act('one', 'lanterns_collect')
        with self.assertRaises(ValueError):
            self.act('three', 'join_lanterns')
        lanterns.advance(self.world, .25)
        self.assertEqual(self.world, completed)
        self.act('one', 'leave_lanterns')
        self.assertFalse(lanterns.inside(self.world, self.world['towns']['one']))
