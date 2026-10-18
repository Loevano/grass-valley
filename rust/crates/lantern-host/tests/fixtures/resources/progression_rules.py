"""R13 rule parity from current Python helpers and disposable commands; no terrain generation."""
import copy
import math
import progression as p
import equipment
import logistics
import server


def town_for(building, workers=(), sandbox=False, level=10):
    goods = {k:1000 for k in p.STORAGE_KEYS if k not in {'wood','food'}}
    return dict(town_origin=[0,0],map_resources=[],terrain=['g'*8]*8,map_size=8,npcs=[],
                inventory_version=1,food_balance_version=2,market_storage_version=1,
                wood_chain_version=1,grain_chain_version=1,acre_upgrade_version=1,worker_capacity_version=1,
                wood=1000,food=sum(goods.get(k,0)*n for k,n in p.EDIBLE.items()),coins=1000,goods=goods,
                harvested=0,level=level,sandbox=sandbox,player={},
                buildings=[dict(kind='hall',x=0,y=0,stored_goods=dict(goods,wood=1000)),dict(copy.deepcopy(building),x=3,y=3)],
                villagers=[dict(copy.deepcopy(w),building_index=1) for w in workers])


def command_ok(town, data):
    try:
        server.command(town,data)
        return True
    except ValueError:
        return False


def cases():
    practice_cases = []
    jobs = sorted(server.JOBS | {'gathering'})
    for job in jobs:
        for talent in (-1,0,1):
            for value in (0,19.999999999999996,20,39.99999999999999,40):
                w=dict(job=job,talents={p.TALENT_ROOTS.get(job,job):talent},**{job:value})
                before=copy.deepcopy(w)
                p.practice(w,job,6)
                practice_cases.append(dict(worker=before,job=job,seconds=6,aptitude=p.aptitude(before,job),
                    efficiency=p.efficiency(before,job),qualified=p.qualification(before,job) is None,after=w[job]))
    for value in (0.0005,0.0015,1.2345,1.2355,2.6755,9.9995,123456.7895,1e16,1e100):
        for n in (math.nextafter(value,0),value,math.nextafter(value,math.inf)):
            w=dict(job='building',building=n)
            before=copy.deepcopy(w)
            p.practice(w,'building',0)
            practice_cases.append(dict(worker=before,job='building',seconds=0,aptitude=0,
                efficiency=p.efficiency(before,'building'),qualified=True,after=w['building']))
    defaults=[]
    for index in range(8):
        for initial in ({},{'talents':{}},{'talents':{'fishing':.5}}):
            w=copy.deepcopy(initial);p.ensure_talents(w,index)
            defaults.append(dict(index=index,initial=initial,expected=w['talents']))
    batches=[]
    for kind in p.RECIPES:
        recipes=p.PROFESSION_RECIPES.get(kind,[dict(output=p.RECIPES[kind][1],level=1)])
        for recipe in recipes:
            job=p.BUILDINGS[kind]['job']
            for skill,talent,sandbox,queued in [(0,0,False,True),(recipe['level'],1,False,True),
                     (max(0,recipe['level']-.001),-1,False,True),(40,0,False,True),(0,0,True,True),(0,0,True,False)]:
                w=dict(job=job,talents={p.TALENT_ROOTS.get(job,job):talent},sandbox=sandbox,
                       worker_tool='crafted_saw',**p.JOB_REQUIREMENTS.get(job,{}),**{job:skill})
                b=dict(kind=kind,recipe=recipe['output'],profession_upgrades=[k for k,s in p.PROFESSION_UPGRADES.items() if s['building']==kind])
                if kind=='workshop' and queued:
                    b['craft_queue']=[dict(recipe=recipe['output'],remaining=.75)]
                inputs={'fish':.25,'trout':.5,'salmon':10,'sturgeon':10}
                selected=p.production_recipe(b)
                reason=p.qualification(w,job) or p.recipe_requirement(w,b)
                result=dict(worker=w,building=b,inputs=inputs,qualified=reason is None,
                    recipe=dict(inputs=selected[0],output=selected[1],amount=selected[2]),
                    seconds=p.production_seconds(b),speed=p.processing_efficiency(w,b))
                if reason is None:
                    costs=p.production_costs(b,result['speed'],inputs)
                    outputs=({f'smoked_{k}':n for k,n in costs.items() if k in p.FISH} if kind=='smokehouse'
                             else {selected[1]:selected[2]*result['speed']})
                    result.update(costs=costs,outputs=outputs)
                batches.append(result)
        # Unknown saved selection falls back to the original recipe.
        b=dict(kind=kind,recipe='unknown_saved_recipe')
        selected=p.production_recipe(b)
        batches.append(dict(building=b,worker=dict(job=p.BUILDINGS[kind]['job'],sandbox=True),inputs={},qualified=kind!='workshop',
            recipe=dict(inputs=selected[0],output=selected[1],amount=selected[2]),seconds=p.production_seconds(b),speed=1,
            **(dict(costs=selected[0],outputs={selected[1]:selected[2]}) if kind!='workshop' else {})))
    selections=[]
    for kind,recipes in p.PROFESSION_RECIPES.items():
        job=p.BUILDINGS[kind]['job']
        for recipe in recipes:
            for mode in ('none','ready','low','unqualified','wrong_job','sandbox'):
                skill=recipe['level'] if mode!='low' else max(0,recipe['level']-.001)
                w=dict(job=job,**p.JOB_REQUIREMENTS.get(job,{}),**{job:skill})
                if mode=='unqualified':
                    w.update({k:0 for k in p.JOB_REQUIREMENTS.get(job,{})})
                if mode=='wrong_job':
                    w['job']='idle'
                assigned=[] if mode in {'none','sandbox'} else [w]
                town=town_for(dict(kind=kind),assigned,mode=='sandbox')
                data=dict(action='queue_workshop' if kind=='workshop' else 'set_recipe',index=1,recipe=recipe['output'],amount=1)
                selections.append(dict(kind=kind,output=recipe['output'],workers=assigned,sandbox=mode=='sandbox',accepted=command_ok(town,data)))
    upgrades=[]
    for key,spec in p.PROFESSION_UPGRADES.items():
        for mode in ('ready','low_hall','low_skill','previous_missing','already','sandbox'):
            keys=[spec['requires']] if spec.get('requires') and mode!='previous_missing' else []
            if mode=='already':keys.append(key)
            w=dict(job='idle',**{spec.get('skill','fishing'):spec.get('xp',0) if mode!='low_skill' else max(0,spec.get('xp',0)-.001)})
            level=spec['level']-1 if mode=='low_hall' else spec['level']
            b=dict(kind=spec['building'],profession_upgrades=keys)
            town=town_for(b,[w],mode=='sandbox',level)
            accepted=command_ok(town,dict(action='profession_upgrade',index=1,upgrade=key))
            upgrades.append(dict(key=key,building=b,level=level,workers=[w],sandbox=mode=='sandbox',
                accepted=accepted,costs={} if mode=='sandbox' else p.costs_of(spec),
                instant='construction' not in town['buildings'][1]))
    crops=[]
    for kind in ('farm','acre','well'):
        for crop in (*p.CROPS,'unknown'):
            for level in range(1,5):
                town=town_for(dict(kind=kind),level=level)
                crops.append(dict(kind=kind,crop=crop,level=level,
                    accepted=command_ok(town,dict(action='set_crop',index=1,crop=crop))))
    fish=[]
    for keys in ([],['boat'],['improved_boat'],['master_boat'],['boat','improved_boat','master_boat']):
        b=dict(kind='fishing_hut',profession_upgrades=keys)
        for depth in range(6):
            try:catch=p.fishing_catch(b,depth)
            except StopIteration:catch=None
            fish.append(dict(equipment=keys,depth=depth,catch=catch,level=p.fishing_boat_level(b),capacity=p.fishing_boat_capacity(b)))
    clothes=[]
    for good in ('wood','herbs','berries'):
        for worn in ({},{'feet':'swamp_boots'},{'body':'desert_clothing'},{'feet':'desert_clothing'},{'feet':'swamp_boots','body':'desert_clothing'}):
            clothes.append(dict(good=good,equipment=worn,bonus=equipment.gathering_bonus(dict(equipment=worn),good)))
    halls=[]
    for home in ('complete','new','upgrade'):
        for food_kind in ('farm','fishing_hut'):
            for food_status in ('complete','new','upgrade'):
                for source in ('water','well','new_well','upgrading_well'):
                    for distance in (10,11):
                        b=[dict(kind='home',x=0,y=0),dict(kind=food_kind,x=2,y=2)]
                        for building,status in zip(b,(home,food_status)):
                            if status!='complete':building['construction']=dict(upgrade={} if status=='upgrade' else None) if status=='upgrade' else {}
                        if source!='water':
                            well=dict(kind='well',x=2+distance,y=2)
                            if source!='well':well['construction']={'upgrade':{}} if source=='upgrading_well' else {}
                            b.append(well)
                        t=dict(food=20,buildings=b,map_resources=[dict(kind='water',x=2+distance,y=2)] if source=='water' else [])
                        halls.append(dict(level=2,town=t,ready=p.milestone(t,2)))
    for level in range(0,12):
        upgrade=next((s for s in p.HALL_UPGRADES if s['level']==level),{})
        for mode in ('ready','short','import_only','sale_short','trade'):
            produced=dict(upgrade.get('production',{}))
            if level==5:produced['bread']=10 if mode!='short' else 9.999
            if mode=='short' and produced:
                first=next(iter(produced));produced[first]-=.001
            if mode=='import_only':produced={}
            t=dict(food=20 if mode!='short' else 19.999,buildings=[dict(kind='home',x=0,y=0),dict(kind='farm',x=2,y=2)],
                   map_resources=[dict(kind='water',x=8,y=6)],produced=produced,completed_trades=int(mode=='trade'),market_sales=9.999 if mode=='sale_short' else 10)
            halls.append(dict(level=level,town=t,ready=p.milestone(t,level)))
    grain=[]
    for upgraded in (False,True):
        mill=dict(kind='mill',x=2,y=2,input_buffer={'wheat':320 if upgraded else 40},profession_upgrades=['masterwork_millstones'] if upgraded else [])
        bakeries=[dict(kind='bakery',x=4+i,y=4,input_buffer={'flour':0,'wood':80 if upgraded else 5},profession_upgrades=['masterwork_oven'] if upgraded else []) for i in range(1 if upgraded else 2)]
        workers=[dict(job='milling',farming=10,talents={})]+[dict(job='baking',milling=2,talents={}) for _ in bakeries]
        town=dict(inventory_version=1,food_balance_version=2,market_storage_version=1,wood=0,food=0,coins=0,goods={},harvested=0,buildings=[mill,*bakeries],villagers=workers)
        for _ in range(10):
            for _ in range(2):
                assert logistics.produce(town,mill,workers[0],3)=='Working'
            logistics.drop_off(town,mill,workers[0])
            for b,w in zip(bakeries,workers[1:]):
                flour=p.production_recipe(b)[0]['flour']*p.processing_efficiency(w,b)
                p.withdraw(town,'flour',flour,mill);p.add_stock(town,'flour',-flour)
                b['input_buffer']['flour']+=flour
                assert logistics.produce(town,b,w,6)=='Working'
                logistics.drop_off(town,b,w)
        before=town['food'];p.eat(town,300 if upgraded else 60)
        grain.append(dict(upgraded=upgraded,produced=town['produced'],food_before=before,food_after=town['food'],
                          skills=[{w['job']:w[w['job']]} for w in workers],working=200 if upgraded else 40))
    staffing=[dict(kind=kind,tier=tier,capacity=server.settler_capacity(dict(kind=kind,settler_level=tier)))
              for kind in ("hall",*server.COSTS) for tier in (1,2,3,4,8)]
    return dict(practice=practice_cases,staffing=staffing,
                defaults=defaults,batches=batches,selections=selections,upgrades=upgrades,crops=crops,fish=fish,clothes=clothes,halls=halls,grain=grain)
