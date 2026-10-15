"""Butchers fetch one reserved animal and process it only after reaching the shop."""
import copy
import json
import unittest

from fixtures import flat_town
import progression as economy
import server
import work_cycles
from spatial import occupied_tiles, perimeter


class ButcherAnimation(unittest.TestCase):
    def setUp(self):
        self.town = flat_town(size=24)
        self.town.update(level=5, arrival_delay_seconds=10000)
        # Face the entrance toward the shop, keeping this fixture's short trips.
        self.barn = {'kind': 'barn', 'x': 12, 'y': 12, 'fence_orientation': 2, 'animals': {'pig': 2},
                     'animal_growth': {'pig': [240, 240]}}
        self.shop = {'kind': 'butcher', 'x': 5, 'y': 12}
        self.town['buildings'].extend([self.barn, self.shop])
        for worker in self.town['villagers']:
            worker['job'] = 'idle'
        self.worker = self.town['villagers'][0]
        self.worker.update(job='butchery', building_index=2, husbandry=2, talents={},
                           logistics_position={'x': 6, 'y': 12})
        self.start()

    def start(self):
        server.command(self.town, {'action': 'butcher_animal', 'index': 2,
                                  'barn_index': 1, 'animal': 'pig'})

    def advance(self, seconds=.1):
        work_cycles.advance(self.town, self.town, self.worker, self.shop, self.shop, 0, seconds)
        return self.worker['work_animation']

    def reach(self, phase):
        for _ in range(500):
            state = self.advance()
            if state['phase'] == phase:
                return state
        self.fail(f'Never reached {phase}')

    def test_walk_find_drag_and_work_follow_accessible_tiles(self):
        self.town['map_resources'] = [{'kind': 'tree', 'x': 9, 'y': 12},
                                      {'kind': 'rock', 'x': 10, 'y': 12}]
        blocked = occupied_tiles(self.town['buildings'] + self.town['map_resources'])
        barn = copy.deepcopy(self.barn)
        phases, speeds = set(), {}
        previous = dict(self.worker['logistics_position'])
        for _ in range(400):
            state = self.advance()
            phases.add(state['phase'])
            distance = ((state['x'] - previous['x']) ** 2 + (state['y'] - previous['y']) ** 2) ** .5
            self.assertLessEqual(distance, .181)
            self.assertNotIn((round(state['x']), round(state['y'])), blocked)
            if state['phase'] == previous.get('phase') and distance:
                speeds[state['phase']] = max(speeds.get(state['phase'], 0), distance)
            if state['phase'] == 'finding_animal':
                self.assertEqual(abs(state['x'] - 12) + abs(state['y'] - 12), 1)
            if state['phase'] == 'working':
                self.assertEqual(abs(state['x'] - 5) + abs(state['y'] - 12), 1)
                self.assertEqual(self.shop['butchering']['stage'], 'working')
            self.assertEqual(state['butcherAnimal'], 'pig')
            previous = dict(state)
        self.assertTrue({'walking', 'finding_animal', 'dragging', 'working'} <= phases)
        self.assertAlmostEqual(speeds['dragging'] / speeds['walking'], .65)
        self.assertEqual(self.barn, barn, 'Animation cannot consume or grow more livestock')
        self.assertEqual(self.shop['butchering']['remaining'], 60)

    def test_large_shop_processes_animals_outside_the_whole_footprint(self):
        self.shop['footprint'] = [2, 2]
        self.worker['logistics_position'] = {'x': 7, 'y': 12}
        self.reach('dragging')
        state = self.reach('working')
        self.assertIn((state['x'], state['y']), perimeter(self.shop))
        self.assertNotIn((state['x'], state['y']), occupied_tiles([self.shop]))
        self.assertEqual(self.shop['butchering']['stage'], 'working')
        self.assertEqual(self.shop['butchering']['remaining'], 60)
        self.assertEqual(state['butcherAnimal'], 'pig')

    def test_save_reload_preserves_each_leg_and_processing(self):
        for phase in ('walking', 'finding_animal', 'dragging', 'working'):
            with self.subTest(phase=phase):
                self.reach(phase)
                saved = json.loads(json.dumps(self.town))
                self.advance(.3)
                work_cycles.advance(saved, saved, saved['villagers'][0], saved['buildings'][2],
                                    saved['buildings'][2], 0, .3)
                self.assertEqual(saved['villagers'][0]['work_animation'], self.worker['work_animation'])
                self.assertEqual(saved['buildings'][2], self.shop)

    def test_blocked_outbound_and_return_routes_wait_and_resume(self):
        # A wall divides barn and shop; neither leg may produce while blocked.
        wall = [{'kind': 'rock', 'x': 8, 'y': y} for y in range(24)]
        self.town['map_resources'] = wall
        self.assertEqual(self.advance(3)['phase'], 'waiting')
        self.assertEqual(self.shop['butchering']['stage'], 'fetching')
        self.town['map_resources'] = []
        self.reach('dragging')
        self.town['map_resources'] = wall
        for _ in range(50):
            if self.advance()['phase'] == 'waiting':
                break
        self.assertEqual(self.worker['work_animation']['phase'], 'waiting')
        self.assertEqual(self.shop['butchering']['remaining'], 60)
        self.town['map_resources'] = []
        self.reach('working')
        self.assertEqual(self.shop['butchering']['stage'], 'working')

    def test_replacement_butcher_collects_animal_at_saved_drag_position(self):
        self.reach('dragging')
        self.advance(.8)
        position = dict(self.shop['butchering']['position'])
        self.worker['job'] = 'idle'
        replacement = self.town['villagers'][1]
        replacement.update(job='butchery', building_index=2, husbandry=2,
                           logistics_position={'x': 5, 'y': 11})
        self.worker = replacement
        state = self.advance(0)
        self.assertEqual(state['phase'], 'walking')
        self.assertEqual(state['path'][-1], position)
        self.reach('working')
        self.assertEqual(self.shop['butchering']['stage'], 'working')

    def test_host_counts_only_shop_work_once_with_multiple_butchers(self):
        helper = self.town['villagers'][1]
        helper.update(job='butchery', building_index=2, husbandry=2)
        seen = set()
        ticks = 0
        while 'butchering' in self.shop and ticks < 50:
            before = self.shop['butchering']['remaining']
            server.tick({'towns': {'a': self.town}})
            ticks += 1
            state = self.worker['work_animation']
            seen.update(s['phase'] for s in state['playback'])
            if 'butchering' in self.shop:
                self.assertLessEqual(before - self.shop['butchering']['remaining'], 3.000001)
                if state['phase'] != 'working':
                    self.assertEqual(self.shop['butchering']['remaining'], 60)
            self.assertFalse(helper.get('work_animation', {}).get('butcherAnimal'))
        self.assertNotIn('butchering', self.shop)
        self.assertGreater(ticks, 20, 'Travel and pickup add time before the minute of work')
        self.assertTrue({'finding_animal', 'dragging', 'working'} <= seen)
        self.assertEqual(economy.quantity(self.town, 'meat'), 8)
        self.assertEqual(economy.quantity(self.town, 'hides'), 2)
        self.assertEqual(self.barn['animals']['pig'], 1)
        # A second animal gets a fresh trip, even with identical source and species.
        self.start()
        server.tick({'towns': {'a': self.town}})
        self.assertEqual(self.shop['butchering']['remaining'], 60)
        self.assertEqual(self.shop['butchering']['stage'], 'fetching')
        self.assertIn(self.worker['work_animation']['phase'], ('walking', 'finding_animal'))
        self.assertTrue(any(s['phase'] == 'walking' for s in self.worker['work_animation']['playback']))

    def test_older_saved_processing_jobs_remain_at_the_shop(self):
        self.shop['butchering'] = {'animal': 'pig', 'amount': 8, 'remaining': 30}
        self.worker['logistics_position'] = {'x': 6, 'y': 12}
        state = self.advance(3)
        self.assertEqual(state['phase'], 'working')
        self.assertIsNone(state.get('butcherAnimal'))


if __name__ == '__main__':
    unittest.main()
