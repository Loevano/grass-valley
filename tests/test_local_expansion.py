import copy
import json
import unittest
from unittest.mock import patch

import progression as economy
from npcs import (ADDITIONAL_THEMES, BARTERERS, GIFTS, TOOL_SHOPS,
                  WORKER_TOOLS, advance_trades, ensure_npcs, local_key)
from server import command, new_town, tick


class MoreLocals(unittest.TestCase):
    def setUp(self):
        # This roster fixture requires all biome offers, independent of random terrain.
        with patch('terrain.secrets.randbits', return_value=42):
            self.town = new_town('More locals')

    def visit(self, name):
        index = next(i for i, npc in enumerate(self.town['npcs']) if npc['name'] == name)
        npc = self.town['npcs'][index]
        self.town['player'].update(x=npc['x'], y=npc['y'])
        return index

    def reject(self, data):
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            command(self.town, data)
        self.assertEqual(self.town, before)

    def test_every_newcomer_spawns_and_has_an_offer(self):
        newcomers = {name for people in ADDITIONAL_THEMES.values() for name, _, _ in people}
        self.assertEqual(len(newcomers), 12)
        self.assertTrue(newcomers <= {npc['name'] for npc in self.town['npcs']})
        self.assertTrue(newcomers <= GIFTS.keys() | TOOL_SHOPS.keys() | BARTERERS.keys())

    def test_version_two_save_keeps_old_positions_and_progress(self):
        newcomers = {name for people in ADDITIONAL_THEMES.values() for name, _, _ in people}
        self.town['npcs'] = [npc for npc in self.town['npcs'] if npc['name'] not in newcomers]
        self.town['npc_roster_version'] = 2
        old = copy.deepcopy(self.town['npcs'])
        self.town['local_visits'] = {local_key(old[0]): {'gift_claimed': True, 'trade_remaining': 120}}
        progress = copy.deepcopy(self.town['local_visits'])
        ensure_npcs(self.town)
        self.assertEqual(self.town['npcs'][:len(old)], old)
        self.assertEqual(self.town['local_visits'], progress)
        self.assertEqual({npc['name'] for npc in self.town['npcs'][len(old):]}, newcomers)

    def test_clothing_gifts_can_be_equipped_and_cannot_be_repeated_after_reload(self):
        for name, key in (('Dara', 'desert_clothing'), ('Petra', 'swamp_boots')):
            index = self.visit(name)
            command(self.town, {'action': 'claim_local_gift', 'npc_index': index})
            self.assertEqual(self.town['player']['items'][key], 1)
            self.assertNotIn(key, self.town['player'].get('worker_tools', {}))
            command(self.town, {'action': 'equip_item', 'item': key})
            self.assertIn(key, self.town['player']['equipment'].values())
            self.town = json.loads(json.dumps(self.town))
            self.reject({'action': 'claim_local_gift', 'npc_index': index})

    def test_varied_trades_deliver_real_food_and_keep_personal_cooldowns(self):
        for name in ('Pip', 'Hazel', 'Mabel', 'Orin', 'Willow'):
            with self.subTest(name=name):
                self.town = new_town(name)
                index = self.visit(name)
                other = copy.deepcopy(self.town)
                wood, good, amount = BARTERERS[name]
                before = self.town['wood'], self.town['food'], economy.quantity(self.town, good)
                command(self.town, {'action': 'trade_local', 'npc_index': index})
                self.assertEqual(self.town['wood'], before[0] - wood)
                self.assertEqual(economy.quantity(self.town, good), before[2] + amount)
                self.assertEqual(self.town['food'], before[1] + amount * economy.EDIBLE[good])
                self.assertNotIn('local_visits', other)
                self.reject({'action': 'trade_local', 'npc_index': index})
                self.town = json.loads(json.dumps(self.town))
                advance_trades(self.town, 300)
                command(self.town, {'action': 'trade_local', 'npc_index': index})

    def test_bread_requires_full_weighted_storage_before_spending_wood(self):
        index = self.visit('Pip')
        self.town['buildings'][0]['stored_goods'] = {'wood': 65, 'food': 93}
        self.town['food'] = 93
        self.reject({'action': 'trade_local', 'npc_index': index})
        self.town['buildings'][0]['stored_goods']['food'] = 92.5
        self.town['food'] = 92.5
        command(self.town, {'action': 'trade_local', 'npc_index': index})
        self.assertEqual(self.town['food'], 100)
        self.assertEqual(economy.quantity(self.town, 'bread'), 5)

    def test_fishing_rod_shop_charges_coins_and_tools_boost_real_catches(self):
        for key in TOOL_SHOPS['Nell']:
            with self.subTest(item=key):
                self.town = new_town(key)
                index = self.visit('Nell')
                self.town['coins'] = WORKER_TOOLS[key]['price']
                command(self.town, {'action': 'buy_worker_tool', 'npc_index': index, 'item': key})
                self.assertEqual(self.town['coins'], 0)
                command(self.town, {'action': 'give_worker_tool', 'index': 0, 'item': key})
                self.town['buildings'].append({'kind': 'fishing_hut', 'x': 6, 'y': 2})
                self.town['villagers'][0].update(job='fishing', talents={})
                with patch('server.secrets.randbelow', return_value=1):
                    tick({'towns': {'own': self.town}})
                self.assertAlmostEqual(self.town['buildings'][-1]['pending_output']['fish'],
                                       1 + WORKER_TOOLS[key]['bonus'])


if __name__ == '__main__':
    unittest.main()
