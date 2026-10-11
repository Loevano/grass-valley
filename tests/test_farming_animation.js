// Run with: node tests/test_farming_animation.js
const assert=require('assert');
const source=require('fs').readFileSync('web/app.js','utf8');
const animation=require('../web/animations.js');
const marks=[];
const ctx=new Proxy({}, {get(target,key){
  if(key in target)return target[key];
  return (...args)=>marks.push({method:key,args,color:key==='fillRect'?target.fillStyle:target.strokeStyle});
}});
const draw=new Function('ctx','polygon','point','tileW','tileH','npcColors',
  source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+
  '\nreturn drawVillager;'
)(ctx,(points,color)=>marks.push({method:'polygon',points,color}),
  (x,y)=>[(x-y)*23,(x+y)*11.5],46,23,{});
function render(state,job='farming',active=true){
  marks.length=0;ctx.strokeStyle=undefined;ctx.fillStyle=undefined;
  draw({kind:'villager',index:0,x:state.x,y:state.y,state,active,villager:{job}});
  return marks.slice();
}
const stages=[['plowing','#aab9b4'],['sowing','#efd59a'],['watering','#9fd8e5'],['harvesting','#e5c77f']];
const town={map_size:12,buildings:[{kind:'farm',x:6,y:2},{kind:'acre',x:6,y:3,farm_index:0}],
  map_resources:[{kind:'water',x:9,y:2}]};
for(const target of [town.buildings[1],town.buildings[0]]){
  const state=animation.plan(town,{job:'farming',work_target:target},0);
  for(let i=0;i<300&&state.path.length;i++)animation.advance(state,.1,true);
  assert.strictEqual(state.phase,'working');
  if(target.kind==='acre'){
    assert.strictEqual(state.x,target.x,'Farmer works inside the acre');
    assert.strictEqual(state.y,target.y,'Farmer works inside the acre');
  }
  for(const [index,[name,color]] of stages.entries()){
    if(index===2){
      assert(['fetching_water','collecting_water'].includes(state.phase),'Water fetching begins after sowing');
      for(let i=0;i<300&&state.phase!=='working';i++)animation.advance(state,.1,true);
      assert.strictEqual(state.phase,'working','Farmer brings water back before watering');
      assert.strictEqual(state.elapsed,4,'Field work pauses while fetching water');
      assert(state.waterReady,'Watering uses a filled can');
    }
    const output=render(state);
    assert(output.some(mark=>mark.color===color),`Arrival at stage ${index} shows ${name}`);
    for(const [,other] of stages.filter((_,i)=>i!==index)){
      assert(!output.some(mark=>mark.color===other),`${name} has its own tools and effects`);
    }
    animation.advance(state,.25,true);
    assert.notDeepStrictEqual(render(state),output,`${name} moves over time`);
    const elapsed=state.elapsed,paused=render(state,'farming',false);
    animation.advance(state,5,false);
    assert.strictEqual(state.elapsed,elapsed,`Full storage pauses ${name}`);
    assert.deepStrictEqual(render(state,'farming',false),paused,`${name} holds its pose while paused`);
    animation.advance(state,1.75,true);
  }
  assert(['returning','depositing'].includes(state.phase),'Delivery starts after all four actions');
  assert(render(state).some(mark=>mark.color==='#e5c77f'),'Farmer carries harvested grain');
  for(let i=0;i<300&&state.path.length;i++)animation.advance(state,.1,true);
  assert.strictEqual(state.phase,'depositing');
  assert.strictEqual(state.home,town.buildings[0],'Harvest is delivered to the assigned farm');
  animation.advance(state,.7,true);
  for(let i=0;i<300&&state.path.length;i++)animation.advance(state,.1,true);
  assert.strictEqual(state.phase,'working');
  assert.strictEqual(state.elapsed,0);
  assert(render(state).some(mark=>mark.color===stages[0][1]),'Next cycle restarts with plowing');
  const resting=animation.plan(town,{job:'idle'},0,state);
  assert(!render(resting,'idle').some(mark=>stages.some(([,color])=>mark.color===color)),
    'Changing jobs removes farming actions');
}
for(const job of ['woodcutting','fishing']){
  const target={kind:job==='fishing'?'water':'tree',x:8,y:2};
  const workplace={kind:job==='fishing'?'fishing_hut':'woodcutter',x:6,y:2};
  const state=animation.plan({...town,buildings:[workplace],map_resources:[target]}, {job,work_target:target},0);
  for(let i=0;i<300&&state.path.length;i++)animation.advance(state,.1,true);
  animation.advance(state,3.3,true);
  assert.notStrictEqual(state.phase,'working',`${job} keeps its shorter work cycle`);
}
// Connected acres remain accessible even when a field has no empty neighbor.
const fields=[{kind:'acre',x:6,y:3,farm_index:0},
  ...[{x:5,y:3},{x:7,y:3},{x:6,y:4}].map(tile=>({kind:'acre',...tile,farm_index:0}))];
const clustered={...town,buildings:[town.buildings[0],...fields]};
const farmer=animation.plan(clustered,{job:'farming',work_target:fields[0]},0);
for(let i=0;i<300&&farmer.path.length;i++)animation.advance(farmer,.1,true);
assert.strictEqual(farmer.phase,'working','Farmer can enter a field surrounded by other acres');
assert.strictEqual(farmer.x,6);assert.strictEqual(farmer.y,3);
for(let i=0;i<300&&!['returning','depositing'].includes(farmer.phase);i++)animation.advance(farmer,.1,true);
assert(['returning','depositing'].includes(farmer.phase),'Farmer can leave the cluster to deliver grain');
const builder=animation.plan(clustered,{job:'farming',construction_target:town.buildings[0]},0);
assert(!builder.blocked.has('6,3'),'Construction workers can cross completed acres');
const delivery=animation.plan(clustered,{job:'farming',logistics_position:{x:6,y:3},
  logistics_target:{kind:'delivery',x:6,y:4}},0);
assert(!delivery.blocked.has('6,4'),'Delivery animation can cross completed acres');
assert.deepStrictEqual(delivery.path,[{x:6,y:4}],'Delivery follows the host path through fields');
const unfinished=animation.plan({...clustered,buildings:[...clustered.buildings,
  {kind:'acre',x:6,y:5,construction:{total:30,progress:0}}]},
  {job:'farming',logistics_position:{x:6,y:4},logistics_target:{kind:'delivery',x:6,y:5}},0);
assert(unfinished.blocked.has('6,5'),'Delivery animation avoids unfinished acres');

const crops=new Function('ctx','polygon','point','tileW','tileH',
  source.slice(source.indexOf('function buildingLevel('),source.indexOf('function storageCapacity('))+
  source.slice(source.indexOf('function drawFarmCrops('),source.indexOf('function drawBuilding('))+
  '\nreturn drawFarmCrops;'
)(ctx,(points,color)=>marks.push({method:'polygon',points,color}),
  (x,y)=>[(x-y)*23,(x+y)*11.5],46,23);
function fieldAt(elapsed,crop='wheat'){
  marks.length=0;crops({...fields[0],crop},elapsed===null?null:{elapsed});return marks.slice();
}
const soil=fieldAt(0),sown=fieldAt(3),growing=fieldAt(5),ripe=fieldAt(6),cut=fieldAt(7);
assert(!soil.some(mark=>mark.color==='#efd59a'||mark.color==='#8db568'),'Plowing leaves bare furrows');
assert.strictEqual(sown.filter(mark=>mark.color==='#efd59a').length,8,'Sowing gradually fills the field');
assert(growing.some(mark=>mark.color==='#8db568'),'Watering grows green shoots');
assert.notDeepStrictEqual(growing,fieldAt(5.25),'Growing crops move with the work cycle');
const grain=output=>output.filter(mark=>mark.method==='stroke'&&['#d9bd70','#b8b458'].includes(mark.color));
assert.strictEqual(grain(ripe).length,16,'Harvest starts with ripe grain');
assert.strictEqual(grain(cut).length,8,'Harvest gradually cuts the grain');
assert.strictEqual(grain(fieldAt(null)).length,16,'Unattended fields keep their normal ripe appearance');
const ripeCrops=['vegetables','wheat','carrots','fodder'].map(crop=>fieldAt(null,crop));
for(let i=0;i<ripeCrops.length;i++)for(let j=i+1;j<ripeCrops.length;j++){
  assert.notDeepStrictEqual(ripeCrops[i],ripeCrops[j],'Each crop has distinct ripe artwork');
}
assert(ripeCrops[0].some(mark=>mark.method==='polygon'&&mark.color==='#68934e'),'Vegetables have green leafy heads');
assert(ripeCrops[1].some(mark=>mark.method==='polygon'&&mark.color==='#efd59a'),'Wheat has golden ears');
assert(ripeCrops[2].some(mark=>mark.method==='polygon'&&mark.color==='#dd9048'),'Carrots show orange roots');
assert(ripeCrops[3].some(mark=>mark.method==='stroke'&&mark.color==='#72a65c'),'Fodder has green grass tufts');
assert.deepStrictEqual(fieldAt(null,null),ripeCrops[0],'Fields without a crop use vegetables');
assert.deepStrictEqual(fieldAt(null,'unknown'),ripeCrops[0],'Unknown crop types fall back to vegetables');
for(const crop of ['vegetables','carrots','fodder']){
  assert.deepStrictEqual(fieldAt(0,crop),soil,'Plowing preserves bare soil for every crop');
  assert.deepStrictEqual(fieldAt(3,crop),sown,'Every crop is planted progressively');
  assert.notDeepStrictEqual(fieldAt(5,crop),fieldAt(5.25,crop),'Each growing crop moves during tending');
  const collected=[];
  crops({...fields[0],crop},{elapsed:7},plant=>collected.push(plant));
  assert.strictEqual(collected.length,8,'Harvest removes half of each crop at mid-cycle');
  assert(collected.every(plant=>plant.cropType===crop),'Depth-sorted plants retain their crop identity');
}
console.log('Farming animation checks passed');
