"""Field approaches are authoritative walking, followed by stationary work."""
import math
import unittest

from fixtures import flat_town
import farming
import work_cycles


class FarmingDestinations(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Approaches')
        self.town.update(terrain=['g' * 16] * 16, map_size=16, map_resources=[], npcs=[])
        self.farm = {'kind': 'farm', 'x': 3, 'y': 3}
        self.field = {'kind': 'acre', 'x': 3, 'y': 4, 'farm_index': 1}
        self.town['buildings'].extend([self.farm, self.field, {'kind': 'well', 'x': 9, 'y': 4}])
        self.worker = self.town['villagers'][0]
        self.worker.update(job='farming', building_index=1, talents={},
                           construction_position={'x': 3, 'y': 4})

    def advance(self, seconds):
        work_cycles.advance(self.town, self.town, self.worker, self.farm, self.field, 0, seconds)
        return self.worker['work_animation']

    def assert_approach(self, samples, action):
        working = next(sample for sample in samples if sample['farmAction'] == action)
        self.assertAlmostEqual(working['x'], 2.55)
        self.assertAlmostEqual(working['y'], 4.1)
        self.assertTrue(any(sample['phase'] in ('walking', 'returning_water') for sample in samples))
        for start, end in zip(samples, samples[1:]):
            distance = math.hypot(end['x'] - start['x'], end['y'] - start['y'])
            self.assertLessEqual(distance, 1.8 * (end['time'] - start['time']) + 1e-8)
            if start['farmAction'] == action:
                self.assertEqual((start['x'], start['y']), (end['x'], end['y']))

    def test_sowing_walks_to_destination_before_starting_and_stays_afterward(self):
        farming.field_state(self.field).update(stage='plowed')
        state = self.advance(3)
        self.assert_approach(state['playback'], 'sowing')
        self.assertEqual(self.field['field']['stage'], 'sprouting')
        self.assertEqual((state['x'], state['y']), (2.55, 4.1))

    def test_watering_returns_to_the_same_destination_after_filling(self):
        farming.field_state(self.field).update(stage='thirsty')
        state = self.advance(20)
        self.assert_approach(state['playback'], 'watering')
        self.assertEqual(self.field['field']['stage'], 'growing')
        self.assertEqual((state['x'], state['y']), (2.55, 4.1))

    def test_plowing_to_sowing_walks_instead_of_sliding(self):
        self.advance(2)
        state = self.advance(.1)
        self.assertEqual(state['phase'], 'walking')
        self.assertIsNone(state.get('farmAction'))
        self.assertEqual(self.field['field']['progress'], 0)
        state = self.advance(3)
        self.assert_approach(state['playback'], 'sowing')

    def test_border_destinations_stay_in_the_field_tile(self):
        for x, y in ((0, 0), (0, 15), (15, 0), (15, 15)):
            field = {'x': x, 'y': y, 'field': {'stage': 'plowed'}}
            spot = work_cycles.field_work_spot(field)
            self.assertEqual((round(spot['x']), round(spot['y'])), (x, y))
            self.assertTrue(0 <= spot['x'] < 16 and 0 <= spot['y'] < 16)


if __name__ == '__main__':
    unittest.main()
