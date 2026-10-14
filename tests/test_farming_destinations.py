"""Field approaches are authoritative walking, followed by stationary work."""
import math
import unittest
from unittest.mock import patch

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

    def test_plowing_walks_to_the_same_tool_length_destination(self):
        state = self.advance(3)
        self.assert_approach(state['playback'], 'plowing')
        self.assertEqual(self.field['field']['stage'], 'plowed')
        self.assertEqual((state['x'], state['y']), (2.55, 4.1))
        state = self.advance(.1)
        self.assertEqual((state['x'], state['y']), (2.55, 4.1))
        self.assertEqual(state['farmAction'], 'sowing')

    def test_border_destinations_stay_in_the_field_tile(self):
        for x, y in ((0, 0), (0, 15), (15, 0), (15, 15)):
            field = {'x': x, 'y': y, 'field': {'stage': 'unplowed'}}
            spot = work_cycles.field_work_spot(field)
            self.assertEqual((round(spot['x']), round(spot['y'])), (x, y))
            self.assertTrue(0 <= spot['x'] < 16 and 0 <= spot['y'] < 16)

    def advance_unreachable(self):
        route = work_cycles.route
        attempts = 0

        def bounded_route(*args, **kwargs):
            nonlocal attempts
            attempts += 1
            # Fail promptly if the host retries without consuming tick time.
            self.assertLessEqual(attempts, 2, 'Unreachable routes must wait for the next tick')
            return route(*args, **kwargs)

        with patch.object(work_cycles, 'route', side_effect=bounded_route):
            return self.advance(3)

    def test_unreachable_acre_waits_and_resumes_when_access_is_restored(self):
        self.worker['construction_position'] = {'x': 3, 'y': 6}
        self.town['map_resources'] = [{'kind': 'rock', 'x': x, 'y': y}
                                      for x, y in ((2, 4), (4, 4), (3, 5))]
        for stage in ('unplowed', 'plowed', 'thirsty', 'ripe'):
            with self.subTest(stage=stage):
                self.worker.pop('work_animation', None)
                field = farming.field_state(self.field)
                field.update(stage=stage, progress=0)
                self.worker['watering_round'] = farming.watering_round(self.farm, self.field)
                state = self.advance_unreachable()
                self.assertEqual(state['phase'], 'waiting')
                self.assertEqual((state['x'], state['y']), (3, 6))
                self.assertEqual(field['progress'], 0)
                self.assertEqual(field['stage'], stage)

        self.town['map_resources'].clear()
        state = self.advance(3)
        self.assertEqual(state['phase'], 'working')
        self.assertGreater(self.field['field']['progress'], 0)

    def test_unreachable_farm_keeps_harvest_cargo_until_access_is_restored(self):
        self.farm.update(x=8, y=8)
        self.worker['construction_position'] = {'x': 3, 'y': 6}
        self.worker['pending_output'] = {'vegetables': 1}
        farming.field_state(self.field).update(stage='ripe')
        self.farm['output_buffer'] = {'vegetables': farming.storage_capacity(self.farm)}
        self.town['map_resources'] = [{'kind': 'rock', 'x': x, 'y': y}
                                      for x, y in ((7, 8), (9, 8), (8, 7), (8, 9))]
        state = self.advance_unreachable()
        self.assertEqual(state['phase'], 'waiting')
        self.assertEqual(self.worker['pending_output'], {'vegetables': 1})
        self.assertEqual(self.field['field']['stage'], 'ripe')

        self.town['map_resources'].clear()
        state = self.advance(1)
        self.assertEqual(state['phase'], 'returning')
        self.assertGreater(math.hypot(state['x'] - 3, state['y'] - 6), 0)
        self.assertEqual(self.worker['pending_output'], {'vegetables': 1})


if __name__ == '__main__':
    unittest.main()
