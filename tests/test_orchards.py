import copy
import json
import unittest

from fixtures import flat_town
import orchards
import progression as economy
import server


class Orchards(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Orchard')
        self.town.update(level=5, wood=500, food=500)
        economy.ensure_inventory(self.town)
        self.town['buildings'][0]['stored_goods'] = {'wood': 500, 'food': 500}
        self.orchard = {'kind': 'orchard', 'x': 8, 'y': 6}
        self.town['buildings'].append(self.orchard)
        self.worker = self.town['villagers'][0]
        self.worker.update(job='orcharding', building_index=1, farming=20, orcharding=0,
                           talents={}, construction_position={'x': 9, 'y': 4})
        self.world = {'towns': {'own': self.town}}

    def plant(self, **changes):
        server.command(self.town, {**dict(action='build', kind='orchard_tree',
                                         orchard_index=1, x=9, y=5), **changes})
        return self.town['buildings'][-1]

    def ripe(self, x=9, y=5):
        tree = self.plant(**({'x': x, 'y': y} if (x, y) != (9, 5) else {}))
        tree['tree'].update(stage='ripe', elapsed=0)
        return tree

    def ticks(self, count=12):
        for _ in range(count):
            server.tick(self.world)

    def test_plant_save_and_grow_without_workers(self):
        tree = self.plant()
        self.assertEqual(self.town['wood'], 495)
        self.assertNotIn('construction', tree)
        self.assertEqual(server.settler_capacity(tree), 0)
        self.assertEqual(economy.storage_size(tree), 0)
        self.worker.update(job='idle')
        orchards.grow(self.town, 60)
        self.assertEqual(tree['tree']['stage'], 'blossom')
        restored = json.loads(json.dumps(self.town))
        orchards.grow(restored, 30)
        self.assertEqual(restored['buildings'][-1]['tree']['stage'], 'fruit')
        orchards.grow(restored, 60)
        self.assertEqual(restored['buildings'][-1]['tree']['stage'], 'ripe')
        orchards.grow(restored, 1000)
        self.assertEqual(restored['buildings'][-1]['tree'], {'stage': 'ripe', 'elapsed': 0, 'fruit': 8})
        self.assertEqual(restored.get('produced', {}).get('fruit', 0), 0)

    def test_invalid_planting_does_not_charge(self):
        for changes in ({'orchard_index': None}, {'orchard_index': True}, {'orchard_index': 0},
                        {'orchard_index': 99}, {'x': 12}, {'x': 8, 'y': 6}):
            before = copy.deepcopy(self.town)
            data = dict(action='build', kind='orchard_tree', orchard_index=1, x=9, y=5)
            with self.assertRaises(ValueError):
                server.command(self.town, {**data, **changes})
            self.assertEqual(self.town, before)
        self.town['level'] = 4
        with self.assertRaisesRegex(ValueError, 'level 5'):
            self.plant()

    def test_tree_has_no_worker_or_upgrades(self):
        tree = self.plant()
        for action in ('upgrade_building', 'upgrade_capacity', 'assign_settler'):
            data = {'action': action, 'index': 2} if action != 'assign_settler' else {
                'action': action, 'index': 0, 'building_index': 2}
            with self.assertRaises(ValueError):
                server.command(self.town, data)
        self.assertEqual(server.settler_capacity(dict(tree, settler_level=3)), 0)

    def test_automatic_harvest_is_delivered_and_repeats(self):
        tree = self.ripe(x=11, y=3)
        self.worker['construction_position'] = {'x': 11, 'y': 2}
        self.ticks(1)
        self.assertEqual(self.town.get('produced', {}).get('fruit', 0), 0)
        self.assertGreater(self.worker['orcharding'], 0)
        self.ticks(1)
        self.assertEqual(self.worker['pending_output']['fruit'], 8)
        self.assertEqual(economy.quantity(self.town, 'fruit'), 0)
        self.assertEqual(tree['tree']['stage'], 'harvested')
        self.ticks()
        self.assertEqual(economy.quantity(self.town, 'fruit'), 8)
        self.assertEqual(self.orchard['output_buffer']['fruit'], 8)
        self.ticks(40)
        self.assertEqual(self.town['produced']['fruit'], 16)
        self.assertEqual(economy.quantity(self.town, 'fruit'), 16)

    def test_two_workers_do_not_duplicate_a_tree(self):
        self.ripe()
        self.town['villagers'][1].update(job='orcharding', building_index=1, farming=20,
                                         talents={}, construction_position={'x': 9, 'y': 4})
        self.ticks()
        self.assertEqual(self.town['produced']['fruit'], 8)
        self.assertEqual(economy.quantity(self.town, 'fruit'), 8)
        self.assertEqual(self.town['villagers'][1].get('orcharding', 0), 0)

    def test_two_workers_pick_different_trees(self):
        self.ripe()
        self.ripe(x=10)
        self.town['villagers'][1].update(job='orcharding', building_index=1, farming=20,
                                         talents={}, construction_position={'x': 10, 'y': 4})
        self.ticks()
        self.assertEqual(economy.quantity(self.town, 'fruit'), 16)
        self.assertGreater(self.town['villagers'][1]['orcharding'], 0)

    def test_empty_or_growing_orchard_does_not_produce_or_train(self):
        self.ticks(2)
        self.assertIn('Plant fruit trees', self.worker['production_status'])
        self.plant()
        self.ticks(2)
        self.assertEqual(self.worker['production_status'], 'Waiting for ripe fruit')
        self.assertEqual(self.worker['orcharding'], 0)
        self.assertEqual(self.town.get('produced', {}).get('fruit', 0), 0)

    def test_full_storage_and_production_cap_preserve_ripe_fruit(self):
        tree = self.ripe()
        self.orchard['output_buffer'] = {'fruit': 50}
        economy.add_stock(self.town, 'fruit', 50)
        self.ticks(2)
        self.assertEqual(self.worker['production_status'], 'Workplace buffer full')
        self.assertEqual(tree['tree']['fruit'], 8)
        self.assertEqual(self.worker['orcharding'], 0)
        economy.withdraw(self.town, 'fruit', 50, source=self.orchard)
        economy.add_stock(self.town, 'fruit', -50)
        self.town.setdefault('rules', {})['production_caps'] = {'fruit': 0}
        self.ticks(2)
        self.assertIn('cap reached', self.worker['production_status'])
        self.assertEqual(tree['tree']['fruit'], 8)
        self.assertEqual(self.worker['orcharding'], 0)

    def test_partial_harvest_keeps_remaining_fruit_on_tree(self):
        tree = self.ripe()
        self.orchard['output_buffer'] = {'fruit': 49.5}
        economy.add_stock(self.town, 'fruit', 49.5)
        self.ticks()
        self.assertEqual(tree['tree']['stage'], 'ripe')
        self.assertEqual(tree['tree']['fruit'], 7.5)
        self.assertEqual(self.orchard['output_buffer']['fruit'], 50)
        self.assertEqual(self.town['produced']['fruit'], .5)

    def test_save_during_delivery_preserves_cargo(self):
        self.ripe(x=11, y=3)
        self.worker['construction_position'] = {'x': 11, 'y': 2}
        self.ticks(2)
        self.assertEqual(self.worker['pending_output']['fruit'], 8)
        restored = json.loads(json.dumps(self.world))
        for _ in range(12):
            server.tick(restored)
        self.assertEqual(economy.quantity(restored['towns']['own'], 'fruit'), 8)
        self.assertEqual(restored['towns']['own']['produced']['fruit'], 8)

    def test_removing_picked_tree_does_not_lose_delivery(self):
        self.ripe(x=11, y=3)
        self.worker['construction_position'] = {'x': 11, 'y': 2}
        self.ticks(2)
        self.assertEqual(self.worker['pending_output']['fruit'], 8)
        server.command(self.town, {'action': 'destroy', 'index': 2})
        self.ticks()
        self.assertEqual(economy.quantity(self.town, 'fruit'), 8)

    def test_orchard_moves_with_trees_and_rejects_blocked_group(self):
        tree = self.ripe()
        tree['tree']['fruit'] = 4
        self.town['buildings'].append({'kind': 'home', 'x': 10, 'y': 5})
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            server.command(self.town, {'action': 'move', 'index': 1, 'x': 9, 'y': 6})
        self.assertEqual(self.town, before)
        self.town['buildings'].pop()
        server.command(self.town, {'action': 'move', 'index': 1, 'x': 9, 'y': 6})
        self.assertEqual((tree['x'], tree['y'], tree['orchard_index']), (10, 5, 1))
        self.assertEqual(tree['tree']['fruit'], 4)
        self.assertEqual(self.town['coins'], before['coins'] - 1)

    def test_indices_remap_and_orchard_demolition_preserves_stored_fruit(self):
        self.town['buildings'].insert(1, {'kind': 'home', 'x': 1, 'y': 1})
        self.worker['building_index'] = 2
        server.command(self.town, {'action': 'build', 'kind': 'orchard_tree',
                                   'orchard_index': 2, 'x': 9, 'y': 5})
        tree = self.town['buildings'][-1]
        server.command(self.town, {'action': 'destroy', 'index': 1})
        self.assertEqual(tree['orchard_index'], 1)
        self.orchard['output_buffer'] = {'fruit': 3}
        economy.add_stock(self.town, 'fruit', 3)
        server.command(self.town, {'action': 'destroy', 'index': 1})
        self.assertEqual(len(self.town['buildings']), 1)
        self.assertEqual(self.worker['job'], 'idle')
        self.assertEqual(economy.quantity(self.town, 'fruit'), 3)
        self.assertEqual(self.town['buildings'][0]['stored_goods']['fruit'], 3)


if __name__ == '__main__':
    unittest.main()
