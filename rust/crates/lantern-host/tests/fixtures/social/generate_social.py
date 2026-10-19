"""Freeze Python social catalogs and independent command scenarios."""
import argparse
import copy
import json
from pathlib import Path
import sys
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[6]
sys.path[:0]=[str(ROOT/'server'),str(ROOT/'tests')]
import npcs as n
import equipment as e
import progression as p
import trading as t
from fixtures import flat_town

def catalog():
    return dict(themes=list(n.THEMES.items()),new_locals=sorted(n.NEW_LOCALS),tools=n.WORKER_TOOLS,gifts=n.GIFTS,shops=n.TOOL_SHOPS,barter=n.BARTERERS,stories=n.STORIES,equipment=e.ITEMS)

def town():
    v=flat_town('social',size=32)
    v.update(level=5,wood=100,food=50,coins=100,goods={'herbs':5,'crafted_axe':2},inventory_version=1,
             buildings=[dict(kind='hall',x=1,y=1,stored_goods={'wood':100,'herbs':5,'crafted_axe':2,'food':50}),dict(kind='market',x=2,y=1)],
             villagers=[dict(name='Ada',job='idle'),dict(name='Rowan',job='idle')],roads=[],npcs=[],local_visits={})
    v['player']=dict(x=5.,y=5.)
    return v

def snapshot(v):
    return dict(coins=v['coins'],food=v['food'],goods={k:p.quantity(v,k) for k in ('wood','herbs','crafted_axe','berries','vegetables','bread','eggs','fish','stone')},player=copy.deepcopy(v['player']),visits=copy.deepcopy(v.get('local_visits',{})),tools=[w.get('worker_tool') for w in v['villagers']])

def cases():
    result=[]
    def run(label,v,commands):
        before=copy.deepcopy(v);steps=[]
        for cmd in commands:
            old=copy.deepcopy(v);ok=True
            try:
                if cmd['action']=='move':v['player'].update(x=cmd['x'],y=cmd['y'])
                elif cmd['action']=='roads':v['roads']=[dict(x=i,y=2) for i in range(cmd['count'])]
                elif cmd['action']=='restock':n.advance_trades(v,cmd['seconds'])
                elif cmd['action'] in ('buy_equipment','sell_equipment','equip_item','unequip_item'):e.change(v,cmd)
                else:n.change(v,cmd)
            except ValueError:
                ok=False;assert v==old
            steps.append(dict(command=cmd,ok=ok,after=snapshot(v)))
        result.append(dict(label=label,before=before,steps=steps))
    for name,item in n.GIFTS.items():
        v=town();v['npcs']=[dict(name=name,x=5,y=6)]
        run('gift-'+name,v,[dict(action='interact_local',npc_index=0),dict(action='claim_local_gift',npc_index=0),dict(action='claim_local_gift',npc_index=0)])
    for name,items in n.TOOL_SHOPS.items():
        v=town();v['npcs']=[dict(name=name,x=5,y=6)]
        run('shop-'+name,v,[dict(action='buy_worker_tool',npc_index=0,item=item) for item in items]+[dict(action='buy_worker_tool',npc_index=0,item='fine_axe')])
    for name in n.BARTERERS:
        v=town();v['npcs']=[dict(name=name,x=5,y=6)]
        run('barter-'+name,v,[dict(action='trade_local',npc_index=0),dict(action='trade_local',npc_index=0),dict(action='restock',seconds=300),dict(action='trade_local',npc_index=0)])
    v=town();v['player']['worker_tools']={'fine_axe':1}
    run('worker-transfers',v,[dict(action='give_worker_tool',index=0,item='fine_axe'),dict(action='give_worker_tool',index=0,item='crafted_axe'),dict(action='transfer_worker_tool',index=0,target_index=1,item='fine_axe'),dict(action='transfer_worker_tool',index=0,target_index=1,item='fine_axe'),dict(action='take_worker_tool',index=1),dict(action='give_worker_tool',index=0,item='crafted_axe'),dict(action='take_worker_tool',index=0)])
    for name in n.STORIES:
        v=town();v['npcs']=[dict(name=name,x=5,y=6)]
        commands=[dict(action='advance_local_story',npc_index=0)]
        if name=='Flint':
            v['map_resources']=[dict(kind='rock',x=x,y=6) for x in (4,5,6)]
            commands += [dict(action='inspect_story_rock',x=x,y=6) for x in (4,5,6)]
            commands += [dict(action='inspect_story_rock',x=4,y=6)]
        commands += [dict(action='advance_local_story',npc_index=0)]
        if name=='Tobin':commands += [dict(action='advance_local_story',npc_index=0),dict(action='roads',count=3)]
        commands += [dict(action='advance_local_story',npc_index=0),dict(action='advance_local_story',npc_index=0)]
        run('story-'+name,v,commands)
    for item,name in [('swamp_boots','Petra'),('desert_clothing','Dara')]:
        v=town();v['npcs']=[dict(name=name,x=5,y=6)]
        run('equipment-'+item,v,[dict(action='buy_equipment',npc_index=0,item=item),dict(action='equip_item',item=item),dict(action='sell_equipment',npc_index=0,item=item),dict(action='unequip_item',item=item),dict(action='sell_equipment',npc_index=0,item=item)])
    v=town();v['npcs']=[dict(name='Sylva',x=20,y=20,routine=dict(x=5.,y=6.))]
    run('routine-position',v,[dict(action='claim_local_gift',npc_index=0),dict(action='move',x=20,y=20),dict(action='interact_local',npc_index=0)])
    return result


def trade_cases():
    result=[]
    def run(label,commands,stock=None):
        world={'towns':{k:town() for k in ('one','two','three')}}
        for v in world['towns'].values():
            v['buildings'].append(dict(kind='storehouse',x=3,y=1,storage_enabled={'leather':True,'iron':True},stored_goods={'leather':10,'iron':10}))
            v['goods'].update(leather=10,iron=10)
        if stock:
            for key,goods in stock.items():
                world['towns'][key]['buildings'][2]['stored_goods'].update(goods)
                world['towns'][key]['goods'].update(goods)
        before=copy.deepcopy(world);steps=[];sequence=0
        for cmd in commands:
            actor=cmd['actor'];v=world['towns'][actor];old=copy.deepcopy(world);ok=True
            def next_id(*args):
                nonlocal sequence
                sequence+=1
                return f'offer-{sequence}'
            try:
                if cmd['action']=='spend':p.spend(v,{cmd['resource']:cmd['amount']})
                elif cmd['action']=='level':v['level']=cmd['level']
                else:
                    with patch.object(t.secrets,'token_hex',side_effect=next_id):t.change(v,cmd,world)
            except ValueError:ok=False;assert world==old
            after={k:dict(goods={r:p.quantity(v,r) for r in p.TRADABLE_GOODS},offers=copy.deepcopy(v.get('trade_offers',[])),completed=v.get('completed_trades',0)) for k,v in world['towns'].items()}
            steps.append(dict(command=cmd,ok=ok,after=after,sequence=sequence))
        result.append(dict(label=label,before=before,steps=steps))
    create=dict(actor='one',action='create_trade',target_town='two',give_resource='leather',give_amount=5,want_resource='coins',want_amount=10)
    accept=dict(actor='two',action='accept_trade',source_town='one',offer_id='offer-1')
    run('target-reload-once',[create,{**accept,'actor':'three'},accept,accept])
    run('stale-stock',[create,dict(actor='one',action='spend',resource='leather',amount=8),accept])
    run('outgoing-before-incoming',[{**create,'want_resource':'iron','give_amount':10,'want_amount':10},accept])
    run('full-incoming',[create,accept],{'two':{'leather':50}})
    run('cancel-and-limit',[create]*11+[dict(actor='two',action='cancel_trade',offer_id='offer-1'),dict(actor='one',action='cancel_trade',offer_id='offer-1'),accept])
    run('market-level-rechecked',[create,dict(actor='one',action='level',level=4),accept])
    for change in [dict(target_town='one'),dict(target_town='absent'),dict(give_amount=True),dict(give_amount=0),dict(give_amount=101),dict(want_amount=1.5),dict(give_resource='horse'),dict(want_resource='leather')]:
        run('invalid-'+str(change),[{**create,**change}])
    return result

def activity_cases():
    result=[]
    def run(label,v,commands):
        before=copy.deepcopy(v);steps=[]
        for cmd in commands:
            old=copy.deepcopy(v);ok=True
            try:
                if cmd['action']=='tick':
                    with patch.object(p.secrets,'randbelow',return_value=cmd.get('roll',1)):p.player_tick(v,v,1000)
                elif cmd['action']=='move':v['player'].update(x=cmd['x'],y=cmd['y'])
                else:p.start_activity(v,v,cmd['action'])
            except ValueError:ok=False;assert v==old
            steps.append(dict(command=cmd,ok=ok,after=dict(state=snapshot(v),resources=copy.deepcopy(v['map_resources']),stumps=copy.deepcopy(v.get('tree_stumps',[])))))
        result.append(dict(label=label,before=before,steps=steps))
    def prepared():
        v=town();v['buildings'].append(dict(kind='storehouse',x=3,y=1,storage_enabled={k:True for k in ('wood','herbs','berries','fish','stone','food')}));v['terrain_version']=7;v['map_resources']=[];v['tree_stumps']=[];p.ensure_inventory(v)
        return v
    for kind,action in [('tree','chop'),('berry_bush','gather'),('herb_patch','gather'),('water','fish')]:
        v=prepared();v['map_resources']=[dict(kind=kind,x=5,y=6,supply=3.)]
        run(kind,v,[dict(action=action)]+[dict(action='tick',roll=i%5) for i in range(6)]+[dict(action='stop_activity')])
    for version,biome,species in [(7,'g',None),(5,'g',None),(7,'f',None),(7,'b','ancient')]:
        v=prepared();v['terrain_version']=version;rows=list(v['terrain']);rows[6]=rows[6][:5]+biome+rows[6][6:];v['terrain']=rows
        r=dict(kind='tree',x=5,y=6,supply=.4)
        if species:r['species']=species
        v['map_resources']=[r];run(f'tree-stump-{version}-{biome}',v,[dict(action='chop'),dict(action='tick'),dict(action='tick')])
    for biome in ('g','m','r'):
        v=prepared();rows=list(v['terrain']);rows[6]=rows[6][:5]+biome+rows[6][6:];v['terrain']=rows;v['map_resources']=[dict(kind='rock',x=5,y=6)]
        run('mine-'+biome,v,[dict(action='mine'),dict(action='mine')])
    v=prepared();v['rules']={'production_caps':{'stone':9}};v['map_resources']=[dict(kind='rock',x=5,y=6)];run('mine-cap',v,[dict(action='mine')])
    v=prepared();v['map_resources']=[dict(kind='chest',x=5,y=6)];run('one-chest',v,[dict(action='gather'),dict(action='gather')])
    v=prepared();v['map_resources']=[dict(kind='water',x=5,y=6,supply=12.),dict(kind='water',x=5,y=7,supply=2.),dict(kind='water',x=20,y=20)];run('water-shared-miss',v,[dict(action='fish'),dict(action='tick',roll=0),dict(action='tick',roll=1)])
    v=prepared();v['player'].update(boat=True,boat_level=3,fishing=40,x=5.,y=6.);v['map_resources']=[dict(kind='water',x=5,y=6,supply=12.)];run('afloat-founder',v,[dict(action='fish'),dict(action='tick')])
    v=prepared();v['player'].update(equipment={'body':'desert_clothing'},gathering=20);v['map_resources']=[dict(kind='berry_bush',x=5,y=6,supply=6.)];run('clothing-practice',v,[dict(action='gather'),dict(action='tick')])
    for cap in (0,100):
        v=prepared();v['rules']={'production_caps':{'berries':cap}};v['map_resources']=[dict(kind='berry_bush',x=5,y=6,supply=0.)];run(f'cap-supply-{cap}',v,[dict(action='gather'),dict(action='tick')])
    v=prepared();v['map_resources']=[dict(kind='tree',x=5,y=6,supply=8.),dict(kind='tree',x=6,y=5,supply=8.)];run('toggle-and-range',v,[dict(action='chop'),dict(action='chop'),dict(action='chop'),dict(action='move',x=20,y=20),dict(action='tick')])
    return result

def roster_cases():
    result=[]
    for seed in (42,'00042',-7):
        for existing in (False,True):
            for ancient in (False,True):
                v=town();v['terrain_seed']=seed;v['terrain']=['gfmbrs'[y%6]*32 for y in range(32)];v.pop('npc_roster_version',None);v.pop('npc_placement_version',None)
                if existing:v['npcs']=[dict(name='Dara',biome='s',x=18,y=17,dialogue='old')];v['local_visits']={'Dara:18:17':{'gift_claimed':True,'story_stage':2,'trade_remaining':7.5}}
                else:v.pop('npcs')
                v['map_resources']=[dict(kind='tree',x=20,y=20,**({'species':'ancient'} if ancient else {}))];v['tree_stumps']=[dict(kind='tree',x=22,y=20,remaining=50.)]
                before=copy.deepcopy(v);moves=n.ensure_npcs(v);after=copy.deepcopy(v);n.ensure_npcs(v)
                result.append(dict(before=before,after=after,repeated=v,moved=moves))
    return result

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    outputs={ROOT/'rust/crates/lantern-host/src/social/catalog.json':catalog(),Path(__file__).with_name('python.json'):cases(),Path(__file__).with_name('trades.json'):trade_cases(),Path(__file__).with_name('activities.json'):activity_cases(),Path(__file__).with_name('roster.json'):roster_cases()}
    for path,value in outputs.items():
        text=json.dumps(value,ensure_ascii=False,separators=(',',':'))+'\n'
        if args.check:
            if path.read_text()!=text:raise SystemExit(f'social fixture/catalog drift: {path.name}')
        else:path.write_text(text)
    print(f'{len(outputs[Path(__file__).with_name("python.json")])} Python social scenarios and catalog match')
