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
// A fresh browser has no visible route to preserve. Planning must not overwrite
// its saved offshore position with a stale delivery or construction position.
for(const confirmed of [false,true]){
  const reloadClient=client();
  reloadClient.run(`
    const finish=steps=>{let r;do{r=steps.next();}while(!r.done);return r.value;};
    const water=[[3,2],[3,3],[3,4],[4,4],[5,4],[5,3],[5,2]].map(([x,y])=>({kind:'water',x,y}));
    const worker={name:'Ada',job:'fishing',building_index:1,work_target:water.at(-1),
      production_status:'Sailing',logistics_position:{x:2,y:3},construction_position:{x:1,y:8},
      work_animation:{task:['fishing'],version:1,x:5,y:3.25,phase:'returning',elapsed:0,facing:1,
        path:[{x:5,y:4},{x:4,y:4},{x:3,y:4},{x:3,y:3}],
        playback:${confirmed}?[{time:0,x:5,y:3.5,phase:'returning',elapsed:0},
          {time:3,x:5,y:3.25,phase:'returning',elapsed:0}]:[]}};
    const town={map_size:16,terrain:Array(16).fill('g'.repeat(16)),map_resources:water,npcs:[],
      buildings:[{kind:'hall',x:1,y:6},{kind:'fishing_hut',x:2,y:2,profession_upgrades:['boat']}],villagers:[worker]};
    world={world_id:'reload',towns:{own:town}};viewed='own';
    ClientScene.commit(finish(ClientScene.prepare(world,null)));
    const saved=JSON.stringify(worker);
    const check=()=>{const state=workers.get(0);
      if(state.x!==5||state.y!==3.25)throw Error('Reloaded fisher walked from an old task position');};
    animatedVillagers(town,0);check();
    ClientScene.queue.run();check();
    animatedVillagers(town,0);check();
    if(workers.get(0).phase!=='returning')throw Error('Reloaded fisher lost the host work phase');
    if(JSON.stringify(worker)!==saved)throw Error('Reload changed authoritative worker state');

    for(const job of ['delivery','construction']){
      workers.clear();ClientScene.queue.clear();
      delete worker.logistics_target;delete worker.logistics_trip;delete worker.construction_target;
      if(job==='delivery'){
        worker.logistics_target={kind:'delivery',x:1,y:3};
        worker.logistics_trip={path:[[2,3],[1,3]],step:0};
      }else worker.construction_target={kind:'home',x:1,y:7};
      animatedVillagers(town,0);
      const expected=job==='delivery'?worker.logistics_position:worker.construction_position;
      const checkTask=()=>{const state=workers.get(0);
        if(state.x!==expected.x||state.y!==expected.y)throw Error('Reload ignored active '+job+' position');};
      checkTask();ClientScene.queue.run();checkTask();
    }
  `);
}
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

// One simulation tick can contain actions on multiple acres.
const batchState={x:acre.x,y:acre.y,clock:0};
Animation.playback(batchState,sowing(1,acre),0);
const batch={...host(2,[fieldSample(0,acre,'ripe','working','harvesting'),
  fieldSample(1,acre,'unplowed','harvest_ready',null),
  {...fieldSample(1,secondAcre,'ripe','walking',null),x:acre.x},
  fieldSample(2,secondAcre,'ripe','working','harvesting'),
  fieldSample(3,secondAcre,'unplowed','harvest_ready',null)]),task:firstSowing.task};
const batchSnapshot=JSON.stringify(batch);
Animation.playback(batchState,batch,.5);
assert.equal(Animation.fieldPresentation(batchState,acre).stage,'ripe');
assert.equal(Animation.fieldPresentation(batchState,secondAcre).stage,'ripe','Future acres in the same tick retain their crops');
Animation.playback(batchState,batch,2);
assert.equal(Animation.fieldPresentation(batchState,acre),undefined,'Finished acres release their presentation');
assert.equal(Animation.fieldPresentation(batchState,secondAcre).stage,'ripe');
Animation.playback(batchState,batch,.5);
assert.equal(Animation.fieldPresentation(batchState,secondAcre),undefined);
assert.equal(JSON.stringify(batch),batchSnapshot,'Batch presentation preserves authoritative snapshots');

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

// A move must replace queued travel to the old workplace without teleporting.
const movedState={x:10.25,y:2,clock:0};
const oldTask=['woodcutting','woodcutter',9,3,'tree',12,3];
const oldHost={...host(1,[sample(0,10.25,2),sample(3,9,2,'depositing')]),task:oldTask};
Animation.playback(movedState,oldHost,0);
Animation.playback(movedState,{...oldHost,version:2},0);
assert.equal(movedState.playbackQueue.length,1);
const relocated={...host(3,[sample(0,10.25,2),sample(3,8,3,'depositing')]),
  task:['woodcutting','woodcutter',8,4,'tree',12,3]};
const relocationSnapshot=JSON.stringify(relocated);
Animation.playback(movedState,relocated,0);
assert.equal(movedState.x,9); // Initial host pose was the end of oldHost.
assert.equal(movedState.playbackQueue.length,1,'Relocation discards the obsolete queued return');
assert.equal(movedState.playbackQueue[0].hostSamples.at(-1).x,8);
assert.equal(JSON.stringify(relocated),relocationSnapshot);
const movedField={...relocated,version:4,relocation:1,
  task:['woodcutting','woodcutter',8,4,'tree',13,3]};
Animation.playback(movedState,movedField,0);
assert.equal(movedState.playbackQueue.length,1,'Moving only a linked source also discards its obsolete route');
