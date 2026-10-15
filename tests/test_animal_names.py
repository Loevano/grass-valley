import copy
import json
from pathlib import Path
import tempfile
from unittest.mock import patch
import unittest

from fixtures import flat_town
import server


class AnimalNames(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Named animals')
        self.town['level'] = 5
        self.barn = {'kind': 'barn', 'x': 3, 'y': 6,
                     'animals': {'chicken': 2, 'sheep': 1},
                     'animal_growth': {'chicken': [10, 60], 'sheep': [40]}}
        self.shop = {'kind': 'butcher', 'x': 8, 'y': 6}
        self.town['buildings'].extend([self.barn, self.shop])
        self.town['villagers'][0].update(job='butchery', building_index=2, husbandry=2)

    def count(self, animal, count):
        server.command(self.town, {'action': 'set_animal_count', 'index': 1,
                                  'animal': animal, 'count': count})

    def test_existing_animals_receive_unique_names_without_changing_growth(self):
        before = copy.deepcopy(self.barn)
        self.town['buildings'].append({'kind': 'barn', 'x': 15, 'y': 6,
                                      'animals': {'pig': 40},
                                      'animal_names': {'pig': ['Poppy']}})
        server.ensure_animal_names(self.town)
        names = [name for barn in self.town['buildings'][1:] if barn['kind'] == 'barn'
                 for names in barn['animal_names'].values() for name in names]
        self.assertEqual(len(names), 43)
        self.assertEqual(len(set(names)), 43)
        self.assertEqual(self.town['buildings'][-1]['animal_names']['pig'][0], 'Poppy')
        self.assertEqual(self.barn['animals'], before['animals'])
        self.assertEqual(self.barn['animal_growth'], before['animal_growth'])
        saved = json.loads(json.dumps(self.town))
        server.ensure_animal_names(saved)
        self.assertEqual(saved, self.town)

    def test_loading_old_worlds_assigns_names_and_saving_preserves_them(self):
        world = {'name': 'World', 'tokens': {}, 'world': {'towns': {'own': self.town}}}
        with tempfile.TemporaryDirectory() as directory:
            save = Path(directory) / 'savegame.json'
            save.write_text(json.dumps({'worlds': {'active': world, 'other': world},
                                        'active_world': 'active'}))
            with patch.object(server, 'SAVE', save), patch.multiple(
                    server, WORLDS={}, WORLD={}, TOKENS={}, ACTIVE_WORLD=None):
                server.load()
                names = [entry['world']['towns']['own']['buildings'][1]['animal_names']
                         for entry in server.WORLDS.values()]
                self.assertEqual(len(names), 2)
                self.assertTrue(all(name for herd in names for group in herd.values() for name in group))
                server.save()
                server.load()
                self.assertEqual(names, [entry['world']['towns']['own']['buildings'][1]['animal_names']
                                         for entry in server.WORLDS.values()])

    def test_resizing_preserves_remaining_names_and_new_animals_get_fresh_names(self):
        server.ensure_animal_names(self.town)
        original = self.barn['animal_names']['chicken'][:]
        self.count('chicken', 1)
        self.assertEqual(self.barn['animal_names']['chicken'], original[:1])
        self.count('chicken', 3)
        names = self.barn['animal_names']['chicken']
        self.assertEqual(names[0], original[0])
        self.assertNotIn(original[1], names)
        self.assertEqual(self.barn['animal_growth']['chicken'], [10, 0, 0])
        self.count('chicken', 0)
        self.assertEqual(self.barn['animal_names']['chicken'], [])

    def test_butchery_preserves_survivor_names_and_names_the_last_replacement(self):
        server.ensure_animal_names(self.town)
        original = self.barn['animal_names']['chicken'][:]
        action = {'action': 'butcher_animal', 'index': 2, 'barn_index': 1, 'animal': 'chicken'}
        server.command(self.town, action)
        self.assertEqual(self.barn['animal_names']['chicken'], original[:1])
        self.assertEqual(self.barn['animal_growth']['chicken'], [10])
        self.assertEqual(self.shop['butchering']['name'], original[1])
        self.shop.pop('butchering')
        self.barn['animal_growth']['chicken'] = [60]
        server.command(self.town, action)
        replacement = self.barn['animal_names']['chicken']
        self.assertEqual(len(replacement), 1)
        self.assertNotIn(replacement[0], original)
        self.assertEqual(self.barn['animal_growth']['chicken'], [0])
        self.assertEqual(self.shop['butchering']['name'], original[0])


if __name__ == '__main__':
    unittest.main()
