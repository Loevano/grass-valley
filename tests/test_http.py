"""Exercise the real HTTP host with separate client ownership tokens."""
import http.client
import json
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch
from http.server import ThreadingHTTPServer

import server


class LanHost(unittest.TestCase):
    def test_slow_world_transfer_does_not_block_movement_or_player_snapshots(self):
        owner = json.loads(self.call('/api/join', {'name': 'LAN home'})[1])
        original_player = dict(server.WORLD['towns'][owner['town_id']]['player'])
        sending, release = threading.Event(), threading.Event()
        original_reply = server.Handler.reply_body

        def slow_reply(handler, status, body):
            if handler.path == '/api/world':
                sending.set()
                if not release.wait(5):
                    raise TimeoutError('World transfer was not released.')
            original_reply(handler, status, body)

        with patch.object(server.Handler, 'reply_body', slow_reply), ThreadPoolExecutor(max_workers=1) as client:
            transfer = client.submit(self.call, '/api/world')
            try:
                self.assertTrue(sending.wait(2), 'World snapshot reaches the transfer stage.')
                status, body = self.call('/api/command', {'action': 'walk', 'dx': 1, 'dy': -1}, owner['token'])
                self.assertEqual(status, 200)
                moved_player = json.loads(body)['player']
                self.assertGreater(moved_player['x'], original_player['x'])
                status, body = self.call('/api/players')
                self.assertEqual(status, 200)
                self.assertEqual(json.loads(body)['players'][owner['town_id']], moved_player)
            finally:
                release.set()
            status, body = transfer.result(timeout=3)
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)['towns'][owner['town_id']]['player'], original_player,
                         'The in-flight world snapshot stays consistent while the live player moves.')

    def test_production_rules_are_owned_and_saved_with_the_town(self):
        _, owner = self.call('/api/join', {'name': 'Rules home'})
        _, friend = self.call('/api/join', {'name': 'Friend'})
        owner, friend = json.loads(owner), json.loads(friend)
        rule = {'action': 'configure_rule', 'resource': 'planks', 'cap': 100,
                'town_id': friend['town_id']}
        self.assertEqual(self.call('/api/command', rule)[0], 401)
        self.assertEqual(self.call('/api/command', rule, owner['token'])[0], 200)
        server.load()
        self.assertEqual(server.WORLD['towns'][owner['town_id']]['rules']['production_caps'],
                         {'planks': 100})
        self.assertNotIn('rules', server.WORLD['towns'][friend['town_id']])

    def test_npc_story_catalog_and_progress_are_owned_and_saved(self):
        _, owner = self.call('/api/join', {'name': 'Story home'})
        _, friend = self.call('/api/join', {'name': 'Friend'})
        owner, friend = json.loads(owner), json.loads(friend)
        town = server.WORLD['towns'][owner['town_id']]
        town['npcs'] = [{'name': 'Moss', 'x': 5, 'y': 8}]
        status, body = self.call('/api/world')
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)['local_stories'], server.npcs.STORIES)
        request = {'action': 'advance_local_story', 'npc_index': 0,
                   'town_id': friend['town_id']}
        self.assertEqual(self.call('/api/command', request)[0], 401)
        self.assertEqual(self.call('/api/command', request, owner['token'])[0], 200)
        key = server.npcs.local_key(town['npcs'][0])
        self.assertEqual(town['local_visits'][key]['story_stage'], 1)
        self.assertNotIn('local_visits', server.WORLD['towns'][friend['town_id']])
        server.load()
        self.assertEqual(server.WORLD['towns'][owner['town_id']]['local_visits'][key]['story_stage'], 1)

    def test_character_movement_is_owned_uninterrupted_and_saved(self):
        _, owner = self.call('/api/join', {'name': 'Home'})
        _, friend = self.call('/api/join', {'name': 'Friend'})
        owner, friend = json.loads(owner), json.loads(friend)
        move = {'action': 'walk', 'dx': 1, 'dy': -1,
                'town_id': friend['town_id']}
        status, _ = self.call('/api/command', move)
        self.assertEqual(status, 401)
        status, result = self.call('/api/command', move, owner['token'])
        self.assertEqual(status, 200)
        result = json.loads(result)
        self.assertGreater(result['player']['x'], 5)
        friend_player = server.WORLD['towns'][friend['town_id']]['player']
        self.assertEqual(friend_player['x'], 5)
        batch = dict(move, steps=[{'dx': 1, 'dy': -1}] * 3)
        self.assertEqual(self.call('/api/command', batch)[0], 401)
        status, body = self.call('/api/command', batch, owner['token'])
        self.assertEqual(status, 200)
        next_result = json.loads(body)
        self.assertGreater(next_result['player']['x'], result['player']['x'])
        self.assertEqual(friend_player['x'], 5)
        result = next_result
        for _ in range(3):
            status, body = self.call('/api/command', move, owner['token'])
            self.assertEqual(status, 200)
            next_result = json.loads(body)
            self.assertGreater(next_result['player']['x'], result['player']['x'])
            result = next_result
        server.load()
        self.assertEqual(server.WORLD['towns'][owner['town_id']]['player'], result['player'])

    def test_player_snapshot_is_small_current_and_has_no_ownership_tokens(self):
        _, owner = self.call('/api/join', {'name': 'Home'})
        owner = json.loads(owner)
        status, body = self.call('/api/players')
        self.assertEqual(status, 200)
        snapshot = json.loads(body)
        self.assertEqual(snapshot['world_id'], server.ACTIVE_WORLD)
        self.assertEqual(snapshot['players'][owner['town_id']],
                         server.WORLD['towns'][owner['town_id']]['player'])
        self.assertNotIn(owner['token'].encode(), body)
        self.assertNotIn(b'terrain', body)
        self.assertLess(len(body), 1000)
        status, result = self.call('/api/command', {'action': 'walk', 'dx': 1, 'dy': -1}, owner['token'])
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(self.call('/api/players')[1])['players'][owner['town_id']],
                         json.loads(result)['player'])
        full = json.loads(self.call('/api/world')[1])
        self.assertGreaterEqual(full['players_sampled_at'], snapshot['sampled_at'])
        server.activate(None)
        empty = json.loads(self.call('/api/players')[1])
        self.assertIsNone(empty['world_id'])
        self.assertEqual(empty['players'], {})

    def setUp(self):
        self.previous = server.WORLD, server.TOKENS, server.SAVE, server.WORLDS, server.ACTIVE_WORLD
        self.directory = tempfile.TemporaryDirectory()
        server.WORLD, server.TOKENS = {'towns': {}}, {}
        server.WORLDS = {'legacy': {'name': 'My first world', 'world': server.WORLD, 'tokens': server.TOKENS}}
        server.activate('legacy')
        server.SAVE = Path(self.directory.name) / 'savegame.json'
        self.host = ThreadingHTTPServer(('127.0.0.1', 0), server.Handler)
        self.thread = threading.Thread(target=self.host.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.host.shutdown()
        self.host.server_close()
        self.thread.join()
        server.WORLD, server.TOKENS, server.SAVE, server.WORLDS, server.ACTIVE_WORLD = self.previous
        self.directory.cleanup()

    def call(self, path, data=None, token='', headers=None):
        if data is not None and path in {'/api/join', '/api/command'}:
            data = dict(data)
            data.setdefault('world_id', server.ACTIVE_WORLD)
        connection = http.client.HTTPConnection('127.0.0.1', self.host.server_port, timeout=3)
        connection.request('POST' if data is not None else 'GET', path,
                           json.dumps(data) if data is not None else None,
                           dict({'Content-Type': 'application/json', 'Authorization': token}, **(headers or {})))
        response = connection.getresponse()
        status, body = response.status, response.read()
        connection.close()
        return status, body

    def login(self, name='Rowan', pin='4826', **extra):
        return self.call('/api/login', dict(player_name=name, pin=pin,
                                          world_id=server.ACTIVE_WORLD, **extra))

    def test_player_login_recovers_progress_from_another_browser_after_reload(self):
        self.assertTrue(json.loads(self.login()[1])['needs_town'])
        status, body = self.login(town_name='Rowan Village')
        self.assertEqual(status, 200)
        identity = json.loads(body)
        server.WORLD['towns'][identity['town_id']]['coins'] = 73
        server.save()
        server.load()
        recovered = json.loads(self.login(' rowan ')[1])
        self.assertEqual(recovered, identity)
        self.assertEqual(server.WORLD['towns'][identity['town_id']]['coins'], 73)
        self.assertEqual(len(server.WORLD['towns']), 1)
        self.assertEqual(self.login(pin='9999')[0], 400)
        saved = server.SAVE.read_text()
        account = json.loads(saved)['worlds']['legacy']['players']['rowan']
        self.assertNotIn('pin', account)
        self.assertNotEqual(account['pin_hash'], '4826')
        public = self.call('/api/world')[1]
        self.assertNotIn(b'pin_hash', public)
        self.assertNotIn(b'pin_hash', self.call('/api/worlds')[1])

    def test_player_login_validates_credentials_and_full_world_reconnects(self):
        for name, pin in [('', '4826'), ([], '4826'), ('Rowan', 4826),
                          ('Rowan', '123'), ('Rowan', 'abcd'), ('Rowan', '１２３４')]:
            self.assertEqual(self.login(name, pin, town_name='No')[0], 400)
        identity = json.loads(self.login(town_name='One')[1])
        for name in ['Two', 'Three', 'Four']:
            self.call('/api/join', {'name': name})
        self.assertEqual(json.loads(self.login()[1]), identity)
        self.assertEqual(self.login('New', town_name='Fifth')[0], 400)
        self.assertEqual(self.call('/api/login', {'world_id': 'missing'})[0], 409)

    def test_old_town_claim_requires_its_token_and_private_provision_survives_load(self):
        import player_login
        identity = json.loads(self.call('/api/join', {'name': 'Existing Village'})[1])
        data = {'world_id': server.ACTIVE_WORLD, 'player_name': 'Founder', 'pin': '4826'}
        self.assertTrue(json.loads(self.call('/api/login', data)[1])['needs_town'])
        self.assertEqual(json.loads(self.call('/api/login', data, identity['token'])[1]), identity)
        self.assertEqual(self.call('/api/login', dict(data, player_name='Other'), identity['token'])[0], 400)
        server.SAVE.with_suffix('.players.json').write_text(json.dumps({
            'legacy': {'founder': player_login.account('Founder', '9999', identity['town_id'])}}))
        server.load()
        self.assertEqual(json.loads(self.login('Founder')[1]), identity)
        self.assertEqual(self.login('Founder', '9999')[0], 400)

    def test_host_provision_links_an_existing_village_without_changing_progress(self):
        import player_login
        identity = json.loads(self.call('/api/join', {'name': 'Existing Village'})[1])
        before = json.loads(json.dumps(server.WORLD['towns'][identity['town_id']]))
        server.SAVE.with_suffix('.players.json').write_text(json.dumps({
            'legacy': {'founder': player_login.account('Founder', '4826', identity['town_id'])}}))
        server.load()
        self.assertEqual(json.loads(self.login('Founder')[1]), identity)
        self.assertEqual(server.WORLD['towns'][identity['town_id']], before)
        server.load()
        self.assertEqual(json.loads(self.login('Founder')[1]), identity)

    def test_clients_own_separate_towns_and_save_tokens_privately(self):
        identities = []
        for name in ['Willowbrook', 'Oakvale', 'Birchwood', 'Meadow']:
            status, body = self.call('/api/join', {'name': name})
            self.assertEqual(status, 200)
            identities.append(json.loads(body))
        self.assertEqual(self.call('/api/join', {'name': 'Fifth'})[0], 400)
        build = {'action': 'build', 'kind': 'farm', 'x': 1, 'y': 2,
                 'town_id': identities[1]['town_id']}
        self.assertEqual(self.call('/api/command', build)[0], 401)
        self.assertEqual(self.call('/api/command', build, identities[0]['token'])[0], 200)
        status, body = self.call('/api/world')
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)['hall_upgrades'], server.HALL_UPGRADES)
        towns = json.loads(body)['towns']
        self.assertEqual(len(towns[identities[0]['town_id']]['buildings']), 2)
        self.assertEqual(len(towns[identities[1]['town_id']]['buildings']), 1)
        for identity in identities:
            self.assertNotIn(identity['token'].encode(), body)
        saved = json.loads(server.SAVE.read_text())
        self.assertEqual(saved['worlds']['legacy']['world']['towns'], towns)
        self.assertEqual(saved['worlds']['legacy']['tokens'][identities[0]['token']], identities[0]['town_id'])
        self.assertEqual(self.call('/')[0], 200)
        self.assertEqual(self.call('/app.js')[0], 200)

    def test_worlds_keep_towns_and_tokens_separate_and_survive_reload(self):
        first = json.loads(self.call('/api/join', {'name': 'First town'})[1])
        self.assertEqual(self.call('/api/worlds/create', {'name': 'Second world'})[0], 200)
        second_id = server.ACTIVE_WORLD
        second = json.loads(self.call('/api/join', {'name': 'Second town'})[1])
        # Generated terrain and locals vary; isolate ownership using a valid clearing tile.
        town = server.WORLDS['legacy']['world']['towns'][first['town_id']]
        occupied = {(b['x'], b['y']) for b in town['buildings'] + town['map_resources'] + town['npcs']}
        x, y = next((x, y) for x in range(2, 8) for y in range(2, 9)
                    if (x, y) not in occupied and (x, y) != (5, 7))
        build = {'action': 'build', 'kind': 'farm', 'x': x, 'y': y}
        self.assertEqual(self.call('/api/command', build, first['token'])[0], 401)
        self.assertEqual(self.call('/api/command', dict(build, world_id='legacy'), second['token'])[0], 409)
        self.assertEqual(self.call('/api/join', {'name': 'Stale', 'world_id': 'legacy'})[0], 409)
        self.assertEqual(self.call('/api/worlds/open', {'world_id': 'legacy'})[0], 200)
        self.assertEqual(self.call('/api/command', build, first['token'])[0], 200)
        server.load()
        self.assertEqual(server.ACTIVE_WORLD, 'legacy')
        self.assertEqual(server.TOKENS[first['token']], first['town_id'])
        self.assertEqual(server.WORLDS[second_id]['tokens'][second['token']], second['town_id'])
        catalog = self.call('/api/worlds')[1]
        self.assertNotIn(first['token'].encode(), catalog)
        self.assertNotIn(second['token'].encode(), catalog)

    def test_delete_requires_name_and_preserves_other_worlds(self):
        self.call('/api/worlds/create', {'name': 'Keep me'})
        keep_id = server.ACTIVE_WORLD
        self.assertEqual(self.call('/api/worlds/delete', {'world_id': 'legacy'})[0], 400)
        self.assertIn('legacy', server.WORLDS)
        self.assertEqual(self.call('/api/worlds/delete', {'world_id': 'legacy', 'confirmation': 'My first world'})[0], 200)
        self.assertEqual(server.ACTIVE_WORLD, keep_id)
        self.assertEqual(self.call('/api/worlds/delete', {'world_id': keep_id, 'confirmation': 'Keep me'})[0], 200)
        self.assertIsNone(server.ACTIVE_WORLD)
        self.assertEqual(json.loads(self.call('/api/world')[1])['towns'], {})
        self.assertEqual(self.call('/api/join', {'name': 'No world'})[0], 409)
        server.load()
        self.assertEqual(server.WORLDS, {})
        self.assertEqual(self.call('/api/worlds/create', {'name': 'Fresh start'})[0], 200)

    def test_sandbox_regeneration_resets_only_confirmed_sandbox_and_survives_reload(self):
        self.assertEqual(self.call('/api/worlds/regenerate', {
            'world_id': 'legacy', 'confirmation': 'My first world'})[0], 400)
        self.assertEqual(self.call('/api/worlds/create', {'name': 'Terrain lab', 'sandbox': True})[0], 200)
        old_id = server.ACTIVE_WORLD
        owner = json.loads(self.call('/api/join', {'name': 'Disposable town'})[1])
        old_seed = server.WORLD['shared_map']['terrain_seed']
        chunk_folder = server.SAVE.with_name(server.SAVE.stem+'_chunks') / str(old_seed)
        self.assertTrue(chunk_folder.exists())
        self.assertEqual(self.call('/api/worlds/regenerate', {'world_id': old_id})[0], 400)
        self.assertIn(owner['town_id'], server.WORLD['towns'])
        self.assertEqual(self.call('/api/worlds/regenerate', {
            'world_id': old_id, 'confirmation': 'Terrain lab'},
            headers={'Host': '192.168.1.2:8000'})[0], 403)
        self.assertEqual(self.call('/api/worlds/regenerate', {
            'world_id': old_id, 'confirmation': 'Terrain lab'})[0], 200)
        replacement_id = server.ACTIVE_WORLD
        self.assertNotEqual(replacement_id, old_id)
        self.assertNotIn(old_id, server.WORLDS)
        self.assertIn('legacy', server.WORLDS)
        self.assertEqual(server.WORLD['towns'], {})
        self.assertEqual(server.TOKENS, {})
        self.assertEqual(server.WORLD['shared_map']['terrain_version'], 5)
        self.assertNotEqual(server.WORLD['shared_map']['terrain_seed'], old_seed)
        self.assertFalse(chunk_folder.exists())
        server.load()
        self.assertEqual(server.ACTIVE_WORLD, replacement_id)
        self.assertTrue(server.WORLDS[replacement_id]['world']['sandbox'])
        catalog = json.loads(self.call('/api/worlds')[1])
        self.assertTrue(next(w for w in catalog['worlds'] if w['id']==replacement_id)['sandbox'])

    def test_sandbox_regeneration_retains_credentials_and_recreates_villages(self):
        self.call('/api/worlds/create', {'name': 'Terrain lab', 'sandbox': True})
        old_id = server.ACTIVE_WORLD
        owners = [json.loads(self.login(name, pin, town_name=town)[1])
                  for name, pin, town in [('Rowan', '4826', 'Willowbrook'),
                                          ('Hazel', '7351', 'Oakvale')]]
        accounts = json.loads(json.dumps(server.WORLDS[old_id]['players']))
        for owner in owners:
            server.WORLD['towns'][owner['town_id']]['wood'] = 999
        self.assertEqual(self.call('/api/worlds/regenerate', {
            'world_id': old_id, 'confirmation': 'Terrain lab'})[0], 200)
        entry = server.WORLDS[server.ACTIVE_WORLD]
        self.assertEqual(entry['players'], accounts)
        self.assertEqual(entry['tokens'], {})
        for owner, name, pin, town_name in zip(owners, ['Rowan', 'Hazel'], ['4826', '7351'], ['Willowbrook', 'Oakvale']):
            town = server.WORLD['towns'][owner['town_id']]
            self.assertEqual((town['name'], town['wood'], len(town['buildings'])), (town_name, 65, 1))
            self.assertEqual(town['level'], 10)
            self.assertEqual(self.login(name, '0000')[0], 400)
            status, body = self.login(name, pin)
            self.assertEqual(status, 200)
            self.assertEqual(json.loads(body)['town_id'], owner['town_id'])
            self.assertNotEqual(json.loads(body)['token'], owner['token'])
        snapshot = self.call('/api/world')[1]
        self.assertNotIn(b'pin_hash', snapshot)
        server.load()
        self.assertEqual(server.WORLDS[server.ACTIVE_WORLD]['players'], accounts)
        self.assertEqual(self.login('Rowan', '4826')[0], 200)

    def test_world_management_rejects_remote_addresses_and_origins(self):
        for headers in [{'Host': '192.168.1.2:8000'}, {'Origin': 'http://evil.example'}]:
            catalog = json.loads(self.call('/api/worlds', headers=headers)[1])
            self.assertFalse(catalog['can_manage'])
            for path, data in [('/api/worlds/create', {'name': 'No'}),
                               ('/api/worlds/open', {'world_id': 'legacy'}),
                               ('/api/worlds/regenerate', {'world_id': 'legacy', 'confirmation': 'My first world'}),
                               ('/api/worlds/delete', {'world_id': 'legacy', 'confirmation': 'My first world'})]:
                self.assertEqual(self.call(path, data, headers=headers)[0], 403)
        from unittest.mock import patch
        with patch.object(server.Handler, 'is_host', return_value=False):
            self.assertEqual(self.call('/api/worlds/create', {'name': 'Remote'})[0], 403)
        self.assertEqual(list(server.WORLDS), ['legacy'])

    def test_host_check_uses_connection_address(self):
        handler = server.Handler.__new__(server.Handler)
        handler.headers = {'Host': 'localhost:8000'}
        handler.client_address = ('192.168.1.20', 12345)
        self.assertFalse(handler.is_host())
        handler.client_address = ('127.0.0.1', 12345)
        self.assertTrue(handler.is_host())
        handler.headers = {'Host': '[invalid'}
        self.assertFalse(handler.is_host())

    def test_only_the_open_world_produces(self):
        from unittest.mock import patch
        identity = json.loads(self.call('/api/join', {'name': 'Paused town'})[1])
        paused = server.WORLD['towns'][identity['town_id']]
        food = paused['food']
        self.call('/api/worlds/create', {'name': 'Playing'})
        current_identity = json.loads(self.call('/api/join', {'name': 'Current town'})[1])
        current = server.WORLD['towns'][current_identity['town_id']]
        with patch.object(server.time, 'sleep', side_effect=[None, RuntimeError('stop')]):
            with self.assertRaises(RuntimeError):
                server.simulation()
        self.assertEqual(paused['food'], food)
        self.assertLess(current['food'], food)

    def test_legacy_save_migration_preserves_progress_and_ownership(self):
        identity = json.loads(self.call('/api/join', {'name': 'Old town'})[1])
        old_world, old_tokens = server.WORLD, server.TOKENS
        server.SAVE.write_text(json.dumps({'world': old_world, 'tokens': old_tokens}))
        server.load()
        self.assertEqual(server.ACTIVE_WORLD, 'legacy')
        self.assertEqual(server.WORLD, old_world)
        self.assertEqual(server.TOKENS, old_tokens)
        server.save()
        server.load()
        self.assertEqual(server.TOKENS[identity['token']], identity['town_id'])

    def test_invalid_world_names_and_missing_worlds_are_rejected(self):
        for name in ['', '   ', 'x' * 25, None, []]:
            self.assertEqual(self.call('/api/worlds/create', {'name': name})[0], 400)
        for world_id in ['missing', None, []]:
            for path in ['/api/worlds/open', '/api/worlds/delete']:
                self.assertEqual(self.call(path, {'world_id': world_id})[0], 400)
        self.assertEqual(list(server.WORLDS), ['legacy'])


if __name__ == '__main__':
    unittest.main()
