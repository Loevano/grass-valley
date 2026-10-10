import copy
import unittest

from server import capacity, command, new_town, tick


class VillageRules(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Willowbrook')

    def test_build_and_move_preserve_building(self):
        command(self.town, {'action': 'build', 'kind': 'farm', 'x': 2, 'y': 3})
        self.assertEqual(self.town['wood'], 50)
        command(self.town, {'action': 'move', 'index': 1, 'x': 4, 'y': 4})
        self.assertEqual(self.town['buildings'][1], {'kind': 'farm', 'x': 4, 'y': 4})
        self.assertEqual(self.town['coins'], 19)

    def test_rejected_build_does_not_spend_resources(self):
        for x, y in [(5, 5), (-1, 3), (12, 2)]:
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                command(self.town, {'action': 'build', 'kind': 'home', 'x': x, 'y': y})
            self.assertEqual(before, self.town)

    def test_moving_remains_available_without_coins(self):
        self.town['coins'] = 0
        command(self.town, {'action': 'move', 'index': 0, 'x': 1, 'y': 1})
        self.assertEqual(self.town['coins'], 0)
        self.assertEqual(self.town['buildings'][0]['x'], 1)

    def test_job_requires_matching_building(self):
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        command(self.town, {'action': 'build', 'kind': 'woodcutter', 'x': 2, 'y': 2})
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        wood, skill = self.town['wood'], self.town['villagers'][0]['woodcutting']
        tick({'towns': {'absent-owner': self.town}})
        self.assertGreater(self.town['wood'], wood)
        self.assertEqual(self.town['villagers'][0]['woodcutting'], skill + 1)

    def test_full_storage_pauses_production_and_skill(self):
        command(self.town, {'action': 'build', 'kind': 'woodcutter', 'x': 1, 'y': 1})
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        self.town['wood'] = capacity(self.town)
        skill = self.town['villagers'][0]['woodcutting']
        tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['wood'], capacity(self.town))
        self.assertEqual(self.town['villagers'][0]['woodcutting'], skill)

    def test_food_shortage_pauses_growth_without_losing_villagers(self):
        command(self.town, {'action': 'build', 'kind': 'home', 'x': 1, 'y': 1})
        self.town['food'] = 0
        tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 5)
        self.town['food'] = 40
        tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 6)

    def test_upgrade_requires_milestone_and_explicit_choice(self):
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'upgrade'})
        self.town['harvested'] = 30
        tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['level'], 1)
        command(self.town, {'action': 'upgrade'})
        self.assertEqual(self.town['level'], 2)
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'upgrade'})


if __name__ == '__main__':
    unittest.main()
