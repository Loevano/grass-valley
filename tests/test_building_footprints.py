"""Full placement, saved layouts and outdoor access for larger workplaces."""
import copy
import unittest
from unittest.mock import patch

from fixtures import flat_town
from navigation import RouteCache
from spatial import footprint_tiles, occupied_tiles, perimeter, SpatialIndex
from server import command, construction_route, migrate_footprints, walkable
from shared_world import bind_map, export_world
import work_cycles
import horses
import npcs
import village_life


class BuildingFootprints(unittest.TestCase):
    def setUp(self):
        self.town = flat_town(size=32)
        self.town.update(level=10, sandbox=True, player={'x': 3., 'y': 9., 'facing': 1})

    def build(self, kind='mill', x=10, y=10):
        command(self.town, {'action': 'build', 'kind': kind, 'x': x, 'y': y})
        return self.town['buildings'][-1]

    def test_dimensions_and_collision_survive_json_save(self):
        import json
        for kind, count in [('mill', 4), ('sawpit', 6), ('workshop', 4)]:
            building = self.build(kind)
            self.assertEqual(len(footprint_tiles(building)), count)
            saved = json.loads(json.dumps(building))
            self.assertEqual(footprint_tiles(saved), footprint_tiles(building))
            for x, y in footprint_tiles(building):
                self.assertFalse(walkable(self.town, x, y))
                with self.assertRaisesRegex(ValueError, 'open ground'):
                    command(self.town, {'action': 'build_road', 'x': x, 'y': y})
            self.town['buildings'].pop()
        self.assertEqual(len(footprint_tiles(self.town['buildings'][0])), 4)

    def test_placement_clears_only_forage_inside_the_full_footprint(self):
        for kind, resource in ((k, r) for k in ('home', 'mill', 'acre')
                               for r in ('berry_bush', 'herb_patch')):
            with self.subTest(kind=kind, resource=resource):
                town = copy.deepcopy(self.town)
                bushes = [{'kind': resource, 'x': x, 'y': y}
                          for x, y in ((10, 10), (11, 11), (12, 10))]
                town['map_resources'] = bushes.copy()
                if kind == 'acre':
                    command(town, dict(action='build', kind='farm', x=9, y=10))
                command(town, dict(action='build', kind=kind, x=10, y=10, farm_index=1))
                covered = footprint_tiles(town['buildings'][-1])
                self.assertEqual(town['map_resources'],
                                 [b for b in bushes if (b['x'], b['y']) not in covered])
                command(town, dict(action='destroy', index=len(town['buildings']) - 1))
                self.assertNotIn(bushes[0], town['map_resources'])

    def test_failed_build_keeps_forage_and_materials(self):
        self.town['map_resources'] = [{'kind': 'berry_bush', 'x': 10, 'y': 10},
                                      {'kind': 'herb_patch', 'x': 11, 'y': 10}]
        for obstacle in ('rock', 'tree', 'water', 'chest', 'wild_horse', 'materials'):
            with self.subTest(obstacle=obstacle):
                town = copy.deepcopy(self.town)
                if obstacle == 'materials':
                    town.update(sandbox=False, wood=0)
                else:
                    town['map_resources'].append({'kind': obstacle, 'x': 11, 'y': 11})
                before = copy.deepcopy(town)
                with self.assertRaises(ValueError):
                    command(town, dict(action='build', kind='mill', x=10, y=10))
                self.assertEqual(town, before)

    def test_shared_map_move_removes_forage_for_every_town_and_save(self):
        self.build()
        friend = flat_town('Friend', size=32)
        friend['buildings'] = []
        landscape = {key: self.town[key] for key in ('map_size', 'terrain', 'npcs')}
        landscape['map_resources'] = [{'kind': 'herb_patch', 'x': 13, 'y': 12},
                                      {'kind': 'berry_bush', 'x': 14, 'y': 13},
                                      {'kind': 'berry_bush', 'x': 15, 'y': 13}]
        world = dict(shared_map=landscape, towns={'own': self.town, 'friend': friend})
        bind_map(world)
        command(self.town, dict(action='move', index=1, x=13, y=12), world)
        self.assertIs(self.town['map_resources'], landscape['map_resources'])
        self.assertIs(friend['map_resources'], landscape['map_resources'])
        self.assertEqual(landscape['map_resources'], [{'kind': 'berry_bush', 'x': 15, 'y': 13}])
        self.assertEqual(export_world(world)['shared_map']['map_resources'], landscape['map_resources'])

    def test_linked_farm_move_clears_forage_only_after_all_destinations_pass(self):
        farm = self.build('farm')
        command(self.town, dict(action='build', kind='acre', farm_index=1, x=11, y=10))
        self.town['map_resources'] = [{'kind': 'berry_bush', 'x': 12, 'y': 10},
                                      {'kind': 'rock', 'x': 13, 'y': 10}]
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            command(self.town, dict(action='move', index=1, x=12, y=10))
        self.assertEqual(self.town, before)
        self.town['map_resources'][1]['kind'] = 'herb_patch'
        command(self.town, dict(action='move', index=1, x=12, y=10))
        self.assertEqual((farm['x'], farm['y']), (12, 10))
        self.assertEqual(self.town['map_resources'], [])

    def test_barn_range_rejects_builds_and_moves_without_changing_state(self):
        barn = self.build('barn', 16, 16)
        home = self.build('home', 22, 16)
        for data in [dict(action='build', kind='home', x=19, y=19),
                     dict(action='build', kind='mill', x=12, y=16),
                     dict(action='move', index=2, x=13, y=13),
                     dict(action='build', kind='barn', x=23, y=19),
                     dict(action='move', index=1, x=20, y=16)]:
            with self.subTest(data=data):
                before = copy.deepcopy(self.town)
                with self.assertRaisesRegex(ValueError, 'barn|fenced'):
                    command(self.town, data)
                self.assertEqual(self.town, before)
        self.build('home', 20, 20)
        command(self.town, dict(action='move', index=1, x=15, y=16))
        self.assertEqual((barn['x'], barn['y']), (15, 16))
        self.assertEqual((home['x'], home['y']), (22, 16))
        # Moving the pen releases its old far edge, while construction already reserves it.
        self.build('home', 19, 19)

    def test_other_town_barn_range_and_new_pen_stock_overlap_are_reserved(self):
        friend = flat_town('Friend', size=32)
        friend['buildings'] = [{'kind': 'barn', 'x': 16, 'y': 16}]
        world = {'shared_map': {}, 'towns': {'own': self.town, 'friend': friend}}
        for data in [dict(action='build', kind='home', x=19, y=19),
                     dict(action='build', kind='mill', x=12, y=16)]:
            with self.assertRaisesRegex(ValueError, 'fenced'):
                command(self.town, data, world)
        friend['buildings'] = [{'kind': 'mill', 'x': 19, 'y': 19, 'footprint': [2, 2]}]
        with self.assertRaisesRegex(ValueError, 'barn range'):
            command(self.town, dict(action='build', kind='barn', x=22, y=22), world)

    def test_secondary_tile_obstacles_reject_build_without_spending(self):
        for obstacle in ('resource', 'building', 'road', 'npc', 'player', 'void'):
            with self.subTest(obstacle=obstacle):
                town = copy.deepcopy(self.town)
                if obstacle == 'resource':
                    town['map_resources'] = [{'kind': 'tree', 'x': 11, 'y': 11}]
                elif obstacle == 'building':
                    town['buildings'].append({'kind': 'home', 'x': 11, 'y': 11})
                elif obstacle == 'road':
                    town['roads'] = [{'x': 11, 'y': 11}]
                elif obstacle == 'npc':
                    town['npcs'] = [{'kind': 'npc', 'x': 11, 'y': 11}]
                elif obstacle == 'player':
                    town['player'].update(x=11., y=11.)
                else:
                    town['terrain'][11] = 'g' * 11 + 'v' + 'g' * 20
                before = copy.deepcopy(town)
                with self.assertRaises(ValueError):
                    command(town, {'action': 'build', 'kind': 'mill', 'x': 10, 'y': 10})
                self.assertEqual(town, before)

    def test_bounds_and_range_apply_to_every_tile(self):
        with self.assertRaisesRegex(ValueError, 'full building footprint'):
            self.build(x=31, y=10)
        self.town.update(level=4)
        self.town['buildings'][0].update(x=1, y=1)
        with self.assertRaisesRegex(ValueError, 'within 45'):
            self.build(x=25, y=22)

    def test_move_validates_full_destination_and_allows_old_overlap(self):
        building = self.build()
        self.town['map_resources'] = [{'kind': 'rock', 'x': 14, 'y': 13}]
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'move', 'index': 1, 'x': 13, 'y': 12})
        self.assertEqual(self.town, before)
        command(self.town, {'action': 'move', 'index': 1, 'x': 11, 'y': 10})
        self.assertEqual(building['footprint'], [2, 2])
        self.assertEqual(footprint_tiles(building), {(11, 10), (12, 10), (11, 11), (12, 11)})

    def test_access_uses_open_far_edge_for_workers_builders_and_haulers(self):
        building = self.build()
        # The only approach is the south edge, away from the anchor's neighbors.
        blocked = occupied_tiles(self.town['buildings']) | (perimeter(building) - {(11, 12)})
        cache = RouteCache()
        path = cache.route((11, 14), building, blocked, 32, self.town['terrain'])
        self.assertEqual(path[-1], {'x': 11, 'y': 12})
        self.assertFalse(occupied_tiles([building]).intersection((p['x'], p['y']) for p in path))
        self.town['map_resources'] = [{'kind': 'rock', 'x': x, 'y': y}
                                     for x, y in perimeter(building) - {(11, 12)}]
        self.assertEqual(construction_route(self.town, (11, 14), building)[-1], (11, 12))
        self.assertEqual(work_cycles.route({'x': 11, 'y': 14}, building, blocked, 32,
                                          self.town['terrain'])[-1], {'x': 11, 'y': 12})
        index = SpatialIndex(self.town['map_resources'])
        self.assertTrue(footprint_tiles(building).issubset(index.blocked(self.town)))
        building['footprint'] = [3, 2]
        self.assertIn((12, 11), index.blocked(self.town), 'Dimensions invalidate spatial caches')

    def test_other_town_footprints_and_players_block_secondary_tiles(self):
        friend = flat_town('Friend', size=32)
        friend['buildings'] = [{'kind': 'mill', 'x': 11, 'y': 11, 'footprint': [2, 2]}]
        world = {'shared_map': {}, 'towns': {'own': self.town, 'friend': friend}}
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'Another town'):
            command(self.town, {'action': 'build', 'kind': 'mill', 'x': 10, 'y': 10}, world)
        self.assertEqual(self.town, before)
        friend['buildings'] = []
        friend['player'].update(x=11, y=11)
        with self.assertRaisesRegex(ValueError, 'player'):
            command(self.town, {'action': 'build', 'kind': 'mill', 'x': 10, 'y': 10}, world)

    def test_saved_buildings_expand_only_where_clear_and_move_to_new_dimensions(self):
        self.town['buildings'].extend([{'kind': 'mill', 'x': 10, 'y': 10},
                                      {'kind': 'sawpit', 'x': 16, 'y': 10},
                                      {'kind': 'home', 'x': 17, 'y': 11}])
        world = {'towns': {'own': self.town}}
        migrate_footprints(world)
        self.assertEqual(self.town['buildings'][1]['footprint'], [2, 2])
        self.assertEqual(self.town['buildings'][2]['footprint'], [1, 1])
        before = copy.deepcopy(world)
        migrate_footprints(world)
        self.assertEqual(world, before)
        command(self.town, {'action': 'move', 'index': 2, 'x': 20, 'y': 10})
        self.assertEqual(self.town['buildings'][2]['footprint'], [3, 2])

    def test_ambient_movement_and_local_placement_respect_stock_tiles(self):
        building = self.build(x=12, y=12)
        horse = {'kind': 'wild_horse', 'x': 14, 'y': 13, 'wander_remaining': 0}
        landscape = dict(self.town, map_resources=[horse, {'kind': 'rock', 'x': 15, 'y': 13},
                                                  {'kind': 'rock', 'x': 14, 'y': 12}])
        with patch('horses.random.choice', side_effect=lambda choices: choices[0]):
            horses.wander(landscape, [self.town], 3)
        self.assertEqual((horse['x'], horse['y']), (14, 14))
        village = {'x': 12, 'y': 12, 'name': 'Test village', 'layout_version': 2}
        world = {'towns': {'own': self.town}, 'shared_map': landscape}
        self.assertTrue(footprint_tiles(building).issubset(village_life.blocked_tiles(world, village)))
        # The sole grass tile is a stock tile, so it cannot become a local's home.
        rows = ['v' * 32 for _ in range(32)]
        rows[13] = 'v' * 13 + 'g' + 'v' * 18
        town = dict(self.town, terrain=rows, npcs=[])
        npcs.ensure_npcs(town)
        self.assertFalse(town['npcs'])


if __name__ == '__main__':
    unittest.main()
