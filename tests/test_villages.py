import copy
import json
from pathlib import Path
import tempfile
import unittest

import server
from chunk_world import CHUNK_SIZE, attach, chunk_key, generate_chunk
from equipment import change, protected, trader_items
from shared_world import bind_map, export_world, new_world
from villages import apply_villages, ensure_villages


class WildernessVillages(unittest.TestCase):
    def test_all_villages_have_clear_access_residents_and_structures(self):
        for seed in (0, 42, 99):
            world = new_world(seed, explorable=True)
            manager = world['_chunks']
            landscape = world['shared_map']
            self.assertEqual({v['biome'] for v in landscape['villages']}, {'b', 'd', 'i'})
            for village in landscape['villages']:
                x, y = village['x'], village['y']
                residents = [n for n in landscape['npcs'] if n.get('village') == village['name']]
                self.assertEqual(len(residents), 32 if village['biome'] == 'b' else 8)
                records = [manager.read(f'{cx},{cy}')
                           for cy in range((y-24)//CHUNK_SIZE, (y+24)//CHUNK_SIZE+1)
                           for cx in range((x-24)//CHUNK_SIZE, (x+24)//CHUNK_SIZE+1)]
                resources = [r for record in records for r in record['map_resources']]
                structures = [r for r in resources if r.get('village') == village['name']
                              and r['kind'] in ('village_hut', 'village_market')]
                self.assertEqual(len(structures), 56 if village['biome'] == 'b' else 14)
                self.assertEqual(sum(r.get('variant') == 'hall' for r in structures), 1)
                self.assertEqual(sum(r['kind'] == 'village_market' for r in structures),
                                 9 if village['biome'] == 'b' else 3)
                self.assertTrue(any(r.get('animals') for r in structures))
                planted = [r for r in resources if r.get('village') == village['name']
                           and r['kind'] in ('tree', 'herb_patch', 'berry_bush')]
                self.assertEqual(len(planted), 6)
                if village['biome'] == 'd':
                    self.assertTrue(all(r.get('species') == 'palm' for r in planted if r['kind'] == 'tree'))
                blocked = {(r['x'], r['y']) for r in resources}
                for npc in residents:
                    self.assertNotIn((npc['x'], npc['y']), blocked)
                    self.assertTrue(npc['dialogue'])
                for dx in range(-24, 25):
                    for tx, ty in ((x+dx, y), (x, y+dx)):
                        record = manager.read(chunk_key(tx, ty))
                        biome = record['terrain'][ty % CHUNK_SIZE][tx % CHUNK_SIZE]
                        self.assertNotIn((tx, ty), blocked)
                        self.assertTrue(protected({}, biome, tx, ty))

    def test_residents_trade_with_proximity_and_structures_block_building(self):
        world = new_world(42, explorable=True)
        town = server.new_town('Visitor', world)
        world['towns']['own'] = town
        for village in world['shared_map']['villages']:
            npc = next(n for n in town['npcs'] if n.get('village') == village['name'])
            index = town['npcs'].index(npc)
            for item in trader_items(npc):
                with self.assertRaisesRegex(ValueError, 'beside'):
                    change(town, {'action': 'buy_equipment', 'item': item, 'npc_index': index})
            town['player'].update(x=npc['x'], y=npc['y']+1)
            town['coins'] = 100
            for item in ['swamp_boots', 'desert_clothing']:
                if item in trader_items(npc):
                    change(town, {'action': 'buy_equipment', 'item': item, 'npc_index': index})
                else:
                    with self.assertRaisesRegex(ValueError, 'does not trade'):
                        change(town, {'action': 'buy_equipment', 'item': item, 'npc_index': index})
            world['_chunks'].activate()
            bind_map(world)
            hut = next(r for r in town['map_resources'] if r.get('village') == village['name'])
            self.assertFalse(server.walkable(town, hut['x'], hut['y']))

    def test_overlay_is_idempotent_and_survives_eviction_and_save(self):
        with tempfile.TemporaryDirectory() as directory:
            world = new_world(42, explorable=True, chunk_directory=Path(directory))
            manager = world['_chunks']
            village = world['shared_map']['villages'][0]
            key = chunk_key(village['x'], village['y'])
            record = manager.read(key)
            before = copy.deepcopy(record)
            apply_villages(record, key, world['shared_map']['villages'], CHUNK_SIZE)
            self.assertEqual(record, before)
            manager.records[key] = record
            world['shared_map']['map_resources'] += record['map_resources']
            manager.flush()
            saved = json.loads(json.dumps(export_world(world)))
            restored = attach(saved, Path(directory))
            self.assertEqual(restored.read(key), before)
            self.assertEqual(saved['shared_map']['villages'], world['shared_map']['villages'])
            self.assertEqual(saved['shared_map']['npcs'], world['shared_map']['npcs'])

    def test_migration_avoids_player_buildings_roads_and_existing_locals(self):
        world = new_world(42, explorable=True)
        del world['shared_map']['villages']
        world['shared_map']['npcs'] = [n for n in world['shared_map']['npcs'] if 'village' not in n]
        world['towns']['own'] = {'buildings': [{'x': 80, 'y': 200}], 'roads': [{'x': 360, 'y': 200}]}
        original = copy.deepcopy(world['shared_map']['npcs'])
        ensure_villages(world)
        self.assertEqual(world['shared_map']['npcs'][:len(original)], original)
        for village in world['shared_map']['villages']:
            for obstacle in world['towns']['own']['buildings'] + world['towns']['own']['roads']:
                self.assertTrue(abs(village['x']-obstacle['x']) > 26 or abs(village['y']-obstacle['y']) > 26)
        before = copy.deepcopy(world['shared_map']['villages'])
        residents = [npc for npc in world['shared_map']['npcs'] if npc.get('village')]
        positions = [(npc['x'], npc['y']) for npc in residents]
        for npc in residents:
            if 'village trader' in npc['role']:
                npc['dialogue'] = 'Old travel supplies offer'
        ensure_villages(world)
        self.assertEqual(world['shared_map']['villages'], before)
        self.assertEqual([(npc['x'], npc['y']) for npc in residents], positions)
        for npc in residents:
            if 'village trader' not in npc['role']:
                continue
            if npc['village'] in {'Sunwell', 'Reedhaven'}:
                self.assertIn('20% more', npc['dialogue'])
            else:
                self.assertIn('rest', npc['dialogue'])

    def test_expansion_preserves_traders_progress_and_changed_resources(self):
        world = new_world(42, explorable=True)
        village = world['shared_map']['villages'][0]
        x, y = village['x'], village['y']
        key = chunk_key(x, y)
        record = copy.deepcopy(world['_chunks'].read(key))
        record['village_version'] = 1
        record['map_resources'] = [r for r in record['map_resources']
                                   if not r.get('village') or (r['x']-x, r['y']-y) in
                                   {(-3, -2), (3, -2), (-3, 3), (3, 3)}]
        # A harvested wilderness resource outside the expansion must stay gone.
        record['map_resources'] = [r for r in record['map_resources']
                                   if (r['x'], r['y']) != (x+12, y+12)]
        apply_villages(record, key, [village], CHUNK_SIZE)
        self.assertFalse(any((r['x'], r['y']) == (x+12, y+12) for r in record['map_resources']))
        self.assertEqual(len({(r['x'], r['y']) for r in record['map_resources']}),
                         len(record['map_resources']))
        before = copy.deepcopy(record)
        apply_villages(record, key, [village], CHUNK_SIZE)
        self.assertEqual(record, before)
        landscape = world['shared_map']
        landscape['npcs'] = [n for n in landscape['npcs'] if 'village trader' in n['role'] or not n.get('village')]
        for v in landscape['villages']:
            v.pop('layout_version')
        traders = copy.deepcopy(landscape['npcs'])
        world['towns']['own'] = {'buildings': [], 'roads': [], 'local_visits': {'Fen:old': {'met': True}}}
        ensure_villages(world)
        self.assertEqual(landscape['npcs'][:len(traders)], traders)
        self.assertEqual(world['towns']['own']['local_visits'], {'Fen:old': {'met': True}})
        residents = copy.deepcopy(landscape['npcs'])
        ensure_villages(world)
        self.assertEqual(landscape['npcs'], residents)

    def test_expansion_preserves_villages_near_player_buildings_or_roads(self):
        for field in ('buildings', 'roads'):
            world = new_world(42, explorable=True)
            landscape = world['shared_map']
            village = landscape['villages'][0]
            village.pop('layout_version')
            landscape['npcs'] = [n for n in landscape['npcs'] if n.get('village') != village['name']
                                 or 'village trader' in n['role']]
            world['towns']['own'] = {'buildings': [], 'roads': []}
            world['towns']['own'][field].append({'x': village['x']+7, 'y': village['y']+6})
            ensure_villages(world)
            self.assertEqual(village['layout_version'], 1)
            self.assertEqual(sum(n.get('village') == village['name'] for n in landscape['npcs']), 2)

    def test_new_layout_preserves_harvested_plants_on_repeated_reads(self):
        world = new_world(42, explorable=True)
        village = world['shared_map']['villages'][0]
        x, y = village['x']-5, village['y']-4
        key = chunk_key(x, y)
        record = world['_chunks'].read(key)
        record['map_resources'] = [r for r in record['map_resources'] if (r['x'], r['y']) != (x, y)]
        record['tree_stumps'].append({'kind': 'tree', 'x': x, 'y': y, 'remaining': 300})
        before = copy.deepcopy(record)
        apply_villages(record, key, world['shared_map']['villages'], CHUNK_SIZE)
        self.assertEqual(record, before)

    def test_swamp_upgrade_preserves_old_center_and_avoids_player_work(self):
        for field in ('buildings', 'roads', 'npcs', None):
            world = new_world(42, explorable=True)
            village = world['shared_map']['villages'][0]
            vx, vy = village['x'], village['y']
            village.pop('swamp_version')
            world['shared_map']['npcs'] = [n for n in world['shared_map']['npcs']
                if not n.get('ambient_stop') and not n.get('pose')]
            old_npcs = copy.deepcopy(world['shared_map']['npcs'])
            world['towns']['own'] = {'buildings': [], 'roads': []}
            if field == 'npcs':
                world['shared_map']['npcs'].append(dict(name='Visitor', x=vx+17, y=vy+17))
            elif field:
                # Include the full footprint, not just the building's origin.
                world['towns']['own'][field].append(dict(x=vx-20, y=vy+17, footprint=[2, 2]))
            ensure_villages(world)
            self.assertEqual(village.get('swamp_version'), None if field else 1)
            self.assertEqual(world['shared_map']['npcs'][:len(old_npcs)], old_npcs)
            if field:
                continue
            key = chunk_key(vx, vy)
            record = copy.deepcopy(world['_chunks'].read(key))
            record.pop('swamp_version')
            record['map_resources'] = [r for r in record['map_resources']
                if max(abs(r['x']-vx), abs(r['y']-vy)) <= 9 or not r.get('village')]
            harvested = (vx-5, vy-4)
            record['map_resources'] = [r for r in record['map_resources']
                if (r['x'], r['y']) != harvested]
            stump = dict(kind='tree', x=harvested[0], y=harvested[1], remaining=120)
            record['tree_stumps'].append(stump)
            apply_villages(record, key, [village], CHUNK_SIZE)
            self.assertIn(stump, record['tree_stumps'])
            self.assertFalse(any((r['x'], r['y']) == harvested for r in record['map_resources']))
            self.assertEqual(len(record['map_resources']), len({(r['x'], r['y']) for r in record['map_resources']}))
            before = copy.deepcopy(record)
            apply_villages(record, key, [village], CHUNK_SIZE)
            self.assertEqual(record, before)

    def test_swamp_quarters_have_decorations_and_connected_safe_paths(self):
        import swamp_village
        import village_life
        world = new_world(99, explorable=True)
        village = world['shared_map']['villages'][0]
        vx, vy = village['x'], village['y']
        manager = world['_chunks']
        records = [manager.read(f'{cx},{cy}')
                   for cy in range((vy-19)//CHUNK_SIZE, (vy+18)//CHUNK_SIZE+1)
                   for cx in range((vx-19)//CHUNK_SIZE, (vx+18)//CHUNK_SIZE+1)]
        resources = [r for record in records for r in record['map_resources']]
        decor = [r for r in resources if r['kind'] == 'village_decor']
        self.assertEqual(len(decor), 130)
        self.assertTrue({'bench', 'candle_lantern', 'barrels', 'ferns', 'cattails', 'mushrooms', 'reeds'}
                        <= {r['variant'] for r in decor})
        self.assertEqual(sum(r.get('variant') == 'tavern' for r in resources), 1)
        blocked = {(r['x'], r['y']) for r in resources}
        for npc in world['shared_map']['npcs']:
            if npc.get('village') != village['name']:
                continue
            start = (npc['x'], npc['y'])
            self.assertNotIn(start, blocked)
            self.assertTrue(village_life.route(start, (vx, vy), blocked, village))
        for y in range(vy+swamp_village.MIN_OFFSET, vy+swamp_village.MAX_OFFSET+1):
            for x in range(vx+swamp_village.MIN_OFFSET, vx+swamp_village.MAX_OFFSET+1):
                if (x, y) in blocked:
                    continue
                record = manager.read(chunk_key(x, y))
                self.assertTrue(protected({}, record['terrain'][y % CHUNK_SIZE][x % CHUNK_SIZE], x, y))

    def test_coordinate_noise_produces_desert_and_ice(self):
        biomes = set()
        for key in ('0,0', '2,2', '8,8', '11,0', '11,11', '5,0'):
            biomes.update(''.join(generate_chunk(42, key)['terrain']))
        self.assertIn('d', biomes)
        self.assertIn('i', biomes)
