const assert=require('node:assert/strict');
const Animation=require('../web/animations');
const client=require('./client_harness');
for(const prepared of [false,true])for(const type of ['building','woodcutting','trading']){
  const c=client();
  c.run(`
    const finish=steps=>{let r;do{r=steps.next();}while(!r.done);return r.value;};
    const tree={kind:'tree',x:12,y:3};
    const worker={name:'Ada',job:'${type}',building_index:1,work_target:tree,
      construction_position:{x:3,y:8},logistics_position:{x:4,y:8},route_waiting:true,
      production_status:'Waiting for a route',pending_output:{wood:2}};
    if(worker.job==='building')worker.construction_target={kind:'home',x:12,y:5};
    if(worker.job==='trading'){
      worker.logistics_position={x:4,y:10};
      worker.logistics_target={kind:'delivery',x:5,y:10};
      worker.logistics_trip={id:1,operation:'sell',picked_up:true,delivered:false,cargo:{planks:4},
        path:[[4,10],[5,10],[6,10]],step:0,pickup_step:0,delivery_step:2};
    }
    const town={map_size:24,terrain:Array(24).fill('g'.repeat(24)),map_resources:[tree],npcs:[],
      buildings:[{kind:'hall',x:5,y:5},{kind:'woodcutter',x:12,y:5}],villagers:[worker]};
    world={world_id:'host-queue',towns:{own:town}};viewed='own';
    const commit=()=>{if(${prepared})ClientScene.commit(finish(ClientScene.prepare(world,ClientScene.current)));};
    commit();animatedVillagers(town,0);ClientScene.queue.run();
    const before=JSON.stringify(world),state=workers.get(0),x=state.x,y=state.y;
    for(let i=0;i<3;i++){animatedVillagers(town,3);ClientScene.queue.run();}
    if(state.x!==x||state.y!==y||state.phase!=='waiting'||state.path.length)
      throw Error('A queued host search moved its waiting sprite');
    if(worker.job==='trading'&&state.cargo.planks!==4)throw Error('Waiting lost carried sale cargo');
    if(JSON.stringify(world)!==before)throw Error('Waiting playback mutated authoritative state');
    delete worker.route_waiting;
    worker.production_status='Working';
    commit();animatedVillagers(town,0);ClientScene.queue.run();animatedVillagers(town,1);ClientScene.queue.run();
    const resumed=workers.get(0);
    if(resumed.x===x&&resumed.y===y)throw Error('A completed search did not resume movement: ${type}/${prepared}');
  `);
}
const waiting={x:3,y:4,path:[{x:4,y:4}],routeWaiting:true,phase:'walking'};
Animation.advance(waiting,3,true);
assert.equal(waiting.x,3);assert.equal(waiting.y,4);
assert.equal(waiting.path.length,1,'Pending host work cannot consume old waypoints');
console.log('Queued host searches hold browser position/cargo and resume without snapshot mutation');
