from fixtures import flat_town
import copy
import json
import math
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

    def advance(self, seconds=3):
        return work_cycles.advance(self.town, self.town, self.worker, self.farm, self.acre, 0, seconds)

    def test_new_placement_and_old_fields_start_unplowed(self):
        self.assertEqual(farming.field_state(self.acre)['stage'], 'unplowed')
        server.command(self.town, {'action': 'build', 'kind': 'acre', 'farm_index': 1, 'x': 4, 'y': 4})
        self.assertEqual(self.town['buildings'][-1]['field']['stage'], 'unplowed')

    def test_field_playback_keeps_before_and_after_action_snapshots(self):
        self.advance()
        self.advance(3)
        samples = self.worker['work_animation']['playback']
        self.assertEqual(samples[0]['farmField']['stage'], 'plowed')
        completed = next(s for s in samples if s['farmField']['stage'] == 'sprouting')
        self.assertAlmostEqual(completed['time'], farming.WORK_SECONDS)
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
        self.advance(3)
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

    def test_next_acre_uses_the_rest_of_the_tick(self):
        second = {'kind': 'acre', 'x': 4, 'y': 4, 'farm_index': 1}
        self.town['buildings'].append(second)
        server.tick(self.world)
        self.assertEqual(self.acre['field']['stage'], 'plowed')
        approach = math.hypot(.45, .1) / 1.8
        next_approach = 1 / 1.8
        self.assertAlmostEqual(second['field']['progress'], 1 - approach - next_approach)
        samples = self.worker['work_animation']['playback']
        self.assertEqual(samples[-1]['time'], server.TICK_SECONDS)
        walking = next(s for s in samples if s['phase'] == 'walking' and s['farmField']['x'] == second['x'])
        self.assertAlmostEqual(walking['time'], farming.WORK_SECONDS + math.hypot(.45, .1) / 1.8)
        self.assertEqual(walking['farmField']['x'], second['x'])

    def test_last_plowing_starts_sowing_without_waiting_for_a_tick(self):
        server.tick(self.world)
        self.assertEqual(self.acre['field']['stage'], 'plowed')
        self.assertAlmostEqual(self.acre['field']['progress'], 1 - math.hypot(.45, .1) / 1.8)
        self.assertEqual(self.worker['work_animation']['farmAction'], 'sowing')

    def harvest_fields(self, crops=('vegetables', 'vegetables')):
        self.acre.update(y=5, crop=crops[0])
        fields = [self.acre]
        for x, crop in enumerate(crops[1:], 4):
            field = {'kind': 'acre', 'x': x, 'y': 5, 'farm_index': 1, 'crop': crop}
            self.town['buildings'].append(field)
            fields.append(field)
        for field in fields:
            farming.field_state(field).update(stage='ripe', crop=field['crop'])
        self.worker['construction_position'] = {'x': 3, 'y': 5}
        return fields

    def test_maximum_farm_holds_two_full_upgraded_harvests(self):
        self.farm.update(level=3, crop='wheat', x=8, y=8)
        self.town['buildings'][0].update(x=2, y=2)
        fields = [{'kind': 'acre', 'x': x, 'y': y, 'farm_index': 1,
                   'level': 3, 'crop': 'wheat'}
                  for y in range(5, 12) for x in range(5, 12) if (x, y) != (8, 8)]
        self.town['buildings'] = [self.town['buildings'][0], self.farm, *fields]
        self.worker['construction_position'] = {'x': 5, 'y': 5}
        self.assertEqual(farming.storage_capacity(self.farm), 1920)
        for cycle in range(2):
            for field in fields:
                farming.field_state(field).update(stage='ripe', crop='wheat', cycle=cycle, progress=0)
            source = server.work_source(self.town, 'farming', 0)
            server.advance_farmer(self.town, self.town, self.worker, self.farm, source, 0, 180)
            self.assertTrue(all(field['field']['cycle'] == cycle + 1 for field in fields))
            self.assertNotIn('pending_output', self.worker)
            self.assertEqual(self.farm['output_buffer']['wheat'], 720 * (cycle + 1))
        self.assertEqual(economy.local_room(self.farm, 'wheat'), 480)

    def test_blocked_storage_clears_the_previous_harvest_pose(self):
        farming.field_state(self.acre).update(stage='ripe', crop='wheat')
        self.advance(.5)
        self.assertEqual(self.worker['work_animation']['farmAction'], 'harvesting')
        self.farm['output_buffer'] = {'wheat': farming.storage_capacity(self.farm)}
        self.acre['stored_goods'] = {'wheat': farming.storage_capacity(self.acre)}
        economy.ensure_inventory(self.town)
        self.town['buildings'][0]['stored_goods']['wheat'] = 100
        server.tick(self.world)
        animation = self.worker['work_animation']
        self.assertEqual(animation['phase'], 'waiting_storage')
        self.assertIsNone(animation['farmAction'])
        self.assertTrue(all(s['farmAction'] is None for s in animation['playback']))
        self.assertEqual(self.acre['field']['progress'], .5)
        self.farm['output_buffer']['wheat'] = 0
        server.tick(self.world)
        self.assertEqual(self.acre['field']['stage'], 'unplowed')
        self.assertTrue(any(s['farmAction'] == 'harvesting' for s in self.worker['work_animation']['playback']))

    def test_harvest_batch_visits_acres_before_returning_to_farm(self):
        fields = self.harvest_fields()
        server.advance_farmer(self.town, self.town, self.worker, self.farm, fields[0], 0, 5)
        self.assertTrue(all(field['field']['cycle'] == 1 for field in fields))
        self.assertEqual(self.worker['pending_output'], {'vegetables': 9.6})
        self.assertEqual(self.farm['pending_output'], {'vegetables': 9.6})
        self.assertEqual(self.town.get('goods', {}).get('vegetables', 0), 0)
        animation = self.worker['work_animation']
        self.assertEqual(animation['phase'], 'returning')
        harvests = [s for s in animation['playback'] if s['phase'] == 'harvest_ready']
        returning = next(s for s in animation['playback'] if s['phase'] == 'returning')
        self.assertGreater(returning['time'], harvests[-1]['time'] - 1e-9)
        self.assertAlmostEqual(harvests[-1]['time'], 4 + 1 / 1.8)

    def test_harvest_batch_returns_early_at_existing_storage_allowance(self):
        fields = self.harvest_fields(('vegetables',) * 3)
        self.farm['stored_goods'] = {'vegetables': farming.storage_capacity(self.farm) - 9.6}
        server.advance_farmer(self.town, self.town, self.worker, self.farm, fields[0], 0, 5)
        self.assertEqual([field['field']['stage'] for field in fields], ['unplowed', 'unplowed', 'ripe'])
        self.assertEqual(self.worker['pending_output'], {'vegetables': 9.6})
        self.assertEqual(self.worker['work_animation']['phase'], 'returning')
        self.assertEqual(economy.local_room(self.farm, 'vegetables'), 0)

    def test_harvest_batch_finishes_ripe_acres_across_farm_passes(self):
        for stage in ('unplowed', 'plowed', 'sprouting', 'thirsty', 'growing', 'ripe'):
            with self.subTest(stage=stage):
                self.setUp()
                fields = self.harvest_fields()
                # End exactly at the first harvest, before choosing the next acre.
                server.advance_farmer(self.town, self.town, self.worker, self.farm, fields[0], 0, 2)
                self.assertEqual(self.worker['pending_output'], {'vegetables': 4.8})
                fields[1]['field']['cycle'] = 2
                blocker = {'kind': 'acre', 'x': 5, 'y': 5, 'farm_index': 1}
                farming.field_state(blocker).update(stage=stage)
                self.town['buildings'].append(blocker)

                source = server.work_source(self.town, 'farming', 0)
                self.assertIs(source, fields[1])
                server.advance_farmer(self.town, self.town, self.worker, self.farm, source, 0, 3)
                self.assertEqual(fields[1]['field']['stage'], 'unplowed')
                self.assertEqual(self.worker['pending_output'], {'vegetables': 9.6})
                self.assertFalse(any(s['phase'] in ('returning', 'depositing')
                                     for s in self.worker['work_animation']['playback']
                                     if s['time'] < 2 + 1 / 1.8 - 1e-9))

    def test_harvest_batch_does_not_visit_another_farms_ripe_acres(self):
        fields = self.harvest_fields(('vegetables',))
        other_farm = {'kind': 'farm', 'x': 6, 'y': 3}
        other_index = len(self.town['buildings'])
        other_acre = {'kind': 'acre', 'x': 4, 'y': 5, 'farm_index': other_index}
        farming.field_state(other_acre).update(stage='ripe')
        self.town['buildings'].extend([other_farm, other_acre])

        server.advance_farmer(self.town, self.town, self.worker, self.farm, fields[0], 0, 3)
        self.assertEqual(self.worker['pending_output'], {'vegetables': 4.8})
        self.assertIn(self.worker['work_animation']['phase'], ('returning', 'depositing'))
        self.assertEqual(other_acre['field']['stage'], 'ripe')

    def test_harvest_batch_combines_crops_then_returns_when_no_acres_remain(self):
        fields = self.harvest_fields(('wheat', 'vegetables', 'fodder'))
        server.advance_farmer(self.town, self.town, self.worker, self.farm, fields[0], 0, 8)
        self.assertTrue(all(field['field']['cycle'] == 1 for field in fields))
        self.assertEqual(self.worker['pending_output'], {'wheat': 10, 'vegetables': 4.8, 'fodder': 10})
        self.assertEqual(self.worker['work_animation']['phase'], 'returning')
        self.assertGreater(economy.local_room(self.farm, 'vegetables'), 0)

    def test_interrupted_harvest_batch_preserves_all_carried_crops(self):
        fields = self.harvest_fields()
        server.advance_farmer(self.town, self.town, self.worker, self.farm, fields[0], 0, 5)
        server.logistics.cancel_output(self.town, self.worker)
        self.assertNotIn('pending_output', self.worker)
        self.assertEqual(self.farm['orphaned_output'], {'vegetables': 9.6})
        self.assertEqual(self.farm['pending_output'], {'vegetables': 9.6})
        self.assertEqual(self.town.get('goods', {}).get('vegetables', 0), 0)

    def test_harvest_batch_survives_reload_and_is_delivered_once(self):
        fields = self.harvest_fields()
        server.tick(self.world)
        self.assertEqual(self.worker['pending_output'], {'vegetables': 4.8})
        self.assertEqual(self.worker['work_animation']['task'][5:7], [4, 5])
        restored = json.loads(json.dumps(self.world))
        town = restored['towns']['fields']
        worker = town['villagers'][0]
        for _ in range(4):
            server.tick(restored)
            if not worker.get('pending_output'):
                break
        self.assertNotIn('pending_output', worker)
        self.assertEqual(town['produced']['vegetables'], 9.6)
        self.assertEqual(town['goods']['vegetables'], 9.6)
        self.assertEqual(town['buildings'][1]['output_buffer']['vegetables'], 9.6)

    def test_harvest_batch_respects_production_cap_with_multiple_farmers(self):
        self.farm['settler_level'] = 2
        self.harvest_fields()
        self.town.setdefault('rules', {}).setdefault('production_caps', {})['vegetables'] = 9
        other = self.town['villagers'][1]
        other.update(job='farming', building_index=1, talents={},
                     construction_position={'x': 4, 'y': 5})
        server.tick(self.world)
        self.assertEqual(self.town['produced']['vegetables'], 9)
        self.assertEqual(economy.production_room(self.town, 'vegetables'), 0)
        self.assertAlmostEqual(sum(farming.field_state(field).get('yield_remaining', 0)
                                   for field in farming.acres(self.town, self.farm)), .6)

    def test_returning_harvest_does_not_change_destination_when_room_opens(self):
        fields = self.harvest_fields(('vegetables',) * 3)
        self.farm['stored_goods'] = {'vegetables': farming.storage_capacity(self.farm) - 9.6}
        server.advance_farmer(self.town, self.town, self.worker, self.farm, fields[0], 0, 5)
        # A newly available crop allowance must not interrupt unloading.
        fields[2]['crop'] = fields[2]['field']['crop'] = 'wheat'
        self.assertGreater(farming.harvest_room(self.town, self.farm, fields[2]), 0)
        self.assertIs(server.work_source(self.town, 'farming', 0), fields[1])
        self.assertEqual(self.worker['work_animation']['phase'], 'returning')

    def test_unfinished_first_cycle_takes_precedence_over_replowing(self):
        farming.field_state(self.acre).update(stage='plowed', cycle=0)
        replanted = {'kind': 'acre', 'x': 4, 'y': 3, 'farm_index': 1,
                     'field': {'stage': 'unplowed', 'elapsed': 0, 'progress': 0, 'cycle': 2}}
        self.town['buildings'].append(replanted)
        self.assertIs(server.work_source(self.town, 'farming', 0), self.acre)
        self.advance(3)
        self.assertEqual(self.acre['field']['stage'], 'sprouting')
        self.assertEqual(replanted['field']['stage'], 'unplowed')

    def test_growth_survives_reload_and_needs_no_farmer_or_storage_room(self):
        self.advance()
        self.advance(3)
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
            if self.town.get('produced', {}).get('wheat', 0) and not saw_harvest:
                saw_harvest = True
                self.assertEqual(collections, 1)
                self.assertEqual(self.town.get('goods', {}).get('wheat', 0)
                                 + self.worker.get('pending_output', {}).get('wheat', 0), farming.HARVEST_YIELD)
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
        self.advance(3)
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
        self.advance(3)
        farming.grow(self.town, 15)
        self.advance(1)
        second = self.town['villagers'][1]
        second.update(job='farming', building_index=1, construction_position={'x': 3, 'y': 4})
        work_cycles.advance(self.town, self.town, second, self.farm, self.acre, 1, 5)
        self.assertEqual(second['work_animation']['phase'], 'waiting_field')
        self.assertNotIn('waterSource', second['work_animation'])

    def test_blocked_water_return_keeps_the_filled_can(self):
        self.advance()
        self.advance(3)
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
        self.advance(3)
        self.acre['crop'] = 'vegetables'
        self.assertEqual(self.acre['field']['crop'], 'wheat')
        snapshot = copy.deepcopy(self.acre['field'])
        farming.grow(self.town, 3)
        self.assertEqual(self.acre['field']['crop'], snapshot['crop'])

    def test_partial_harvest_retains_uncollected_yield(self):
        field = farming.field_state(self.acre)
        field.update(stage='ripe', crop='wheat', progress=2)
        self.farm['output_buffer'] = {'wheat': farming.storage_capacity(self.farm) - .25}
        economy.ensure_inventory(self.town)
        server.tick(self.world)
        self.assertEqual(self.town['produced']['wheat'], .25)
        self.assertEqual(self.farm['output_buffer']['wheat'], farming.storage_capacity(self.farm))
        self.assertEqual(field['yield_remaining'], farming.HARVEST_YIELD - .25)
        self.assertEqual(field['stage'], 'ripe')
        self.assertEqual(field['cycle'], 0)

    def test_crop_specific_base_harvests_for_acres_and_legacy_farms(self):
        for kind in ('acre', 'farm'):
            for crop, expected in (('vegetables', 4.8), ('wheat', 10), ('carrots', 10), ('fodder', 10)):
                with self.subTest(kind=kind, crop=crop):
                    self.setUp()
                    source = self.acre if kind == 'acre' else self.farm
                    farming.field_state(source).update(stage='ripe', crop=crop)
                    self.worker['work_animation'] = {}
                    self.assertTrue(server.harvest_crop(self.town, self.worker, self.farm, source))
                    self.assertEqual(self.worker['pending_output'], {crop: expected})
                    self.assertEqual(source['field']['cycle'], 1)

    def test_saved_partial_vegetable_harvest_keeps_its_remaining_yield(self):
        farming.field_state(self.acre).update(stage='ripe', crop='vegetables', yield_remaining=4)
        self.worker['work_animation'] = {}
        restored = json.loads(json.dumps(self.world))
        town = restored['towns']['fields']
        self.assertTrue(server.harvest_crop(town, town['villagers'][0], town['buildings'][1], town['buildings'][2]))
        self.assertEqual(town['villagers'][0]['pending_output'], {'vegetables': 4})
        self.assertEqual(town['buildings'][2]['field']['cycle'], 1)

    def test_vegetable_harvest_reduction_keeps_manure_bonus(self):
        farming.field_state(self.acre).update(stage='ripe', crop='vegetables')
        self.town.setdefault('goods', {})['manure'] = .1
        self.worker['work_animation'] = {}
        self.assertTrue(server.harvest_crop(self.town, self.worker, self.farm, self.acre))
        self.assertEqual(self.worker['pending_output'], {'vegetables': 6})
        self.assertEqual(economy.quantity(self.town, 'manure'), 0)

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
        self.farm['settler_level'] = 2
        second_row = {'kind': 'acre', 'x': 3, 'y': 5, 'farm_index': 1}
        same_row = {'kind': 'acre', 'x': 4, 'y': 4, 'farm_index': 1}
        self.town['buildings'].extend([same_row, second_row])
        self.advance(.5)
        self.town['villagers'][1].update(job='farming', building_index=1)
        self.assertIs(server.work_source(self.town, 'farming', 1), second_row)

    def test_full_unrelated_crop_does_not_stop_multiple_farmers(self):
        self.farm['settler_level'] = 2
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
        self.assertEqual(town['produced']['wheat'], farming.HARVEST_YIELD)
        for _ in range(5):
            server.tick(restored)
            if not worker.get('pending_output'):
                break
        self.assertNotIn('pending_output', worker)
        self.assertEqual(town['goods']['wheat'], farming.HARVEST_YIELD)

    def test_current_crop_uses_its_own_storage_and_waits_when_full(self):
        self.farm['crop'] = 'carrots'
        self.acre['crop'] = 'carrots'
        farming.field_state(self.acre).update(stage='ripe', crop='carrots')
        self.farm['stored_goods'] = {'vegetables': 20, 'carrots': farming.storage_capacity(self.farm)}
        self.acre['stored_goods'] = {'vegetables': 20, 'carrots': 20}
        economy.ensure_inventory(self.town)
        self.town['buildings'][0]['stored_goods']['carrots'] = 100
        server.tick(self.world)
        self.assertIn('Workplace buffer full', self.worker['production_status'])
        self.assertNotIn('pending_output', self.worker)
        self.assertEqual(self.acre['field']['stage'], 'ripe')
        self.assertEqual(self.town.get('produced', {}).get('carrots', 0), 0)
        self.assertEqual(economy.local_room(self.farm, 'carrots'), 0)

    def test_full_current_crop_waits_for_hauler_before_resuming(self):
        self.farm['output_buffer'] = {'wheat': farming.storage_capacity(self.farm)}
        self.acre['stored_goods'] = {'wheat': 10}
        farming.field_state(self.acre).update(stage='ripe', crop='wheat')
        store = {'kind': 'storehouse', 'x': 12, 'y': 12,
                 'storage_enabled': {'wheat': True}}
        self.town['buildings'].append(store)
        server.tick(self.world)
        self.assertNotIn('logistics_trip', self.worker)
        self.assertIn('Workplace buffer full', self.worker['production_status'])
        self.assertEqual(self.farm['output_buffer']['wheat'], farming.storage_capacity(self.farm))
        self.assertNotIn('pending_output', self.worker)
        self.town['villagers'][1].update(job='hauling', building_index=len(self.town['buildings']) - 1)
        for _ in range(20):
            server.tick(self.world)
            if self.town.get('produced', {}).get('wheat', 0):
                break
        self.assertEqual(self.town['produced']['wheat'], farming.HARVEST_YIELD)
        self.assertEqual(store.get('stored_goods', {}).get('wheat', 0), 0,
                         'Pickup frees farm storage before the hauler arrives at the storehouse')
        hauler = self.town['villagers'][1]
        self.assertEqual(hauler['logistics_trip']['cargo'], {'wheat': economy.storage_size(store)})
        for _ in range(20):
            server.tick(self.world)
            if store.get('stored_goods', {}).get('wheat', 0):
                break
        self.assertEqual(store['stored_goods']['wheat'], economy.storage_size(store))

    def test_saved_output_trip_does_not_block_a_farmer_with_water_ready(self):
        farming.field_state(self.acre).update(stage='thirsty', crop='wheat')
        self.worker['watering_round'] = farming.watering_round(self.farm, self.acre)
        self.worker['logistics_trip'] = {
            'operation': 'collect', 'workplace': {'kind': 'farm', 'x': 3, 'y': 3},
            'storage': {'kind': 'hall', 'x': 5, 'y': 5},
            'path': [[3, 4], [4, 4]], 'delivery_step': 1, 'delivered': False,
        }
        restored = json.loads(json.dumps(self.world))
        server.tick(restored)
        farmer = restored['towns']['fields']['villagers'][0]
        field = restored['towns']['fields']['buildings'][2]['field']
        self.assertNotIn('logistics_trip', farmer)
        self.assertEqual(field['stage'], 'growing')

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
                for sample in animation.get('playback', []):
                    field = sample.get('farmField', {})
                    tile = field.get('x'), field.get('y')
                    if tile in actions and sample.get('farmAction'):
                        actions[tile].add(sample['farmAction'])
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
