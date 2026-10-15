import copy
import json
import unittest

from fixtures import flat_town
import logistics
import progression as economy
import resource_activity
import server


class ResourceActivity(unittest.TestCase):
    def setUp(self):
        self.town = flat_town()
        economy.ensure_inventory(self.town)
        self.town.pop('resource_activity', None)

    def flow(self, key, direction):
        return sum(b[direction].get(key, 0) for b in self.town.get('resource_activity', {}).get('buckets', []))

    def test_records_production_and_use_separately_and_expires_at_sixty_seconds(self):
        resource_activity.advance(self.town, 3)
        economy.deliver(self.town, 'wood', 7, 100)
        economy.spend(self.town, {'wood': 3})
        self.assertEqual(self.flow('wood', 'in'), 7)
        self.assertEqual(self.flow('wood', 'out'), 3)
        resource_activity.advance(self.town, 57)
        self.assertEqual(self.flow('wood', 'in'), 7)
        resource_activity.advance(self.town, 3)
        self.assertEqual(self.flow('wood', 'in'), 0)

    def test_pending_output_is_recorded_once_and_dropoff_is_not_new_production(self):
        building = {'kind': 'woodcutter', 'x': 7, 'y': 7}
        self.town['buildings'].append(building)
        worker = {}
        logistics.record_output(self.town, building, 'wood', 4, worker)
        logistics.drop_off(self.town, building, worker)
        self.assertEqual(self.flow('wood', 'in'), 4)

    def test_food_records_item_units_and_food_value(self):
        economy.deliver(self.town, 'bread', 4, 100, record=False)
        economy.spend(self.town, {'bread': 2})
        self.assertEqual(self.flow('bread', 'in'), 4)
        self.assertEqual(self.flow('food', 'in'), 4 * economy.EDIBLE['bread'])
        self.assertEqual(self.flow('bread', 'out'), 2)
        self.assertEqual(self.flow('food', 'out'), 2 * economy.EDIBLE['bread'])

    def test_previews_do_not_mutate_shared_history_and_history_is_bounded(self):
        economy.record_activity(self.town, 'wood', 1, 'in')
        before = copy.deepcopy(self.town['resource_activity'])
        trial = dict(self.town, _activity_preview=True)
        economy.spend(trial, {'coins': 1})
        self.assertEqual(self.town['resource_activity'], before)
        for _ in range(100):
            resource_activity.advance(self.town, 3)
            economy.record_activity(self.town, 'wood', 1, 'in')
        self.assertEqual(len(self.town['resource_activity']['buckets']), 20)
        restored = json.loads(json.dumps(self.town))
        self.assertEqual(restored['resource_activity'], self.town['resource_activity'])


class ResourceTrade(unittest.TestCase):
    def setUp(self):
        self.town = flat_town()
        economy.ensure_inventory(self.town)
        self.town['buildings'].append({'kind': 'storehouse', 'x': 9, 'y': 5,
                                       'storage_enabled': {'planks': True}})
        self.town['buildings'].append({'kind': 'market', 'x': 8, 'y': 5})
        self.town['coins'] = 100
        self.worker = {'job': 'trading', 'trading': 0}

    def configure(self, buy=None, sell=None, key='planks'):
        return server.command(self.town, {'action': 'configure_resource_trade', 'resource': key,
                                         'buy_below': buy, 'sell_above': sell})

    def test_buying_obeys_threshold_coins_storage_and_does_not_count_as_production(self):
        self.configure(buy=7, sell=20)
        before = self.town.get('produced', {}).copy()
        self.assertTrue(logistics.purchase_batch(self.town, self.worker, 1))
        self.assertEqual(economy.quantity(self.town, 'planks'), 0)
        logistics.purchase_batch(self.town, self.worker, 2)
        self.assertEqual(economy.quantity(self.town, 'planks'), 7)
        self.assertEqual(self.town['coins'], 100 - 7 * 2 * economy.sale_price('planks'))
        self.assertFalse(logistics.purchase_batch(self.town, self.worker, 3))
        self.assertEqual(self.town.get('produced', {}), before)
        self.configure(buy=50)
        self.town['coins'] = 2 * economy.sale_price('planks')
        logistics.purchase_batch(self.town, self.worker, 3)
        self.assertEqual(economy.quantity(self.town, 'planks'), 8)
        self.assertEqual(self.town['coins'], 0)
        self.town['coins'] = 1000
        self.town['buildings'][-2]['stored_goods'] = {'planks': economy.storage_limit(self.town, 'planks')}
        self.assertFalse(logistics.purchase_batch(self.town, self.worker, 3))

    def test_strict_sales_retain_threshold_and_recheck_changes_during_trip(self):
        self.configure(sell=10)
        building = self.town['buildings'][0]
        building['stored_goods']['planks'] = 13
        self.town.setdefault('goods', {})['planks'] = 13
        self.assertEqual(logistics.sale_cargo(self.town, building, self.worker), {'planks': 3})
        cargo = logistics.pickup_sale(self.town, building, self.worker)
        self.worker['logistics_trip'] = {'picked_up': True}
        self.configure(sell=12)
        sold = logistics.transfer(self.town, building, self.town['buildings'][-1], 'sell', self.worker, cargo)
        self.assertEqual(sold, {'planks': 1})
        self.assertEqual(economy.quantity(self.town, 'planks'), 12)
        self.assertEqual(logistics.sale_cargo(self.town, building, self.worker), {})

    def test_invalid_thresholds_are_atomic_and_blank_disables(self):
        self.configure(buy=5, sell=10)
        before = copy.deepcopy(self.town['rules'])
        for buy, sell in [(10, 5), (-1, None), (True, None), (float('nan'), None), (None, float('inf'))]:
            with self.assertRaises(ValueError):
                self.configure(buy, sell)
            self.assertEqual(self.town['rules'], before)
        self.configure()
        self.assertNotIn('planks', self.town['rules']['market_purchases'])
        self.assertFalse(self.town['rules']['market_sales']['planks']['enabled'])
        self.town['buildings'].pop()
        with self.assertRaises(ValueError):
            self.configure(buy=10)

    def test_each_merchant_rechecks_current_stock_and_coins(self):
        self.configure(buy=12)
        logistics.purchase_batch(self.town, self.worker, 3)
        logistics.purchase_batch(self.town, {'job': 'trading'}, 3)
        self.assertEqual(economy.quantity(self.town, 'planks'), 12)
        self.assertEqual(self.town['coins'], 100 - 12 * 2 * economy.sale_price('planks'))

    def test_purchases_require_an_assigned_merchant_in_a_completed_market(self):
        self.configure(buy=7)
        world = {'towns': {'own': self.town}}
        server.tick(world)
        self.assertEqual(economy.quantity(self.town, 'planks'), 0)
        worker = self.town['villagers'][0]
        worker.update(job='trading', building_index=len(self.town['buildings']) - 1)
        server.tick(world)
        self.assertEqual(economy.quantity(self.town, 'planks'), 7)
        history = self.town['resource_activity']
        self.assertEqual(sum(b['in'].get('planks', 0) for b in history['buckets']), 7)
        self.assertEqual(sum(b['out'].get('coins', 0) for b in history['buckets']),
                         7 * 2 * economy.sale_price('planks'))

    def test_legacy_sales_editor_cannot_overlap_buying_threshold(self):
        self.configure(buy=5, sell=10)
        with self.assertRaises(ValueError):
            server.command(self.town, {'action': 'configure_market_sale',
                                      'resource': 'planks', 'cap': 4})
        server.command(self.town, {'action': 'configure_market_sale',
                                  'resource': 'planks', 'cap': 12})
        self.assertTrue(self.town['rules']['market_sales']['planks']['strict'])
