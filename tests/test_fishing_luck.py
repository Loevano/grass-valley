import unittest
from unittest.mock import patch

from server import WORKER_FOOD, capacity, new_town, tick


class FishingLuck(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Fishing luck')
        # Completed workplaces also cover saved towns from before construction.
        self.town['buildings'].append({'kind': 'fishing_hut', 'x': 6, 'y': 2})
        self.fisher = self.town['villagers'][0]
        self.fisher['job'] = 'fishing'
        self.world = {'towns': {'a': self.town}}
        self.upkeep = len(self.town['villagers']) * .12 + WORKER_FOOD

    def test_success_and_failure_have_different_food_rewards(self):
        food = self.town['food']
        with patch('server.secrets.randbelow', side_effect=[1, 0]) as roll:
            tick(self.world)
            self.assertAlmostEqual(self.town['food'], food + 1 - self.upkeep)
            self.assertEqual(self.fisher['fishing_catch'], {'attempt': 1, 'caught': True})
            food = self.town['food']
            tick(self.world)
            self.assertAlmostEqual(self.town['food'], food - self.upkeep)
            self.assertEqual(self.fisher['fishing_catch'], {'attempt': 2, 'caught': False})
            self.assertEqual(roll.call_count, 2)
            roll.assert_called_with(5)
        self.assertEqual(self.fisher['fishing'], 2, 'Practice earns skill even without a catch')

    def test_every_successful_roll_and_master_reward(self):
        self.fisher['fishing'] = 20
        for result in [1, 2, 3, 4]:
            with self.subTest(result=result), patch('server.secrets.randbelow', return_value=result):
                food = self.town['food']
                tick(self.world)
                self.assertAlmostEqual(self.town['food'], food + 2 - self.upkeep)
                self.assertTrue(self.fisher['fishing_catch']['caught'])

    def test_full_storage_and_missing_workplace_do_not_roll(self):
        with patch('server.secrets.randbelow') as roll:
            self.town['food'] = capacity(self.town)
            tick(self.world)
            roll.assert_not_called()
            self.assertNotIn('fishing_catch', self.fisher)
            self.town['buildings'].pop()
            tick(self.world)
            roll.assert_not_called()
            self.assertNotIn('fishing_catch', self.fisher)
