import copy
import unittest

import progression as economy


class HallWaterRequirement(unittest.TestCase):
    def setUp(self):
        self.town = {
            'food': 20,
            'buildings': [
                {'kind': 'home', 'x': 5, 'y': 5},
                {'kind': 'farm', 'x': 20, 'y': 20},
            ],
            'map_resources': [],
        }

    def test_water_range_is_ten_tile_steps_from_the_food_building(self):
        for kind in ('farm', 'fishing_hut'):
            for source in ('water', 'well'):
                for dx, dy, ready in ((8, 0, True), (6, 4, True),
                                      (6, 5, False), (-6, -4, True)):
                    with self.subTest(kind=kind, source=source, dx=dx, dy=dy):
                        town = copy.deepcopy(self.town)
                        town['buildings'][1]['kind'] = kind
                        target = town['buildings'] if source == 'well' else town['map_resources']
                        target.append({'kind': source, 'x': 20 + dx, 'y': 20 + dy})
                        self.assertEqual(economy.milestone(town, 2), ready)

    def test_unfinished_well_does_not_count(self):
        self.town['buildings'].append({
            'kind': 'well', 'x': 24, 'y': 24,
            'construction': {'progress': 0, 'total': 15},
        })
        self.assertFalse(economy.milestone(self.town, 2))
        del self.town['buildings'][-1]['construction']
        self.assertTrue(economy.milestone(self.town, 2))

    def test_existing_buildings_still_count_during_upgrades(self):
        self.town['buildings'].append({'kind': 'well', 'x': 26, 'y': 24})
        for kind in ('farm', 'fishing_hut'):
            for upgrade in ({'level': 2}, {'settler_level': 2}):
                with self.subTest(kind=kind, upgrade=upgrade):
                    town = copy.deepcopy(self.town)
                    town['buildings'][1]['kind'] = kind
                    for building in town['buildings'][:2]:
                        building['construction'] = {
                            'progress': 0, 'total': 30, 'upgrade': upgrade,
                        }
                    self.assertTrue(economy.milestone(town, 2))

    def test_housing_finished_workplace_and_food_are_still_required(self):
        self.town['map_resources'] = [{'kind': 'water', 'x': 26, 'y': 24}]
        for index in (0, 1):
            with self.subTest(unfinished_building=index):
                town = copy.deepcopy(self.town)
                town['buildings'][index]['construction'] = {'progress': 0, 'total': 15}
                self.assertFalse(economy.milestone(town, 2))
        self.town['food'] = 19
        self.assertFalse(economy.milestone(self.town, 2))

    def test_water_source_is_required(self):
        self.assertFalse(economy.milestone(self.town, 2))
        self.town['map_resources'] = [{'kind': 'tree', 'x': 21, 'y': 20}]
        self.assertFalse(economy.milestone(self.town, 2))

    def test_jenstown_layout_qualifies_with_its_well(self):
        self.town['buildings'] = [
            {'kind': 'hall', 'x': 257, 'y': 251},
            {'kind': 'home', 'x': 257, 'y': 247},
            {'kind': 'farm', 'x': 236, 'y': 248},
            {'kind': 'well', 'x': 240, 'y': 252},
        ]
        self.town['map_resources'] = [{'kind': 'water', 'x': 261, 'y': 243}]
        self.assertTrue(economy.milestone(self.town, 2))
