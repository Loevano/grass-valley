import json
import unittest

from server import (command, construct, new_town, resource_reach,
                    settler_building, work_assignment)
from spatial import SpatialIndex


class FishingRange(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Fishing range')
        self.hut = {'kind': 'fishing_hut', 'x': 6, 'y': 2}
        self.town['buildings'].append(self.hut)
        self.worker = self.town['villagers'][0]
        self.worker['job'] = 'fishing'

    def test_levels_and_boat_reach_boundaries(self):
        for level in (1, 2, 3):
            for boat in (False, True):
                with self.subTest(level=level, boat=boat):
                    self.hut.update(level=level, profession_upgrades=['boat'] if boat else [])
                    reach = 2 + level + (3 if boat else 0)
                    self.assertEqual(resource_reach(self.hut), reach)
                    for distance in (reach, reach + 1):
                        water = {'kind': 'water', 'x': 6 + distance - 1, 'y': 3}
                        self.town['map_resources'] = [water]
                        self.assertEqual(settler_building(self.town, 0), 1 if distance == reach else None)
                        for explicit in (False, True):
                            if explicit:
                                self.worker['building_index'] = 1
                            else:
                                self.worker.pop('building_index', None)
                            for spatial in (None, SpatialIndex([water])):
                                assignment = work_assignment(self.town, 'fishing', 0, spatial)
                                self.assertEqual(assignment, (self.hut, water) if distance == reach else None)
                        self.worker.pop('building_index', None)

    def test_range_increases_on_upgrade_completion_and_survives_save(self):
        self.town['map_resources'] = [{'kind': 'water', 'x': 10, 'y': 2}]
        command(self.town, {'action': 'upgrade_building', 'index': 1})
        self.assertEqual(resource_reach(self.hut), 3)
        self.assertIsNone(work_assignment(self.town, 'fishing', 0))
        for _ in range(100):
            if 'construction' not in self.hut:
                break
            construct(self.town, self.town)
        self.assertNotIn('construction', self.hut)
        self.assertEqual(resource_reach(self.hut), 4)
        self.town = json.loads(json.dumps(self.town))
        self.assertIsNotNone(work_assignment(self.town, 'fishing', 0))


if __name__ == '__main__':
    unittest.main()
