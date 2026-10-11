import copy
import json
import unittest
from unittest.mock import patch

import progression as economy
from server import command, construct, new_town, prepare_landscapes, tick


class Progression(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Progression')
        # These tests isolate base rates; talent effects have dedicated coverage.
        for villager in self.town['villagers']:
            villager['talents'] = {}
        self.town.update(terrain=['g' * 120] * 120, npcs=[], forage_version=1,
                         forage_halls=[[5, 5]], goods={}, food=40)
        self.town['map_resources'] = [
            {'kind': 'tree', 'x': 6, 'y': 7},
            {'kind': 'water', 'x': 5, 'y': 6},
            {'kind': 'water', 'x': 6, 'y': 6},
            {'kind': 'berry_bush', 'x': 4, 'y': 7},
            {'kind': 'herb_patch', 'x': 5, 'y': 8},
        ]
        self.world = {'towns': {'a': self.town}}
        prepare_landscapes(self.world)

    def workplace(self, kind, worker=0, **extra):
        building = {'kind': kind, 'x': 3, 'y': 6, **extra}
        self.town['buildings'].append(building)
        index = len(self.town['buildings']) - 1
        job = economy.BUILDINGS.get(kind, {}).get('job')
        self.town['villagers'][worker].update(economy.JOB_REQUIREMENTS.get(job, {}))
        command(self.town, {'action': 'assign_settler', 'index': worker, 'building_index': index})
        return building, index

    def test_settler_training_reaches_unlock_and_output_thresholds_on_time(self):
        worker = {'woodcutting': 0, 'talents': {}}
        for _ in range(249):
            economy.practice(worker, 'woodcutting')
        self.assertIsNotNone(economy.qualification(worker, 'woodworking'))
        economy.practice(worker, 'woodcutting')
        self.assertEqual(worker['woodcutting'], 10)
        self.assertIsNone(economy.qualification(worker, 'woodworking'))
        for _ in range(249):
            economy.practice(worker, 'woodcutting')
        self.assertEqual(economy.efficiency(worker, 'woodcutting'), 1)
        worker = json.loads(json.dumps(worker))
        economy.practice(worker, 'woodcutting')
        self.assertEqual(worker['woodcutting'], 20)
        self.assertEqual(economy.efficiency(worker, 'woodcutting'), 2)

    def test_locked_buildings_crops_animals_and_tools_do_not_spend(self):
        self.town['wood'] = 500
        for kind in economy.BUILDINGS:
            before = copy.deepcopy(self.town)
            with self.assertRaisesRegex(ValueError, 'hall level'):
                command(self.town, {'action': 'build', 'kind': kind, 'x': 3, 'y': 3})
            self.assertEqual(self.town, before)
        barn, index = self.workplace('barn')
        for action in ['buy_animal', 'breed_animal']:
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                command(self.town, {'action': action, 'index': index, 'animal': 'chicken'})
            self.assertEqual(self.town, before)
        farm, index = self.workplace('farm')
        self.town['buildings'].append({'kind':'acre','x':3,'y':7,'farm_index':index})
        with self.assertRaises(ValueError):
            command(self.town, {'action':'set_crop','index':index+1,'crop':'wheat'})
        self.town['buildings'].append({'kind':'woodcutter','x':7,'y':7})
        with self.assertRaises(ValueError):
            command(self.town, {'action':'profession_upgrade','index':index+2,'upgrade':'axe'})

    @patch('progression.secrets.randbelow', return_value=1)
    def test_character_actions_repeat_learn_save_and_stop_on_move(self, _roll):
        for action, job in [('chop','woodcutting'),('fish','fishing'),('gather','gathering')]:
            command(self.town, {'action': action}, self.world)
            # Starting or spamming requests never grants production or practice.
            self.assertEqual(self.town['player'].get(job,0),0)
            tick(self.world)
            self.assertEqual(self.town['player'][job],1)
            tick(self.world)
            self.assertEqual(self.town['player'][job],2)
        saved = json.loads(json.dumps(self.world))
        self.assertEqual(saved['towns']['a']['player']['gathering'],2)
        command(self.town, {'action':'walk','dx':1,'dy':0}, self.world)
        self.assertNotIn('activity',self.town['player'])
        xp=self.town['player']['gathering'];tick(self.world)
        self.assertEqual(self.town['player']['gathering'],xp)

    def test_gather_herbs_and_full_storage_pause_practice(self):
        self.town['buildings'].append({'kind':'storehouse','x':3,'y':3,'storage_allocations':{'herbs':100}})
        self.town['player'].update(x=5,y=9)
        command(self.town, {'action':'gather'}, self.world)
        tick(self.world)
        self.assertEqual(self.town['goods']['herbs'],1)
        self.town['goods']['herbs']=100
        self.town['buildings'][-1]['stored_goods']['herbs']=100
        tick(self.world)
        self.assertEqual(self.town['player']['gathering'],1)
        self.assertEqual(self.town['player']['activity_status'],'Storage full')
        command(self.town, {'action':'stop_activity'})
        self.assertNotIn('activity',self.town['player'])
        self.town['player'].update(x=20,y=20)
        before=copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            command(self.town, {'action':'fish'})
        self.assertEqual(self.town,before)

    @patch('progression.secrets.randbelow', return_value=1)
    def test_connected_pond_is_shared_across_players_and_regenerates_once(self, _roll):
        friend=copy.deepcopy(self.town)
        shared={k:self.town[k] for k in ['terrain','map_resources','map_size','npcs','forage_version','forage_halls']}
        self.world={'towns':{'a':self.town,'b':friend},'shared_map':shared}
        prepare_landscapes(self.world)
        for source in shared['map_resources']:
            if source['kind']=='water':source['supply']=0
        for town in [self.town,friend]:
            town['player']['fishing']=40
            command(town, {'action':'fish'}, self.world)
        tick(self.world)
        self.assertAlmostEqual(sum(t.get('produced',{}).get('fish',0) for t in [self.town,friend]),3.2)
        pond=[r for r in shared['map_resources'] if r['kind']=='water']
        self.assertTrue(all(r['supply']==0 for r in pond))
        # GET-style preparation never replenishes supply.
        prepare_landscapes(self.world)
        self.assertTrue(all(r['supply']==0 for r in pond))
        tick(self.world)
        self.assertAlmostEqual(sum(t.get('produced',{}).get('fish',0) for t in [self.town,friend]),6.4)

    def test_legacy_implicit_workers_cannot_overfill_one_workplace(self):
        self.town['buildings'].append({'kind':'woodcutter','x':7,'y':7})
        for v in self.town['villagers']:
            v['job']='woodcutting';v['woodcutting']=0
        self.town['wood']=0
        tick(self.world)
        self.assertEqual(self.town['wood'],.5)
        self.assertEqual(self.town['buildings'][1]['output_buffer']['wood'],.5)
        self.assertEqual(sum(v['woodcutting'] for v in self.town['villagers']),.04)

    def test_processing_consumes_local_materials_and_pauses_when_buffer_full(self):
        self.town['buildings'].append({'kind': 'storehouse', 'x': 10, 'y': 10,
                                      'storage_allocations': {'planks': 100}})
        self.town.update(level=4, wood=0)
        b,index=self.workplace('sawpit', input_buffer={'wood':10})
        tick(self.world)
        self.assertEqual(b['input_buffer']['wood'],8)
        self.assertEqual(b['output_buffer']['planks'],1)
        self.assertEqual(self.town['goods'].get('planks',0),1)
        b['output_buffer']['planks']=50
        self.town['buildings'][1]['stored_goods'] = {'planks': 100}
        tick(self.world)
        self.assertEqual(b['input_buffer']['wood'],8)
        self.assertEqual(self.town['villagers'][0]['woodworking'],.04)
        b['output_buffer']['planks']=0;b['input_buffer']['wood']=0
        self.town['buildings'][1]['stored_goods'] = {}
        tick(self.world)
        self.assertEqual(self.town['villagers'][0]['production_status'],'Waiting for delivery: wood')

    def test_full_chain_crop_mill_and_tool_production(self):
        self.town['buildings'].append({'kind': 'storehouse', 'x': 10, 'y': 10,
                                      'storage_allocations': {'flour': 50, 'tools': 50}})
        self.town.update(level=4,goods={},wood=0)
        mill,_=self.workplace('mill',0,input_buffer={'wheat':4})
        workshop,_=self.workplace('workshop',1,x=4,y=6,input_buffer={'planks':4,'wood':10})
        tick(self.world)
        self.assertEqual(mill['output_buffer']['flour'],1)
        self.assertEqual(workshop['output_buffer']['tools'],1)
        self.assertEqual(mill['input_buffer']['wheat'],2)
        self.assertEqual(workshop['input_buffer']['planks'],2)
        self.assertTrue(economy.milestone(self.town,5))

    def test_animals_feed_breed_and_obey_capacity_without_worker_multiplier(self):
        self.town['buildings'].append({'kind': 'storehouse', 'x': 10, 'y': 10,
                                      'storage_allocations': {'wool': 50, 'manure': 50}})
        self.town.update(level=3,coins=100,goods={'fodder':10,'herbs':2})
        barn,index=self.workplace('barn',input_buffer={'fodder':10})
        for _ in range(2):
            command(self.town, {'action':'buy_animal','index':index,'animal':'sheep'})
        command(self.town, {'action':'assign_settler','index':1,'building_index':index})
        tick(self.world)
        self.assertEqual(barn['output_buffer']['wool'],.5)
        self.assertEqual(barn['input_buffer']['fodder'],9.5)
        self.assertEqual(self.town['goods']['fodder'],10)
        for _ in range(2):
            command(self.town, {'action':'breed_animal','index':index,'animal':'sheep'})
        self.assertEqual(barn['animals']['sheep'],4)
        before=copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError,'Barn full'):
            command(self.town, {'action':'buy_animal','index':index,'animal':'chicken'})
        self.assertEqual(self.town,before)
        self.town['goods']['fodder']=0;barn['input_buffer']['fodder']=0
        tick(self.world)
        self.assertEqual(barn['output_buffer']['wool'],.5)

    def test_profession_upgrade_needs_skill_and_charges_once(self):
        self.town.update(level=2,wood=30,coins=10)
        b,index=self.workplace('woodcutter',x=7,y=7)
        with self.assertRaises(ValueError):
            command(self.town, {'action':'profession_upgrade','index':index,'upgrade':'axe'})
        self.town['player']['woodcutting']=10
        command(self.town, {'action':'profession_upgrade','index':index,'upgrade':'axe'})
        self.assertEqual(self.town['wood'],20)
        self.assertEqual(self.town['coins'],5)
        self.assertNotIn('axe', b.get('profession_upgrades', []))
        with self.assertRaisesRegex(ValueError, 'Finish construction'):
            command(self.town, {'action':'profession_upgrade','index':index,'upgrade':'axe'})
        for _ in range(100):
            construct(self.town, self.town)
            if 'construction' not in b:
                break
        self.assertNotIn('construction', b)
        self.assertIn('axe', b['profession_upgrades'])
        if 'construction' in b:
            for _ in range(100):
                construct(self.town, self.town)
                if 'construction' not in b:break
        tick(self.world)
        self.assertEqual(self.town['wood'],20.625)
        self.assertEqual(b['output_buffer']['wood'],.625)
        with self.assertRaisesRegex(ValueError,'Already unlocked'):
            command(self.town, {'action':'profession_upgrade','index':index,'upgrade':'axe'})

    def test_edible_goods_are_counted_once_when_processed_eaten_and_sold(self):
        self.town.update(level=5,goods={},food=0,wood=0)
        b,index=self.workplace('smokehouse',input_buffer={'fish':2,'wood':.5})
        tick(self.world)
        self.assertAlmostEqual(self.town['food'],3 - .78)
        self.assertEqual(b['input_buffer']['fish'],0)
        self.assertAlmostEqual(b['output_buffer']['smoked_fish'],2 - .78 / 1.5)
        import logistics
        logistics.transfer(self.town,b,self.town['buildings'][0],'collect',self.town['villagers'][0])
        self.assertAlmostEqual(self.town['food'],3 - .78)
        self.town['buildings'].append({'kind':'market','x':4,'y':5})
        command(self.town, {'action':'sell_goods','resource':'smoked_fish','amount':1})
        self.assertAlmostEqual(self.town['food'],1.5 - .78)
        self.assertAlmostEqual(self.town['goods']['smoked_fish'],1 - .78 / 1.5)
        economy.eat(self.town,1.5)
        self.assertEqual(self.town['food'],0)
        self.assertAlmostEqual(self.town['goods']['smoked_fish'],0)

    def test_forage_is_saved_and_added_for_towns_founded_after_world_initialization(self):
        landscape={'map_resources':[], 'terrain':['g'*120]*120,'map_size':120,'npcs':[]}
        economy.ensure_forage(landscape,[])
        economy.ensure_forage(landscape,[{'kind':'hall','x':5,'y':5}])
        self.assertEqual({r['kind'] for r in landscape['map_resources']},{'berry_bush','herb_patch'})
        before=json.dumps(landscape)
        economy.ensure_forage(landscape,[{'kind':'hall','x':5,'y':5}])
        self.assertEqual(json.dumps(landscape),before)
        economy.ensure_forage(landscape,[{'kind':'hall','x':6,'y':5}])
        self.assertEqual(json.dumps(landscape),before)  # Moving halls cannot create supplies.
