// Run with: node tests/test_remote_players.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8'),elements=new Map(),timers=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
documentMock.getElementById('world').getContext=()=>new Proxy({}, {get:()=>()=>{}});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','assert','timers',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  return (async()=>{
    world={world_id:'shared',shared_map:{},towns:{own:{player:{x:5,y:7,facing:1}},friend:{player:{x:10,y:10,facing:1}}}};
    identity={town_id:'own'};viewed='own';
    const friend=world.towns.friend;
    remotePlayerSprite('friend',friend,0);
    let snapshot={world_id:'shared',sampled_at:10,players:{own:{x:99,y:99,facing:-1},friend:{x:10.3,y:10,facing:1}}};
    request=async path=>{assert.equal(path,'/api/players');return snapshot;};
    await refreshPlayers();
    assert.equal(friend.player.x,10.3,'Remote position arrives without a full-world refresh');
    assert.equal(world.towns.own.player.x,5,'Position polling preserves own movement authority');
    let sprite=remotePlayerSprite('friend',friend,.1);
    assert.equal(sprite.x,10,'A short buffer absorbs uneven snapshot arrivals');
    sprite=remotePlayerSprite('friend',friend,.05);
    assert(Math.abs(playerMovementDistance(sprite.x-10,sprite.y-10)-.15)<1e-9,'Remote movement uses the normal walking speed');
    assert(sprite.state.path.length,'Walking animates between samples');
    await refreshPlayers();
    sprite=remotePlayerSprite('friend',friend,.05);
    assert.equal(sprite.x,10.3,'Repeated identical samples do not restart interpolation');
    assert.equal(sprite.state.path.length,0,'Walking animation stops when position settles');
    // Jittery arrivals and batched updates retain steady speed on every frame.
    const step=.3/Math.SQRT2;
    let previous={x:sprite.x,y:sprite.y};
    const sendStep=count=>{
      snapshot.players.friend.x+=step*count;snapshot.players.friend.y-=step*count;
      snapshot.sampled_at+=.1*count;updateRemotePlayers(snapshot);
    };
    sendStep(1);remotePlayerSprite('friend',friend,.1);
    for(let frame=0;frame<60;frame++){
      // Alternate short and long polling intervals; some responses contain two steps.
      if([0,9,21,24,33,36,45,48,57].includes(frame))sendStep(frame===9?2:1);
      sprite=remotePlayerSprite('friend',friend,1/60);
      assert(Math.abs(playerMovementDistance(sprite.x-previous.x,sprite.y-previous.y)-.05)<1e-9,
        'Polling jitter cannot speed up or slow down remote walking');
      previous={x:sprite.x,y:sprite.y};
    }
    // Drain the buffer after stopping, with no extrapolation past the host position.
    sprite=remotePlayerSprite('friend',friend,1);
    assert.equal(sprite.x,snapshot.players.friend.x);
    assert.equal(sprite.y,snapshot.players.friend.y);
    assert.equal(sprite.state.path.length,0,'Stopped characters stop animating');
    // Preserve a sampled corner instead of cutting diagonally to the latest target.
    const corner={x:sprite.x+step,y:sprite.y-step};
    snapshot.players.friend={...corner,facing:1};snapshot.sampled_at+=.1;updateRemotePlayers(snapshot);
    snapshot.players.friend={x:corner.x+step,y:corner.y+step,facing:-1};
    snapshot.sampled_at+=.1;updateRemotePlayers(snapshot);
    sprite=remotePlayerSprite('friend',friend,.2);
    assert(Math.abs(sprite.x-corner.x)<1e-9&&Math.abs(sprite.y-corner.y)<1e-9,'Queued turns pass through the sampled corner');
    sprite=remotePlayerSprite('friend',friend,.2);
    assert.equal(sprite.x,snapshot.players.friend.x);assert.equal(sprite.y,snapshot.players.friend.y);
    assert.equal(sprite.state.facing,-1,'Facing follows the played movement');
    previous=sprite.x;
    const state=sprite.state;
    world=JSON.parse(JSON.stringify(world));world.towns.friend.player.x=1;
    sprite=remotePlayerSprite('friend',world.towns.friend,0);
    assert.strictEqual(sprite.state,state,'Full-world refresh preserves ongoing remote interpolation');
    assert.equal(sprite.x,previous,'A stale full-world position does not rewind the character');
    let release,calls=0;
    request=()=>{calls++;return new Promise(resolve=>{release=resolve;});};
    const pending=refreshPlayers();await refreshPlayers();assert.equal(calls,1,'Slow connections cannot overlap position polls');
    world={...world};snapshot.sampled_at++;release(snapshot);await pending;
    assert.equal(world.towns.friend.player.x,snapshot.players.friend.x,'Same-world replacement accepts fresh position samples');
    const latest=world.towns.friend.player.x;
    updateRemotePlayers({world_id:'shared',sampled_at:snapshot.sampled_at-1,players:{friend:{x:1,y:10,facing:1}}});
    assert.equal(world.towns.friend.player.x,latest,'Older snapshots cannot rewind the character');
    request=async()=>({world_id:'old-world',players:{friend:{x:99,y:99,facing:1}}});
    await refreshPlayers();assert.equal(world.towns.friend.player.x,latest,'Responses from another world are ignored');
    document.hidden=true;request=async()=>{throw Error('Hidden browser must not poll');};await refreshPlayers();document.hidden=false;
    request=async()=>{throw Error('Offline');};await assert.rejects(refreshPlayers(),/Offline/);
    assert.equal(refreshingPlayers,false,'Failed polls allow retrying');
    // Exercise the real full refresh when the fast endpoint has failed.
    const fallback=JSON.parse(JSON.stringify(world));fallback.players_sampled_at=snapshot.sampled_at+1;
    fallback.towns.friend.player={x:20,y:10,facing:1};identities.shared=identity;
    request=async path=>path==='/api/world'?fallback:{worlds:[]};
    render=()=>{};renderWorldMenu=()=>{};menuStarted=true;
    await refresh();sprite=remotePlayerSprite('friend',world.towns.friend,.12);
    assert.equal(sprite.x,20,'Full-world refresh updates cached characters when fast polling fails');
    const stale=JSON.parse(JSON.stringify(fallback));stale.players_sampled_at--;
    stale.towns.friend.player.x=1;request=async path=>path==='/api/world'?stale:{worlds:[]};
    await refresh();assert.equal(remotePlayerSprite('friend',world.towns.friend,0).x,20,'Stale world refresh does not rewind a fresh position');
    request=(_path,_data,{signal})=>new Promise((_resolve,reject)=>signal.addEventListener('abort',()=>reject(Error('Timed out'))));
    const stalled=refreshPlayers();timers.at(-1)();await assert.rejects(stalled,/Timed out/);
    assert.equal(refreshingPlayers,false,'Timeout releases the polling gate');
    request=async()=>({world_id:'shared',sampled_at:snapshot.sampled_at+2,players:{friend:{x:21,y:10,facing:1}}});
    await refreshPlayers();assert.equal(world.towns.friend.player.x,21,'Polling recovers after a stalled request');
    world={world_id:'new-world',towns:{friend:{player:{x:3,y:4,facing:-1}}}};
    sprite=remotePlayerSprite('friend',world.towns.friend,0);
    assert.equal(sprite.x,3,'World changes clear old player positions');
    assert.equal(sprite.state.facing,-1);
  })();`);
run(documentMock,{addEventListener(){}},{getItem:()=>null},fn=>{timers.push(fn);return fn;},fn=>{const index=timers.indexOf(fn);if(index>=0)timers.splice(index,1);},assert,timers)
  .then(()=>console.log('Remote player checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
