"""Small, isolated towns for rule tests that do not exercise terrain generation."""
import copy
from functools import lru_cache
from unittest.mock import patch

import server
import work_cycles


@lru_cache(maxsize=1)
def _town_template():
    # Use real town initialization once, retaining migration and inventory
    # defaults. Terrain, NPC placement and map generation have separate tests.
    with patch('terrain.secrets.randbits', return_value=42):
        town = server.new_town('Fixture')
    town.update(terrain=[], map_resources=[], npcs=[], tree_stumps=[],
                forage_version=1, forage_halls=[[5, 5]])
    return town


def flat_town(name='Test town', size=16):
    town = copy.deepcopy(_town_template())
    # An explicit origin identifies a prepared map, preventing legacy-map
    # expansion from replacing the small test landscape during commands.
    town.update(name=name, map_size=size, terrain=['g' * size] * size,
                town_origin={'x': 5, 'y': 5})
    return town


def start_woodcutting(town, index=0):
    """Begin a productive cycle at the tree for isolated rate and buffer checks."""
    building, source = server.work_assignment(town, 'woodcutting', index)
    worker = town['villagers'][index]
    work_cycles.advance(town, town, worker, building, source, index, 0)
    animation = worker['work_animation']
    if animation['phase'] == 'waiting':
        raise AssertionError('The fixture must provide a reachable tree')
    animation.update(**(animation['path'][-1] if animation['path'] else {}),
                     path=[], phase='working', elapsed=0)
    return worker
