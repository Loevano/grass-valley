const assert=require('node:assert/strict');
const Animation=require('../web/animations');
const Scene=require('../web/scene');
const client=require('./client_harness')();
client.run(`
  const finish=steps=>{let r;do{r=steps.next();}while(!r.done);return r.value;};
  const tree={kind:'tree',x:12,y:3};
  const worker={name:'Ada',job:'woodcutting',building_index:1,work_target:tree,production_status:'Working'};
  const town={map_size:24,terrain:Array(24).fill('g'.repeat(24)),map_resources:[tree],npcs:[],
    buildings:[{kind:'hall',x:5,y:5},{kind:'woodcutter',x:12,y:5}],villagers:[worker]};
  world={world_id:'retained-paths',towns:{own:town}};viewed='own';
  const commit=()=>ClientScene.commit(finish(ClientScene.prepare(world,ClientScene.current)));
  commit();animatedVillagers(town,0);ClientScene.queue.run();
  const state=workers.get(0),original=state.path;
  town.map_resources.push({kind:'rock',x:20,y:20});
  town.roads=[{x:state.x,y:state.y}];
  commit();animatedVillagers(town,0);
  if(workers.get(0)!==state || state.path!==original || ClientScene.queue.jobs.has('plan/0'))
    throw Error('Unrelated geometry or roads discard a valid route');
  const obstacle={kind:'rock',...state.path[2]};
  town.map_resources.push(obstacle);commit();animatedVillagers(town,0);
  if(state.path!==original)throw Error('A future obstruction replans before it is reached');
  const snapshot=JSON.stringify(world);
  animatedVillagers(town,2);ClientScene.queue.run();
  if(state.x===obstacle.x && state.y===obstacle.y)throw Error('Worker entered a blocked waypoint');
  animatedVillagers(town,0);ClientScene.queue.run();
  const detour=workers.get(0);
  if(detour.path===original || detour.path.some(p=>p.x===obstacle.x&&p.y===obstacle.y))
    throw Error('A blocked next step does not produce a detour');
  if(JSON.stringify(world)!==snapshot)throw Error('Navigation changed the authoritative snapshot');
  tree.x=13;commit();animatedVillagers(town,0);ClientScene.queue.run();
  if(workers.get(0).source.x!==13)throw Error('Changing the task retains an obsolete destination');
`);
let probes=0;
const blocked=new Set(['5,5','12,5','12,3']);
const town={map_size:24,terrain:Array(24).fill('g'.repeat(24)),map_resources:[{kind:'tree',x:12,y:3}],
  buildings:[{kind:'hall',x:5,y:5},{kind:'woodcutter',x:12,y:5}],npcs:[],navigation:{
    overlay(){const overlay=new Scene.Overlay(blocked),has=overlay.has.bind(overlay);
      overlay.has=k=>{probes++;return has(k);};return overlay;},roads:new Set()}};
Animation.plan(town,{job:'woodcutting',building_index:1,work_target:town.map_resources[0],
  work_animation:{playback:[{time:0,x:3,y:8},{time:3,x:7,y:8}]}},0);
assert(probes<10,'Confirmed host playback does not perform a duplicate browser route search');
console.log('Valid path retention, blocked-step detours, task changes, and snapshot isolation passed');

// Fallback replanning must retain the visible pose when a workplace moves,
// changes its footprint, or disappears from a new authoritative snapshot.
for(const edit of ['move','footprint','remove']){
  const buildings=[{kind:'hall',x:5,y:5},{kind:'woodcutter',x:12,y:5}];
  const resource={kind:'tree',x:12,y:3};
  const snapshot={map_size:24,terrain:Array(24).fill('g'.repeat(24)),map_resources:[resource],buildings,npcs:[]};
  const worker={job:'woodcutting',building_index:1,work_target:resource};
  const visual=Animation.plan(snapshot,worker,0);
  Object.assign(visual,{x:9.25,y:3.5});
  const next=JSON.parse(JSON.stringify(snapshot));
  if(edit==='move')next.buildings[1].x=14;
  if(edit==='footprint')next.buildings[1].footprint=[2,2];
  if(edit==='remove'){next.buildings.pop();delete worker.building_index;worker.job='idle';worker.work_target=null;}
  assert.equal(Animation.refreshNavigation(visual,next,worker),false,edit+' invalidates the old destination');
  visual.needsPlan=true;
  const replanned=Animation.plan(next,worker,0,visual);
  assert.equal(replanned.x,9.25);
  assert.equal(replanned.y,3.5);
  if(edit==='move')assert.equal(replanned.home.x,14);
}
