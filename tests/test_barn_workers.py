"""Host-confirmed keeper rounds stay in the pen and return to unload."""
import copy
import json
import unittest

from fixtures import flat_town
from chunk_world import CHUNK_SIZE, ChunkTerrain
from spatial import occupied_tiles
import server
import work_cycles


class BarnWorkers(unittest.TestCase):
    def setUp(self):
        self.town = flat_town(size=24)
        self.barn = {'kind': 'barn', 'x': 12, 'y': 12, 'animals': {'chicken': 2},
                     'animal_growth': {'chicken': [30, 60]}, 'input_buffer': {'fodder': 20}}
        self.town['buildings'].append(self.barn)
        self.worker = {'job': 'husbandry', 'building_index': 1,
                       'logistics_position': {'x': 11, 'y': 12}}

    def advance(self, seconds=.1):
        return work_cycles.advance(self.town, self.town, self.worker,
                                   self.barn, self.barn, 0, seconds)

    def test_rounds_walk_pause_and_unload_beside_the_barn(self):
        self.town['map_resources'] = [{'kind': 'tree', 'x': 13, 'y': 12},
                                     {'kind': 'rock', 'x': 12, 'y': 14}]
        blocked = occupied_tiles(self.town['buildings'] + self.town['map_resources'])
        stock = copy.deepcopy(self.barn)
        tended, phases, deliveries = set(), set(), 0
        previous = self.worker['logistics_position']
        for _ in range(800):
            deliveries += self.advance()
            state = self.worker['work_animation']
            self.assertLessEqual(max(abs(state['x'] - 12), abs(state['y'] - 12)), 3)
            self.assertNotIn((round(state['x']), round(state['y'])), blocked)
            self.assertLessEqual(((state['x'] - previous['x']) ** 2
                                  + (state['y'] - previous['y']) ** 2) ** .5, .181)
            previous = dict(state)
            phases.add(state['phase'])
            if state['phase'] == 'working':
                tended.add((state['x'], state['y']))
            for sample in state['playback']:
                if sample['phase'] == 'depositing':
                    self.assertEqual(abs(sample['x'] - 12) + abs(sample['y'] - 12), 1)
        self.assertTrue({'walking', 'working', 'returning', 'depositing'} <= phases)
        self.assertGreater(len(tended), 2)
        self.assertGreater(deliveries, 2)
        self.assertEqual(self.barn, stock, 'Movement cannot produce, grow, or consume stock')

    def test_save_reload_preserves_an_active_round(self):
        for _ in range(100):
            self.advance()
            state = self.worker['work_animation']
            if state.get('barn_round') and state['phase'] == 'walking':
                break
        self.assertEqual(state['phase'], 'walking')
        saved_town, saved_worker = json.loads(json.dumps([self.town, self.worker]))
        self.advance()
        saved_barn = saved_town['buildings'][1]
        work_cycles.advance(saved_town, saved_town, saved_worker, saved_barn, saved_barn, 0, .1)
        self.assertEqual(saved_worker['work_animation'], self.worker['work_animation'])

    def test_isolated_keeper_stays_on_the_only_open_tile(self):
        self.worker['logistics_position'] = {'x': 11, 'y': 12}
        self.town['map_resources'] = [{'kind': 'rock', 'x': x, 'y': y}
                                     for x, y in [(10, 12), (11, 11), (11, 13)]]
        for _ in range(10):
            self.advance(3)
            state = self.worker['work_animation']
            self.assertEqual((state['x'], state['y']), (11, 12))
            self.assertFalse(state['path'])

    def test_map_edge_rounds_stay_loaded_and_inside_the_map(self):
        for center, size in [(0, 24), (CHUNK_SIZE - 1, CHUNK_SIZE * 2)]:
            with self.subTest(center=center):
                self.barn.update(x=center, y=center)
                self.worker = {'job': 'husbandry', 'building_index': 1,
                               'logistics_position': {'x': max(1, center - 1), 'y': center}}
                self.town.update(chunked=True, map_size=size, terrain=ChunkTerrain(
                    {'0,0': {'terrain': ['g' * CHUNK_SIZE] * CHUNK_SIZE}}, size))
                for _ in range(200):
                    self.advance()
                    state = self.worker['work_animation']
                    self.assertGreaterEqual(state['x'], 0)
                    self.assertGreaterEqual(state['y'], 0)
                    self.assertLessEqual(max(abs(state['x'] - center), abs(state['y'] - center)), 3)
                    self.assertNotEqual(self.town['terrain'][round(state['y'])][round(state['x'])], 'v')

    def test_empty_barns_and_other_workplaces_keep_stationary_cycles(self):
        for kind, job, animals in [('barn', 'husbandry', {}),
                                   ('stable', 'horsekeeping', {'horse': 2})]:
            with self.subTest(kind=kind):
                self.barn.update(kind=kind, animals=animals)
                self.worker = {'job': job, 'building_index': 1,
                               'logistics_position': {'x': 11, 'y': 12}}
                for _ in range(100):
                    self.advance()
                    state = self.worker['work_animation']
                    self.assertEqual((state['x'], state['y']), (11, 12))
                    self.assertFalse(state['path'])


class BarnFeedingWorkers(unittest.TestCase):
    def setUp(self):
        self.town = flat_town(size=24)
        self.town.update(level=5, arrival_delay_seconds=60)
        self.barn = {'kind': 'barn', 'x': 12, 'y': 12, 'settler_level': 2,
                     'animals': {'chicken': 2}, 'animal_growth': {'chicken': [0, 0]},
                     'input_buffer': {'fodder': 20}}
        self.town['buildings'].append(self.barn)
        template = self.town['villagers'][0]
        self.town['villagers'] = [dict(copy.deepcopy(template), job='husbandry',
                                       building_index=1, husbandry=0, talents={},
                                       logistics_position={'x': 11, 'y': 12}) for _ in range(4)]

    def tick(self):
        server.tick({'towns': {'a': self.town}})

    def test_all_keepers_speed_growth_and_train_without_consuming_feed(self):
        self.barn.update(animals={'chicken': 1}, animal_growth={'chicken': [0]})
        self.tick()
        self.assertEqual(self.barn['animal_growth']['chicken'], [15])
        self.assertEqual(self.barn['input_buffer']['fodder'], 20)
        self.assertAlmostEqual(self.town['produced']['eggs'], 6.25, msg='Output advances once at the shared speed')
        self.assertAlmostEqual(self.town['produced']['manure'], .5)
        for worker in self.town['villagers']:
            self.assertEqual(worker['production_status'], 'Working')
            self.assertAlmostEqual(worker['husbandry'], .08)
            self.assertEqual(worker['work_animation']['task'][0], 'husbandry')

    def test_missing_feed_and_blocked_outputs_do_not_pause_or_duplicate_growth(self):
        self.barn['input_buffer'] = {}
        self.town['rules'] = {'production_caps': {'eggs': 0, 'meat': 0, 'manure': 0}}
        for _ in range(3):
            self.tick()
        self.assertEqual(self.barn['animal_growth']['chicken'], [45, 45])
        self.assertFalse(self.barn.get('output_buffer'))
        self.assertEqual(self.barn['input_buffer'], {})

    def test_only_assigned_keepers_up_to_capacity_speed_growth(self):
        self.barn['settler_level'] = 1  # Two worker spaces.
        self.tick()
        self.assertEqual(self.barn['animal_growth']['chicken'], [9, 9])
        self.town['villagers'][0]['job'] = 'idle'
        self.tick()
        self.assertEqual(self.barn['animal_growth']['chicken'], [15, 15])


if __name__ == '__main__':
    unittest.main()
