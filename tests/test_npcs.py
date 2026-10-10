import copy
import json
import unittest

from npcs import ensure_npcs
from server import command, ensure_resources, new_town, tick


class BiomeLocals(unittest.TestCase):
    def test_locals_match_biomes_and_spawn_on_clear_tiles(self):
        town = new_town('Locals')
        occupied = {(b['x'], b['y']) for b in town['buildings'] + town['map_resources']}
        positions = set()
        for npc in town['npcs']:
            tile = npc['x'], npc['y']
            self.assertNotIn(tile, occupied | positions)
            self.assertEqual(town['terrain'][npc['y']][npc['x']], npc['biome'])
            self.assertTrue(npc['name'] and npc['role'] and npc['dialogue'])
            positions.add(tile)
        self.assertEqual({n['role'] for n in town['npcs'] if n['biome'] == 'b'},
                         {'Swamp herbalist', 'Marsh fisher'})

    def test_spawns_are_saved_deterministic_and_migrate_without_changing_town(self):
        town = new_town('Saved')
        before = copy.deepcopy(town)
        del town['npcs']
        ensure_npcs(town)
        self.assertEqual(town, before)
        saved = json.loads(json.dumps(town))
        ensure_resources(saved)
        self.assertEqual(saved, town)

    def test_locals_do_not_count_as_workers_or_consume_food(self):
        town = new_town('Quiet')
        without = copy.deepcopy(town)
        without['npcs'] = []
        tick({'towns': {'with': town, 'without': without}})
        for key in ('food', 'wood', 'coins', 'villagers'):
            self.assertEqual(town[key], without[key])

    def test_building_and_relocation_cannot_cover_a_local(self):
        town = new_town('Reserved')
        npc = town['npcs'][0]
        before = copy.deepcopy(town)
        for action in ('build', 'move'):
            with self.assertRaisesRegex(ValueError, 'local'):
                command(town, {'action': action, 'kind': 'home', 'index': 0,
                               'x': npc['x'], 'y': npc['y']})
            self.assertEqual(town, before)

    def test_fully_occupied_biome_has_no_spawn(self):
        town = {'terrain': ['b' * 14] * 14, 'buildings': [],
                'map_resources': [{'x': x, 'y': y} for x in range(14) for y in range(14)]}
        ensure_npcs(town)
        self.assertEqual(town['npcs'], [])


if __name__ == '__main__':
    unittest.main()
