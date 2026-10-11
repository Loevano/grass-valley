import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import server


class VillagerNames(unittest.TestCase):
    def test_arrivals_have_unique_names_up_to_population_limit(self):
        town = server.new_town('Growing town')
        town['food'] = 1000
        town['buildings'].extend({'kind': 'home', 'x': x, 'y': 7}
                                 for x in range(6))
        for _ in range(11):
            server.tick({'towns': {'a': town}})
        names = [v['name'] for v in town['villagers']]
        self.assertEqual(len(names), 16)
        self.assertEqual(len(set(names)), 16)
        self.assertTrue(all(name in server.VILLAGER_NAMES for name in names))

    def test_migration_preserves_names_and_villager_data_and_is_stable(self):
        town = {'villagers': [{'name': 'Finn', 'job': 'farming', 'farming': 25},
                             {'name': 'Settler 6', 'job': 'idle'},
                             {'name': 'Settler 7', 'job': 'fishing'},
                             {'job': 'idle'}, {'name': ' ', 'job': 'idle'}]}
        original = copy.deepcopy(town)
        server.ensure_villager_names(town)
        names = [v['name'] for v in town['villagers']]
        self.assertEqual(names[0], 'Finn')
        self.assertEqual(len(set(names)), 5)
        self.assertTrue(all(name in server.VILLAGER_NAMES for name in names))
        for before, after in zip(original['villagers'], town['villagers']):
            self.assertEqual({k: v for k, v in before.items() if k != 'name'},
                             {k: v for k, v in after.items() if k != 'name'})
        migrated = copy.deepcopy(town)
        server.ensure_villager_names(town)
        self.assertEqual(town, migrated)

    def test_load_migrates_active_and_inactive_worlds_and_saves_names(self):
        entry = {'name': 'World', 'tokens': {}, 'world': {'towns': {
            'a': {'villagers': [{'name': 'Settler 6', 'job': 'idle'}]}}}}
        with tempfile.TemporaryDirectory() as directory:
            save = Path(directory) / 'savegame.json'
            save.write_text(json.dumps({'worlds': {'active': entry, 'other': entry},
                                        'active_world': 'active'}))
            with patch.object(server, 'SAVE', save), patch.multiple(
                    server, WORLDS={}, WORLD={}, TOKENS={}, ACTIVE_WORLD=None):
                server.load()
                names = [e['world']['towns']['a']['villagers'][0]['name']
                         for e in server.WORLDS.values()]
                self.assertTrue(all(name in server.VILLAGER_NAMES for name in names))
                server.save()
                server.load()
                self.assertEqual(names, [e['world']['towns']['a']['villagers'][0]['name']
                                         for e in server.WORLDS.values()])


if __name__ == '__main__':
    unittest.main()
