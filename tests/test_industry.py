import copy
import json
import unittest

import logistics
import progression as economy
import server
from terrain import ensure_industry
from unittest.mock import patch


class IndustryFixture(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Industry')
        self.town.update(level=10, terrain=['g' * 24] * 24, map_size=24,
                         map_resources=[], npcs=[], goods={}, industry_version=1, town_origin={'x': 5, 'y': 5})
        self.world = {'towns': {'own': self.town}}
        economy.ensure_inventory(self.town)
        self.worker = self.town['villagers'][0]
        self.worker['talents'] = {}

    def workplace(self, kind, **extra):
        building = {'kind': kind, 'x': 4, 'y': 6, **extra}
        self.town['buildings'].append(building)
        job = economy.BUILDINGS[kind]['job']
        self.worker.update(job=job, building_index=len(self.town['buildings']) - 1)
        self.worker.update(economy.JOB_REQUIREMENTS.get(job, {}))
        return building

class Industry(IndustryFixture):
    def test_late_recipes_consume_delivered_inputs_and_haul_outputs(self):
        for kind in ('tannery', 'smelter', 'saddler', 'steelworks', 'tradehouse'):
            with self.subTest(kind=kind):
                self.setUp()
                inputs, key, amount = economy.RECIPES[kind]
                building = self.workplace(kind, input_buffer=dict(inputs))
                self.assertEqual(logistics.produce(self.town, building, self.worker, economy.production_seconds(building)), 'Working')
                self.assertEqual(economy.quantity(self.town, key), 0)
                self.assertEqual(self.worker['pending_output'], {key: amount})
                logistics.drop_off(self.town, building, self.worker)
                self.assertEqual(economy.quantity(self.town, key), amount)
                self.assertTrue(all(n == 0 for n in building['input_buffer'].values()))
                store = {'kind': 'storehouse', 'storage_enabled': {key: True}}
                self.town['buildings'].append(store)
                logistics.transfer(self.town, building, store, 'collect', self.worker)
                self.assertEqual(store['stored_goods'][key], amount)

    def test_mining_requires_nearby_ore_and_respects_supply_and_caps(self):
        mine = self.workplace('mine')
        self.assertIn('iron vein', logistics.produce(self.town, mine, self.worker, 9))
        deposit = {'kind': 'rock', 'ore': 'iron', 'x': 4, 'y': 4, 'supply': 2}
        self.town['map_resources'].append(deposit)
        self.town['rules'] = {'production_caps': {'iron_ore': 0}}
        self.assertEqual(logistics.produce(self.town, mine, self.worker, 9), 'Production cap reached')
        self.assertEqual(deposit['supply'], 2)
        self.town['rules'] = {}
        self.assertEqual(logistics.produce(self.town, mine, self.worker, 9), 'Working')
        self.assertEqual(deposit['supply'], 0)
        self.assertEqual(self.worker['pending_output'], {'iron_ore': 2})
        self.assertIn('regenerate', logistics.produce(self.town, mine, self.worker, 9))
        economy.renew(self.town)
        self.assertEqual(deposit['supply'], .15)

    def test_steel_pickaxe_uses_real_ore_and_adds_a_quarter_output(self):
        mine = self.workplace('mine', profession_upgrades=['steel_pickaxe'])
        deposit = {'kind': 'rock', 'ore': 'iron', 'x': 4, 'y': 4, 'supply': 5}
        self.town['map_resources'].append(deposit)
        self.assertEqual(logistics.produce(self.town, mine, self.worker, 9), 'Working')
        self.assertEqual(self.worker['pending_output'], {'iron_ore': 2.5})
        self.assertEqual(deposit['supply'], 2.5)

    def test_hides_from_butchery_share_buffer_and_preserve_production_cap(self):
        shop = self.workplace('butcher', butchering={'animal': 'cow', 'amount': 10, 'remaining': 0})
        self.town['rules'] = {'production_caps': {'hides': 0}}
        self.assertIn('hide', logistics.butcher_animal(self.town, shop, self.worker, 3))
        self.assertIn('butchering', shop)
        self.assertEqual(economy.quantity(self.town, 'meat'), 0)
        self.town['rules'] = {}
        self.assertEqual(logistics.butcher_animal(self.town, shop, self.worker, 3), 'Working')
        self.assertEqual(shop['output_buffer'], {'meat': 10, 'hides': 3})

    def test_terrain_migration_preserves_tiles_and_never_respawns_captured_horses(self):
        self.town.pop('industry_version')
        self.town['terrain'] = ['m' * 24] * 24
        self.town['map_resources'] = [{'kind': 'rock', 'x': 1, 'y': 1}]
        terrain = list(self.town['terrain'])
        ensure_industry(self.town, self.town['buildings'])
        self.assertEqual(self.town['terrain'], terrain)
        self.assertEqual(self.town['map_resources'][0]['ore'], 'iron')
        horse = next(r for r in self.town['map_resources'] if r['kind'] == 'wild_horse')
        self.town['map_resources'].remove(horse)
        restored = json.loads(json.dumps(self.town))
        ensure_industry(restored)
        self.assertNotIn(horse, restored['map_resources'])

    def test_solo_market_sales_can_complete_level_ten_milestone(self):
        self.town.update(level=9, produced={'steel': 15})
        self.assertFalse(economy.milestone(self.town, 10))
        self.town['market_sales'] = 10
        self.assertTrue(economy.milestone(self.town, 10))


class Horses(IndustryFixture):
    def setUp(self):
        super().setUp()
        self.town['level'] = 8
        self.stable = self.workplace('stable', input_buffer={'fodder': 100})
        self.town['player'].update(x=4., y=7.)

    def act(self, action, **extra):
        return server.command(self.town, {'action': action, 'index': 1, **extra}, self.world)

    def tick(self):
        with patch('work_cycles.advance', return_value=False), patch('logistics.worker', return_value=False):
            server.tick(self.world)

    def capture(self, x=3, y=7):
        self.town['map_resources'].append({'kind': 'wild_horse', 'x': x, 'y': y})
        self.act('lead_horse', x=x, y=y)
        self.act('stable_horse')

    def test_capture_settle_feed_breed_saddle_ride_and_return(self):
        self.capture()
        self.assertEqual(self.stable['animal_growth']['horse'], [0])
        with self.assertRaisesRegex(ValueError, 'fully grown'):
            self.act('breed_animal', animal='horse')
        self.capture()
        # Starvation pauses settling; actual production consumes feed once per stable tick.
        self.stable['input_buffer'] = {}
        self.tick()
        self.assertEqual(self.stable['animal_growth']['horse'], [0, 0])
        self.stable['animal_growth']['horse'] = [297, 297]
        self.stable['input_buffer'] = {'fodder': 10}
        self.worker['work_animation'] = {'phase': 'working'}
        self.tick()
        self.assertEqual(self.stable['animal_growth']['horse'], [300, 300])
        self.assertEqual(self.stable['input_buffer']['fodder'], 9)
        self.town['goods'].update(fodder=3, herbs=1, saddles=1)
        self.act('breed_animal', animal='horse')
        self.assertEqual(self.stable['animal_growth']['horse'], [300, 300, 0])
        self.act('ride_horse')
        self.assertTrue(self.town['player']['mounted'])
        self.assertEqual(self.stable['animal_growth']['horse'], [300, 0])
        self.assertEqual(economy.quantity(self.town, 'saddles'), 0)
        self.assertTrue(economy.milestone(self.town, 9))
        self.act('toggle_mount')
        self.assertFalse(self.town['player']['mounted'])
        self.act('return_horse')
        self.assertEqual(self.stable['animal_growth']['horse'], [300, 0, 300])
        self.act('ride_horse')  # Reuses the player's saddle.
        restored = json.loads(json.dumps(self.town))
        self.assertTrue(restored['player']['horse'])
        self.assertTrue(restored['player']['mounted'])

    def test_capture_and_stabling_validate_distance_capacity_and_single_lead(self):
        self.town['map_resources'].append({'kind': 'wild_horse', 'x': 15, 'y': 15})
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            self.act('lead_horse', x=15, y=15)
        self.assertEqual(self.town, before)
        self.capture()
        self.stable['animals']['horse'] = 4
        self.stable['animal_growth']['horse'] = [300] * 4
        self.town['player']['leading_horse'] = True
        with self.assertRaisesRegex(ValueError, 'full'):
            self.act('stable_horse')
        self.assertTrue(self.town['player']['leading_horse'])
        with self.assertRaisesRegex(ValueError, 'Bring the horse'):
            self.act('lead_horse', x=15, y=15)
        with self.assertRaises(ValueError):
            self.act('buy_animal', animal='horse')

    def test_riding_movement_keeps_authoritative_land_speed_and_collision(self):
        self.town['player'].update(x=10., y=10.)
        plain = copy.deepcopy(self.town)
        server.move_player(plain, {'dx': 1, 'dy': -1})
        self.town['player'].update(horse=True, mounted=True)
        server.move_player(self.town, {'dx': 1, 'dy': -1})
        self.assertAlmostEqual(self.town['player']['x'] - 10, 1.75 * (plain['player']['x'] - 10))
        self.town['player'].update(x=10., y=10.)
        self.town['map_resources'].append({'kind': 'rock', 'x': 11, 'y': 10})
        for _ in range(5):
            server.move_player(self.town, {'dx': 1, 'dy': 0})
        self.assertLess(self.town['player']['x'], 10.32)
