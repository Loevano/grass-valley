const assert=require('node:assert/strict');
const client=require('./client_harness')();

async function main(){
  client.run(`
    let worldRequests=0,commandFinished=false,releasePoll,rendered=[];
    refreshing=null;
    world={world_id:'commands',towns:{own:{buildings:[],villagers:[],map_resources:[],npcs:[],roads:[]}}};
    identity={town_id:'own',token:'fixture'};identities.commands=identity;viewed='own';
    document.activeElement={tagName:'SELECT'};
    render=()=>rendered.push(!!world.towns.own.confirmed);
    renderWorldMenu=()=>{};commandEffect=()=>{};
    request=async path=>{
      if(path==='/api/command'){commandFinished=true;return {ok:true};}
      if(path==='/api/worlds')return {worlds:[]};
      worldRequests++;
      const snapshot=JSON.parse(JSON.stringify(world));
      snapshot.towns.own.confirmed=commandFinished;
      if(worldRequests===1)await new Promise(resolve=>releasePoll=resolve);
      return snapshot;
    };
    const oldPoll=refresh();
    const action=act({action:'butcher_animal',index:0,barn_index:1,animal:'pig'});
  `);
  // Let the command acknowledgement arrive while the earlier snapshot is stalled.
  await Promise.resolve();await Promise.resolve();
  client.run('releasePoll()');
  await client.run('Promise.all([oldPoll,action])');
  assert.equal(client.run('worldRequests'),2,'A command needs a snapshot requested after its acknowledgement');
  assert.equal(client.run('world.towns.own.confirmed'),true,'The action never settles with a pre-command snapshot');
  assert.equal(client.run('rendered.at(-1)'),true,'The acknowledged action renders even while its dropdown retains focus');

  client.run(`
    worldRequests=0;rendered=[];
    request=async path=>{
      if(path==='/api/worlds')return {worlds:[]};
      worldRequests++;await new Promise(resolve=>releasePoll=resolve);
      return JSON.parse(JSON.stringify(world));
    };
    const first=refresh(),second=refresh();
  `);
  client.run('releasePoll()');await client.run('Promise.all([first,second])');
  assert.equal(client.run('worldRequests'),1,'Ordinary polls still share one request');
  assert.equal(client.run('rendered.length'),0,'Ordinary polls still preserve a focused dropdown');

  client.run(`
    let errors=[];notice=message=>errors.push(message);
    request=async path=>{
      if(path==='/api/command')throw Error('Assign a qualified butcher first.');
      if(path==='/api/worlds')return {worlds:[]};
      return JSON.parse(JSON.stringify(world));
    };
  `);
  await client.run("act({action:'butcher_animal'})");
  assert.equal(client.run('errors[0]'),'Assign a qualified butcher first.','Host rejections still reach the player');
  assert.equal(client.run('rendered.length'),1,'A rejected command restores current controls even with dropdown focus');

  client.run(`
    worldRequests=0;
    request=async path=>{
      if(path==='/api/command')return {ok:true};
      if(path==='/api/worlds')return {worlds:[]};
      if(++worldRequests===1){await new Promise(resolve=>releasePoll=resolve);throw Error('Poll failed');}
      return JSON.parse(JSON.stringify(world));
    };
    const failedPoll=refresh().catch(()=>{});
    const retryAction=act({action:'butcher_animal'});
  `);
  await Promise.resolve();client.run('releasePoll()');
  await client.run('Promise.all([failedPoll,retryAction])');
  assert.equal(client.run('worldRequests'),2,'A failed earlier poll cannot prevent confirming a command');
}
main().then(()=>console.log('Command response refresh checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
