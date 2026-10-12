const assert=require('node:assert/strict');
const Scene=require('../web/scene.js');
const Animation=require('../web/animations.js');
const finish=steps=>{let r;do{r=steps.next();}while(!r.done);return r.value;};
function snapshot(resources=[],buildings=[]){
  const terrain=Array(240).fill('g'.repeat(240));
  const town={chunked:true,map_size:240,town_origin:{x:80,y:80},terrain,map_resources:resources,npcs:[],
    buildings:[{kind:'hall',x:80,y:80},...buildings],villagers:[{name:'Ada',job:'idle'}]};
  return {world_id:'test',shared_map:{chunked:true,map_size:240,terrain,map_resources:resources,npcs:[]},towns:{own:town}};
}
const initial=snapshot([{kind:'tree',x:82,y:82}]);
const first=finish(Scene.prepare(initial,null));Scene.commit(first);
const region=first.regions.get('own');
assert(region.blocked.has('82,82'));
const overlay=region.routing.navigation.overlay();overlay.delete('82,82');overlay.add('83,83');
assert(!overlay.has('82,82'));assert(overlay.has('83,83'));
assert(region.blocked.has('82,82'));assert(!region.blocked.has('83,83'),'Worker exceptions never mutate shared occupancy');
const updated=snapshot([{kind:'tree',x:82,y:82,supply:3},{kind:'rock',x:230,y:230}]);
const second=finish(Scene.prepare(updated,first));
assert.strictEqual(second.regions.get('own').blocked,region.blocked,'Supply and distant geometry preserve local navigation');
assert.strictEqual(second.regions.get('own').layoutVersion,region.layoutVersion,'Unchanged geometry retains the worker layout identity across snapshots');
assert.equal(typeof region.layoutVersion,'object','Worker frames compare identity instead of large geometry strings');
updated.shared_map.npcs=[{kind:'npc',x:84,y:84}];
const third=finish(Scene.prepare(updated,second));
assert.notStrictEqual(third.regions.get('own').blocked,region.blocked,'Nearby NPC movement changes navigation');
assert(third.regions.get('own').blocked.has('84,84'));
assert.notStrictEqual(third.regions.get('own').layoutVersion,region.layoutVersion,'Nearby geometry invalidates the worker layout identity');
const roadUpdate=snapshot([{kind:'tree',x:82,y:82}]);
roadUpdate.towns.own.roads=[{x:81,y:81}];
const roadScene=finish(Scene.prepare(roadUpdate,second));
assert.strictEqual(roadScene.regions.get('own').blocked,region.blocked,'Road updates retain occupancy');
assert.notStrictEqual(roadScene.regions.get('own').layoutVersion,region.layoutVersion,'Completed roads invalidate worker route costs');
const removal=snapshot([]);const removed=finish(Scene.prepare(removal,third));
assert(!removed.regions.get('own').blocked.has('82,82'),'Evicted resources leave navigation');
assert.equal(removed.resources.length,0);assert.equal(removed.buckets.size,0);
const another=snapshot();another.world_id='another';
assert.notStrictEqual(finish(Scene.prepare(another,removed)).regions.get('own').blocked,removed.regions.get('own').blocked);

// Expensive searches yield and still produce the same route as synchronous callers.
const blocked=new Set();const steps=Animation.routeSteps({x:0,y:0},{x:119,y:119},blocked,120,false);
let yields=0,result;do{result=steps.next();if(!result.done)yields++;}while(!result.done);
assert(yields>10);assert.deepEqual(result.value,Animation.route({x:0,y:0},{x:119,y:119},blocked,120,false));
let ticks=0,obsolete=false,completed=false;
const queue=new Scene.Queue(()=>ticks++);
function* slow(){for(let i=0;i<100;i++)yield;return 42;}
queue.schedule('worker','old',slow(),()=>{obsolete=true;});
queue.run(2);assert.equal(queue.jobs.size,1,'Preparation budget yields before completion');
queue.schedule('worker','new',slow(),value=>{completed=value===42;});
for(let i=0;i<110;i++)queue.run(2);
assert(completed);assert(!obsolete,'Superseded work never publishes');
queue.schedule('worker','cancelled',slow(),()=>{obsolete=true;});queue.clear();queue.run();assert(!obsolete);
let fair=[];const roundRobin=new Scene.Queue(()=>0);
function* job(label){for(let i=0;i<2;i++){fair.push(label);yield;}}
roundRobin.schedule('a',1,job('a'),()=>{});roundRobin.schedule('b',1,job('b'),()=>{});roundRobin.run();
assert.deepEqual(fair,['a','b','a','b'],'Long searches do not starve other workers');

const resources=Array.from({length:50000},(_,i)=>({kind:'tree',x:150+i%80,y:150+Math.floor(i/80)}));
const barn={kind:'barn',x:80,y:80,animals:{pig:1}};
const world=snapshot(resources,[barn]);const scene=finish(Scene.prepare(world));Scene.commit(scene);
let scans=0;resources.filter=()=>{scans++;throw Error('Global resource scan');};
const routes=Scene.animalRoutes(scene.buildings,world.shared_map,barn);
for(let i=0;i<120;i++)assert.strictEqual(Scene.animalRoutes(scene.buildings,world.shared_map,barn),routes);
let survived=false;Scene.queue.schedule('ongoing',1,slow(),()=>{survived=true;});
const same=finish(Scene.prepare(world,scene));Scene.commit(same);
assert.equal(Scene.queue.jobs.size,1,'Unchanged snapshots preserve ongoing searches instead of starving workers');
Scene.queue.clear();
assert.equal(scans,0,'Warm animal frames never filter the global resource array');
assert.equal(scene.regions.get('own').routing.map_resources.length,0,'Worker plans exclude distant resources');
const worker=Animation.plan(scene.regions.get('own').routing,world.towns.own.villagers[0],0);
assert.strictEqual(worker.blocked.base,scene.regions.get('own').blocked,'Worker planning shares terrain and obstacle indexes');

const hut={kind:'fishing_hut',x:15,y:15};
const pierWorld=snapshot([
  {kind:'water',x:16,y:15},{kind:'water',x:17,y:15},
  {kind:'rock',x:15,y:16},{kind:'tree',x:230,y:230}
],[hut]);
const pierScene=finish(Scene.prepare(pierWorld,null));
const pierTown= pierScene.regions.get('own').routing;
// This hut is outside the town's worker region; artwork still uses map buckets.
const artworkTown={...pierWorld.towns.own,resourceBuckets:pierScene.buckets};
const expectedPier=Animation.fishingPier({...artworkTown,resourceBuckets:null},hut);
assert(expectedPier.length>1);
artworkTown.map_resources.filter=()=>{throw Error('Pier scanned global resources');};
assert.deepEqual(Animation.fishingPier(artworkTown,hut),expectedPier,'Indexed piers preserve routes across bucket boundaries');
assert.strictEqual(pierTown.resourceBuckets,pierScene.buckets,'Worker piers share the resource index');
artworkTown.terrain[15]='g'.repeat(16)+'v'+'g'.repeat(223);
assert.deepEqual(Animation.fishingPier(artworkTown,hut),[],'Terrain changes still block indexed piers');

const fs=require('node:fs');
const app=fs.readFileSync('web/app.js','utf8');
new Function('assert','ClientScene','WorkerAnimation',`
  const world={world_id:'pending',towns:{}},viewed='own',workers=new Map();let workerTown=null;
  const town={buildings:[{kind:'hall',x:5,y:5}],map_resources:[],villagers:[{name:'Ada',job:'idle',production_status:'Paused'}]};
  const layout='large local map geometry'.repeat(10000);
  const region={routing:town};ClientScene.region=()=>region;
  ClientScene.current={tasks:new WeakMap([[town.villagers[0],{task:'idle',signature:'first'}]])};
  ${app.slice(app.indexOf('function animatedVillagers('),app.indexOf('function townWorkerLayout('))}
  animatedVillagers(town,0,'own',layout);
  const pending=workers.get(0).pendingPlan,job=ClientScene.queue.jobs.get('plan/0');
  assert.equal(pending.layout,layout);
  assert.equal(pending.signature,'first');
  animatedVillagers(town,0,'own',layout);
  assert.strictEqual(ClientScene.queue.jobs.get('plan/0'),job,'Unchanged frames retain unfinished plans');
  ClientScene.current.tasks.set(town.villagers[0],{task:'idle',signature:'second'});
  animatedVillagers(town,0,'own',layout);
  assert.notStrictEqual(ClientScene.queue.jobs.get('plan/0'),job,'Changed tasks replace obsolete plans');
`)(assert,{queue:new Scene.Queue()},Animation);
console.log('Scene invalidation, bounded scheduling, and navigation checks passed');
