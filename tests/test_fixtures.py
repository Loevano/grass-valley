import json
import unittest

from fixtures import flat_town
import server


class TownFixtures(unittest.TestCase):
    def test_gameplay_mutations_do_not_leak_between_towns(self):
        first, second = flat_town('First'), flat_town('Second')
        before = json.dumps(second, sort_keys=True)
        first['buildings'][0]['stored_goods'] = {'wood': 1}
        first['villagers'][0]['talents']['woodcutting'] = 9
        first['map_resources'].append({'kind': 'tree', 'x': 3, 'y': 3})
        server.tick({'towns': {'first': first}})
        self.assertEqual(json.dumps(second, sort_keys=True), before)
        self.assertEqual(flat_town('Third')['map_resources'], [])

    def test_fixture_has_valid_ground_and_real_town_defaults(self):
        town = flat_town(size=20)
        self.assertEqual(town['terrain'], ['g' * 20] * 20)
        self.assertEqual(town['map_size'], 20)
        self.assertEqual(len(town['villagers']), 5)
        self.assertEqual(town['wood'], 65)
        self.assertEqual(town['food'], 40)
        self.assertEqual(town['npcs'], [])
