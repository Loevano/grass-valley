import json
import unittest

from server import assigned_settlers, command, new_town, tick


class CottageAutofill(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Cottages')
        self.town.update(terrain=['g' * 120] * 120, map_resources=[], npcs=[],
                         food=0, arrival_delay_seconds=60)
        self.world = {'towns': {'own': self.town}}

    def cottage(self, **extra):
        self.town['buildings'].append({'kind': 'home', 'x': 6, 'y': 6, **extra})
        return len(self.town['buildings']) - 1

    def test_fills_available_capacity_across_cottages(self):
        first = self.cottage()
        second = self.cottage(settler_level=2)
        tick(self.world)
        self.assertEqual(assigned_settlers(self.town, first), [0, 1])
        self.assertEqual(assigned_settlers(self.town, second), [2, 3, 4])
        self.assertEqual(assigned_settlers(self.town, 0), [])
        tick(self.world)
        self.assertEqual(assigned_settlers(self.town, first), [0, 1])
        self.assertEqual(assigned_settlers(self.town, second), [2, 3, 4])

    def test_preserves_workers_builders_and_existing_residents(self):
        home = self.cottage()
        self.town['buildings'].append({'kind': 'farm', 'x': 7, 'y': 6})
        command(self.town, {'action': 'assign_settler', 'index': 0, 'building_index': 2})
        command(self.town, {'action': 'job', 'index': 1, 'job': 'building'})
        command(self.town, {'action': 'assign_settler', 'index': 4, 'building_index': home})
        tick(self.world)
        self.assertEqual(assigned_settlers(self.town, home), [2, 4])
        self.assertEqual(self.town['villagers'][0]['building_index'], 2)
        self.assertEqual(self.town['villagers'][0]['job'], 'farming')
        self.assertEqual(self.town['villagers'][1]['job'], 'building')
        self.assertNotIn('building_index', self.town['villagers'][3])

    def test_waits_for_construction_then_fills_in_same_tick(self):
        home = self.cottage(construction={'progress': 0, 'total': 30})
        tick(self.world)
        self.assertEqual(assigned_settlers(self.town, home), [])
        self.town['buildings'][home]['construction']['progress'] = 29
        tick(self.world)
        self.assertNotIn('construction', self.town['buildings'][home])
        self.assertEqual(assigned_settlers(self.town, home), [0, 1])

    def test_new_arrival_fills_free_cottage_space(self):
        home = self.cottage()
        self.town['villagers'] = self.town['villagers'][:1]
        command(self.town, {'action': 'assign_settler', 'index': 0, 'building_index': home})
        self.town.update(food=40, arrival_delay_seconds=0)
        tick(self.world)
        self.assertEqual(len(self.town['villagers']), 2)
        self.assertEqual(assigned_settlers(self.town, home), [0, 1])

    def test_loaded_town_fills_vacancies_after_resident_reassignment(self):
        home = self.cottage()
        tick(self.world)
        self.town['buildings'].append({'kind': 'farm', 'x': 7, 'y': 6})
        command(self.town, {'action': 'assign_settler', 'index': 0, 'building_index': 2})
        self.world = json.loads(json.dumps(self.world))
        self.town = self.world['towns']['own']
        tick(self.world)
        self.assertEqual(assigned_settlers(self.town, home), [1, 2])
        self.assertEqual(self.town['villagers'][0]['job'], 'farming')


if __name__ == '__main__':
    unittest.main()
