"""Authoritative, free rotation for player-crafted decorations."""
import copy
import json
import unittest

import progression
import server
from fixtures import flat_town
from shared_world import export_world


class DecorationRotation(unittest.TestCase):
    def setUp(self):
        self.town = flat_town()

    def rotate(self, index, orientation):
        return server.command(self.town, {'action': 'set_decoration_orientation',
                                          'index': index, 'orientation': orientation})

    def test_all_decorations_turn_freely_without_changing_their_footprint_or_stock(self):
        for kind in progression.DECORATIONS:
            decoration = {'kind': kind, 'x': 8, 'y': 8}
            self.town['buildings'] = [decoration]
            original = copy.deepcopy(decoration)
            coins = self.town['coins']
            for orientation in range(4):
                with self.subTest(kind=kind, orientation=orientation):
                    self.assertEqual(self.rotate(0, orientation), {})
                    self.assertEqual(decoration, dict(original, orientation=orientation))
                    self.assertEqual(self.town['coins'], coins)

    def test_orientation_survives_export_reload_and_moving(self):
        self.town['buildings'].append({'kind': 'wooden_bench', 'x': 8, 'y': 8})
        self.rotate(1, 3)
        stored = json.loads(json.dumps(export_world({'towns': {'own': self.town}})))
        self.town = stored['towns']['own']
        self.town['sandbox'] = True
        server.command(self.town, {'action': 'move', 'index': 1, 'x': 10, 'y': 10})
        self.assertEqual(self.town['buildings'][1],
                         {'kind': 'wooden_bench', 'x': 10, 'y': 10, 'orientation': 3})

    def test_invalid_indices_directions_and_non_decorations_are_rejected(self):
        self.town['buildings'].append({'kind': 'lantern', 'x': 8, 'y': 8})
        original = copy.deepcopy(self.town['buildings'])
        for index in [-1, 2, True, '1', None]:
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.rotate(index, 0)
        for orientation in [-1, 4, True, 1.0, '1', None]:
            with self.subTest(orientation=orientation), self.assertRaises(ValueError):
                self.rotate(1, orientation)
        with self.assertRaises(ValueError):
            self.rotate(0, 1)
        self.assertEqual(self.town['buildings'], original)

    def test_unfinished_decorations_cannot_turn(self):
        self.town['buildings'].append({'kind': 'wooden_statue', 'x': 8, 'y': 8,
                                       'construction': {'progress': 0, 'total': 30}})
        with self.assertRaisesRegex(ValueError, 'completed decoration'):
            self.rotate(1, 2)
        self.assertNotIn('orientation', self.town['buildings'][1])


if __name__ == '__main__':
    unittest.main()
