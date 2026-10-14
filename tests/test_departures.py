import copy
import unittest
from unittest.mock import patch

from server import (SHORTAGE_GRACE_SECONDS, assigned_settlers, feed_settlers,
                    housing_capacity, housing_loyalty, new_town, tick)


class SettlerDepartures(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Hungry town')
        self.town['food'] = 0

    @patch('server.secrets.randbelow', return_value=0)
    def test_grace_period_and_recovery(self, roll):
        for _ in range(19):
            feed_settlers(self.town)
        self.assertEqual(len(self.town['villagers']), 5)
        roll.assert_not_called()
        self.town['food'] = .15  # Enough to feed everyone, even if the pantry empties.
        feed_settlers(self.town)
        self.assertEqual(self.town['food_shortage_seconds'], 0)
        self.assertEqual(self.town['food'], 0)

    @patch('server.secrets.randbelow', return_value=0)
    def test_departure_loses_specialist_and_frees_slot(self, _roll):
        self.town['buildings'].append({'kind': 'bakery', 'x': 3, 'y': 3})
        worker = self.town['villagers'][0]
        worker.update(job='baking', building_index=1, baking=80, milling=20)
        self.town['food_shortage_seconds'] = 57
        tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 4)
        self.assertEqual(assigned_settlers(self.town, 1), [])
        self.assertNotIn(worker, self.town['villagers'])
        self.assertEqual(self.town['last_departure']['name'], 'Ada')
        self.assertEqual(self.town['last_departure']['job'], 'baking')
        self.assertEqual(self.town['last_departure']['practice']['baking'], 80)
        self.assertEqual(self.town['food_shortage_seconds'], 0)
        self.assertEqual(self.town['arrival_delay_seconds'], SHORTAGE_GRACE_SECONDS)

    @patch('server.secrets.randbelow', side_effect=[400, 400, 0])
    def test_upgraded_housing_prevents_same_departure_roll(self, _roll):
        home = {'kind': 'home', 'x': 3, 'y': 3, 'level': 3}
        self.town['buildings'].append(home)
        self.town['food_shortage_seconds'] = 57
        basic = copy.deepcopy(self.town)
        basic['buildings'][-1]['level'] = 1
        feed_settlers(self.town)
        feed_settlers(basic)
        self.assertEqual(len(self.town['villagers']), 5)
        self.assertEqual(len(basic['villagers']), 4)

    def test_housing_benefits_require_finished_upgrades_and_ignore_capacity(self):
        home = {'kind': 'home', 'x': 3, 'y': 3, 'level': 1, 'settler_level': 3}
        self.town['buildings'].append(home)
        self.assertEqual(housing_loyalty(self.town), 0)
        home['level'] = 2
        self.assertAlmostEqual(housing_loyalty(self.town), .2)
        home['level'] = 3
        self.assertAlmostEqual(housing_loyalty(self.town), .5)
        home['profession_upgrades'] = ['timber_frame']
        self.assertEqual(housing_loyalty(self.town), .75)
        home['construction'] = {'progress': 0, 'total': 30}
        self.assertEqual(housing_loyalty(self.town), 0)

    @patch('server.secrets.randbelow', return_value=0)
    def test_last_settler_stays(self, roll):
        self.town['villagers'] = self.town['villagers'][:1]
        self.town['food_shortage_seconds'] = 60
        feed_settlers(self.town)
        self.assertEqual(len(self.town['villagers']), 1)
        roll.assert_not_called()

    @patch('server.secrets.randbelow', return_value=0)
    def test_replacement_is_delayed_and_starts_without_lost_skills(self, _roll):
        self.town['villagers'][0]['woodcutting'] = 100
        self.town['food_shortage_seconds'] = 57
        tick({'towns': {'a': self.town}})
        self.town['food'] = 100
        for _ in range(19):
            tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 4)
        tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 5)
        self.assertEqual(self.town['villagers'][-1]['woodcutting'], 0)

    @patch('server.secrets.randbelow', return_value=0)
    def test_departure_and_arrival_keep_generated_names_unique(self, _roll):
        departed_name = self.town['villagers'][0]['name']
        self.town['food_shortage_seconds'] = 57
        feed_settlers(self.town)
        self.town['food'] = 100
        self.town['arrival_delay_seconds'] = 0
        tick({'towns': {'a': self.town}})
        names = [v['name'] for v in self.town['villagers']]
        self.assertEqual(len(names), len(set(names)))
        self.assertNotIn(departed_name, names)
        self.assertFalse(any(name.startswith('Settler ') for name in names))

    def test_housing_capacity_uses_finished_cottages_and_comfort_upgrades(self):
        self.assertEqual(housing_capacity(self.town), 5)
        self.town['buildings'].extend([
            {'kind': 'home', 'level': 3, 'settler_level': 3},
            {'kind': 'home', 'level': 2, 'profession_upgrades': ['timber_frame']},
            {'kind': 'home', 'level': 3, 'construction': {'progress': 0, 'total': 30}},
        ])
        self.assertEqual(housing_capacity(self.town), 17)

    @patch('server.secrets.randbelow', return_value=0)
    def test_lost_housing_causes_departures_despite_plentiful_food(self, roll):
        self.town['food'] = 1000
        self.town['buildings'].append({'kind': 'home', 'x': 3, 'y': 3})
        tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 6)
        self.town['buildings'].pop()
        for _ in range(19):
            tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 6)
        roll.assert_not_called()
        tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 5)
        self.assertEqual(self.town['last_departure']['reason'], 'housing')
        self.assertEqual(self.town['housing_shortage_seconds'], 0)
        self.assertEqual(self.town['food_shortage_seconds'], 0)
        self.assertEqual(self.town['arrival_delay_seconds'], 60)

    @patch('server.secrets.randbelow', return_value=0)
    def test_restoring_housing_resets_grace_period(self, roll):
        self.town['food'] = 1000
        self.town['villagers'].append({'name': 'Extra', 'job': 'idle'})
        self.town['housing_shortage_seconds'] = 57
        self.town['buildings'].append({'kind': 'home', 'x': 3, 'y': 3})
        feed_settlers(self.town)
        self.assertEqual(self.town['housing_shortage_seconds'], 0)
        self.assertEqual(len(self.town['villagers']), 6)
        roll.assert_not_called()

    def test_arrivals_require_free_housing_and_food_after_meals(self):
        self.town['food'] = 100
        world = {'towns': {'a': self.town}}
        tick(world)
        self.assertEqual(len(self.town['villagers']), 5)
        home = {'kind': 'home', 'x': 3, 'y': 3, 'construction': {'progress': 0, 'total': 30}}
        self.town['buildings'].append(home)
        tick(world)
        self.assertEqual(len(self.town['villagers']), 5)
        home.pop('construction')
        self.town['food'] = 20
        tick(world)
        self.assertEqual(len(self.town['villagers']), 5)
        self.town['food'] = 21
        tick(world)
        self.assertEqual(len(self.town['villagers']), 6)
        self.assertAlmostEqual(self.town['food'], 10.85)


if __name__ == '__main__':
    unittest.main()
