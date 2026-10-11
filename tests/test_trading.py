import copy
import json
import unittest

import progression as economy
import server


class TownTrading(unittest.TestCase):
    def setUp(self):
        self.world = {'towns': {}}
        for key in ('a', 'b', 'c'):
            town = server.new_town(key)
            town.update(level=5, wood=30, goods={'leather': 10, 'iron': 10}, coins=30)
            town['buildings'].extend([{'kind': 'market', 'x': 4, 'y': 6},
                                     {'kind': 'storehouse', 'x': 3, 'y': 6,
                                      'storage_enabled': {'leather': True, 'iron': True}}])
            economy.ensure_inventory(town)
            self.world['towns'][key] = town
        self.a, self.b = self.world['towns']['a'], self.world['towns']['b']

    def act(self, town, action, **data):
        return server.command(town, {'action': action, **data}, self.world)

    def offer(self, **data):
        terms = dict(target_town='b', give_resource='leather', give_amount=5,
                     want_resource='coins', want_amount=10)
        terms.update(data)
        self.act(self.a, 'create_trade', **terms)
        return self.a['trade_offers'][-1]['id']

    def accept(self, offer_id, town=None):
        return self.act(self.b if town is None else town, 'accept_trade', source_town='a', offer_id=offer_id)

    def test_offer_survives_save_and_only_target_owner_can_accept_once(self):
        offer_id = self.offer()
        self.assertEqual(economy.quantity(self.a, 'leather'), 10, 'Offers do not reserve stock')
        self.world = json.loads(json.dumps(self.world))
        self.a, self.b = self.world['towns']['a'], self.world['towns']['b']
        before = copy.deepcopy(self.world)
        with self.assertRaises(ValueError):
            self.accept(offer_id, self.world['towns']['c'])
        self.assertEqual(self.world, before)
        self.accept(offer_id)
        self.assertEqual(economy.quantity(self.a, 'leather'), 5)
        self.assertEqual(economy.quantity(self.b, 'leather'), 15)
        self.assertEqual((self.a['coins'], self.b['coins']), (40, 20))
        self.assertEqual((self.a['completed_trades'], self.b['completed_trades']), (1, 1))
        self.assertNotIn('produced', self.b, 'Trading does not satisfy production milestones')
        before = copy.deepcopy(self.world)
        with self.assertRaises(ValueError):
            self.accept(offer_id)
        self.assertEqual(self.world, before)

    def test_acceptance_rechecks_stock_without_partially_exchanging(self):
        offer_id = self.offer()
        economy.spend(self.a, {'leather': 8})
        before = copy.deepcopy(self.world)
        with self.assertRaisesRegex(ValueError, 'still have'):
            self.accept(offer_id)
        self.assertEqual(self.world, before)

    def test_full_storage_rejects_both_sides_atomically(self):
        offer_id = self.offer()
        self.b['buildings'][0]['stored_goods'].update(wood=100, food=100)
        self.b['buildings'][1]['stored_goods'] = {'leather': 3}
        self.b['buildings'][2]['stored_goods'] = {'leather': 50}
        before = copy.deepcopy(self.world)
        with self.assertRaisesRegex(ValueError, 'storage'):
            self.accept(offer_id)
        self.assertEqual(self.world, before)

    def test_barter_removes_outgoing_stock_before_checking_incoming_room(self):
        offer_id = self.offer(give_amount=10, want_resource='iron', want_amount=10)
        self.accept(offer_id)
        self.assertEqual(economy.quantity(self.a, 'leather'), 0)
        self.assertEqual(economy.quantity(self.a, 'iron'), 20)
        self.assertEqual(economy.quantity(self.b, 'leather'), 20)
        self.assertEqual(economy.quantity(self.b, 'iron'), 0)

    def test_offers_require_markets_valid_quantities_and_different_goods(self):
        for change in ({'target_town': 'a'}, {'target_town': 'missing'},
                       {'give_amount': True}, {'give_amount': 0}, {'give_amount': 101},
                       {'want_amount': 1.5}, {'give_resource': 'horse'},
                       {'want_resource': 'leather'}):
            with self.subTest(change=change):
                before = copy.deepcopy(self.world)
                with self.assertRaises(ValueError):
                    self.offer(**change)
                self.assertEqual(self.world, before)
        self.b['buildings'][1]['construction'] = {'progress': 0, 'total': 30}
        with self.assertRaisesRegex(ValueError, 'market'):
            self.offer()

    def test_cancel_and_offer_limit_preserve_stock(self):
        offers = [self.offer() for _ in range(10)]
        before = copy.deepcopy(self.world)
        with self.assertRaisesRegex(ValueError, 'maximum 10'):
            self.offer()
        self.assertEqual(self.world, before)
        with self.assertRaises(ValueError):
            self.act(self.b, 'cancel_trade', offer_id=offers[0])
        self.act(self.a, 'cancel_trade', offer_id=offers[0])
        self.assertEqual(len(self.a['trade_offers']), 9)
        self.assertEqual(economy.quantity(self.a, 'leather'), 10)
        with self.assertRaises(ValueError):
            self.accept(offers[0])
