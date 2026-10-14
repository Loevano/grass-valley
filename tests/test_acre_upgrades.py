"""Acres have fixed capacity and no purchasable or saved upgrade effects."""
import copy
import json
import unittest

from fixtures import flat_town
import farming
import progression as economy
import server


class AcreUpgrades(unittest.TestCase):
    def setUp(self):
        self.town = flat_town()
        self.town['buildings'].extend([{'kind': 'farm', 'x': 3, 'y': 3},
                                      {'kind': 'acre', 'x': 3, 'y': 4, 'farm_index': 1}])
        self.acre = self.town['buildings'][2]

    def test_host_rejects_both_upgrade_commands_without_spending(self):
        for sandbox in (False, True):
            self.town['sandbox'] = sandbox
            for action in ('upgrade_building', 'upgrade_capacity'):
                before = copy.deepcopy(self.town)
                with self.assertRaisesRegex(ValueError, 'Acres have no upgrades'):
                    server.command(self.town, {'action': action, 'index': 2})
                self.assertEqual(self.town, before)

    def test_legacy_levels_have_no_production_storage_or_worker_bonus(self):
        self.acre.update(level=3, settler_level=3)
        self.assertEqual(server.production_bonus(self.acre), 0)
        self.assertEqual(server.settler_capacity(self.acre), 1)
        self.assertEqual(farming.storage_capacity(self.acre), 10)
        self.assertEqual(economy.storage_allocations(self.acre)['manure'], 10)

    def test_saved_progress_and_excess_stock_survive_upgrade_retirement(self):
        self.acre.update(level=3, settler_level=2, crop='wheat', stored_goods={'wheat': 35},
                         field={'stage': 'ripe', 'crop': 'wheat', 'elapsed': 0,
                                'progress': 1, 'cycle': 4})
        for worker in self.town['villagers'][:2]:
            worker.update(job='farming', building_index=2)
        self.town.pop('acre_upgrade_version')
        self.town = json.loads(json.dumps(self.town))
        server.ensure_resources(self.town)
        acre = self.town['buildings'][2]
        self.assertEqual(acre['stored_goods'], {'wheat': 35})
        self.assertEqual((acre['crop'], acre['field']['stage'], acre['field']['progress']), ('wheat', 'ripe', 1))
        self.assertNotIn('level', acre)
        self.assertNotIn('settler_level', acre)
        self.assertEqual(self.town['villagers'][0]['building_index'], 2)
        self.assertEqual(self.town['villagers'][1]['job'], 'idle')
        before = copy.deepcopy(self.town)
        server.ensure_resources(self.town)
        self.assertEqual(self.town, before)

    def test_pending_upgrade_refunds_once_and_initial_construction_completes(self):
        for key, cost in [('level', 10), ('settler_level', 40)]:
            with self.subTest(key=key):
                self.setUp()
                self.town.pop('acre_upgrade_version')
                self.acre.update(level=2, settler_level=2,
                                 construction={'progress': 11, 'total': 60, 'upgrade': {key: 3}})
                self.town['buildings'].append({'kind': 'acre', 'x': 4, 'y': 4, 'farm_index': 1,
                                              'construction': {'progress': 5, 'total': 30}})
                economy.ensure_inventory(self.town)
                before = self.town['wood']
                server.ensure_resources(self.town)
                self.assertNotIn('construction', self.acre)
                self.assertNotIn('construction', self.town['buildings'][3])
                self.assertEqual(self.town['wood'], before + cost)
                self.assertEqual(self.town['buildings'][0]['stored_goods']['wood'], before + cost)
                server.ensure_resources(self.town)
                self.assertEqual(self.town['wood'], before + cost)
