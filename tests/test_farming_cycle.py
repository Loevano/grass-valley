from fixtures import flat_town
import copy
import json
import unittest

import farming
import progression as economy
import server
import work_cycles
from navigation import routes


class FarmingCycle(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Fields')
        self.town.update(terrain=['g' * 16] * 16, map_size=16, map_resources=[], npcs=[],
                         forage_version=1, forage_halls=[[5, 5]], sandbox=True)
        self.farm = {'kind': 'farm', 'x': 3, 'y': 3}
        self.acre = {'kind': 'acre', 'x': 3, 'y': 4, 'farm_index': 1, 'crop': 'wheat'}
        self.well = {'kind': 'well', 'x': 9, 'y': 4}
        self.town['buildings'].extend([self.farm, self.acre, self.well])
        self.worker = self.town['villagers'][0]
        self.worker.update(job='farming', building_index=1, talents={},
                           construction_position={'x': 3, 'y': 4})
        self.world = {'towns': {'fields': self.town}}

    def advance(self, seconds=2):
        return work_cycles.advance(self.town, self.town, self.worker, self.farm, self.acre, 0, seconds)

    def test_new_placement_and_old_fields_start_unplowed(self):
        self.assertEqual(farming.field_state(self.acre)['stage'], 'unplowed')
        server.command(self.town, {'action': 'build', 'kind': 'acre', 'farm_index': 1, 'x': 4, 'y': 4})
        self.assertEqual(self.town['buildings'][-1]['field']['stage'], 'unplowed')

    def test_field_playback_keeps_before_and_after_action_snapshots(self):
        self.advance()
        self.advance()
        samples = self.worker['work_animation']['playback']
        self.assertEqual(samples[0]['farmField']['stage'], 'plowed')
        completed = next(s for s in samples if s['farmField']['stage'] == 'sprouting')
        self.assertEqual(completed['time'], farming.WORK_SECONDS)
        farming.grow(self.town, 5)
        self.assertEqual(completed['farmField']['elapsed'], 0)
        self.assertEqual(self.acre['field']['elapsed'], 5)

    def test_harvest_playback_clears_at_action_completion(self):
        farming.field_state(self.acre).update(stage='ripe', crop='wheat')
        server.tick(self.world)
        samples = self.worker['work_animation']['playback']
        self.assertEqual(samples[0]['farmField']['stage'], 'ripe')
        cleared = next(s for s in samples if s['farmField']['stage'] == 'unplowed')
        self.assertEqual(cleared['time'], farming.WORK_SECONDS)
        self.assertEqual(cleared['phase'], 'harvest_ready')
        self.assertEqual(cleared['farmField']['cycle'], 1)

    def test_seeding_waits_for_own_farms_acres_only(self):
        second = {'kind': 'acre', 'x': 4, 'y': 4, 'farm_index': 1}
        other_farm = {'kind': 'farm', 'x': 12, 'y': 10}
        other_acre = {'kind': 'acre', 'x': 12, 'y': 11, 'farm_index': 5}
        self.town['buildings'].extend([second, other_farm, other_acre])
        self.advance()
        self.assertEqual(self.acre['field']['stage'], 'plowed')
        self.advance(20)
        self.assertEqual(self.worker['work_animation']['phase'], 'waiting_plowing')
        self.assertEqual(self.acre['field']['stage'], 'plowed')
        farming.complete_action(second)
        self.advance()
        self.assertEqual(self.acre['field']['stage'], 'sprouting')
        self.assertEqual(farming.field_state(other_acre)['stage'], 'unplowed')

    def test_farmer_moves_to_remaining_unplowed_acres(self):
        second = {'kind': 'acre', 'x': 4, 'y': 4, 'farm_index': 1}
        self.town['buildings'].append(second)
        server.tick(self.world)
        self.assertEqual(self.acre['field']['stage'], 'plowed')
        self.assertIs(server.work_source(self.town, 'farming', 0), second)
        server.tick(self.world)
        self.assertEqual(second['field']['stage'], 'plowed')

    def test_unfinished_first_cycle_takes_precedence_over_replowing(self):
        farming.field_state(self.acre).update(stage='plowed', cycle=0)
        replanted = {'kind': 'acre', 'x': 4, 'y': 3, 'farm_index': 1,
                     'field': {'stage': 'unplowed', 'elapsed': 0, 'progress': 0, 'cycle': 2}}
        self.town['buildings'].append(replanted)
        self.assertIs(server.work_source(self.town, 'farming', 0), self.acre)
        self.advance()
        self.assertEqual(self.acre['field']['stage'], 'sprouting')
        self.assertEqual(replanted['field']['stage'], 'unplowed')

    def test_growth_survives_reload_and_needs_no_farmer_or_storage_room(self):
        self.advance()
        self.advance()
        self.worker['job'] = 'idle'
        self.farm['output_buffer'] = {'wheat': 10}
        farming.grow(self.town, 6)
        restored = json.loads(json.dumps(self.town))
        acre = restored['buildings'][2]
        farming.grow(restored, 8)
        self.assertEqual(acre['field']['stage'], 'sprouting')
        self.assertEqual(acre['field']['elapsed'], 14)
        farming.grow(restored, 1)
        self.assertEqual(acre['field']['stage'], 'thirsty')
        farming.grow(restored, 100)
        self.assertEqual(acre['field']['stage'], 'thirsty')
        farming.complete_action(acre)
        farming.grow(restored, 14)
        self.assertEqual(acre['field']['stage'], 'growing')
        farming.grow(restored, 1)
        self.assertEqual(acre['field']['stage'], 'ripe')

    def test_one_water_collection_per_round_then_harvest_and_delivery(self):
        collections = 0
        previous_phase = None
        saw_harvest = False
        for _ in range(45):
            server.tick(self.world)
            animation = self.worker.get('work_animation', {})
            for sample in animation.get('playback', []):
                phase = sample['phase']
                if phase == 'collecting_water' and previous_phase != phase:
                    collections += 1
                previous_phase = phase
            if self.worker.get('pending_output') and not saw_harvest:
                saw_harvest = True
                self.assertEqual(collections, 1)
                self.assertEqual(self.town.get('goods', {}).get('wheat', 0), 0)
                self.assertTrue(any(sample.get('farmAction') == 'harvesting'
                                    for sample in animation['playback']))
                self.assertEqual(self.acre['field']['stage'], 'unplowed')
            if self.town.get('goods', {}).get('wheat', 0) > 0:
                break
        self.assertTrue(saw_harvest)
        self.assertEqual(collections, 1)
        self.assertEqual(self.town['goods']['wheat'], farming.HARVEST_YIELD)

    def test_filled_can_covers_remaining_acres_after_save_and_state_reset(self):
        second = {'kind': 'acre', 'x': 4, 'y': 4, 'farm_index': 1, 'crop': 'wheat'}
        self.town['buildings'].append(second)
        for acre in (self.acre, second):
            farming.field_state(acre).update(stage='thirsty')
        self.advance(30)
        self.assertEqual(self.acre['field']['stage'], 'growing')
        restored = json.loads(json.dumps(self.town))
        farm, second = restored['buildings'][1], restored['buildings'][-1]
        worker = restored['villagers'][0]
        worker.pop('work_animation')  # A logistics trip can replace the animation state.
        restored['buildings'] = [b for b in restored['buildings'] if b['kind'] != 'well']
        work_cycles.advance(restored, restored, worker, farm, second, 0, 30)
        self.assertEqual(second['field']['stage'], 'growing')
        self.assertTrue(worker['work_animation']['waterReady'])
        self.assertFalse(any(s['phase'] in ('fetching_water', 'collecting_water', 'returning_water')
                             for s in worker['work_animation']['playback']))

    def test_next_crop_round_requires_a_new_water_fill(self):
        farming.field_state(self.acre).update(stage='thirsty')
        self.advance(30)
        self.assertEqual(self.worker['watering_round'], ['farm', 3, 3, 0])
        self.acre['field'].update(stage='thirsty', cycle=1, elapsed=0, progress=0)
        self.advance(30)
        self.assertEqual(self.worker['watering_round'], ['farm', 3, 3, 1])
        self.assertTrue(any(s['phase'] == 'collecting_water' for s in self.worker['work_animation']['playback']))

    def test_another_farmer_still_collects_their_own_water(self):
        second = {'kind': 'acre', 'x': 4, 'y': 4, 'farm_index': 1, 'crop': 'wheat'}
        self.town['buildings'].append(second)
        for acre in (self.acre, second):
            farming.field_state(acre).update(stage='thirsty')
        self.advance(30)
        other = self.town['villagers'][1]
        other.update(job='farming', building_index=1, construction_position={'x': 4, 'y': 4})
        work_cycles.advance(self.town, self.town, other, self.farm, second, 1, 30)
        self.assertTrue(any(s['phase'] == 'collecting_water' for s in other['work_animation']['playback']))
        self.assertEqual(self.worker['watering_round'], ['farm', 3, 3, 0])
        self.assertEqual(other['watering_round'], ['farm', 3, 3, 0])
        self.assertEqual(self.acre['field']['stage'], 'growing')
        self.assertEqual(second['field']['stage'], 'growing')

    def test_missing_water_waits_and_new_well_resumes_same_crop(self):
        self.advance()
        self.advance()
        farming.grow(self.town, 15)
        self.town['buildings'].remove(self.well)
        self.advance(30)
        self.assertEqual(self.worker['work_animation']['phase'], 'waiting_water')
        self.assertEqual(self.acre['field']['stage'], 'thirsty')
        self.town['buildings'].append(self.well)
        for _ in range(20):
            self.advance(1)
            if self.acre['field']['stage'] == 'growing':
                break
        self.assertEqual(self.acre['field']['stage'], 'growing')
        self.assertEqual(self.acre['field']['crop'], 'wheat')

    def test_two_workers_do_not_collect_water_for_the_same_acre(self):
        self.advance()
        self.advance()
        farming.grow(self.town, 15)
        self.advance(1)
        second = self.town['villagers'][1]
        second.update(job='farming', building_index=1, construction_position={'x': 3, 'y': 4})
        work_cycles.advance(self.town, self.town, second, self.farm, self.acre, 1, 5)
        self.assertEqual(second['work_animation']['phase'], 'waiting_field')
        self.assertNotIn('waterSource', second['work_animation'])

    def test_blocked_water_return_keeps_the_filled_can(self):
        self.advance()
        self.advance()
        farming.grow(self.town, 15)
        for _ in range(15):
            self.advance(1)
            if self.worker['work_animation']['phase'] == 'returning_water':
                break
        animation = self.worker['work_animation']
        self.assertEqual(animation['phase'], 'returning_water')
        obstruction = {'kind': 'rock', **animation['path'][0]}
        self.town['map_resources'].append(obstruction)
        self.advance(.1)
        self.assertEqual(animation['phase'], 'waiting')
        self.assertTrue(animation['waterReady'])
        self.town['map_resources'].remove(obstruction)
        self.advance(20)
        self.assertEqual(self.acre['field']['stage'], 'growing')
        self.assertNotIn('collecting_water', {sample['phase'] for sample in self.worker['work_animation']['playback']})

    def test_crop_changes_apply_to_next_seeding(self):
        self.advance()
        self.advance()
        self.acre['crop'] = 'vegetables'
        self.assertEqual(self.acre['field']['crop'], 'wheat')
        snapshot = copy.deepcopy(self.acre['field'])
        farming.grow(self.town, 3)
        self.assertEqual(self.acre['field']['crop'], snapshot['crop'])

    def test_partial_harvest_retains_uncollected_yield(self):
        field = farming.field_state(self.acre)
        field.update(stage='ripe', crop='wheat', progress=2)
        self.farm['output_buffer'] = {'wheat': 9.75}
        economy.ensure_inventory(self.town)
        server.tick(self.world)
        self.assertEqual(self.worker['pending_output'], {'wheat': .25})
        self.assertEqual(field['yield_remaining'], farming.HARVEST_YIELD - .25)
        self.assertEqual(field['stage'], 'ripe')
        self.assertEqual(field['cycle'], 0)

    def test_all_four_actions_follow_rows_with_shorter_actual_routes(self):
        for stage in farming.ACTIONS:
            with self.subTest(stage=stage):
                farm = {'kind': 'farm', 'x': 8, 'y': 8}
                fields = [{'kind': 'acre', 'x': x, 'y': y, 'farm_index': 1,
                           'field': {'stage': stage, 'elapsed': 0, 'progress': 0, 'cycle': 0}}
                          for y in range(5, 12) for x in range(5, 12) if (x, y) != (8, 8)]
                worker = {'job': 'farming', 'building_index': 1, 'construction_position': {'x': 8, 'y': 9}}
                town = {'buildings': [{'kind': 'hall', 'x': 2, 'y': 2}, farm, *fields], 'villagers': [worker]}
                position, walking, rows, visited = (8, 9), 0, [], set()
                for _ in fields:
                    target = farming.choose_field(town, farm, fields, 0)
                    tile = target['x'], target['y']
                    self.assertNotIn(tile, visited)
                    visited.add(tile)
                    if not rows or rows[-1] != tile[1]:
                        self.assertNotIn(tile[1], rows, 'Each row finishes before changing rows')
                        rows.append(tile[1])
                    walking += len(routes.route(position, target, {(2, 2), (8, 8)}, 16, ['g' * 16] * 16, False))
                    if stage == 'ripe':
                        target['field'].update(stage='unplowed', cycle=1)
                    else:
                        farming.complete_action(target)
                    position = tile
                    worker['work_animation'] = {'x': tile[0], 'y': tile[1], 'phase': 'idle',
                        'task': ['farming', 'farm', 8, 8, 'acre', *tile, 'field_cycle', 0]}
                self.assertEqual(len(visited), 48)
                # The starting revision's farm-distance ordering needed 235 actual tile steps.
                self.assertLess(walking, 80)

    def test_workers_choose_separate_rows(self):
        second_row = {'kind': 'acre', 'x': 3, 'y': 5, 'farm_index': 1}
        same_row = {'kind': 'acre', 'x': 4, 'y': 4, 'farm_index': 1}
        self.town['buildings'].extend([same_row, second_row])
        self.advance(.5)
        self.town['villagers'][1].update(job='farming', building_index=1)
        self.assertIs(server.work_source(self.town, 'farming', 1), second_row)

    def test_full_unrelated_crop_does_not_stop_multiple_farmers(self):
        second = {'kind': 'acre', 'x': 4, 'y': 5, 'farm_index': 1, 'crop': 'wheat',
                  'stored_goods': {'fodder': 10}}
        self.town['buildings'].append(second)
        self.acre['stored_goods'] = {'fodder': 10}
        self.farm.update(stored_goods={'fodder': 10}, buffer_full=True)
        economy.ensure_inventory(self.town)
        self.town['buildings'][0]['stored_goods']['fodder'] = 100
        self.town['villagers'][1].update(job='farming', building_index=1, talents={},
                                       construction_position={'x': 4, 'y': 5})
        for _ in range(3):
            server.tick(self.world)
        for field in (self.acre, second):
            self.assertEqual(field['field']['stage'], 'sprouting')
        for worker in self.town['villagers'][:2]:
            self.assertGreater(worker['farming'], 0)
            self.assertNotIn('logistics_trip', worker)
        self.assertEqual(self.farm['stored_goods']['fodder'], 10)

    def test_full_unrelated_crop_allows_harvest_and_delivery_after_reload(self):
        self.farm.update(stored_goods={'fodder': 10}, buffer_full=True)
        self.acre['stored_goods'] = {'fodder': 10}
        economy.ensure_inventory(self.town)
        self.town['buildings'][0]['stored_goods']['fodder'] = 100
        farming.field_state(self.acre).update(stage='ripe', crop='wheat')
        restored = json.loads(json.dumps(self.world))
        town = restored['towns']['fields']
        worker = town['villagers'][0]
        server.tick(restored)
        self.assertEqual(worker['pending_output'], {'wheat': farming.HARVEST_YIELD})
        for _ in range(5):
            server.tick(restored)
            if not worker.get('pending_output'):
                break
        self.assertNotIn('pending_output', worker)
        self.assertEqual(town['goods']['wheat'], farming.HARVEST_YIELD)

    def test_current_crop_uses_shared_food_space_and_waits_when_full(self):
        self.acre['crop'] = 'carrots'
        farming.field_state(self.acre).update(stage='ripe', crop='carrots')
        self.farm['stored_goods'] = {'vegetables': 20}
        self.acre['stored_goods'] = {'vegetables': 20}
        economy.ensure_inventory(self.town)
        self.town['buildings'][0]['stored_goods']['food'] = 100
        server.tick(self.world)
        self.assertIn('Workplace buffer full', self.worker['production_status'])
        self.assertNotIn('pending_output', self.worker)
        self.assertEqual(self.acre['field']['progress'], 0)

    def test_full_current_crop_still_hauls_before_resuming(self):
        self.farm['output_buffer'] = {'wheat': 10}
        self.acre['stored_goods'] = {'wheat': 10}
        farming.field_state(self.acre).update(stage='ripe', crop='wheat')
        store = {'kind': 'storehouse', 'x': 12, 'y': 12,
                 'storage_enabled': {'wheat': True}}
        self.town['buildings'].append(store)
        server.tick(self.world)
        self.assertEqual(self.worker['logistics_trip']['operation'], 'collect')
        self.assertNotIn('pending_output', self.worker)
        for _ in range(20):
            server.tick(self.world)
            if self.worker.get('pending_output'):
                break
        self.assertEqual(store['stored_goods']['wheat'], 10)
        self.assertEqual(self.worker['pending_output'], {'wheat': farming.HARVEST_YIELD})

    def test_direct_acre_worker_waits_for_the_older_farm_cycle(self):
        farming.field_state(self.acre).update(stage='plowed', cycle=0)
        newer = {'kind': 'acre', 'x': 4, 'y': 4, 'farm_index': 1,
                 'field': {'stage': 'unplowed', 'elapsed': 0, 'progress': 0, 'cycle': 1}}
        self.town['buildings'].append(newer)
        worker = self.town['villagers'][1]
        worker.update(job='farming', building_index=4, construction_position={'x': 4, 'y': 4})
        work_cycles.advance(self.town, self.town, worker, self.farm, newer, 1, 3)
        self.assertEqual(worker['work_animation']['phase'], 'waiting_pass')
        self.assertEqual(newer['field']['stage'], 'unplowed')

    def test_full_farm_finishes_every_acre_before_repeating(self):
        self.town['buildings'][0].update(x=2, y=2)
        self.farm.update(x=8, y=8)
        self.well.update(x=3, y=8)
        self.town['buildings'].remove(self.acre)
        fields = [{'kind': 'acre', 'x': x, 'y': y, 'farm_index': 1, 'crop': 'wheat'}
                  for y in range(5, 12) for x in range(5, 12) if (x, y) != (8, 8)]
        self.town['buildings'].extend(fields)
        for index, worker in enumerate(self.town['villagers'][:4]):
            worker.update(job='farming', building_index=1, talents={},
                          construction_position={'x': 8, 'y': 9 + index // 3})
        actions = { (a['x'], a['y']): set() for a in fields }
        for _ in range(500):
            server.tick(self.world)
            for worker in self.town['villagers'][:4]:
                animation = worker.get('work_animation', {})
                task = animation.get('task', [])
                if len(task) >= 7 and tuple(task[5:7]) in actions:
                    actions[tuple(task[5:7])].update(sample['farmAction'] for sample in animation.get('playback', [])
                                                   if sample.get('farmAction'))
            cycles = [a['field']['cycle'] for a in fields]
            self.assertLessEqual(max(cycles) - min(cycles), 1)
            # Keep storage available; this check isolates scheduling rather than stock capacity.
            stock = economy.quantity(self.town, 'wheat')
            economy.withdraw(self.town, 'wheat', stock)
            economy.add_stock(self.town, 'wheat', -stock)
            if min(cycles) == 1:
                break
        self.assertEqual(min(a['field']['cycle'] for a in fields), 1)
        self.assertTrue(all(seen == set(farming.ACTIONS.values()) for seen in actions.values()))


if __name__ == '__main__':
    unittest.main()
