import json
import unittest

import logistics
import progression as economy
import server
import work_cycles


class WorkplaceDropoff(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Drop-off')
        self.town.update(terrain=['g' * 16] * 16, map_size=16, map_resources=[], npcs=[],
                         forage_version=1, forage_halls=[[5, 5]], town_origin={'x': 5, 'y': 5})
        self.building = {'kind': 'woodcutter', 'x': 3, 'y': 3}
        self.source = {'kind': 'tree', 'x': 3, 'y': 1}
        self.town['buildings'].append(self.building)
        self.town['map_resources'].append(self.source)
        self.worker = self.town['villagers'][0]
        self.worker.update(job='woodcutting', building_index=1, talents={},
                           construction_position={'x': 3, 'y': 0})
        economy.ensure_inventory(self.town)

    def finish_dropoff(self, town=None, worker=None, building=None, source=None):
        town, worker = town or self.town, worker or self.worker
        building, source = building or self.building, source or self.source
        phases = set()
        for _ in range(300):
            if work_cycles.advance(town, town, worker, building, source, 0, .1):
                logistics.drop_off(town, building, worker)
                return phases
            phases.add(worker['work_animation']['phase'])
        self.fail('Worker never finished unloading')

    def test_playback_records_confirmed_turns_without_per_step_history(self):
        work_cycles.advance(self.town, self.town, self.worker, self.building, self.source, 0, 3)
        state = self.worker['work_animation']
        samples = state['playback']
        self.assertEqual(samples[0]['time'], 0)
        self.assertEqual(samples[-1]['time'], 3)
        self.assertEqual((samples[-1]['x'], samples[-1]['y']), (state['x'], state['y']))
        self.assertLess(len(samples), 12)
        self.assertEqual([p['time'] for p in samples], sorted(p['time'] for p in samples))
        for a, b in zip(samples, samples[1:]):
            self.assertTrue(a['x'] == b['x'] or a['y'] == b['y'], 'Turns retain their waypoint')
        first = json.dumps(samples)
        work_cycles.advance(self.town, self.town, self.worker, self.building, self.source, 0, 3)
        self.assertEqual(json.dumps(samples), first, 'Later cycles cannot mutate earlier playback')

    def test_blocked_return_replans_to_workplace_instead_of_resource(self):
        self.worker['work_animation'] = {
            'task': ['woodcutting', 'woodcutter', 3, 3, 'tree', 3, 1],
            'x': 2, 'y': 1, 'phase': 'returning', 'elapsed': 0, 'version': 1,
            'waterReady': False, 'facing': 1, 'path': [{'x': 2, 'y': 2}, {'x': 3, 'y': 2}]}
        logistics.record_output(self.town, self.building, 'wood', 5, self.worker)
        obstacle = {'kind': 'rock', 'x': 2, 'y': 2}
        self.town['map_resources'].append(obstacle)
        work_cycles.advance(self.town, self.town, self.worker, self.building, self.source, 0, .1)
        self.assertEqual(self.worker['work_animation']['phase'], 'waiting')
        self.town['map_resources'].remove(obstacle)
        work_cycles.advance(self.town, self.town, self.worker, self.building, self.source, 0, 0)
        animation = self.worker['work_animation']
        self.assertEqual(animation['phase'], 'returning')
        end = animation['path'][-1]
        self.assertEqual(abs(end['x'] - 3) + abs(end['y'] - 3), 1)
        self.assertEqual(self.worker['pending_output'], {'wood': 5})

    def test_stock_changes_after_return_and_unloading_only(self):
        before = self.town['wood']
        logistics.record_output(self.town, self.building, 'wood', 5, self.worker)
        self.assertEqual(self.town['wood'], before)
        self.assertFalse(economy.afford(self.town, {'wood': before + 1}))
        phases = self.finish_dropoff()
        self.assertTrue({'working', 'returning', 'depositing'} <= phases)
        self.assertEqual(self.town['wood'], before + 5)
        self.assertEqual(self.building['output_buffer']['wood'], 5)
        logistics.drop_off(self.town, self.building, self.worker)
        self.assertEqual(self.town['wood'], before + 5)

    def test_other_workers_goods_stay_pending(self):
        other = {'job': 'woodcutting'}
        before = self.town['wood']
        logistics.record_output(self.town, self.building, 'wood', 2, self.worker)
        logistics.record_output(self.town, self.building, 'wood', 3, other)
        self.finish_dropoff()
        self.assertEqual(self.town['wood'], before + 2)
        self.assertEqual(self.building['pending_output']['wood'], 3)

    def test_save_reload_preserves_unfinished_delivery_without_double_counting(self):
        before = self.town['wood']
        logistics.record_output(self.town, self.building, 'wood', 5, self.worker)
        work_cycles.advance(self.town, self.town, self.worker, self.building, self.source, 0, 3.3)
        restored = json.loads(json.dumps(self.town))
        economy.ensure_inventory(restored)
        self.assertEqual(restored['wood'], before)
        worker, building, source = restored['villagers'][0], restored['buildings'][1], restored['map_resources'][0]
        self.finish_dropoff(restored, worker, building, source)
        self.assertEqual(restored['wood'], before + 5)
        self.finish_dropoff(restored, worker, building, source)
        self.assertEqual(restored['wood'], before + 5)

    def test_reassignment_preserves_goods_for_the_next_delivery(self):
        before = self.town['wood']
        logistics.record_output(self.town, self.building, 'wood', 5, self.worker)
        server.command(self.town, {'action': 'job', 'index': 0, 'job': 'idle'})
        self.assertEqual(self.town['wood'], before)
        self.assertEqual(self.building['orphaned_output']['wood'], 5)
        server.assign_settler(self.town, 0, 1)
        logistics.record_output(self.town, self.building, 'wood', 1, self.worker)
        self.finish_dropoff()
        self.assertEqual(self.town['wood'], before + 6)

    def test_orphaned_goods_are_counted_once_when_a_hauler_delivers_them(self):
        before = self.town['wood']
        logistics.record_output(self.town, self.building, 'wood', 5, self.worker)
        logistics.cancel_output(self.town, self.worker)
        logistics.transfer(self.town, self.building, self.town['buildings'][0], 'collect', self.worker)
        self.assertEqual(self.town['wood'], before + 5)
        self.assertEqual(logistics.buffered(self.building), 0)
        logistics.transfer(self.town, self.building, self.town['buildings'][0], 'collect', self.worker)
        self.assertEqual(self.town['wood'], before + 5)

    def test_server_ticks_produce_pending_goods_and_release_them_on_dropoff(self):
        before = self.town['wood']
        world = {'towns': {'own': self.town}}
        server.tick(world)
        self.assertEqual(self.town['wood'], before)
        self.assertEqual(self.building['pending_output']['wood'], .75)
        for _ in range(10):
            server.tick(world)
            if self.town['wood'] > before:
                break
        self.assertGreater(self.town['wood'], before)
        self.assertAlmostEqual(self.town['wood'] - before + self.building['pending_output']['wood'],
                               self.town['produced']['wood'])

    def test_gathered_goods_finish_delivery_after_the_source_disappears(self):
        before = self.town['wood']
        world = {'towns': {'own': self.town}}
        server.tick(world)
        self.town['map_resources'].remove(self.source)
        for _ in range(10):
            server.tick(world)
        self.assertEqual(self.town['wood'], before + .75)

    def test_food_becomes_edible_at_dropoff(self):
        before = self.town['food']
        self.building['kind'] = 'farm'
        logistics.record_output(self.town, self.building, 'vegetables', 5, self.worker)
        self.assertEqual(self.town['food'], before)
        logistics.drop_off(self.town, self.building, self.worker)
        self.assertEqual(self.town['food'], before + 5)
        self.assertEqual(self.town['goods']['vegetables'], 5)

    def test_pending_goods_occupy_capacity_but_cannot_supply_processors(self):
        logistics.record_output(self.town, self.building, 'wood', 50, self.worker)
        self.assertEqual(logistics.buffered(self.building), 50)
        self.assertEqual(economy.local_room(self.building, 'wood'), 0)
        self.assertEqual(logistics.production_stock(self.building, 'wood'), 0)

    def test_moving_a_workplace_preserves_its_pending_goods(self):
        before = self.town['wood']
        logistics.record_output(self.town, self.building, 'wood', 5, self.worker)
        server.command(self.town, {'action': 'move', 'index': 1, 'x': 4, 'y': 3})
        self.assertEqual(self.town['wood'], before)
        self.assertEqual(self.building['orphaned_output']['wood'], 5)
        logistics.record_output(self.town, self.building, 'wood', 1, self.worker)
        self.finish_dropoff()
        self.assertEqual(self.town['wood'], before + 6)

    def test_harvested_farming_cargo_returns_without_another_water_trip(self):
        self.worker['job'] = 'farming'
        self.building['kind'] = 'farm'
        self.source.update(kind='acre', farm_index=1)
        self.town['buildings'].append(self.source)
        self.town['map_resources'] = [{'kind': 'water', 'x': 5, 'y': 1}]
        before = self.town['food']
        logistics.record_output(self.town, self.building, 'vegetables', 5, self.worker)
        phases = self.finish_dropoff()
        self.assertTrue({'returning', 'depositing'} <= phases)
        self.assertFalse({'fetching_water', 'collecting_water', 'returning_water'} & phases)
        self.assertEqual(self.town['food'], before + 5)

    def test_blocked_return_does_not_release_goods_and_can_recover(self):
        before = self.town['wood']
        logistics.record_output(self.town, self.building, 'wood', 5, self.worker)
        for x, y in [(2, 3), (4, 3), (3, 2), (3, 4)]:
            self.town['map_resources'].append({'kind': 'rock', 'x': x, 'y': y})
        for _ in range(5):
            finished = work_cycles.advance(self.town, self.town, self.worker, self.building, self.source, 0, 3)
            self.assertFalse(finished)
        self.assertEqual(self.town['wood'], before)
        self.town['map_resources'] = [self.source]
        self.finish_dropoff()
        self.assertEqual(self.town['wood'], before + 5)

    def test_fisher_returns_from_the_pier_before_food_is_available(self):
        self.worker['job'] = 'fishing'
        self.building['kind'] = 'fishing_hut'
        self.source['kind'] = 'water'
        before = self.town['food']
        logistics.record_output(self.town, self.building, 'fish', 5, self.worker)
        phases = self.finish_dropoff()
        self.assertTrue({'walking', 'working', 'returning', 'depositing'} <= phases)
        self.assertEqual(self.town['food'], before + 5)


if __name__ == '__main__':
    unittest.main()
