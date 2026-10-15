import copy
import unittest

from fixtures import flat_town
import logistics
import progression as economy
import server


class MarketSales(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Market')
        self.town['buildings'].append({'kind': 'market', 'x': 7, 'y': 5})
        economy.ensure_inventory(self.town)

    def stock(self, resource, amount):
        self.town['buildings'][0]['stored_goods'] = {resource: amount}
        self.town['wood'] = amount if resource == 'wood' else 0
        self.town['goods'] = {} if resource == 'wood' else {resource: amount}
        self.town['food'] = amount * economy.EDIBLE.get(resource, 0)

    def test_quick_sale_quantities_withdraw_goods_and_award_full_value(self):
        for resource in ('planks', 'smoked_sturgeon', 'wheat', 'vegetables',
                         'berries', 'carrots', 'fodder', 'manure', 'stone', 'wood'):
            for amount in (1, 5, 10, 50, 100, 200):
                with self.subTest(resource=resource, amount=amount):
                    self.setUp()
                    self.stock(resource, 250)
                    coins = self.town['coins']
                    server.command(self.town, {'action': 'sell_goods', 'resource': resource, 'amount': amount})
                    self.assertEqual(economy.quantity(self.town, resource), 250 - amount)
                    self.assertEqual(self.town['buildings'][0]['stored_goods'][resource], 250 - amount)
                    self.assertEqual(self.town['coins'], coins + amount * economy.sale_price(resource))
                    self.assertEqual(self.town['market_sales'], amount)
                    self.assertEqual(self.town['food'], (250 - amount) * economy.EDIBLE.get(resource, 0))

    def test_vegetables_are_offered_for_automatic_and_quick_sales(self):
        catalog = economy.public_catalog()
        self.assertIn('vegetables', catalog['sellable_goods'])
        self.assertIn('vegetables', catalog['manual_sellable_goods'])
        self.assertEqual(economy.sale_price('vegetables'), 1)

    def test_all_stored_resources_are_offered_for_automatic_and_quick_sales(self):
        catalog = economy.public_catalog()
        resources = set(catalog['storage_keys']) - {'food'}
        self.assertEqual(set(catalog['sellable_goods']), resources)
        self.assertEqual(set(catalog['manual_sellable_goods']), resources)
        self.assertEqual(len(catalog['manual_sellable_goods']), len(resources))

    def test_basic_resource_sales_retain_threshold_and_award_coins(self):
        for resource in ('berries', 'carrots', 'fodder', 'manure', 'stone', 'wheat', 'wood'):
            with self.subTest(resource=resource):
                self.setUp()
                self.stock(resource, 13)
                coins, food = self.town['coins'], self.town['food']
                worker = {'job': 'trading'}
                source, market = self.town['buildings'][0], self.town['buildings'][-1]
                server.command(self.town, {'action': 'configure_resource_trade', 'resource': resource,
                                           'buy_below': None, 'sell_above': 10})
                cargo = logistics.pickup_sale(self.town, source, worker)
                self.assertEqual(cargo, {resource: 3})
                worker['logistics_trip'] = {'picked_up': True}
                sold = logistics.transfer(self.town, source, market, 'sell', worker, cargo)
                self.assertEqual(sold, {resource: 3})
                self.assertEqual(economy.quantity(self.town, resource), 10)
                self.assertEqual(source['stored_goods'][resource], 10)
                self.assertEqual(self.town['food'], food - 3 * economy.EDIBLE.get(resource, 0))
                self.assertEqual(self.town['coins'], coins + 3)
                self.assertEqual(logistics.sale_cargo(self.town, source, worker), {})
                server.command(self.town, {'action': 'configure_market_sale', 'resource': resource,
                                           'enabled': True, 'cap': 12})
                self.assertEqual(self.town['rules']['market_sales'][resource]['cap'], 12)

    def test_one_time_wheat_sale_does_not_configure_automatic_sales(self):
        self.stock('wheat', 250)
        rules = copy.deepcopy(self.town.get('rules', {}))
        server.command(self.town, {'action': 'sell_goods', 'resource': 'wheat', 'amount': 200})
        self.assertEqual(self.town.get('rules', {}), rules)

    def test_invalid_sales_preserve_stock_coins_and_sale_progress(self):
        self.stock('planks', 199)
        cases = [('planks', amount) for amount in (0, -1, 201, 1.5, True, '50', None)]
        cases.extend([('unknown', 1), ('food', 1), ('coins', 1), ('planks', 200)])
        for resource, amount in cases:
            with self.subTest(resource=resource, amount=amount):
                before = copy.deepcopy(self.town)
                with self.assertRaises(ValueError):
                    server.command(self.town, {'action': 'sell_goods', 'resource': resource, 'amount': amount})
                self.assertEqual(self.town['coins'], before['coins'])
                self.assertEqual(self.town['goods'], before['goods'])
                self.assertEqual(self.town['buildings'][0]['stored_goods'], before['buildings'][0]['stored_goods'])
                self.assertEqual(self.town.get('market_sales', 0), before.get('market_sales', 0))

    def test_bulk_sale_requires_a_completed_market(self):
        self.stock('planks', 200)
        self.town['buildings'][-1]['construction'] = {'progress': 0, 'total': 30}
        with self.assertRaisesRegex(ValueError, 'market stall'):
            server.command(self.town, {'action': 'sell_goods', 'resource': 'planks', 'amount': 200})
        self.assertEqual(economy.quantity(self.town, 'planks'), 200)
