const assert=require('node:assert/strict');
const client=require('./client_harness')();
async function main(){
  client.run(`
    let polls=0;
    request=async path=>{
      if(path!=='/api/players')throw Error('Unexpected request');
      polls++;return {world_id:world.world_id,sampled_at:polls,players:{}};
    };
    world={world_id:'polling',shared_map:{},towns:{own:{player:{x:1,y:1}}}};
    identity={town_id:'own'};
  `);
  await client.run('refreshPlayers()');
  assert.equal(client.run('polls'),0,'Solo play needs no remote position requests');
  client.run("world.towns.other={player:{x:2,y:2}};");
  await client.run('refreshPlayers()');
  assert.equal(client.run('polls'),1,'Another founder enables frequent position updates');
  client.run('document.hidden=true;');
  await client.run('refreshPlayers()');
  assert.equal(client.run('polls'),1,'Hidden tabs do not poll');
  client.run('document.hidden=false;delete world.shared_map;');
  await client.run('refreshPlayers()');
  assert.equal(client.run('polls'),1,'Separate maps do not render remote founders');
  client.run('world.shared_map={};delete world.towns.other;');
  await client.run('refreshPlayers()');
  assert.equal(client.run('polls'),1,'Removing the remote founder stops position polling');
}
main().then(()=>console.log('Remote founder polling checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
