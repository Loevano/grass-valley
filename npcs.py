"""Friendly locals placed on open ground in their home biome."""
import random


THEMES = {
    'g': [('Tobin', 'Valley traveler', 'The hall lantern is a welcome sight after a long walk.')],
    'f': [('Sylva', 'Forest ranger', 'Leave the young trees standing and the forest will always provide.')],
    'm': [('Clover', 'Meadow beekeeper', 'Follow the flowers. The bees know every quiet corner of this meadow.')],
    's': [('Saffron', 'Dune wanderer', 'Keep water close and watch how the wind shapes the sand.')],
    'r': [('Flint', 'Rock surveyor', 'Every outcrop tells a story. I am still learning to read this one.')],
    'b': [('Moss', 'Swamp herbalist', 'Willow roots love the wet ground. Look for herbs among the reeds.'),
          ('Reed', 'Marsh fisher', 'The pools are quiet, but there is plenty of life beneath the lily pads.')],
}


def ensure_npcs(town):
    if 'npcs' in town:
        return
    rng = random.Random(str(town.get('terrain_seed', 0)) + ':npcs')
    occupied = {(obj['x'], obj['y']) for obj in town['buildings'] + town['map_resources']}
    player = town.get('player')
    if player:
        occupied.add((round(player['x']), round(player['y'])))
    candidates = {biome: [] for biome in THEMES}
    for y, row in enumerate(town.get('terrain', [])):
        for x, biome in enumerate(row):
            if biome in candidates and (x, y) not in occupied and (x >= 12 or y >= 12):
                candidates[biome].append((x, y))
    town['npcs'] = []
    for biome, people in THEMES.items():
        tiles = candidates[biome]
        for name, role, dialogue in people:
            if not tiles:
                break
            x, y = tiles.pop(rng.randrange(len(tiles)))
            town['npcs'].append({'name': name, 'role': role, 'dialogue': dialogue,
                                 'biome': biome, 'x': x, 'y': y})
