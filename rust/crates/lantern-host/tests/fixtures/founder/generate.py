"""Frozen founder traces from actual Python authority; --check never refreshes."""
import copy
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[6] / 'server'))
from server import move_player, walkable


def generate():
    cases = []
    def case(name, *, position=(5., 7.), buildings=(), resources=(), roads=(), terrain=None, gear=None, inputs=((1, 0),), probes=(), browser=True):
        town = dict(map_size=24, buildings=list(buildings), map_resources=list(resources), roads=list(roads),
                    terrain=terrain or ['g'*24 for _ in range(24)],
                    npcs=[dict(kind='resident', x=5, y=7)], villagers=[dict(x=5, y=7)],
                    player=dict(x=position[0], y=position[1], facing=-1, activity='fish', activity_status='Fishing',
                                inventory={'unknown': 12.125}, extension={'keep': 'private'}, **(gear or {})))
        assert walkable(town, *position), name
        original = copy.deepcopy(town)
        trace = []
        for dx, dy in inputs:
            trace.append(copy.deepcopy(move_player(town, dict(dx=dx, dy=dy))['player']))
        expected_probes = [walkable(original, x, y) for x, y in probes]
        # Geometry/snapshots and nonmovement founder fields must not change.
        for key in original:
            if key != 'player':
                assert town[key] == original[key], (name, key)
        cases.append(dict(name=name, source=original, inputs=inputs, expected=trace,
                          probes=probes, open=expected_probes, browser=browser))
    dirs = [(x,y) for x in (-1,0,1) for y in (-1,0,1)]
    for d in dirs:
        for mounted in (False, True):
            case(f'direction-{d}-{mounted}', gear={'mounted':mounted}, inputs=[d]*5)
    for kind in ('hall','rock','water','tree','berry_bush'):
        for boat in (False, True):
            case(f'collision-{kind}-{boat}', position=(3.3, 4.), resources=[dict(kind=kind,x=4,y=4)],
                 gear={'boat':boat}, inputs=[(1,0),(1,1),(0,-1),(1,-1),(1,0)], probes=[(3.32,4),(3.321,4),(4,4)])
    case('footprint-slide',position=(3.3,5.1), buildings=[dict(kind='sawpit',x=4,y=4,footprint=[2,3])],inputs=[(1,-1)]*5)
    case('corner', position=(3.3,3.3), buildings=[dict(kind='hall',x=4,y=4)],inputs=[(1,1)]*5)
    for orientation in range(4):
        barn = [dict(kind='barn',x=10,y=10,fence_orientation=orientation)]
        for position in ((6.3,10.),(13.7,11.),(11.,13.7),(8.,6.3)):
            case(f'fence-{orientation}-{position}',position=position, buildings=barn,inputs=dirs[:5],
                 probes=[(6.5,10),(13.5,11),(11,13.5),(8,6.5)])
    for level in (-2,0,1,2,3,4.5):
        for mounted in (False,True):
            case(f'water-{level}-{mounted}',resources=[dict(kind='water',x=5,y=7,depth=99)],
                 roads=[dict(x=5,y=7)], gear={'boat':True,'boat_level':level,'mounted':mounted},
                 inputs=[(1,1)]*5,browser=level>=1)
    for mounted in (False,True):
        for construction in (None,{}, {'upgrade':{}}):
            road=dict(x=5,y=7)
            if construction is not None: road['construction']=construction
            case(f'road-{mounted}-{construction}',position=(5.4,7.),roads=[road,dict(x=6,y=7)],gear={'mounted':mounted},inputs=[(1,0)]*5)
    for biome in ('b','s','d','i','v'):
        terrain=['g'*24 for _ in range(24)]
        terrain[7]=terrain[7][:4]+biome+terrain[7][5:]
        for boots in (False,True):
            case(f'clothing-{biome}-{boots}',position=(3.3,7.),terrain=terrain,
                 gear={'equipment':{'feet':'swamp_boots' if boots else 'unknown','body':'desert_clothing'}},
                 inputs=[(1,0)]*5,probes=[(3.32,7),(3.321,7),(4,7)])
    # Cover each shade and water-anchor clothing exemption independently.
    for x in range(4,11):
        terrain=['g'*24 for _ in range(24)]
        terrain[7]=terrain[7][:x]+'b'+terrain[7][x+1:]
        for boots in (False,True):
            case(f'swamp-shade-{x}-{boots}',position=(x-.7,7.),terrain=terrain,
                 gear={'equipment':{'feet':'swamp_boots' if boots else 'unknown'}},
                 inputs=[(1,0)]*5,probes=[(x,7)])
    case('water-over-dark-swamp',position=(4.,7.),terrain=['g'*24]*7+['ggggb'+'g'*19]+['g'*24]*16,
         resources=[dict(kind='water',x=4,y=7,depth=100)],gear={'boat':True},inputs=[(1,0)]*5)
    for position in ((-.32,4.),(23.32,4.),(4.,-.32),(4.,23.32)):
        case(f'boundary-{position}',position=position,inputs=dirs[:5])
    case('overlap-water-building',position=(3.3,4.),buildings=[dict(kind='hall',x=4,y=4)],resources=[dict(kind='water',x=4,y=4)],gear={'boat':True},inputs=[(1,0)]*5)
    case('water-footprint-anchor-speed',position=(6.,7.),resources=[dict(kind='water',x=5,y=7,footprint=[3,2])],gear={'boat':True,'boat_level':3,'mounted':True},inputs=[(1,0)]*5)
    return dict(version=1, tolerance=1e-12, cases=cases)


def main():
    target=Path(__file__).with_name('python.json')
    actual=json.dumps(generate(),separators=(',',':'))+'\n'
    if '--check' in sys.argv:
        if actual != target.read_text():
            raise SystemExit('Founder Python traces differ; frozen fixture was not changed.')
        print('Founder frozen traces independently recomputed and match.')
    else:
        target.write_text(actual)
        print('Wrote founder traces.')

if __name__ == '__main__': main()
