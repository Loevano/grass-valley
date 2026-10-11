import copy
import json
import unittest

import progression as economy

from server import (SETTLER_SPACES, assigned_settlers, command, new_town,
                    construct, settler_capacity, tick)


class BuildingSettlers(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Settlers')
        self.town['buildings'].extend([
            {'kind': 'farm', 'x': 6, 'y': 2},
            {'kind': 'farm', 'x': 7, 'y': 3},
            {'kind': 'acre', 'x': 6, 'y': 3, 'farm_index': 1},
            {'kind': 'home', 'x': 2, 'y': 2},
        ])

    def assign(self, settler, building):
        command(self.town, {'action': 'assign_settler', 'index': settler,
                            'building_index': building})

    def test_every_building_supports_assignment_with_its_own_capacity(self):
        for kind, spaces in SETTLER_SPACES.items():
            if kind == 'hall':
                continue
            with self.subTest(kind=kind):
                self.town['buildings'].append({'kind': kind, 'x': 4, 'y': 4})
                index = len(self.town['buildings']) - 1
                job = economy.BUILDINGS.get(kind, {}).get('job')
                self.town['villagers'][0].update(economy.JOB_REQUIREMENTS.get(job, {}))
                self.assign(0, index)
                self.assertEqual(assigned_settlers(self.town, index), [0])
                self.assertEqual(settler_capacity(self.town['buildings'][index]), spaces)
                self.assertEqual(len(self.town['villagers']), 5)

    def test_full_building_rejects_both_direct_and_roster_assignments(self):
        self.assign(0, 4)
        self.assign(2, 4)
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            self.assign(1, 4)
        self.assertEqual(before, self.town)
        for i in range(4):
            self.assign(i, 1)
        self.assign(4, 2)
        self.town['buildings'][2]['settler_level'] = 1
        # Fill the other farm as well using a larger roster.
        for i in range(5, 8):
            self.town['villagers'].append(dict(self.town['villagers'][4], name=str(i)))
        self.town['villagers'].append({'name': 'Waiting', 'job': 'idle'})
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'job', 'index': 8, 'job': 'farming'})
        self.assertEqual(before, self.town)

    def test_capacity_upgrades_charge_wood_and_stop_at_level_three(self):
        self.town['wood'] = 60
        for expected in [8, 12]:
            command(self.town, {'action': 'upgrade_capacity', 'index': 1})
            self.assertEqual(settler_capacity(self.town['buildings'][1]), expected - 4)
            for _ in range(100):
                construct(self.town, self.town)
                if 'construction' not in self.town['buildings'][1]:
                    break
            self.assertNotIn('construction', self.town['buildings'][1])
            self.assertEqual(settler_capacity(self.town['buildings'][1]), expected)
        self.assertEqual(self.town['wood'], 0)
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'upgrade_capacity', 'index': 1})
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'upgrade_capacity', 'index': 4})
        self.assertEqual(self.town['wood'], 0)

    def test_hall_is_reserved_for_controllable_character(self):
        self.town['wood'] = 100
        for level in (1, 2, 3):
            self.town['buildings'][0]['settler_level'] = level
            self.assertEqual(settler_capacity(self.town['buildings'][0]), 0)
            for index in range(len(self.town['villagers'])):
                before = copy.deepcopy(self.town)
                with self.assertRaisesRegex(ValueError, 'controllable character'):
                    self.assign(index, 0)
                self.assertEqual(before, self.town)
            before = copy.deepcopy(self.town)
            with self.assertRaisesRegex(ValueError, 'controllable character'):
                command(self.town, {'action': 'upgrade_capacity', 'index': 0})
            self.assertEqual(before, self.town)

    def test_legacy_hall_residents_are_released_after_loading(self):
        from server import ensure_resources
        for shared in (False, True):
            with self.subTest(shared=shared):
                town = json.loads(json.dumps(self.town))
                if shared:
                    town['town_origin'] = {'x': 5, 'y': 5}
                town['villagers'][0].update(building_index=0, work_target=town['buildings'][0])
                town['villagers'][1]['building_index'] = 4
                ensure_resources(town)
                self.assertNotIn('building_index', town['villagers'][0])
                self.assertNotIn('work_target', town['villagers'][0])
                self.assertEqual(town['villagers'][0]['job'], 'idle')
                self.assertEqual(town['villagers'][1]['building_index'], 4)
                self.assertEqual(assigned_settlers(town, 0), [])

    def test_assignment_and_production_follow_selected_farm_after_save_and_move(self):
        self.assign(0, 2)
        self.town = json.loads(json.dumps(self.town))
        self.assertEqual(self.town['villagers'][0]['work_target']['kind'], 'farm')
        command(self.town, {'action': 'move', 'index': 2, 'x': 4, 'y': 4})
        tick({'towns': {'own': self.town}})
        self.assertEqual(self.town['villagers'][0]['building_index'], 2)
        self.assertIs(self.town['villagers'][0]['work_target'], self.town['buildings'][2])
        # Moving away from water keeps the explicit farm assignment.

    def test_acre_assignments_use_that_specific_field_and_pause_without_their_farm(self):
        self.assign(0, 3)
        self.assertEqual(self.town['villagers'][0]['work_target']['kind'], 'acre')
        self.assertEqual(self.town['villagers'][0]['work_target']['x'], 6)
        command(self.town, {'action': 'destroy', 'index': 1})
        self.assertEqual(self.town['villagers'][0]['building_index'], 2)
        self.assertIsNone(self.town['villagers'][0]['work_target'])

    def test_reassignment_removal_and_destroy_release_spaces(self):
        self.assign(0, 1)
        self.assign(0, 4)
        self.assertEqual(assigned_settlers(self.town, 1), [])
        self.assertEqual(self.town['villagers'][0]['job'], 'idle')
        command(self.town, {'action': 'remove_settler', 'index': 0})
        self.assertEqual(assigned_settlers(self.town, 4), [])
        self.assign(0, 2)
        command(self.town, {'action': 'destroy', 'index': 1})
        self.assertEqual(self.town['villagers'][0]['building_index'], 1)
        command(self.town, {'action': 'destroy', 'index': 1})
        self.assertNotIn('building_index', self.town['villagers'][0])
        self.assertEqual(self.town['villagers'][0]['job'], 'idle')

    def test_invalid_or_unfinished_assignments_do_not_mutate_town(self):
        self.town['buildings'][1]['construction'] = {'progress': 0, 'total': 30}
        for worker, building in [(0, 1), (True, 0), (0, True), (-1, 0), (0, 99)]:
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                self.assign(worker, building)
            self.assertEqual(before, self.town)
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'upgrade_capacity', 'index': 1})


if __name__ == '__main__':
    unittest.main()
