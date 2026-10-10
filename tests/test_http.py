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
    def setUp(self):
        self.previous = server.WORLD, server.TOKENS, server.SAVE
        self.directory = tempfile.TemporaryDirectory()
        server.WORLD, server.TOKENS = {'towns': {}}, {}
        server.SAVE = Path(self.directory.name) / 'savegame.json'
        self.host = ThreadingHTTPServer(('127.0.0.1', 0), server.Handler)
        self.thread = threading.Thread(target=self.host.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.host.shutdown()
        self.host.server_close()
        self.thread.join()
        server.WORLD, server.TOKENS, server.SAVE = self.previous
        self.directory.cleanup()

    def call(self, path, data=None, token=''):
        connection = http.client.HTTPConnection('127.0.0.1', self.host.server_port, timeout=3)
        connection.request('POST' if data is not None else 'GET', path,
                           json.dumps(data) if data is not None else None,
                           {'Content-Type': 'application/json', 'Authorization': token})
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
        towns = json.loads(body)['towns']
        self.assertEqual(len(towns[identities[0]['town_id']]['buildings']), 2)
        self.assertEqual(len(towns[identities[1]['town_id']]['buildings']), 1)
        for identity in identities:
            self.assertNotIn(identity['token'].encode(), body)
        saved = json.loads(server.SAVE.read_text())
        self.assertEqual(saved['world'], json.loads(body))
        self.assertEqual(saved['tokens'][identities[0]['token']], identities[0]['town_id'])
        self.assertEqual(self.call('/')[0], 200)
        self.assertEqual(self.call('/app.js')[0], 200)


if __name__ == '__main__':
    unittest.main()
