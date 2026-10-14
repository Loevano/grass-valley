const assert=require('node:assert/strict');
const client=require('./client_harness')();

(async()=>{
  client.run(`
    refreshing=null;
    let snapshot={world_id:null,towns:{}};
    request=async path=>{
      if(path==='/api/worlds/create'){
        const rows=Array(40).fill('g'.repeat(40));
        snapshot={world_id:'created',name:'New valley',town_sites:[{x:200,y:200}],towns:{},
          shared_map:{chunked:1,chunk_size:40,map_size:480,terrain_seed:42,npcs:[],
            visible_chunks:['5,5'],terrain_chunks:{'5,5':rows},
            resource_chunks:{'5,5':{version:'0123456789abcdef',map_resources:[],tree_stumps:[]}}}};
        return {ok:true};
      }
      if(path==='/api/worlds')return {worlds:[],can_manage:true};
      return JSON.parse(JSON.stringify(snapshot));
    };
  `);
  await client.run('refresh()');
  client.frame(0);
  assert.equal(client.elements.get('world-menu').open,true);
  for(let frame=1;frame<=3;frame++){
    await client.run('refresh()');
    assert.doesNotThrow(()=>client.frame(frame*2000),'Waiting for a world keeps the map animation running');
  }
  const before=client.paints;
  await client.run(`manageWorld('/api/worlds/create',{name:'New valley'},true)`);
  client.frame(6016);
  assert.equal(client.run('world.world_id'),'created');
  assert.equal(client.elements.get('world-menu').open,false);
  assert.equal(client.elements.get('login-dialog').open,true);
  assert.equal(client.run('groundCache.landscape.terrain[200][200]'),'g','New terrain reaches the drawing cache without reloading');
  assert.equal(client.run('camera.x'),200,'The map centers on the new world');
  client.frame(6032);
  assert.equal(client.paints,before+2,'Map frames continue after world creation');
  console.log('New world rendering after empty menu refreshes passed.');
})().catch(error=>{console.error(error);process.exitCode=1;});
