"""R12 linked placement/upgrade vectors against Python; no live saves or production."""
import copy
import json
from pathlib import Path
from generate import base,b
from catalog import ROOT
from generate_upgrade_catalog import catalog
import sys
sys.path.insert(0,str(ROOT/'tests'))
from fixtures import flat_town
import server


def state(t):
    return dict(level=t['level'],coins=t['coins'],buildings=[dict(kind=s['kind'],tile=[s['x'],s['y']],
        level=s.get('level',1),capacity=s.get('settler_level',1),equipment=sorted(s.get('profession_upgrades',[])),
        parent=s.get('farm_index',s.get('orchard_index')),child=s.get('field',s.get('tree')) if s['kind'] in ('acre','orchard_tree') else None,
        remaining=s['construction']['total']-s['construction']['progress'] if 'construction' in s else None,
        stored=s.get('stored_goods',{}),pending=s.get('pending_output',{})) for s in t['buildings']])


def run(name,source,actions,helper=False,hall=False):
    t=flat_town(size=120)
    stored=source['buildings'][0]['stored_goods'];food=source.get('food',0)
    worker=dict(job='farming' if helper else 'building',construction_position=dict(x=29 if hall else 39,y=30))
    if helper:worker['building_index']=1
    worker.update(source.get('skills',{}))
    t.update(level=source['level'],sandbox=source['sandbox'],coins=source['coins'],
        town_origin=dict(x=30,y=30),player=dict(x=1,y=1,**source.get('skills',{})),
        buildings=copy.deepcopy(source['buildings']),map_resources=copy.deepcopy(source['resources']),npcs=[],roads=[],villagers=[worker],
        wood=stored.get('wood',0),food=food,goods={k:v for k,v in stored.items() if k!='wood'},produced=source.get('produced',{}),
        completed_trades=source.get('completed_trades',0),market_sales=source.get('market_sales',0),inventory_version=1,market_storage_version=1)
    server.ensure_resources(t)
    outcomes=[]
    for action in actions:
        try:
            if action['action']=='tick':server.construct(t,t,elapsed=action['seconds'])
            else:server.command(t,action)
            ok=True
        except ValueError:ok=False
        outcomes.append(dict(ok=ok,state=copy.deepcopy(state(t))))
        t=json.loads(json.dumps(t))
    return dict(name=name,source=source,actions=actions,outcomes=outcomes,helper=helper,hall=hall)


def initial(kind='home',**extra):
    s=base();s.update(coins=100,**extra);s['resources']=[]
    s['buildings'][0]['stored_goods'].update(wood=1000,stone=100,planks=200,tools=30,wool=20,steel=20,leather=20)
    s['buildings'].append(b(kind,40,30,footprint=list(server.moved_footprint(dict(kind=kind)))))
    return s


def main():
    cases=[];tick=dict(action='tick',seconds=300)
    s=initial();cases.append(run('structural and capacity upgrades and move continuity',s,[
        dict(action='upgrade_building',index=1),dict(action='upgrade_building',index=1),dict(action='tick',seconds=3),
        dict(action='move',index=1,x=42,y=30),tick,tick,
        dict(action='upgrade_capacity',index=1),tick,tick,dict(action='destroy',index=1)]))
    s=initial();s['buildings'][1]['level']=3
    cases.append(run('stone cottage final level and cap',s,[dict(action='upgrade_building',index=1),tick,dict(action='upgrade_building',index=1)]))
    s=initial('mill',skills=dict(milling=5));cases.append(run('equipment prerequisites and queued completion',s,[
        dict(action='profession_upgrade',index=1,upgrade='geared_millstones'),dict(action='profession_upgrade',index=1,upgrade='improved_millstones'),tick,
        dict(action='profession_upgrade',index=1,upgrade='geared_millstones'),tick,dict(action='profession_upgrade',index=1,upgrade='masterwork_millstones'),tick,
        dict(action='profession_upgrade',index=1,upgrade='improved_millstones')]))
    s=initial('fishing_hut',skills=dict(fishing=40));cases.append(run('boat instant purchase, currency and higher tiers',s,[
        dict(action='profession_upgrade',index=1,upgrade='nets'),tick,dict(action='profession_upgrade',index=1,upgrade='boat'),
        dict(action='profession_upgrade',index=1,upgrade='master_boat'),dict(action='profession_upgrade',index=1,upgrade='improved_boat'),tick,
        dict(action='profession_upgrade',index=1,upgrade='master_boat'),tick]))
    s=initial('farm');s['resources']=[b('berry_bush',41,31),b('rock',47,31)]
    cases.append(run('linked acres atomic move, child limits and group removal',s,[
        dict(action='build',kind='acre',x=41,y=31,farm_index=1),dict(action='build',kind='acre',x=43,y=33,farm_index=1),
        dict(action='move',index=1,x=46,y=30),dict(action='move',index=1,x=42,y=32),
        dict(action='move',index=2,x=46,y=32),dict(action='move',index=2,x=42,y=33),
        dict(action='upgrade_building',index=2),dict(action='destroy',index=1)]))
    s=initial('orchard');cases.append(run('orchard tree group and saved initial state',s,[
        dict(action='build',kind='orchard_tree',x=41,y=31,orchard_index=1),dict(action='move',index=1,x=45,y=30),
        dict(action='move',index=2,x=44,y=31),dict(action='destroy',index=1)]))
    s=initial('farm');cases.append(run('assigned residents help upgrades',s,[dict(action='upgrade_building',index=1),dict(action='tick',seconds=3),tick],helper=True))
    s=initial(sandbox=True,skills={});cases.append(run('sandbox costs and immediate upgrades',s,[dict(action='upgrade_building',index=1),dict(action='upgrade_capacity',index=1),dict(action='profession_upgrade',index=1,upgrade='timber_frame')]))
    for level,produced in ((1,{}),(2,dict(wheat=20,eggs=6)),(4,dict(tools=5,bread=10)),(9,dict(steel=15))):
        s=initial('farm');s.update(level=level,produced=produced,food=20,market_sales=10)
        s['buildings'].append(b('home',36,30));s['resources']=[b('water',41,30)]
        cases.append(run('hall milestone, rewards once: '+str(level),s,[dict(action='upgrade'),tick,tick],hall=True))
    s=initial('farm');s.update(level=2,produced={},food=100)
    cases.append(run('stock does not satisfy production milestones',s,[dict(action='upgrade')],hall=True))
    Path(__file__).with_name('r12_upgrade_cases.json').write_text(json.dumps(dict(catalog=catalog(),cases=cases),separators=(',',':'))+'\n')

if __name__=='__main__':main()
