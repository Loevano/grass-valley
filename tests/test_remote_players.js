// Run with: node tests/test_remote_players.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8'),elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
documentMock.getElementById('world').getContext=()=>new Proxy({}, {get:()=>()=>{}});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','assert',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  return (async()=>{
    world={world_id:'shared',shared_map:{},towns:{own:{player:{x:5,y:7,facing:1}},friend:{player:{x:10,y:10,facing:1}}}};
    identity={town_id:'own'};viewed='own';
    const friend=world.towns.friend;
    remotePlayerSprite('friend',friend,0);
    let snapshot={world_id:'shared',players:{own:{x:99,y:99,facing:-1},friend:{x:10.3,y:10,facing:1}}};
    request=async path=>{assert.equal(path,'/api/players');return snapshot;};
    await refreshPlayers();
    assert.equal(friend.player.x,10.3,'Remote position arrives without a full-world refresh');
    assert.equal(world.towns.own.player.x,5,'Position polling preserves own movement authority');
    let sprite=remotePlayerSprite('friend',friend,.06);
    assert(Math.abs(sprite.x-10.15)<1e-9,'Movement interpolates halfway instead of teleporting');
    assert(sprite.state.path.length,'Walking animates between samples');
    await refreshPlayers();
    sprite=remotePlayerSprite('friend',friend,.06);
    assert.equal(sprite.x,10.3,'Repeated identical samples do not restart interpolation');
    assert.equal(sprite.state.path.length,0,'Walking animation stops when position settles');
    // Continuous 10 Hz movement stays continuous on each display frame.
    let previous=sprite.x;
    for(let sample=0;sample<10;sample++){
      snapshot.players.friend.x+=.3;await refreshPlayers();
      for(let frame=0;frame<6;frame++){
        sprite=remotePlayerSprite('friend',friend,1/60);
        assert(sprite.x>previous,'Remote walking advances every frame between updates');previous=sprite.x;
      }
    }
    const state=sprite.state;
    world=JSON.parse(JSON.stringify(world));world.towns.friend.player.x=1;
    sprite=remotePlayerSprite('friend',world.towns.friend,0);
    assert.strictEqual(sprite.state,state,'Full-world refresh preserves ongoing remote interpolation');
    assert.equal(sprite.x,previous,'A stale full-world position does not rewind the character');
    let release,calls=0;
    request=()=>{calls++;return new Promise(resolve=>{release=resolve;});};
    const pending=refreshPlayers();await refreshPlayers();assert.equal(calls,1,'Slow connections cannot overlap position polls');
    world={...world};release(snapshot);await pending;
    assert.equal(world.towns.friend.player.x,1,'Responses for replaced town objects are ignored');
    request=async()=>({world_id:'old-world',players:{friend:{x:99,y:99,facing:1}}});
    await refreshPlayers();assert.equal(world.towns.friend.player.x,1,'Responses from another world are ignored');
    document.hidden=true;request=async()=>{throw Error('Hidden browser must not poll');};await refreshPlayers();document.hidden=false;
    request=async()=>{throw Error('Offline');};await assert.rejects(refreshPlayers(),/Offline/);
    assert.equal(refreshingPlayers,false,'Failed polls allow retrying');
    world={world_id:'new-world',towns:{friend:{player:{x:3,y:4,facing:-1}}}};
    sprite=remotePlayerSprite('friend',world.towns.friend,0);
    assert.equal(sprite.x,3,'World changes clear old player positions');
    assert.equal(sprite.state.facing,-1);
  })();`);
run(documentMock,{addEventListener(){}},{getItem:()=>null},()=>0,()=>{},assert)
  .then(()=>console.log('Remote player checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
