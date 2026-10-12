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
