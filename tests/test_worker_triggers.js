// Run with: node tests/test_worker_triggers.js
const assert=require('assert'),fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8');
const WorkerAnimation=require('../web/animations.js');
const colors=[];
const ctx=new Proxy({fillRect(){colors.push(this.fillStyle);}}, {get:(target,key)=>key in target?target[key]:()=>{}});
const harness=new Function('WorkerAnimation','ctx','polygon','point','tileW','tileH','npcColors',`
  let world,viewed='own',workerTown=null;
  const workers=new Map();
  ${source.slice(source.indexOf('function buildingLevel('),source.indexOf('function upgradeBenefit('))}
  ${source.slice(source.indexOf('function animatedVillagers('),source.indexOf('let npcClock='))}
  ${source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))}
  return {setWorld(value){world=value;},frame(town,seconds){
    const sprites=animatedVillagers(town,seconds);sprites.forEach(drawVillager);return sprites;
  }};
`)(WorkerAnimation,ctx,(_,color)=>colors.push(color),(x,y)=>[(x-y)*23,(x+y)*11.5],46,23,{});
const town={map_size:12,food:40,wood:40,buildings:[
  {kind:'hall',x:5,y:5},{kind:'farm',x:3,y:3},{kind:'acre',x:3,y:4,farm_index:1},
  {kind:'fishing_hut',x:8,y:5},{kind:'home',x:11,y:11,construction:{progress:0,total:30}}],
  map_resources:[{kind:'water',x:3,y:2},{kind:'water',x:9,y:5},
    {kind:'water',x:9,y:4},{kind:'water',x:10,y:5},{kind:'water',x:9,y:6}],
  villagers:[{job:'farming',work_target:{kind:'acre',x:3,y:4,farm_index:1}},
    {job:'fishing',work_target:{kind:'water',x:9,y:5}}]};
let world={world_id:'trigger-test',towns:{own:town}};
harness.setWorld(world);
let sprites=harness.frame(town,0),farmer=sprites[0].state,fisher=sprites[1].state;
assert.strictEqual(fisher.phase,'walking','Fisher can reach water when the hut occupies the only bank');
assert(fisher.pier.some(tile=>tile.x===9&&tile.y===5),'Pier reaches the fishing target');
assert(fisher.path.some(tile=>tile.x===8&&tile.y===5),'Fisher walks through the hut onto its pier');
const stages=new Set(),farmingColors=new Set(),fishingPhases=new Set();
for(let frame=0;frame<900;frame++){
  if(frame%20===0){
    world=JSON.parse(JSON.stringify(world));
    world.towns.own.buildings[4].construction.progress+=1;
    harness.setWorld(world);
  }
  colors.length=0;
  sprites=harness.frame(world.towns.own,.1);
  assert.strictEqual(sprites[0].state,farmer,'Polling and construction progress preserve the farming cycle');
  assert.strictEqual(sprites[1].state,fisher,'Polling and construction progress preserve the fishing cycle');
  if(farmer.phase==='working'){
    stages.add(Math.min(3,Math.floor(farmer.elapsed/2)));
    for(const color of colors)farmingColors.add(color);
  }
  fishingPhases.add(fisher.phase);
  if(fisher.phase==='working'){
    assert.strictEqual(fisher.x,9,'Fishing starts at the end of the pier');
    assert.strictEqual(fisher.y,5);
  }
}
assert.deepStrictEqual([...stages].sort(),[0,1,2,3],'Browser update loop triggers all four farming stages');
for(const color of ['#aab9b4','#efd59a','#9fd8e5','#e5c77f']){
  assert(farmingColors.has(color),'Every farming action is actually rendered');
}
assert(fishingPhases.has('working')&&fishingPhases.has('depositing'),'Fisher fishes and delivers after reaching the pier');
// Moving a source without changing the resource count must invalidate the route.
world.towns.own.map_resources[0].x=2;
harness.frame(world.towns.own,0);
assert.notStrictEqual(harness.frame(world.towns.own,0)[0].state,farmer,'Changed terrain resources trigger replanning');
console.log('Worker trigger checks passed');
