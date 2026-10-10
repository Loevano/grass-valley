"""Exercise the real HTTP host with separate client ownership tokens."""
import http.client
import json
from pathlib import Path
import tempfile
import threading
import unittest
from http.server import ThreadingHTTPServer

import server


class LanHost(unittest.TestCase):
    def test_character_movement_is_owned_rate_limited_and_saved(self):
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
        status, _ = self.call('/api/command', move, owner['token'])
        self.assertEqual(status, 429)
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
        server.activate(None)
        self.assertEqual(json.loads(self.call('/api/players')[1]), {'world_id': None, 'players': {}})

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
        build = {'action': 'build', 'kind': 'farm', 'x': 1, 'y': 2}
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

    def test_world_management_rejects_remote_addresses_and_origins(self):
        for headers in [{'Host': '192.168.1.2:8000'}, {'Origin': 'http://evil.example'}]:
            catalog = json.loads(self.call('/api/worlds', headers=headers)[1])
            self.assertFalse(catalog['can_manage'])
            for path, data in [('/api/worlds/create', {'name': 'No'}),
                               ('/api/worlds/open', {'world_id': 'legacy'}),
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
