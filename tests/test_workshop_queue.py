import copy
import json
import unittest

from fixtures import flat_town
import logistics
import progression as economy
import server


class WorkshopQueue(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Workshop requests')
        self.town['level'] = 4
        self.building = {'kind': 'workshop', 'x': 7, 'y': 5}
        self.index = len(self.town['buildings'])
        self.town['buildings'].append(self.building)
        self.store = {'kind': 'storehouse', 'x': 12, 'y': 12,
                      'storage_enabled': {'furniture': True, 'lanterns': True}}
        self.town['buildings'].append(self.store)
        self.worker = self.town['villagers'][0]
        self.worker.update(job='crafting', woodworking=2, crafting=40,
                           talents={'woodcutting': 1}, building_index=self.index)
        economy.ensure_inventory(self.town)

    def request(self, recipe, amount):
        server.command(self.town, dict(action='queue_workshop', index=self.index,
                                       recipe=recipe, amount=amount))
        return self.building['craft_queue'][-1]

    def cancel(self, order):
        server.command(self.town, dict(action='cancel_workshop', index=self.index,
                                       request_id=order['id']))

    def deliver(self):
        logistics.drop_off(self.town, self.building, self.worker)
        logistics.transfer(self.town, self.building, self.store,
                           'collect', {'job': 'hauling', 'talents': {}})

    def test_exact_bench_then_lantern_requests_and_placement(self):
        self.request('furniture', 5)
        self.request('lanterns', 10)
        self.building['input_buffer'] = {'planks': 20, 'wool': 5}
        self.assertEqual(logistics.produce(self.town, self.building, self.worker, 12), 'Working')
        self.assertEqual(self.building['craft_queue'][0]['remaining'], 1.25)
        self.assertEqual(economy.quantity(self.town, 'furniture'), 0)
        with self.assertRaises(ValueError):
            server.command(self.town, dict(action='build', kind='wooden_bench', x=9, y=5))
        self.deliver()
        self.assertEqual(logistics.requirements(self.building, self.worker), {'planks': 5, 'wool': 1.25})
        self.assertEqual(logistics.produce(self.town, self.building, self.worker, 12), 'Working')
        self.deliver()
        self.assertEqual(economy.quantity(self.town, 'furniture'), 5)
        self.assertEqual(self.building['craft_queue'][0]['recipe'], 'lanterns')
        while self.building['craft_queue']:
            self.building['input_buffer'] = logistics.requirements(self.building, self.worker)
            self.assertEqual(logistics.produce(self.town, self.building, self.worker, 9), 'Working')
            self.deliver()
        self.assertEqual(economy.quantity(self.town, 'lanterns'), 10)
        self.assertEqual(self.town['produced']['furniture'], 5)
        self.assertEqual(self.town['produced']['lanterns'], 10)
        before = copy.deepcopy(self.town)
        self.assertEqual(logistics.produce(self.town, self.building, self.worker, 99), 'Waiting for workshop requests')
        self.assertEqual(self.town, before)
        self.assertEqual(logistics.supply_request(self.town, self.building, self.worker), {})
        for kind, x, resource, remaining in [('wooden_bench', 9, 'furniture', 4), ('lantern', 10, 'lanterns', 9)]:
            server.command(self.town, dict(action='build', kind=kind, x=x, y=5))
            self.assertEqual(economy.quantity(self.town, resource), remaining)
            self.assertNotIn('construction', self.town['buildings'][-1])

    def test_save_keeps_order_remaining_and_partial_work(self):
        self.request('lanterns', 4)
        self.request('tools', 2)
        self.building['input_buffer'] = {'planks': 8, 'wood': 4}
        logistics.produce(self.town, self.building, self.worker, 3)
        self.town = json.loads(json.dumps(self.town))
        self.building = self.town['buildings'][self.index]
        self.store = self.town['buildings'][self.index + 1]
        self.worker = self.town['villagers'][0]
        self.assertEqual(self.worker['production_work']['elapsed'], 3)
        self.assertEqual(logistics.produce(self.town, self.building, self.worker, 6), 'Working')
        self.assertEqual(self.building['craft_queue'][0]['remaining'], .25)
        self.assertEqual([o['recipe'] for o in self.building['craft_queue']], ['lanterns', 'tools'])

    def test_host_ticks_deliver_last_order_and_then_stop(self):
        self.worker.update(crafting=0, talents={})
        self.building.update(x=9, y=9, input_buffer={'planks': 4, 'wood': 2})
        self.request('tools', 2)
        world = {'towns': {'own': self.town}}
        server.prepare_landscapes(world)
        for _ in range(20):
            server.tick(world)
        self.assertEqual(self.building['craft_queue'], [])
        self.assertEqual(self.town['produced']['tools'], 2)
        self.assertEqual(economy.quantity(self.town, 'tools'), 2)
        self.assertEqual(self.worker.get('pending_output', {}), {})

    def test_cancel_waiting_order_preserves_active_progress_and_finished_goods(self):
        active = self.request('lanterns', 5)
        waiting = self.request('furniture', 2)
        self.building['input_buffer'] = {'planks': 10, 'wood': 5}
        logistics.produce(self.town, self.building, self.worker, 9)
        logistics.produce(self.town, self.building, self.worker, 3)
        progress = copy.deepcopy(self.worker['production_work'])
        self.cancel(waiting)
        self.assertEqual(self.worker['production_work'], progress)
        self.cancel(active)
        self.assertNotIn('production_work', self.worker)
        self.assertEqual(self.worker['pending_output'], {'lanterns': 3.75})
        self.assertEqual(self.building['input_buffer'], {})
        self.assertEqual(self.building['output_buffer'], {'planks': 2.5, 'wood': 1.25})
        self.deliver()
        self.assertEqual(economy.quantity(self.town, 'lanterns'), 3.75)

    def test_inputs_are_limited_to_requested_goods(self):
        self.request('lanterns', 1)
        hall = self.town['buildings'][0]
        hall['stored_goods'] = {'planks': 100, 'wood': 100}
        self.assertEqual(logistics.supply_request(self.town, self.building, self.worker), {'planks': 2, 'wood': 1})

    def test_cancel_returns_in_transit_supplies_once(self):
        order = self.request('lanterns', 1)
        self.store['stored_goods'] = {'planks': 0, 'wood': 0}
        hauler = self.town['villagers'][1]
        hauler['logistics_trip'] = {'operation': 'supply', 'picked_up': True,
            'delivered': False, 'cargo': {'planks': 2, 'wood': 1},
            'workplace': {k: self.building[k] for k in ('kind', 'x', 'y')},
            'storage': {k: self.store[k] for k in ('kind', 'x', 'y')}}
        self.cancel(order)
        self.assertNotIn('logistics_trip', hauler)
        self.assertEqual(self.store['stored_goods'], {'planks': 2, 'wood': 1})
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            self.cancel(order)
        self.assertEqual(self.town, before)

    def test_limits_and_sandbox_requests(self):
        self.town['sandbox'] = True
        self.worker['crafting'] = 0
        self.request('furniture', 1)
        for _ in range(49):
            self.request('tools', 1)
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, '50 requests'):
            self.request('tools', 1)
        self.assertEqual(self.town, before)
        self.building['construction'] = {'progress': 0, 'total': 30}
        with self.assertRaisesRegex(ValueError, 'construction'):
            self.request('tools', 1)

    def test_blocked_queue_neither_advances_nor_consumes(self):
        self.request('lanterns', 5)
        for condition in ('ingredients', 'skill', 'cap', 'storage'):
            with self.subTest(condition=condition):
                building = copy.deepcopy(self.building)
                worker = copy.deepcopy(self.worker)
                building['input_buffer'] = {'planks': 10, 'wood': 5}
                if condition == 'ingredients':
                    building['input_buffer'] = {}
                elif condition == 'skill':
                    worker['crafting'] = 1
                elif condition == 'cap':
                    self.town['rules'] = {'production_caps': {'lanterns': 0}}
                else:
                    building['output_buffer'] = {'lanterns': 5}
                before = copy.deepcopy((building, worker))
                self.assertNotEqual(logistics.produce(self.town, building, worker, 9), 'Working')
                # The existing buffer-full marker is diagnostic only.
                building.pop('buffer_full', None)
                self.assertEqual((building, worker), before)
                self.town.pop('rules', None)

    def test_invalid_requests_are_atomic_and_legacy_selection_cannot_bypass_queue(self):
        for extra in [dict(amount=True), dict(amount=0), dict(amount=1000), dict(amount=1.5),
                      dict(recipe='bread'), dict(index=-1), dict(index=True), dict(index=0)]:
            data = dict(action='queue_workshop', index=self.index, recipe='tools', amount=5)
            data.update(extra)
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                server.command(self.town, data)
            self.assertEqual(self.town, before)
        self.worker['crafting'] = 1
        with self.assertRaisesRegex(ValueError, 'assigned worker'):
            self.request('lanterns', 1)
        with self.assertRaisesRegex(ValueError, 'quantity'):
            server.command(self.town, dict(action='set_recipe', index=self.index, recipe='tools'))
        self.building['recipe'] = 'tools'
        self.assertEqual(logistics.produce(self.town, self.building, self.worker, 9), 'Waiting for workshop requests')


if __name__ == '__main__':
    unittest.main()
