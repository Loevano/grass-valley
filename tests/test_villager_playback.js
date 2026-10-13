const assert=require('node:assert/strict');
const Animation=require('../web/animations.js');
const client=require('./client_harness');
const sample=(time,x,y,phase='walking')=>({time,x,y,phase,elapsed:0,facing:1});
const host=(version,playback)=>({version,task:['woodcutting'],...playback.at(-1),path:[],playback});
const state={x:0,y:0,clock:0};
Animation.playback(state,host(1,[sample(0,0,0),sample(3,0,0)]),0);
const update=host(2,[sample(0,0,0),sample(1,1,0),sample(2,1,1),sample(3,2,1)]);
const unchanged=JSON.stringify(update);
Animation.playback(state,update,0);
assert.equal(state.x,0,'A new snapshot cannot jump the visible position');
Animation.playback(state,update,.5);assert.equal(state.x,.5);assert.equal(state.y,0);
Animation.playback(state,update,1);assert.equal(state.x,1);assert.equal(state.y,.5,'Playback follows corners rather than cutting diagonally');
Animation.playback(state,update,1.5);assert.equal(state.x,2);assert.equal(state.y,1);
Animation.playback(state,update,5);assert.equal(state.x,2,'Missing updates hold the last confirmed position');
assert.equal(JSON.stringify(update),unchanged,'Playback never mutates authoritative snapshots');
Animation.playback(state,host(3,[sample(0,2,1,'working'),sample(3,2,1,'working')]),0);
Animation.playback(state,update,0);assert.equal(state.hostVersion,3,'Older samples cannot rewind playback');
const h=client();
h.run(`
  const town={map_size:20,terrain:Array(20).fill('g'.repeat(20)),map_resources:[],npcs:[],
    buildings:[{kind:'hall',x:5,y:5}],villagers:[{name:'Ada',job:'idle'},{name:'Bea',job:'idle'}]};
  world={world_id:'roster',towns:{own:town}};viewed='own';
  animatedVillagers(town,0);const bea=workers.get(1);
  town.villagers.push({name:'Cal',job:'idle'});animatedVillagers(town,0);
  if(workers.get(1)!==bea)throw Error('Arrival resets existing workers');
  town.villagers.shift();animatedVillagers(town,0);
  if(workers.get(0)!==bea)throw Error('Departure changes surviving worker identity');
`);
console.log('Confirmed villager playback and stable roster checks passed');
const sceneClient=client();
sceneClient.run(`
  const finish=steps=>{let r;do{r=steps.next();}while(!r.done);return r.value;};
  const sample=(time,x,y)=>({time,x,y,phase:'walking',elapsed:0,facing:1});
  const worker={name:'Ada',job:'woodcutting',building_index:1,work_target:{kind:'tree',x:8,y:2},
    production_status:'Working',work_animation:{task:['woodcutting'],version:1,x:3,y:8,path:[],phase:'walking',elapsed:0,
      playback:[sample(0,3,8),sample(3,3,8)]}};
  const town={map_size:20,terrain:Array(20).fill('g'.repeat(20)),map_resources:[worker.work_target],npcs:[],
    buildings:[{kind:'hall',x:5,y:5},{kind:'woodcutter',x:8,y:4}],villagers:[worker]};
  world={world_id:'playback-replan',towns:{own:town}};viewed='own';
  ClientScene.commit(finish(ClientScene.prepare(world,null)));
  animatedVillagers(town,0);ClientScene.queue.run();
  worker.work_animation={...worker.work_animation,version:2,x:6,y:8,
    playback:[sample(0,3,8),sample(3,6,8)]};
  animatedVillagers(town,.5);
  const x=workers.get(0).x;
  town.map_resources.push({kind:'rock',x:10,y:10});
  ClientScene.commit(finish(ClientScene.prepare(world,ClientScene.current)));
  animatedVillagers(town,.5);const visible=workers.get(0).x;ClientScene.queue.run();
  if(Math.abs(visible-x-.5)>1e-9)throw Error('A geometry update interrupts confirmed playback');
  if(workers.get(0).x!==visible)throw Error('A queued replan jumps the visible sprite');
`);

state.playbackQueue=[];state.hostVersion=undefined;state.hostTask=null;
const restarting=host(1,[sample(0,8,1),sample(3,9,1)]);
const previousX=state.x;
Animation.playback(state,restarting,0);
assert.equal(state.x,previousX,'Restarting a paused job cannot snap to a new host position');
Animation.playback(state,restarting,.1);
assert(state.x>previousX,'A restarted host cycle can resume after its version resets');

// The host can finish one acre and assign the next while the browser still sows.
const farmerState={x:0,y:0,clock:0};
const acre={x:3,y:4,field:{stage:'sprouting',elapsed:3,cycle:0}};
const secondAcre={x:4,y:4,field:{stage:'sprouting',elapsed:0,cycle:0}};
const fieldSample=(time,field,stage,phase='working',farmAction='sowing')=>({
  ...sample(time,field.x,field.y,phase),farmAction,
  farmField:{x:field.x,y:field.y,stage,elapsed:0,progress:0,cycle:0,crop:'wheat'}});
const sowing=(version,field)=>({...host(version,[fieldSample(0,field,'plowed'),
  fieldSample(2,field,'sprouting','growing',null),fieldSample(3,field,'sprouting','growing',null)]),
  task:['farming','farm',3,3,'acre',field.x,field.y]});
Animation.playback(farmerState,sowing(1,acre),0);
assert.equal(Animation.fieldPresentation(farmerState,acre),undefined,'Initial loading uses the current field');
const firstSowing=sowing(2,acre),secondSowing=sowing(3,secondAcre);
const snapshots=JSON.stringify([firstSowing,secondSowing]);
Animation.playback(farmerState,firstSowing,1);
Animation.playback(farmerState,secondSowing,0);
assert.equal(Animation.fieldPresentation(farmerState,acre).stage,'plowed','Seeding waits for the visible action');
assert.equal(Animation.fieldPresentation(farmerState,secondAcre).stage,'plowed','A queued acre waits for arrival');
Animation.playback(farmerState,secondSowing,1);
assert.equal(Animation.fieldPresentation(farmerState,acre).stage,'sprouting','Seedlings appear at action completion');
Animation.playback(farmerState,secondSowing,5);
assert.equal(Animation.fieldPresentation(farmerState,acre),undefined,'Finished fields resume independent growth');
assert.equal(JSON.stringify([firstSowing,secondSowing]),snapshots,'Crop presentation preserves authoritative snapshots');

const harvest={...host(4,[fieldSample(0,acre,'ripe','working','harvesting'),
  fieldSample(2,acre,'unplowed','harvest_ready','harvesting'),
  fieldSample(3,acre,'unplowed','harvest_ready','harvesting')]),task:firstSowing.task};
acre.field={stage:'unplowed',cycle:1,elapsed:0,progress:0};
Animation.playback(farmerState,harvest,1.9+1/1.8);
assert.equal(Animation.fieldPresentation(farmerState,acre).stage,'ripe','Ripe crops remain until the sickle animation finishes');
Animation.playback(farmerState,harvest,.2);
assert.equal(Animation.fieldPresentation(farmerState,acre).stage,'unplowed','Harvest completion clears the crop');

// A missing update must not compress several tiles into the next short step.
const delayed={x:0,y:0,clock:0};
Animation.playback(delayed,host(1,[sample(0,0,0),sample(3,0,0)]),0);
const missing=host(3,[sample(0,4,0),sample(.2,4.36,0),sample(3,4.36,0,'working')]);
const missingSnapshot=JSON.stringify(missing);
Animation.playback(delayed,missing,.1);
assert(Math.abs(delayed.x-.18)<1e-9,'A missed update reconnects at normal walking speed');
assert.equal(JSON.stringify(missing),missingSnapshot,'Reconnecting does not retime the authoritative snapshot');

// Hosts can emit multiple action changes at time zero when resuming a job.
const resumed={x:0,y:0,clock:0,playbackQueue:[],hostVersion:undefined};
const resuming={...host(1,[sample(0,5,0,'idle'),sample(0,5,0,'working'),sample(2,5,0,'working')]),task:['farming']};
Animation.playback(resumed,resuming,0);
assert.equal(resumed.x,0,'Duplicate time-zero samples cannot teleport a resumed farmer');
Animation.playback(resumed,resuming,.1);
assert(Math.abs(resumed.x-.18)<1e-9,'Resumed workers walk before starting their action');
assert.equal(resumed.phase,'walking');

// Trimming a backed-up tab must reconnect from its visible position, with time
// for the gap, without retaining the connector for the discarded animations.
const backlog={x:0,y:0,clock:0};
Animation.playback(backlog,host(1,[sample(0,0,0),sample(3,0,0)]),0);
for(let version=2;version<=7;version++){
  const x=version*3;
  Animation.playback(backlog,host(version,[sample(0,x,0),sample(0,x,0,'working'),sample(3,x,0,'working')]),0);
  assert.equal(backlog.x,0,'Queue trimming preserves the visible position');
  assert(backlog.playbackQueue.length<=3,'Playback history stays bounded');
}
Animation.playback(backlog,host(7,[sample(0,21,0),sample(0,21,0,'working'),sample(3,21,0,'working')]),.1);
assert(Math.abs(backlog.x-.18)<1e-9,'Trimmed playback reconnects at walking speed');
