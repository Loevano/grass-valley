"""Workplace moves preserve actors while invalidating coordinate-based routes."""
import copy
import json
import unittest

from fixtures import flat_town
import logistics
import farming
import progression as economy
import server
import work_cycles


class Relocation(unittest.TestCase):
    def setUp(self):
        self.town = flat_town()
        self.camp = {'kind': 'woodcutter', 'x': 9, 'y': 3}
        self.tree = {'kind': 'tree', 'x': 12, 'y': 3}
        self.town['buildings'].append(self.camp)
        self.town['map_resources'] = [self.tree]
        self.worker = self.town['villagers'][0]
        self.worker.update(job='woodcutting', building_index=1)
        self.worker['construction_position'] = {'x': 9.25, 'y': 1}
        work_cycles.advance(self.town, self.town, self.worker, self.camp, self.tree, 0, .1)

    def move(self, index=1, x=8, y=4):
        server.command(self.town, {'action': 'move', 'index': index, 'x': x, 'y': y})

    def test_walking_worker_keeps_fractional_position_and_unaffected_worker(self):
        other = self.town['villagers'][1]
        other_camp = {'kind': 'woodcutter', 'x': 2, 'y': 9}
        other_tree = {'kind': 'tree', 'x': 2, 'y': 11}
        self.town['buildings'].append(other_camp)
        self.town['map_resources'].append(other_tree)
        other.update(job='woodcutting', building_index=2, work_target=other_tree,
                     construction_position={'x': 2, 'y': 10})
        work_cycles.advance(self.town, self.town, other, other_camp, other_tree, 1, .1)
        other_before = copy.deepcopy(other)
        before = copy.deepcopy(self.worker['work_animation'])
        self.move()
        self.assertIn('work_animation', self.worker)
        for key in ('x', 'y'):
            self.assertEqual(self.worker['work_animation'][key], before[key])
        self.assertEqual(other, other_before)
        work_cycles.advance(self.town, self.town, self.worker, self.camp, self.tree, 0, 0)
        self.assertEqual(self.worker['work_animation']['task'][2:4], [8, 4])
        self.assertEqual(self.worker['work_animation']['x'], before['x'])

    def test_carried_output_returns_to_moved_workplace_after_reload_once(self):
        logistics.record_output(self.town, self.camp, 'wood', 2, self.worker)
        animation = self.worker['work_animation']
        animation.update(phase='returning', path=[{'x': 10, 'y': 3}], x=11.25, y=2)
        total = self.town['wood']
        self.move()
        self.assertEqual(self.worker.get('pending_output'), {'wood': 2})
        self.assertEqual(self.worker['pending_workplace'], {'kind': 'woodcutter', 'x': 8, 'y': 4})
        self.assertEqual((animation['x'], animation['y']), (11.25, 2))
        self.town = json.loads(json.dumps(self.town))
        server.ensure_resources(self.town)
        self.worker = self.town['villagers'][0]
        self.camp = self.town['buildings'][1]
        self.assertEqual(self.worker['work_animation']['x'], 11.25)
        for _ in range(30):
            if work_cycles.advance(self.town, self.town, self.worker, self.camp, self.tree, 0, .5):
                logistics.drop_off(self.town, self.camp, self.worker)
                break
        self.assertFalse(self.worker.get('pending_output'))
        self.assertEqual(self.camp['output_buffer']['wood'], 2)
        self.assertEqual(self.town['wood'], total + 2)
        logistics.drop_off(self.town, self.camp, self.worker)
        self.assertEqual(self.town['wood'], total + 2)

    def test_work_at_unmoved_source_keeps_progress(self):
        state = self.worker['work_animation']
        state.update(phase='working', path=[], elapsed=.4)
        self.move()
        work_cycles.advance(self.town, self.town, self.worker, self.camp, self.tree, 0, 0)
        self.assertEqual(state['elapsed'], .4)
        self.assertIs(self.worker['work_animation'], state)

    def test_delivery_cancellation_returns_cargo_once_and_keeps_position(self):
        store = {'kind': 'storehouse', 'x': 2, 'y': 2, 'storage_allocations': {'wood': 100}}
        self.town['buildings'].append(store)
        hauler = self.town['villagers'][1]
        hauler.update(job='hauling', building_index=2, logistics_position={'x': 10, 'y': 3})
        self.camp['output_buffer'] = {'wood': 3}
        economy.add_stock(self.town, 'wood', 3)
        self.assertTrue(logistics.start_trip(self.town, self.town, hauler, self.camp, store,
                                             'collect', server.logistics_route, hauler=True))
        logistics.advance_trip(self.town, self.town, hauler, .2)
        self.assertTrue(hauler['logistics_trip']['picked_up'])
        self.assertFalse(hauler['logistics_trip']['delivered'])
        position = copy.deepcopy(hauler['logistics_position'])
        total = self.town['wood']
        self.move()
        self.assertNotIn('logistics_trip', hauler)
        self.assertEqual(hauler['logistics_position'], position)
        self.assertEqual(self.camp['stored_goods']['wood'], 3)
        self.assertEqual(self.town['wood'], total + 3)
        logistics.cancel_trip(self.town, hauler)
        self.assertEqual(self.town['wood'], total + 3)
        self.assertEqual(logistics.collection_inventory(self.camp)['wood'], 3)

    def test_linked_acre_replans_from_saved_pose_and_retains_cargo(self):
        farm = {'kind': 'farm', 'x': 8, 'y': 7}
        acre = {'kind': 'acre', 'x': 9, 'y': 7, 'farm_index': 2}
        self.town['buildings'].extend([farm, acre])
        self.worker.update(job='farming', building_index=3, work_target=acre)
        self.worker.pop('work_animation')
        work_cycles.advance(self.town, self.town, self.worker, farm, acre, 0, 0)
        state = self.worker['work_animation']
        state.update(x=10.25, y=7, phase='returning', path=[{'x': 8, 'y': 6}])
        logistics.record_output(self.town, farm, 'vegetables', 2, self.worker)
        self.move(index=2, x=8, y=8)
        self.assertEqual((acre['x'], acre['y']), (9, 8))
        self.assertEqual(state['task'][2:7], [8, 8, 'acre', 9, 8])
        self.assertEqual(state['path'], [])
        self.assertEqual((state['x'], state['y']), (10.25, 7))
        work_cycles.advance(self.town, self.town, self.worker, farm, acre, 0, 0)
        self.assertEqual(self.worker['work_animation']['phase'], 'returning')
        self.assertEqual(self.worker['pending_output'], {'vegetables': 2})

    def test_removed_workplace_salvages_output_without_losing_actor(self):
        logistics.record_output(self.town, self.camp, 'wood', 2, self.worker)
        position = {k: self.worker['work_animation'][k] for k in ('x', 'y')}
        total = self.town['wood']
        server.command(self.town, {'action': 'destroy', 'index': 1})
        self.assertEqual(self.worker['logistics_position'], position)
        self.assertFalse(self.worker.get('pending_output'))
        self.assertEqual(self.town['wood'], total + 2)
        logistics.cancel_output(self.town, self.worker)
        self.assertEqual(self.town['wood'], total + 2)

    def test_moved_farmer_keeps_filled_can_and_invalidates_old_field_route(self):
        farm = {'kind': 'farm', 'x': 8, 'y': 7}
        acre = {'kind': 'acre', 'x': 9, 'y': 7, 'farm_index': 2}
        self.town['buildings'].extend([farm, acre])
        self.worker.update(job='farming', building_index=3, work_target=acre,
                           watering_round=['farm', 8, 7, 0])
        self.worker.pop('work_animation')
        work_cycles.advance(self.town, self.town, self.worker, farm, acre, 0, 0)
        state = self.worker['work_animation']
        state.update(x=10.25, y=7, phase='returning_water', waterReady=True)
        self.move(index=2, x=8, y=8)
        self.assertEqual(self.worker['watering_round'], ['farm', 8, 8, 0])
        self.assertEqual(state['resume_phase'], 'returning_water')
        self.assertEqual(state['path'], [])
        self.assertEqual(state['relocation'], 1)
        self.assertEqual(len(state['playback']), 1)
        self.assertEqual(state['playback'][0]['x'], 10.25)

    def test_move_out_of_resource_reach_retains_position_while_waiting(self):
        position = {k: self.worker['work_animation'][k] for k in ('x', 'y')}
        self.move(x=1, y=1)
        server.tick({'towns': {'own': self.town}})
        self.assertNotIn('work_animation', self.worker)
        self.assertEqual(self.worker['logistics_position'], position)
        self.assertIn('Waiting for trees', self.worker['production_status'])

    def test_removed_tree_finishes_carried_goods_at_moved_camp(self):
        logistics.record_output(self.town, self.camp, 'wood', 2, self.worker)
        self.worker['work_animation'].update(phase='returning', x=11.25, y=2)
        self.move()
        self.town['map_resources'] = []
        total = self.town['wood']
        for _ in range(10):
            server.tick({'towns': {'own': self.town}})
            if not self.worker.get('pending_output'):
                break
        self.assertFalse(self.worker.get('pending_output'))
        self.assertEqual(self.camp['output_buffer']['wood'], 2)
        self.assertEqual(self.town['wood'], total + 2)

    def test_linked_field_reservation_survives_waiting_for_relocation_route(self):
        farm = {'kind': 'farm', 'x': 8, 'y': 7, 'settler_level': 2}
        acre = {'kind': 'acre', 'x': 9, 'y': 7, 'farm_index': 2}
        self.town['buildings'].extend([farm, acre])
        self.worker.update(job='farming', building_index=2, work_target=acre)
        self.worker.pop('work_animation')
        work_cycles.advance(self.town, self.town, self.worker, farm, acre, 0, 0)
        self.worker['work_animation'].update(x=10.25, y=7, phase='working', path=[])
        self.town['villagers'][1].update(job='farming', building_index=2)
        self.move(index=2, x=8, y=8)
        self.assertTrue(farming.claimed(self.town, acre, 1))
        self.assertEqual(self.worker['work_animation']['task'][5:7], [9, 8])
