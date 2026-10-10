import copy
import unittest

from server import command, new_town, tick


class EconomyRules(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Economy')
        self.world = {'towns': {'a': self.town}}

    def test_woodcutting_produces_half_a_unit_per_tick(self):
        command(self.town, {'action': 'build', 'kind': 'woodcutter', 'x': 2, 'y': 2})
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        wood = self.town['wood']
        tick(self.world)
        self.assertEqual(self.town['wood'], wood + .5)
        self.assertEqual(self.town['harvested'], .5)

    def test_assigned_workers_consume_extra_food(self):
        idle = copy.deepcopy(self.town)
        command(self.town, {'action': 'build', 'kind': 'woodcutter', 'x': 6, 'y': 6})
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        tick(self.world)
        tick({'towns': {'a': idle}})
        self.assertAlmostEqual(idle['food'] - self.town['food'], .18)
        command(self.town, {'action': 'job', 'index': 0, 'job': 'idle'})
        before = self.town['food']
        tick(self.world)
        self.assertAlmostEqual(before - self.town['food'], .6)

    def test_fishing_requires_hut_and_water_and_learns_skill(self):
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'job', 'index': 0, 'job': 'fishing'})
        command(self.town, {'action': 'build', 'kind': 'fishing_hut', 'x': 6, 'y': 2})
        command(self.town, {'action': 'job', 'index': 0, 'job': 'fishing'})
        del self.town['villagers'][0]['fishing']  # Older saves have no fishing skill.
        food = self.town['food']
        tick(self.world)
        self.assertGreater(self.town['food'], food)
        self.assertEqual(self.town['villagers'][0]['fishing'], 1)
        self.assertEqual(self.town['villagers'][0]['work_target']['kind'], 'water')
        command(self.town, {'action': 'move', 'index': 1, 'x': 6, 'y': 6})
        tick(self.world)
        self.assertIsNone(self.town['villagers'][0]['work_target'])
        self.assertEqual(self.town['villagers'][0]['fishing'], 1)
