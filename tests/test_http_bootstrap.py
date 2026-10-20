"""Shared, disposable no-world HTTP contracts for Python and experimental Rust.

This deliberately does not certify active-world/account/command coverage.
"""
import html
import http.client
import json
import mimetypes
from pathlib import Path
import select
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import unittest
import unicodedata
import re
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch
from urllib.parse import quote

import server

ROOT = Path(__file__).resolve().parents[1]
# Literal cross-language dependencies are consumed by tools/test_selection.py.
HTTP_SOURCES = ('rust/crates/lantern-host/src/http.rs',
                'rust/crates/lantern-host/src/http_bootstrap.rs',
                'rust/crates/lantern-host/src/http/static_assets.rs',
                'rust/crates/lantern-host/src/main.rs')


def assets(directory):
    root = Path(directory)
    shutil.copytree(ROOT / 'web', root / 'web')
    (root / 'icons').mkdir()
    shutil.copyfile(ROOT / 'icons/favicon.ico', root / 'icons/favicon.ico')
    # Far larger than the TCP send buffer: a client that never consumes this
    # body leaves an actual transfer pending rather than merely delaying reads.
    with (root / 'web/slow.bin').open('wb') as stream:
        stream.truncate(16 * 1024 * 1024)
    for name in ('probe.svg', 'probe.json', 'probe.wasm', 'probe.woff', 'probe.mjs', 'probe.UNKNOWN_EXTENSION'):
        (root / 'web' / name).write_bytes(b'Synthetic MIME probe')
    listing = root / 'web' / 'listing'
    listing.mkdir()
    for name in ('z.txt', 'A & <tag>.txt', 'Café 🌿.txt'):
        (listing / name).write_text(name)
    (listing / 'folder').mkdir()
    (listing / 'link').symlink_to('z.txt')
    (listing / 'directory-link').symlink_to('folder', target_is_directory=True)
    index = root / 'web' / 'indexed'
    index.mkdir()
    (index / 'index.htm').write_bytes(b'<p>index.htm fallback</p>')
    return root


class BootstrapProtocol:
    def call(self, path, method='GET', headers=None, body=None):
        connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=3)
        connection.request(method, path, body=body, headers=headers or {})
        response = connection.getresponse()
        result = response.status, {key.lower(): value for key, value in response.getheaders()}, response.read()
        connection.close()
        return result

    def test_snapshot_headers_catalogs_and_clock(self):
        status, headers, body = self.call('/api/world?world_id=stale&chunks=0,0')
        self.assertEqual(status, 200)
        self.assertEqual(headers['content-type'].lower(), 'application/json')
        self.assertEqual(headers['cache-control'], 'no-store')
        self.assertEqual(int(headers['content-length']), len(body))
        snapshot = json.loads(body)
        self.assertEqual(snapshot['towns'], {})
        self.assertIsNone(snapshot['world_id'])
        self.assertIsNone(snapshot['name'])
        self.assertEqual(snapshot['hall_ready'], {})
        self.assertEqual(snapshot['hall_upgrades'], server.HALL_UPGRADES)
        self.assertEqual(snapshot['progression'], json.loads(json.dumps(server.economy.public_catalog())))
        self.assertEqual(snapshot['local_stories'], server.npcs.STORIES)
        self.assertEqual(snapshot['quarry_catalog'], json.loads(json.dumps(server.quests.CATALOG)))
        self.assertEqual(snapshot['simulation_clock'], {'tick_seconds': 3, 'next_tick_in': 3})
        before = snapshot['players_sampled_at']
        status, _, body = self.call('/api/players')
        self.assertEqual(status, 200)
        players = json.loads(body)
        self.assertEqual(players['players'], {})
        self.assertGreaterEqual(players['sampled_at'], before)
        self.assertIsNone(players['world_id'])
        self.assertEqual(json.loads(self.call('/api/lanterns?ignored=yes')[2]),
                         {'world_id': None, 'quest': None})
        for private in ('tokens', 'players', 'salt', 'pin_hash', 'credentials'):
            self.assertNotIn(private, snapshot)

    def test_chunk_query_errors_and_first_nonempty_duplicate(self):
        cases = [
            ('chunks=x', 'Invalid map chunk.'),
            ('chunks=48,x', 'Invalid map chunk.'),
            ('chunks=48,0', 'Map chunk outside the world.'),
            ('chunks=0,0;', 'Invalid map chunk.'),
            ('chunks=' + ';'.join(['0,0'] * 50), 'Too many map chunks.'),
            ('known=' + ';'.join(['0,0'] * 129), 'Too many cached map chunks.'),
            ('resources=0,0:ABCDEF0123456789', 'Invalid resource chunk version.'),
            ('resources=:0123456789abcdef', 'Invalid resource chunk key.'),
            ('resources=0,0', 'Invalid resource chunk version.'),
            ('resources=' + ';'.join(['0,0:0123456789abcdef'] * 129),
             'Too many cached resource chunks.'),
        ]
        for query, message in cases:
            with self.subTest(query=query[:70]):
                status, headers, body = self.call('/api/world?' + query)
                self.assertEqual(status, 400)
                self.assertEqual(json.loads(body), {'error': message})
                self.assertEqual(headers['cache-control'], 'no-store')
        for query in ('chunks=&chunks=01,02&chunks=bad',
                      'known=' + ';'.join(['0,0'] * 128),
                      'resources=47,47:0123456789abcdef',
                      'chunks=' + quote('０１,٢')):
            with self.subTest(query=query[:70]):
                self.assertEqual(self.call('/api/world?' + query)[0], 200)

    def test_raw_paths_head_and_assets(self):
        for path in ('/api/players?x=1', '/api/worlds?x=1', '/missing-static-asset'):
            status, headers, body = self.call(path)
            self.assertEqual(status, 404)
            expected = BaseHTTPRequestHandler.error_message_format % {
                'code': HTTPStatus.NOT_FOUND, 'message': 'File not found',
                'explain': 'Nothing matches the given URI'}
            self.assertEqual(body, expected.encode())
            self.assertEqual(headers['content-type'], 'text/html;charset=utf-8')
        for path in ('/', '/app.js', '/style.css', '/favicon.ico'):
            with self.subTest(path=path):
                status, headers, body = self.call(path)
                self.assertEqual(status, 200)
                self.assertEqual(len(body), int(headers['content-length']))
                source = ROOT / ('icons/favicon.ico' if path == '/favicon.ico'
                                 else 'web/index.html' if path == '/' else 'web' + path)
                self.assertEqual(body, source.read_bytes())
                self.assertIn('last-modified', headers)
                self.assertEqual(self.call(path, headers={'Range': 'bytes=0-1'})[2], body)
                status, head, empty = self.call(path, 'HEAD')
                self.assertEqual(status, 200)
                self.assertEqual(empty, b'')
                self.assertEqual(head['content-type'], headers['content-type'])
                self.assertEqual(head['content-length'], headers['content-length'])
                status, cached, empty = self.call(path, headers={'If-Modified-Since': headers['last-modified']})
                self.assertEqual((status, empty), (304, b''))
                for name in ('content-type', 'content-length', 'last-modified'):
                    self.assertNotIn(name, cached)
        self.assertEqual(self.call('/api/world', 'HEAD')[0], 404)
        self.assertEqual(self.call('//api/world')[0], 200)
        self.assertEqual(self.call('//listing?x=1')[1]['location'], '/listing/?x=1')
        self.assertEqual(self.call('/%2e%2e/AGENTS.md')[0], 404)

    def test_directory_indexes_listings_normalized_paths_and_cache_precedence(self):
        for method in ('GET', 'HEAD'):
            status, headers, body = self.call('/listing?keep=1', method)
            self.assertEqual((status, headers['location'], headers['content-length'], body),
                             (301, '/listing/?keep=1', '0', b''))
        status, headers, body = self.call('/indexed/')
        self.assertEqual((status, body), (200, b'<p>index.htm fallback</p>'))
        self.assertEqual(headers['content-type'], 'text/html')
        for path in ('/listing/../style.css', '/%2e%2e/style.css', '/listing/%2e%2e/style.css'):
            self.assertEqual(self.call(path)[2], (ROOT / 'web/style.css').read_bytes())
        self.assertEqual(self.call('/style.css/')[0], 404)
        self.assertEqual(self.call('/style.css%2f')[0], 200)
        path = '/listing/?q=%3Ctest%3E&keep=yes'
        status, headers, body = self.call(path)
        self.assertEqual(status, 200)
        self.assertEqual(headers['content-type'], 'text/html; charset=utf-8')
        title = 'Directory listing for /listing/?q=&lt;test&gt;&amp;keep=yes'
        lines = ['<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">',
                 '<html>\n<head>', '<meta http-equiv="Content-Type" content="text/html; charset=utf-8">',
                 '<title>' + title + '</title>\n</head>', '<body>\n<h1>' + title + '</h1>', '<hr>\n<ul>']
        entries = [('A & <tag>.txt', 'A & <tag>.txt'), ('Café 🌿.txt', 'Café 🌿.txt'),
                   ('directory-link/', 'directory-link@'), ('folder/', 'folder/'), ('link', 'link@'), ('z.txt', 'z.txt')]
        lines.extend('<li><a href="' + quote(link) + '">' + html.escape(display, quote=False) + '</a></li>'
                     for link, display in entries)
        lines.append('</ul>\n<hr>\n</body>\n</html>\n')
        self.assertEqual(body, '\n'.join(lines).encode())
        self.assertEqual(int(headers['content-length']), len(body))
        status, head, empty = self.call(path, 'HEAD')
        self.assertEqual((status, empty, head['content-length']), (200, b'', headers['content-length']))
        modified = self.call('/style.css')[1]['last-modified']
        for conditions in ({'If-Modified-Since': modified, 'If-None-Match': 'anything'},
                           {'If-Match': 'anything'}, {'If-Unmodified-Since': 'Thu, 01 Jan 1970 00:00:00 GMT'}):
            self.assertEqual(self.call('/style.css', headers=conditions)[0], 200)

    def test_static_mime_types_match_the_existing_host(self):
        for name in ('app.js', 'style.css', 'probe.svg', 'probe.json', 'probe.wasm',
                     'probe.woff', 'probe.mjs', 'probe.UNKNOWN_EXTENSION', 'favicon.ico'):
            with self.subTest(name=name):
                status, headers, _ = self.call('/' + name)
                self.assertEqual(status, 200)
                self.assertEqual(headers['content-type'], mimetypes.guess_type(name)[0] or 'application/octet-stream')

    def test_unsupported_method_matches_static_server_error(self):
        status, headers, body = self.call('/api/world', 'PATCH')
        self.assertEqual(status, 501)
        self.assertEqual(headers['content-type'], 'text/html;charset=utf-8')
        self.assertEqual(body, (BaseHTTPRequestHandler.error_message_format % {
            'code': HTTPStatus.NOT_IMPLEMENTED, 'message': "Unsupported method ('PATCH')",
            'explain': 'Server does not support this operation'}).encode())

    def test_host_metadata_checks_origin_and_ignores_forwarding(self):
        for headers, expected in [
            ({'Host': 'localhost:1234'}, True),
            ({'Host': 'LOCALHOST:1234'}, True),
            ({'Host': '[::1]:1234'}, True),
            ({'Host': '127.0.0.1:1234', 'Origin': 'http://127.0.0.1:1234'}, True),
            ({'Host': '127.0.0.1:1234', 'Origin': 'http://other.example'}, False),
            ({'Host': 'other.example', 'X-Forwarded-Host': 'localhost'}, False),
        ]:
            with self.subTest(headers=headers):
                status, _, body = self.call('/api/worlds', headers=headers)
                self.assertEqual(status, 200)
                self.assertEqual(json.loads(body), {'active_world': None, 'can_manage': expected, 'worlds': []})

    def test_nonreading_socket_does_not_block_new_snapshots(self):
        # Hold a real socket with a tiny receive window without consuming its
        # response. Repeat requests over independent connections while it stalls.
        slow = socket.socket()
        slow.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
        slow.settimeout(3)
        self.addCleanup(slow.close)
        slow.connect(('127.0.0.1', self.port))
        slow.sendall(b'GET /slow.bin HTTP/1.1\r\nHost: localhost\r\n\r\n')
        self.assertTrue(slow.recv(1))
        start = time.monotonic()
        for path in ('/api/players', '/api/worlds', '/api/lanterns', '/api/world'):
            self.assertEqual(self.call(path)[0], 200)
        self.assertLess(time.monotonic() - start, 3)


class CatalogDrift(unittest.TestCase):
    def test_catalogs_match_python_and_unicode_query_digits(self):
        catalogs = json.loads((ROOT / 'rust/crates/lantern-host/src/http_catalogs.json').read_text())
        self.assertEqual(catalogs, json.loads(json.dumps({
            'hall_upgrades': server.HALL_UPGRADES, 'local_stories': server.npcs.STORIES,
            'quarry_catalog': server.quests.CATALOG})))
        # Keep Unicode compatibility explicit rather than silently widening the
        # parser when either runtime changes its Unicode version.
        source = (ROOT / HTTP_SOURCES[0]).read_text()
        zeros = source.split('const ZEROS: &[u32] = &[', 1)[1].split('];', 1)[0]
        actual = [int(value, 16) for value in re.findall(r'0x[0-9a-f]+', zeros)]
        expected = [c for c in range(0x110000) if unicodedata.category(chr(c)) == 'Nd'
                    and unicodedata.decimal(chr(c)) == 0]
        self.assertEqual(actual, expected)


class PythonBootstrap(BootstrapProtocol, unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.directory.cleanup)
        cls.asset_root = assets(cls.directory.name)
        cls.patchers = [patch.object(server, name, value) for name, value in {
            'ROOT': cls.asset_root, 'WORLD': {'towns': {}}, 'TOKENS': {}, 'WORLDS': {}, 'ACTIVE_WORLD': None,
            'NEXT_TICK_AT': None, 'SAVE': Path(cls.directory.name) / 'savegame.json',
        }.items()]
        for patcher in cls.patchers:
            patcher.start()
            cls.addClassCleanup(patcher.stop)
        cls.host = ThreadingHTTPServer(('127.0.0.1', 0), server.Handler)
        cls.addClassCleanup(cls.host.server_close)
        cls.port = cls.host.server_port
        cls.thread = threading.Thread(target=cls.host.serve_forever,
                                      kwargs={'poll_interval': .01}, daemon=True)
        cls.thread.start()
        cls.addClassCleanup(cls.stop)

    @classmethod
    def stop(cls):
        cls.host.shutdown()
        cls.thread.join()


class RustBootstrap(BootstrapProtocol, unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(['cargo', 'build', '-p', 'lantern-host', '--locked', '--offline'],
                       cwd=ROOT / 'rust', check=True, capture_output=True, timeout=180)
        cls.directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.directory.cleanup)
        cls.asset_root = assets(cls.directory.name)
        cls.binary = ROOT / 'rust/target/debug/lantern-host'
        cls.process = subprocess.Popen([str(cls.binary), 'http-bootstrap', '--experimental-empty',
                                        '--bind', '127.0.0.1:0', '--asset-root', str(cls.asset_root)], cwd=cls.directory.name,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        cls.addClassCleanup(cls.stop)
        if not select.select([cls.process.stdout], [], [], 10)[0]:
            raise RuntimeError('Rust HTTP bootstrap did not report its socket')
        line = cls.process.stdout.readline()
        if 'listening on ' not in line:
            raise RuntimeError('Rust HTTP bootstrap failed: ' + line)
        cls.port = int(line.rsplit(':', 1)[1])

    @classmethod
    def stop(cls):
        cls.process.terminate()
        cls.process.communicate(timeout=5)

    def test_save_and_coverage_admission_is_refused(self):
        source = Path(self.directory.name) / 'savegame.json'
        source.write_text('{"worlds":{},"active_world":null}')
        result = subprocess.run([str(self.binary), 'http-bootstrap', '--source', str(source)],
                                capture_output=True, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'unsupported', result.stderr)
        for path in ('/api/command', '/api/login', '/api/join', '/api/worlds/delete'):
            status, _, body = self.call(path, 'POST',
                {'Authorization': 'invented', 'X-Forwarded-Host': 'localhost'},
                json.dumps({'town_id': 'victim', 'world_id': 'victim'}))
            self.assertEqual(status, 501)
            self.assertIn('coverage', json.loads(body)['error'])
        self.assertEqual(json.loads(self.call('/api/worlds')[2])['worlds'], [])
        self.assertEqual(json.loads(source.read_text()), {'worlds': {}, 'active_world': None})


if __name__ == '__main__':
    unittest.main()
