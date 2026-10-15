import copy
import unittest

from fixtures import flat_town
import progression as economy
import server


class MarketSales(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Market')
        self.town['buildings'].append({'kind': 'market', 'x': 7, 'y': 5})
        economy.ensure_inventory(self.town)

    def stock(self, resource, amount):
        self.town['buildings'][0]['stored_goods'] = {resource: amount}
        self.town['goods'] = {resource: amount}
        self.town['food'] = amount * economy.EDIBLE.get(resource, 0)

    def test_quick_sale_quantities_withdraw_goods_and_award_full_value(self):
        for resource in ('planks', 'smoked_sturgeon'):
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

    def test_invalid_sales_preserve_stock_coins_and_sale_progress(self):
        self.stock('planks', 199)
        cases = [('planks', amount) for amount in (0, -1, 201, 1.5, True, '50', None)]
        cases.extend([('unknown', 1), ('planks', 200)])
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
