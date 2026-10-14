const createClient=require('./client_harness');

for(const queued of [false,true])for(const walking of [false,true]){
  const client=createClient();
  client.run(`
    const finish=steps=>{let result;do{result=steps.next();}while(!result.done);return result.value;};
    const field={kind:'acre',x:8,y:4,farm_index:1};
    const site={kind:'home',x:15,y:10,construction:{progress:0,total:30}};
    const worker={name:'Bram',job:'farming',building_index:1,work_target:field,
      work_animation:{task:['farming','farm',8,3,'acre',8,4],version:1,
        x:8,y:4,phase:'working',elapsed:1,
        playback:[{time:0,x:8,y:4,phase:'working',elapsed:0},
          {time:3,x:8,y:4,phase:'working',elapsed:3}]}};
    const town={map_size:24,terrain:Array(24).fill('g'.repeat(24)),map_resources:[],npcs:[],
      buildings:[{kind:'hall',x:5,y:5},{kind:'farm',x:8,y:3},field,site],villagers:[worker]};
    world={world_id:'builder-reassignment',towns:{own:town}};viewed='own';
    const commit=()=>{if(${queued})ClientScene.commit(finish(ClientScene.prepare(world,ClientScene.current)));};
    const frame=seconds=>{animatedVillagers(town,seconds);ClientScene.queue.run();};
    commit();frame(0);frame(0);
    if(${walking}){
      worker.work_animation={...worker.work_animation,version:2,x:9,y:4,phase:'walking',
        playback:[{time:0,x:8,y:4,phase:'walking',elapsed:0},
          {time:3,x:9,y:4,phase:'working',elapsed:0}]};
      frame(.2);
    }
    const before={x:workers.get(0).x,y:workers.get(0).y};
    worker.job='building';delete worker.building_index;worker.work_target=null;
    worker.construction_target={kind:'home',x:15,y:10};worker.construction_position={x:8,y:4};
    // A command can retain the previous host animation until the next tick.
    commit();frame(0);
    const planned=workers.get(0);
    if(planned.x!==before.x||planned.y!==before.y)throw Error('Assignment teleports Bram');
    if(!planned.construction||!planned.path.length)throw Error('Old playback discarded the construction route');
    delete worker.work_animation;
    const snapshot=JSON.stringify(world);
    for(let i=0;i<120;i++){
      const previous={x:workers.get(0).x,y:workers.get(0).y};frame(.1);
      const state=workers.get(0);
      if(Math.hypot(state.x-previous.x,state.y-previous.y)>.180001)throw Error('Bram exceeds walking speed');
    }
    const state=workers.get(0);
    if(Math.abs(state.x-site.x)+Math.abs(state.y-site.y)!==1||state.phase!=='working')
      throw Error('Bram does not reach the construction site');
    if(JSON.stringify(world)!==snapshot)throw Error('Animation mutated authoritative state');
  `);
}
console.log('Builder reassignment from stationary and walking playback passed, with queued and direct planning');
