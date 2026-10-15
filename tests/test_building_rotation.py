"""Saved, free building turns with authoritative footprint collision checks."""
import copy
import json
import unittest

import progression
import server
from fixtures import flat_town
from shared_world import export_world
from spatial import BUILDING_FOOTPRINTS, footprint_tiles


class BuildingRotation(unittest.TestCase):
    def setUp(self):
        self.town = flat_town(size=32)
        self.town.update(level=10, sandbox=True)

    def rotate(self, orientation, index=1, world=None):
        return server.command(self.town, dict(action='set_building_orientation',
                                              index=index, orientation=orientation), world)

    def test_every_building_turns_freely_and_four_turns_restore_its_footprint(self):
        self.town['sandbox'] = False
        for kind in {*progression.BUILDINGS, *server.COSTS, 'hall'}:
            dimensions = list(BUILDING_FOOTPRINTS.get(kind, (1, 1)))
            building = dict(kind=kind, x=12, y=12, footprint=dimensions)
            self.town['buildings'] = [self.town['buildings'][0], building]
            original = copy.deepcopy(building)
            stock = {key: self.town[key] for key in ('wood', 'food', 'coins')}
            for orientation in (1, 2, 3, 0):
                with self.subTest(kind=kind, orientation=orientation):
                    self.assertEqual(self.rotate(orientation), {})
                    self.assertEqual(building, dict(original, orientation=orientation,
                        footprint=dimensions[::-1] if orientation % 2 else dimensions))
                    self.assertEqual({key: self.town[key] for key in stock}, stock)

    def test_failed_turns_are_atomic_for_each_obstacle(self):
        for obstacle in ('building', 'road', 'tree', 'water', 'rock', 'npc', 'player',
                         'boundary', 'edge', 'range', 'barn'):
            with self.subTest(obstacle=obstacle):
                self.setUp()
                building = dict(kind='sawpit', x=12, y=12, footprint=[3, 2])
                self.town['buildings'].append(building)
                x, y = 12, 14  # Newly occupied by the 2-by-3 orientation.
                if obstacle == 'building':
                    self.town['buildings'].append(dict(kind='home', x=x, y=y))
                elif obstacle == 'road':
                    self.town.setdefault('roads', []).append(dict(x=x, y=y))
                elif obstacle in ('tree', 'water', 'rock'):
                    self.town['map_resources'].append(dict(kind=obstacle, x=x, y=y))
                elif obstacle == 'npc':
                    self.town['npcs'].append(dict(x=x, y=y))
                elif obstacle == 'player':
                    self.town['player'].update(x=x, y=y)
                elif obstacle == 'boundary':
                    self.town['terrain'][y] = 'g' * x + 'v' + 'g' * (31-x)
                elif obstacle == 'edge':
                    building['y'] = 30
                elif obstacle == 'range':
                    self.town['level'] = 1
                    building.update(x=5, y=34)
                    self.town['map_size'] = 40
                    self.town['terrain'] = ['g' * 40] * 40
                elif obstacle == 'barn':
                    self.town['buildings'].append(dict(kind='barn', x=12, y=17))
                before = copy.deepcopy(building)
                with self.assertRaises(ValueError):
                    self.rotate(1)
                self.assertEqual(building, before)

    def test_other_towns_buildings_and_players_block_the_added_tiles(self):
        for obstacle in ('building', 'player'):
            with self.subTest(obstacle=obstacle):
                self.setUp()
                self.town['buildings'].append(dict(kind='sawpit', x=12, y=12, footprint=[3, 2]))
                friend = flat_town('Friend', size=32)
                if obstacle == 'building':
                    friend['buildings'].append(dict(kind='home', x=12, y=14))
                else:
                    friend['player'].update(x=12, y=14)
                world = dict(shared_map={}, towns=dict(own=self.town, friend=friend))
                with self.assertRaises(ValueError):
                    self.rotate(1, world=world)
                self.assertEqual(self.town['buildings'][1]['footprint'], [3, 2])

    def test_turn_persists_through_save_reload_and_move_and_releases_old_tiles(self):
        self.town['buildings'].append(dict(kind='sawpit', x=12, y=12, footprint=[3, 2]))
        self.rotate(3)
        self.assertNotIn((14, 12), footprint_tiles(self.town['buildings'][1]))
        self.assertIn((12, 14), footprint_tiles(self.town['buildings'][1]))
        self.town = json.loads(json.dumps(export_world(dict(towns=dict(own=self.town)))))['towns']['own']
        server.command(self.town, dict(action='move', index=1, x=18, y=18))
        self.assertEqual(self.town['buildings'][1], dict(kind='sawpit', x=18, y=18,
                                                       footprint=[2, 3], orientation=3))
        server.migrate_footprints(dict(towns=dict(own=self.town)))
        self.assertEqual(self.town['buildings'][1]['footprint'], [2, 3])

    def test_blocked_move_uses_rotated_dimensions_and_preserves_position(self):
        self.town['buildings'].append(dict(kind='sawpit', x=12, y=12, footprint=[3, 2]))
        self.rotate(1)
        self.town['map_resources'].append(dict(kind='rock', x=18, y=20))
        before = copy.deepcopy(self.town['buildings'][1])
        with self.assertRaises(ValueError):
            server.command(self.town, dict(action='move', index=1, x=18, y=18))
        self.assertEqual(self.town['buildings'][1], before)

    def test_rotation_clears_only_forage_inside_the_new_footprint(self):
        self.town['buildings'].append(dict(kind='sawpit', x=12, y=12, footprint=[3, 2]))
        inside = dict(kind='herb_patch', x=12, y=14)
        outside = dict(kind='berry_bush', x=14, y=14)
        self.town['map_resources'] = [inside, outside]
        self.rotate(1)
        self.assertEqual(self.town['map_resources'], [outside])

    def test_legacy_compact_sites_turn_without_expanding(self):
        self.town['buildings'].append(dict(kind='sawpit', x=12, y=12, footprint=[1, 1]))
        self.rotate(1)
        self.assertEqual(self.town['buildings'][1]['footprint'], [1, 1])
        server.command(self.town, dict(action='move', index=1, x=18, y=18))
        self.assertEqual(self.town['buildings'][1]['footprint'], [2, 3])

    def test_migration_of_legacy_sites_uses_the_saved_direction(self):
        self.town['buildings'].append(dict(kind='sawpit', x=12, y=12))
        self.rotate(1)
        server.migrate_footprints(dict(towns=dict(own=self.town)))
        self.assertEqual(self.town['buildings'][1]['footprint'], [2, 3])

    def test_invalid_input_and_construction_cannot_change_orientation(self):
        self.town['buildings'].append(dict(kind='home', x=12, y=12))
        for value in (-1, 4, True, 1.0, '1', None):
            with self.subTest(orientation=value), self.assertRaises(ValueError):
                self.rotate(value)
        for value in (-1, 2, True, 1.0, '1', None):
            with self.subTest(index=value), self.assertRaises(ValueError):
                self.rotate(1, index=value)
        self.town['buildings'][1]['construction'] = dict(progress=0, total=30)
        with self.assertRaisesRegex(ValueError, 'completed building'):
            self.rotate(1)
        self.assertNotIn('orientation', self.town['buildings'][1])


if __name__ == '__main__':
    unittest.main()
