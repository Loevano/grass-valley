import copy
import json
import unittest
from unittest.mock import patch

import logistics
import progression as economy
import server


class ProductionRules(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Rules')
        economy.ensure_inventory(self.town)

    def rule(self, resource, cap):
        return server.command(self.town, {'action': 'configure_rule', 'resource': resource, 'cap': cap})

    def test_rules_validate_save_and_clear_without_changing_other_towns(self):
        other = server.new_town('Other')
        self.rule('wood', 0)
        self.rule('planks', 100)
        restored = json.loads(json.dumps(self.town))
        self.assertEqual(restored['rules']['production_caps'], {'wood': 0, 'planks': 100})
        self.assertNotIn('rules', other)
        before = copy.deepcopy(self.town)
        for cap in (-1, True, '10', float('nan'), float('inf'), [], {}):
            with self.assertRaises(ValueError):
                self.rule('wood', cap)
            self.assertEqual(self.town, before)
        with self.assertRaises(ValueError):
            self.rule('coins', 10)
        self.rule('wood', None)
        self.assertEqual(self.town['rules']['production_caps'], {'planks': 100})
        self.assertEqual(economy.production_room(self.town, 'wood'), float('inf'))

    def test_counts_reserved_and_pending_once_and_resumes_after_spending(self):
        self.town['buildings'].append({'kind': 'sawpit', 'x': 3, 'y': 3,
                                      'input_buffer': {'wood': 2}, 'pending_output': {'wood': .5},
                                      'orphaned_output': {'wood': .5}})
        self.rule('wood', self.town['wood'] + 2.5)
        self.assertEqual(economy.production_room(self.town, 'wood'), 0)
        economy.spend(self.town, {'wood': 1})
        self.assertEqual(economy.production_room(self.town, 'wood'), 1)

    def test_recipe_waits_without_consuming_inputs_or_gaining_practice(self):
        building = {'kind': 'sawpit', 'x': 3, 'y': 3, 'input_buffer': {'wood': 10}}
        self.town['buildings'].append(building)
        worker = {'job': 'woodworking', 'woodworking': 0, 'woodcutting': 10, 'talents': {}}
        self.rule('planks', .5)
        before = copy.deepcopy(building)
        self.assertEqual(logistics.produce(self.town, building, worker, elapsed=economy.production_seconds(building)), 'Production cap reached')
        self.assertEqual(building, before)
        self.assertEqual(worker['woodworking'], 0)
        self.rule('planks', 1)
        self.assertEqual(logistics.produce(self.town, building, worker, elapsed=economy.production_seconds(building)), 'Working')
        self.assertEqual(building['input_buffer']['wood'], 8)
        self.assertEqual(building['pending_output']['planks'], 1)
        self.assertEqual(economy.production_room(self.town, 'planks'), 0)
        second = {'kind': 'sawpit', 'x': 4, 'y': 3, 'input_buffer': {'wood': 10}}
        self.town['buildings'].append(second)
        self.assertEqual(logistics.produce(self.town, second, worker, elapsed=economy.production_seconds(second)), 'Production cap reached')
        logistics.drop_off(self.town, building, worker)
        economy.spend(self.town, {'planks': 1})
        self.assertEqual(logistics.produce(self.town, second, worker, elapsed=economy.production_seconds(second)), 'Working')

    def test_barn_waits_for_all_outputs_including_manure(self):
        barn = {'kind': 'barn', 'x': 3, 'y': 3, 'animals': {'chicken': 1},
                'animal_growth': {'chicken': [0]}, 'input_buffer': {'vegetables': 10}}
        self.town['buildings'].append(barn)
        worker = {'job': 'husbandry', 'husbandry': 0, 'farming': 10, 'talents': {}}
        self.rule('manure', 0)
        before = copy.deepcopy(barn)
        self.assertEqual(logistics.produce(self.town, barn, worker), 'Production cap reached')
        self.assertEqual(barn, before)

    def test_gathering_stops_without_depleting_source_and_resumes(self):
        player = self.town['player']
        player.update(x=3, y=3, activity={'job': 'woodcutting', 'kind': 'tree', 'x': 3, 'y': 2})
        landscape = {'map_resources': [{'kind': 'tree', 'x': 3, 'y': 2, 'supply': 8}]}
        self.rule('wood', self.town['wood'])
        economy.player_tick(self.town, landscape, 100)
        self.assertEqual(player['activity_status'], 'Production cap reached')
        self.assertEqual(landscape['map_resources'][0]['supply'], 8)
        economy.spend(self.town, {'wood': .25})
        economy.player_tick(self.town, landscape, 100)
        self.assertEqual(economy.production_room(self.town, 'wood'), 0)
        self.assertEqual(landscape['map_resources'][0]['supply'], 7.75)

    def test_workers_share_cap_and_partial_harvest_fits(self):
        self.town.update(terrain=['g' * 120] * 120, npcs=[], level=5,
                         forage_version=1, forage_halls=[[5, 5]],
                         map_resources=[{'kind': 'tree', 'x': 3, 'y': 1},
                                        {'kind': 'tree', 'x': 4, 'y': 1}])
        for i in range(2):
            building = {'kind': 'woodcutter', 'x': 3 + i, 'y': 2}
            self.town['buildings'].append(building)
            self.town['villagers'][i]['talents'] = {}
            self.town['villagers'][i]['construction_position'] = {'x': 3 + i, 'y': 0}
            server.assign_settler(self.town, i, len(self.town['buildings']) - 1)
        self.rule('wood', self.town['wood'] + .25)
        server.tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['buildings'][1]['pending_output']['wood'], .25)
        self.assertEqual(self.town['villagers'][1]['production_status'], 'Production cap reached')


if __name__ == '__main__':
    unittest.main()
