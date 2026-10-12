// Run with: node tests/test_fishing_animation.js
const assert=require('assert');
const source=require('fs').readFileSync('web/app.js','utf8');
const animation=require('../web/animations.js');
const marks=[];
const ctx=new Proxy({}, {get(target,key){
  if(key in target)return target[key];
  return (...args)=>marks.push({method:key,args,color:key==='fillRect'?target.fillStyle:target.strokeStyle});
}});
const point=(x,y)=>[(x-y)*23,(x+y)*11.5];
const draw=new Function('ctx','polygon','point','tileW','tileH','npcColors',
  source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+
  '\nreturn drawVillager;'
)(ctx,(points,color)=>marks.push({method:'polygon',points,color}),point,46,23,{});
function render(state,active=true,index=0){
  marks.length=0;
  ctx.strokeStyle=undefined;ctx.fillStyle=undefined;
  draw({kind:'villager',index,x:state.x,y:state.y,state,active,villager:{job:'fishing'}});
  return marks.slice();
}
const state={x:5,y:5,path:[],phase:'working',clock:0,elapsed:0,facing:1,source:{x:6,y:5}};
const cast=render(state);
const drawSmall=new Function('ctx','polygon','point','tileW','tileH','npcColors',
  source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+
  '\nreturn drawVillager;'
)(ctx,(points,color)=>marks.push({method:'polygon',points,color}),(x,y)=>[(x-y)*11.5,(x+y)*5.75],23,11.5,{});
function renderSmall(elapsed){
  marks.length=0;
  drawSmall({kind:'villager',index:0,x:5,y:5,state:{...state,elapsed},active:true,villager:{job:'fishing'}});
  return marks.slice();
}
const smallCast=renderSmall(0),smallWait=renderSmall(1);
assert.notDeepStrictEqual(smallCast,smallWait,'Zoomed-out fishing still animates the cast');
assert(smallWait.some(mark=>mark.color==='#c3e1dc'),'Zoomed-out fishing shows the float ripple');
assert(renderSmall(2.7).some(mark=>mark.color==='#b8d8d5'),'Zoomed-out fishing reels in the catch');
state.elapsed=1;
const waiting=render(state);
assert.notDeepStrictEqual(cast,waiting,'Casting moves the rod and float');
for(const [dx,dy] of [[1,0],[-1,0],[0,1],[0,-1]]){
  state.source={x:state.x+dx,y:state.y+dy};
  const output=render(state,true,5);
  const ripple=output.find(mark=>mark.method==='ellipse'&&mark.color==='#c3e1dc');
  const facing=Math.sign(dx-dy);
  assert(ripple,'Waiting produces a ripple');
  assert.strictEqual(ripple.args[0],((dx-dy)*23-3)*facing,'Float reaches the selected water horizontally');
  assert.strictEqual(ripple.args[1],(dx+dy)*11.5-23*.65-1,'Float reaches the selected water vertically');
}
state.elapsed=2.7;
assert(render(state).some(mark=>mark.color==='#b8d8d5'),'Reeling lifts a visible fish');
const paused=render(state,false);
state.clock+=5;
assert.deepStrictEqual(render(state,false),paused,'Full storage holds the fishing pose without continuing to cast');
state.phase='returning';state.path=[{x:4,y:5}];
assert(render(state).some(mark=>mark.color==='#b8d8d5'),'Delivery displays the caught fish');
assert(!render(state).some(mark=>mark.color==='#c3e1dc'),'Walking back removes the water ripple');

const town={buildings:[{kind:'fishing_hut',x:6,y:2}],map_resources:[{kind:'water',x:9,y:2}]};
const worker=animation.plan(town,{job:'fishing',work_target:town.map_resources[0]},0);
for(let i=0;i<300&&worker.path.length;i++)animation.advance(worker,.1,true);
animation.advance(worker,2.7,true);
assert(render(worker).some(mark=>mark.color==='#b8d8d5'),'Worker timing reaches the catch before leaving the water');
animation.advance(worker,.6,true);
assert.strictEqual(worker.phase,'returning','Catching leads to delivery');
for(let i=0;i<300&&worker.path.length;i++)animation.advance(worker,.1,true);
assert.strictEqual(worker.phase,'depositing');
animation.advance(worker,.7,true);
for(let i=0;i<300&&worker.path.length;i++)animation.advance(worker,.1,true);
assert.strictEqual(worker.phase,'working');
assert.strictEqual(worker.elapsed,0,'Returning to the water starts a fresh cast');

const pond={map_size:16,buildings:[{kind:'fishing_hut',x:6,y:2}],
  map_resources:Array.from({length:5},(_,i)=>({kind:'water',x:9+i,y:2}))};
const deep=animation.plan(pond,{job:'fishing',work_target:pond.map_resources[0]},0);
assert.deepStrictEqual(deep.castTarget,{x:12,y:2},'A connected pond allows casts three tiles beyond the pier');
for(let i=0;i<300&&deep.path.length;i++)animation.advance(deep,.1,true);
deep.latestFishingCatch={attempt:1,caught:false};
animation.advance(deep,1,true);
const deepRipple=render(deep).find(mark=>mark.method==='ellipse'&&mark.color==='#c3e1dc');
assert.strictEqual(deepRipple.args[0],72,'Float is rendered beyond the end of the pier');
animation.advance(deep,1.7,true);
assert(!render(deep).some(mark=>mark.color==='#b8d8d5'),'An unsuccessful host attempt reels in an empty line');
deep.latestFishingCatch={attempt:2,caught:true};
animation.advance(deep,.1,true);
assert.strictEqual(deep.fishingCaught,false,'New host results do not change a catch during reeling');
animation.advance(deep,.5,true);
assert.strictEqual(deep.phase,'working','An unsuccessful cast retries without a delivery');
assert.strictEqual(deep.elapsed,0);
animation.advance(deep,2.7,true);
assert(render(deep).some(mark=>mark.color==='#b8d8d5'),'A successful host attempt shows a caught fish');
animation.advance(deep,.6,true);
assert(['returning','depositing'].includes(deep.phase),'Successful catches are delivered');

const separated={...pond,map_resources:[pond.map_resources[0],pond.map_resources[1],pond.map_resources[3]]};
assert.deepStrictEqual(animation.plan(separated,{job:'fishing',work_target:separated.map_resources[0]},0).castTarget,
  {x:10,y:2},'Casts stop before a strip of land rather than jumping to a separate pond');
const narrow=animation.plan({...pond,map_resources:[pond.map_resources[0]]},
  {job:'fishing',work_target:pond.map_resources[0]},0);
assert.deepStrictEqual(narrow.castTarget,pond.map_resources[0],'A small pool keeps the float in the available water');

const sailingTown={map_size:16,buildings:[{kind:'fishing_hut',x:2,y:2,profession_upgrades:['boat']}],
  map_resources:[[3,2],[3,3],[3,4],[4,4],[5,4],[5,3],[5,2]].map(([x,y])=>({kind:'water',x,y}))};
const sailor=animation.plan(sailingTown,{job:'fishing',building_index:0,work_target:sailingTown.map_resources.at(-1),
  logistics_position:{x:2,y:3}},0);
assert(sailor.boating,'Boat-equipped fishers sail to their target');
assert.deepStrictEqual(sailor.pier,[sailingTown.buildings[0],{x:3,y:2}],'Boat purchase preserves the dock at the bank');
const sea=new Set(sailingTown.map_resources.map(r=>r.x+','+r.y));
const boarding=sailor.path.findIndex(p=>sea.has(p.x+','+p.y));
assert(boarding>=0);
assert(sailor.path.slice(boarding).every(p=>sea.has(p.x+','+p.y)),'Sailing follows the curved water rather than crossing land');
for(let i=0;i<300&&sailor.path.length;i++)animation.advance(sailor,.1,true);
assert.deepStrictEqual([sailor.x,sailor.y],[5,2],'Boat arrives at the offshore fishing spot');
const pausedElapsed=sailor.elapsed;
animation.advance(sailor,1,false);
assert.strictEqual(sailor.elapsed,pausedElapsed,'Full storage pauses fishing offshore');
animation.advance(sailor,3.3,true);
assert.strictEqual(sailor.phase,'returning');
assert(sailor.path.every(p=>sea.has(p.x+','+p.y)),'Catch returns through connected water to the berth');
for(let i=0;i<300&&sailor.path.length;i++)animation.advance(sailor,.1,true);
assert.strictEqual(sailor.phase,'depositing');
assert.deepStrictEqual([sailor.x,sailor.y],[3,2],'Catch unloads beside the hut');
animation.advance(sailor,.7,true);
assert.strictEqual(sailor.phase,'walking','Boat departs again after unloading');

const offshore=animation.plan(sailingTown,{job:'fishing',building_index:0,work_target:sailingTown.map_resources.at(-1),
  logistics_position:{x:2,y:3}},0,{...sailor,x:4.4,y:4,path:[{x:5,y:4}]});
assert.deepStrictEqual([offshore.x,offshore.y],[4.4,4],'Retargeting preserves the visible offshore position');
assert(offshore.path.every(p=>sea.has(p.x+','+p.y)),'Retargeting offshore keeps the entire approach on water');
assert.deepStrictEqual(offshore.path[0],{x:5,y:4},'Retargeting continues to the existing waypoint before turning');
const resumed=animation.plan(sailingTown,{job:'fishing',building_index:0,work_target:sailingTown.map_resources.at(-1),
  logistics_position:{x:5,y:3}},0);
assert.deepStrictEqual([resumed.x,resumed.y],[5,3],'A saved offshore position resumes on water');
const lake={map_size:24,buildings:[],map_resources:[]};
for(let x=3;x<18;x++)for(let y=1;y<16;y++)lake.map_resources.push({kind:'water',x,y});
const depths=animation.waterDepths(lake.map_resources);
assert.strictEqual(depths.get('3,8'),1,'Water beside the bank is shallow');
assert.strictEqual(depths.get('10,8'),8,'Water gets deeper toward the center of the lake');
for(let tier=1;tier<=3;tier++){
  lake.buildings=[{kind:'fishing_hut',x:2,y:8,profession_upgrades:['boat','improved_boat','master_boat'].slice(0,tier)}];
  const target=lake.map_resources.find(r=>r.x===3+tier&&r.y===8);
  const fisher=animation.plan(lake,{job:'fishing',building_index:0,work_target:target,logistics_position:{x:2,y:9}},0);
  assert(fisher.path.length,'Every boat tier can reach its unlocked fishing depth');
  assert(fisher.path.every(p=>!depths.has(p.x+','+p.y)||depths.get(p.x+','+p.y)<=tier+1),'Boats stay within their tier depth limit');
  for(let i=0;i<300&&fisher.path.length;i++)animation.advance(fisher,.1,true);
  assert.deepStrictEqual([fisher.x,fisher.y],[target.x,target.y],'Boat reaches deeper water after upgrading');
}
const island=lake.map_resources.filter(r=>r.x!==10||r.y!==8);
assert.strictEqual(animation.waterDepths(island).get('10,7'),1,'Island shores make adjacent water shallow');

const crewTown={...sailingTown,buildings:[{...sailingTown.buildings[0],settler_level:4}],
  villagers:Array.from({length:4},()=>({job:'fishing',building_index:0}))};
for(let tier=1;tier<=3;tier++){
  crewTown.buildings[0].profession_upgrades=['boat','improved_boat','master_boat'].slice(0,tier);
  for(let i=0;i<4;i++){
    const source=crewTown.map_resources[i<tier?crewTown.map_resources.length-1:0];
    const fisher=animation.plan(crewTown,{...crewTown.villagers[i],work_target:source},i);
    assert.strictEqual(!!fisher.boating,i<tier,'Boat crew is limited to its upgraded capacity');
    assert(fisher.pier.length,'Every tier keeps the dock');
    if(i>=tier){
      for(let step=0;step<300&&fisher.path.length;step++)animation.advance(fisher,.1,true);
      assert.strictEqual(fisher.phase,'working');
      assert(fisher.pier.some(p=>p.x===fisher.x&&p.y===fisher.y),'Rod fishers stand on the dock');
    }
  }
}
console.log('Fishing animation checks passed');
