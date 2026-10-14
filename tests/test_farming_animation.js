// Run with: node tests/test_farming_animation.js
const assert=require('assert');
const source=require('fs').readFileSync('web/app.js','utf8');
const animation=require('../web/animations.js');
const marks=[];
let transform=[1,0,0,1,0,0];
const transforms=[];
const ctx=new Proxy({}, {get(target,key){
  if(key in target)return target[key];
  return (...args)=>{
    const [a,b,c,d,e,f]=transform;
    if(key==='save')transforms.push(transform.slice());
    if(key==='restore')transform=transforms.pop();
    if(key==='translate')transform=[a,b,c,d,e+a*args[0]+c*args[1],f+b*args[0]+d*args[1]];
    if(key==='scale')transform=[a*args[0],b*args[0],c*args[1],d*args[1],e,f];
    if(key==='rotate'){
      const cos=Math.cos(args[0]),sin=Math.sin(args[0]);
      transform=[a*cos+c*sin,b*cos+d*sin,c*cos-a*sin,d*cos-b*sin,e,f];
    }
    const center=key==='fillRect'?[e+a*(args[0]+args[2]/2)+c*(args[1]+args[3]/2),
      f+b*(args[0]+args[2]/2)+d*(args[1]+args[3]/2)]:undefined;
    marks.push({method:key,args,color:key==='fillRect'?target.fillStyle:target.strokeStyle,center});
  };
}});
const draw=new Function('ctx','polygon','point','tileW','tileH','npcColors',
  source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+
  '\nreturn drawVillager;'
)(ctx,(points,color)=>marks.push({method:'polygon',points,color}),
  (x,y)=>[(x-y)*23,(x+y)*11.5],46,23,{});
function render(state,job='farming',active=true,index=0,drawWorker=draw){
  marks.length=0;ctx.strokeStyle=undefined;ctx.fillStyle=undefined;
  drawWorker({kind:'villager',index,x:state.x,y:state.y,state,active,villager:{job}});
  return marks.slice();
}
const stages=[['plowing','#aab9b4'],['sowing','#efd59a'],['watering','#9fd8e5'],['harvesting','#e5c77f']];
// Follow particles to the ground through the same canvas transforms as the worker.
for(const tileWidth of [46,92]){
  const drawWorker=new Function('ctx','polygon','point','tileW','tileH','npcColors',
    source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+
    '\nreturn drawVillager;'
  )(ctx,()=>{},(x,y)=>[(x-y)*tileWidth/2,(x+y)*tileWidth/4],tileWidth,tileWidth/2,{});
  for(const [farmAction,color,rate,count] of [['sowing','#efd59a',2,5],['watering','#9fd8e5',3,3]]){
    for(const facing of [-1,1])for(const index of [0,5,11])for(const confirmed of [false,true]){
      const field={x:6,y:3};
      const state={x:field.x+(facing===1?-.45:.1),y:field.y+(facing===1?.1:-.45),
        path:[],phase:'working',clock:0,facing,farmAction,
        source:confirmed?{x:6+facing,y:3}:field,farmField:confirmed?field:undefined};
      assert.strictEqual(render(state,'farming',true,index,drawWorker)
        .filter(mark=>mark.method==='fillRect'&&mark.color===color).length,count,
        `${farmAction} uses the intended sparse particle count`);
      for(let particle=0;particle<count;particle++){
        state.elapsed=(1-1e-7-particle/count)/rate;
        const mark=render(state,'farming',true,index,drawWorker)
          .filter(mark=>mark.method==='fillRect'&&mark.color===color)[particle];
        const [px,py]=mark.center;
        const x=px/tileWidth+(py-tileWidth/4)/(tileWidth/2);
        const y=(py-tileWidth/4)/(tileWidth/2)-px/tileWidth;
        assert(Math.abs(x-field.x)+Math.abs(y-field.y)<.8,
          `${farmAction} lands inside the worked acre at either facing and zoom`);
        const plants=Array.from({length:16},(_,i)=>[field.x-.3+Math.floor(i/4)*.2,field.y-.3+i%4*.2]);
        assert(plants.some(([cx,cy])=>Math.hypot(x-cx,y-cy)<1e-5),
          `${farmAction} lands on a crop anchor, including buffered work on the previous acre`);
      }
    }
  }
}
const waiting={x:6,y:3,path:[],phase:'working',clock:0,elapsed:7,facing:1,fieldCycle:true};
assert(!render(waiting,'farming',false).some(mark=>stages.some(([,color])=>mark.color===color)),
  'A host-owned field without confirmed work cannot display a stale harvesting pose');
for(const farmAction of ['plowing','sowing','watering']){
  const state={x:6,y:3,path:[],phase:'working',clock:0,facing:1,
    elapsed:0,farmAction,source:{x:6,y:3}};
  const original=JSON.stringify(state),body=elapsed=>{state.elapsed=elapsed;return render(state).find(mark=>mark.method==='fillRect'&&mark.args.join(',')==='-4,-13,8,10').center;};
  const beginning=body(0),during=body(.5),ending=body(2);
  assert.strictEqual(during[0],beginning[0],'Work never slides the tender horizontally');
  assert(Math.abs(during[1]-beginning[1])<=(farmAction==='plowing'?2:1),'Only the working pose can bob vertically');
  assert.deepStrictEqual(ending,beginning,'The tender keeps its actual destination at action completion');
  state.elapsed=0;assert.strictEqual(JSON.stringify(state),original,'Drawing offsets never mutate confirmed worker coordinates');
}
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

const crops=new Function('ctx','polygon','point','tileW','tileH','world','WorkerAnimation',
  source.slice(source.indexOf('function buildingLevel('),source.indexOf('function storageCapacity('))+
  source.slice(source.indexOf('function drawFarmCrops('),source.indexOf('function drawBuilding('))+
  '\nreturn drawFarmCrops;'
)(ctx,(points,color)=>marks.push({method:'polygon',points,color}),
  (x,y)=>[(x-y)*23,(x+y)*11.5],46,23,{progression:{farming_growth_seconds:15}},animation);
function fieldAt(stage,elapsed=0,crop='wheat',progress=0){
  marks.length=0;crops({...fields[0],crop,field:{stage,elapsed,crop,progress}},null);return marks.slice();
}
const soil=fieldAt('unplowed'),sown=fieldAt('sprouting'),growing=fieldAt('growing',7.5),ripe=fieldAt('ripe'),cut=fieldAt('ripe',0,'wheat',1);
assert(!soil.some(mark=>mark.color==='#efd59a'||mark.color==='#8db568'),'Plowing leaves bare furrows');
assert.strictEqual(sown.filter(mark=>mark.method==='stroke'&&mark.color==='#8db568').length,16,'Seeded fields start sprouting');
assert(growing.some(mark=>mark.color==='#8db568'),'Watered crops continue growing green shoots');
assert.notDeepStrictEqual(growing,fieldAt('growing',7.75),'Crop size follows saved growth progress');
const grain=output=>output.filter(mark=>mark.method==='stroke'&&['#d9bd70','#b8b458'].includes(mark.color));
assert.strictEqual(grain(ripe).length,16,'Harvest starts with ripe grain');
assert.strictEqual(grain(cut).length,8,'Harvest gradually cuts the grain');
assert.strictEqual(grain(fieldAt('ripe')).length,16,'Unattended ripe fields keep their saved appearance');
const visibleField={...fields[0],crop:'wheat',field:{stage:'sprouting',elapsed:0,progress:0,crop:'wheat'}};
const waitingForSeeds={playbackQueue:[{index:0,samples:[{phase:'working',farmAction:'sowing',
  farmField:{x:visibleField.x,y:visibleField.y,stage:'plowed',elapsed:0,progress:0,crop:'wheat'}}]}]};
const delayedPlants=[];
crops(visibleField,waitingForSeeds,plant=>delayedPlants.push(plant));
assert.strictEqual(delayedPlants.length,0,'The renderer waits for sowing playback before showing seedlings');
waitingForSeeds.playbackQueue=[];
crops(visibleField,waitingForSeeds,plant=>delayedPlants.push(plant));
assert.strictEqual(delayedPlants.length,16,'Completed playback restores the seeded field');
const ripeCrops=['vegetables','wheat','carrots','fodder'].map(crop=>fieldAt('ripe',0,crop));
for(let i=0;i<ripeCrops.length;i++)for(let j=i+1;j<ripeCrops.length;j++){
  assert.notDeepStrictEqual(ripeCrops[i],ripeCrops[j],'Each crop has distinct ripe artwork');
}
assert(ripeCrops[0].some(mark=>mark.method==='polygon'&&mark.color==='#68934e'),'Vegetables have green leafy heads');
assert(ripeCrops[1].some(mark=>mark.method==='polygon'&&mark.color==='#efd59a'),'Wheat has golden ears');
assert(ripeCrops[2].some(mark=>mark.method==='polygon'&&mark.color==='#dd9048'),'Carrots show orange roots');
assert(ripeCrops[3].some(mark=>mark.method==='stroke'&&mark.color==='#72a65c'),'Fodder has green grass tufts');
assert.deepStrictEqual(fieldAt('ripe',0,null),ripeCrops[0],'Fields without a crop use vegetables');
assert.deepStrictEqual(fieldAt('ripe',0,'unknown'),ripeCrops[0],'Unknown crop types fall back to vegetables');
for(const crop of ['vegetables','carrots','fodder']){
  assert.deepStrictEqual(fieldAt('unplowed',0,crop),soil,'Plowing preserves bare soil for every crop');
  assert(fieldAt('sprouting',0,crop).some(mark=>mark.color==='#8db568'),'Every seeded crop starts sprouting');
  assert.notDeepStrictEqual(fieldAt('growing',7.5,crop),fieldAt('growing',7.75,crop),'Each crop grows without a tender');
  const collected=[];
  crops({...fields[0],crop,field:{stage:'ripe',elapsed:0,progress:1,crop}},null,plant=>collected.push(plant));
  assert.strictEqual(collected.length,8,'Harvest removes half of each crop at mid-cycle');
  assert(collected.every(plant=>plant.cropType===crop),'Depth-sorted plants retain their crop identity');
}
console.log('Farming animation checks passed');
