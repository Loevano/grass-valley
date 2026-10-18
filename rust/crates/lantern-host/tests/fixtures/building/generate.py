"""Independent R12 command vectors against the current Python host, no live saves."""
import copy
import json
from pathlib import Path
import sys
from catalog import ROOT, catalog
sys.path.insert(0, str(ROOT / 'tests'))
from fixtures import flat_town
import server
import progression


def b(kind, x, y, **extra):
    return dict(kind=kind, x=x, y=y, **extra)


def base(**extra):
    return dict(level=10, sandbox=False, coins=2.5, origin=[30, 30], player=[1, 1],
                buildings=[b('hall', 30, 30, footprint=[3, 3], stored_goods={
                    'wood': 100, 'planks': 30, 'tools': 5, 'iron': 20, 'steel': 15,
                    'furniture': 2})], resources=[], roads=[], unavailable=[], locals=[], **extra)


def state(town):
    return dict(coins=town['coins'], buildings=[dict(
        kind=b['kind'], tile=[b['x'], b['y']], dimensions=b.get('footprint', [1, 1]),
        orientation=b.get('orientation', 0), fence_orientation=b.get('fence_orientation', 0),
        remaining=(b['construction']['total'] - b['construction'].get('progress', 0)
                   if 'construction' in b else None),
        stored=b.get('stored_goods', {}), pending=b.get('pending_output', {}))
        for b in town['buildings']],
        roads=[dict(tile=[r['x'], r['y']], remaining=(r['construction']['total'] - r['construction'].get('progress', 0)
                  if 'construction' in r else None)) for r in town.get('roads', [])],
        resources=town['map_resources'])


def run(name, source, commands):
    town = flat_town(size=120)
    town.update(level=source['level'], sandbox=source['sandbox'], coins=source['coins'],
                buildings=copy.deepcopy(source['buildings']), villagers=[],
                town_origin=dict(zip(('x','y'), source['origin'])),
                player=dict(zip(('x','y'), source['player'])),
                map_resources=copy.deepcopy(source['resources']), npcs=source['locals'],
                roads=copy.deepcopy(source['roads']), inventory_version=1, market_storage_version=1)
    for x, y in source['unavailable']:
        row = town['terrain'][y]
        town['terrain'][y] = row[:x] + 'v' + row[x+1:]
    totals = {}
    for building in town['buildings']:
        for key, value in building.get('stored_goods', {}).items():
            totals[key] = totals.get(key, 0) + value
    town.update(wood=totals.pop('wood', 0), food=0, goods=totals)
    # Initialize existing migration/version fields before the measured commands.
    server.ensure_resources(town)
    outcomes=[]
    for command in commands:
        try:
            server.command(town, command)
            ok=True
        except ValueError:
            ok=False
        outcomes.append(dict(ok=ok, state=copy.deepcopy(state(town))))
    return dict(name=name, source=source, commands=commands, outcomes=outcomes)


def main():
    cases=[]
    source=base()
    source['resources']=[b('berry_bush',40,30), b('herb_patch',41,30), b('rock',45,30)]
    cases.append(run('ordinary costs, forage, unfinished move, idempotent roads', source, [
        dict(action='build',kind='sawpit',x=40,y=30,orientation=1),
        dict(action='set_building_orientation',index=1,orientation=2),
        dict(action='move',index=1,x=45,y=30),
        dict(action='move',index=1,x=50,y=30),
        dict(action='build_road',x=40,y=30),dict(action='build_road',x=40,y=30),
        dict(action='build',kind='home',x=40,y=30),
        dict(action='remove_road',x=40,y=30),dict(action='remove_road',x=40,y=30),
        dict(action='destroy',index=1),dict(action='destroy',index=0)]))
    source=base(); source.update(sandbox=True, coins=0)
    source['resources']=[b('berry_bush',44,30),b('tree',44,31)]
    source['buildings'].append(b('sawpit',40,30,footprint=[3,2],stored_goods={'wood':2.125},pending_output={'planks':0.375}))
    cases.append(run('rotation, salvage, sandbox',source,[
        dict(action='set_building_orientation',index=1,orientation=1),
        dict(action='set_building_orientation',index=1,orientation=2),
        dict(action='move',index=1,x=44,y=30),
        dict(action='build',kind='wooden_bench',x=48,y=30,orientation=3),
        dict(action='set_decoration_orientation',index=2,orientation=2),
        dict(action='destroy',index=1),dict(action='destroy',index=1)]))
    source=base(); source.update(level=1, coins=0.25)
    cases.append(run('full-footprint bounds, range, unlock and fee clamp',source,[
        dict(action='build',kind='mill',x=40,y=30),
        dict(action='build',kind='home',x=61,y=30),
        dict(action='build',kind='home',x=60,y=30),
        dict(action='move',index=1,x=59,y=30),dict(action='move',index=1,x=58,y=30),
        dict(action='build_road',x=30,y=61),dict(action='build_road',x=30,y=60)]))
    source=base(); source['buildings'].append(b('barn',40,30))
    cases.append(run('barn protection and gate orientation',source,[
        dict(action='build',kind='home',x=43,y=33),dict(action='build',kind='sawpit',x=44,y=30),
        dict(action='set_fence_orientation',index=1,orientation=2),
        dict(action='build_road',x=42,y=30),dict(action='move',index=1,x=44,y=30)]))
    source=base(); source['buildings'][0]['stored_goods']={ 'wood': 24.9999999995 }
    cases.append(run('fractional affordability tolerance',source,[dict(action='build',kind='sawpit',x=40,y=30)]))
    source=base(); source['buildings'][0]['stored_goods']={'wood':1}
    source['resources']=[b('berry_bush',40,30)]
    cases.append(run('unaffordable failure preserves forage',source,[dict(action='build',kind='home',x=40,y=30)]))
    for obstacle in ('building', 'road', 'resource', 'local', 'player', 'void', 'range', 'edge', 'barn'):
        source=base()
        source['buildings'].append(b('sawpit',40,30,footprint=[3,2]))
        x,y=41,32
        if obstacle == 'building': source['buildings'].append(b('home',x,y))
        elif obstacle == 'road': source['roads']=[dict(x=x,y=y)]
        elif obstacle == 'resource': source['resources']=[b('rock',x,y)]
        elif obstacle == 'local': source['locals']=[b('resident',x,y)]
        elif obstacle == 'player': source['player']=[x+0.25,y-0.25]
        elif obstacle == 'void': source['unavailable']=[[x,y]]
        elif obstacle == 'barn': source['buildings'].append(b('barn',44,35))
        elif obstacle == 'range': source['level']=1; source['buildings'][1].update(x=58,y=30)
        elif obstacle == 'edge': source['buildings'][0].update(x=40,y=110); source['buildings'][1].update(x=40,y=118)
        # Quarter-turn adds y+2; source is already placed, so only new geometry
        # validates. A half-turn keeps occupancy and succeeds even in crowding.
        cases.append(run('rotation obstacle: '+obstacle,source,[
            dict(action='set_building_orientation',index=1,orientation=1),
            dict(action='set_building_orientation',index=1,orientation=2)]))
    source=base();source['buildings'].append(b('sawpit',40,30,footprint=[1,1]))
    cases.append(run('compact saved footprint rotates then expands on relocation',source,[
        dict(action='set_building_orientation',index=1,orientation=1),
        dict(action='move',index=1,x=50,y=30)]))
    output={'catalog':catalog(),'cases':cases}
    Path(__file__).with_name('python.json').write_text(json.dumps(output,indent=2,sort_keys=True)+'\n')

if __name__ == '__main__': main()
