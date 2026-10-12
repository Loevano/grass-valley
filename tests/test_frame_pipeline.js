const assert=require('node:assert/strict');
const client=require('./client_harness')();
client.run(`
  const terrain=Array(120).fill('g'.repeat(120));
  const town={map_size:120,terrain,map_resources:[],npcs:[],roads:[],level:1,wood:65,food:40,
    buildings:[{kind:'hall',x:30,y:30}],villagers:[{name:'Ada',job:'idle'}]};
  world={world_id:'pipeline',shared_map:{map_size:120,terrain,map_resources:[],npcs:[]},towns:{own:town}};
  viewed='own';identity={town_id:'own'};cameraTown='pipeline/shared';camera={x:30,y:30,zoom:1};
  const prepared=ClientScene.prepare(world);let ready;do{ready=prepared.next();}while(!ready.done);ClientScene.commit(ready.value);
  let updates=0;const previousUpdate=updateActors;updateActors=function(seconds){updates++;return previousUpdate(seconds);};
`);
const before=client.paints;
client.run('render();render();draw();draw();');
assert.equal(client.paints,before,'UI callbacks never draw between animation frames');
client.frame(0);
assert.equal(client.paints,before+1,'One browser frame draws once');
assert.equal(client.run('updates'),1,'Actors advance once per browser frame');
client.frame(16);
assert.equal(client.paints,before+2);assert.equal(client.run('updates'),2);
client.run('addBuildingEffect({x:30,y:30},"build");');
client.frame(116);
assert.equal(client.run('buildingEffects[0].age'),.1,'Feedback effects advance in the update phase');
client.run('render();');
assert.equal(client.run('buildingEffects[0].age'),.1,'UI updates cannot advance effects twice');
for(let time=216;time<=1416;time+=100)client.frame(time);
assert.equal(client.run('buildingEffects.length'),0,'Feedback effects expire even though drawing has no time step');
client.run(`
  let stale=false;
  function* slow(){while(true)yield;}
  ClientScene.queue.schedule('obsolete',1,slow(),()=>{stale=true;});
  const next={...world,world_id:'different'};
  const steps=ClientScene.prepare(next);let result;do{result=steps.next();}while(!result.done);ClientScene.commit(result.value);
`);
assert.equal(client.run('ClientScene.queue.jobs.size'),0,'Scene publication cancels obsolete work');
assert.equal(client.run('stale'),false);
client.run('lastFrame=9000;');client.listeners.get('visibilitychange')();
assert.equal(client.run('lastFrame'),null,'Returning to a tab cannot accumulate simulation debt');
console.log('Full client frame ownership and publication checks passed');
