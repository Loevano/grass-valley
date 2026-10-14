const assert=require('node:assert/strict');
const client=require('./client_harness')();
client.run(`
  const terrain=Array(120).fill('g'.repeat(120));
  const town=(name,x)=>({name,level:1,map_size:120,terrain,map_resources:[],npcs:[],roads:[],
    player:{x:x+2,y:33,facing:1,equipment:{}},
    buildings:[{kind:'hall',x,y:30},{kind:'home',x:x+4,y:30}],villagers:[{name:'Ada',job:'idle'}]});
  world={world_id:'overview',shared_map:{map_size:120,terrain,map_resources:[{kind:'water',x:28,y:28}],npcs:[]},
    towns:{own:town('Home',30),friend:town('Neighbor',45)}};
  viewed='own';identity={town_id:'own'};cameraTown='overview/shared';camera={x:30,y:30,zoom:.35};
  const steps=ClientScene.prepare(world);let prepared;do{prepared=steps.next();}while(!prepared.done);ClientScene.commit(prepared.value);
  const beforeState=JSON.stringify(world);
  ClientScene.queue.schedule('plan/own/0',1,(function*(){while(true)yield;})(),()=>{});
  let workerUpdates=0,resourceDraws=0,buildingDraws=0,terrainScans=0,outlines=0;
  const oldWorkers=animatedVillagers,oldResource=drawResource,oldBuilding=drawBuilding,oldScan=groundMatchesCache,oldOutline=townOutline;
  animatedVillagers=(...args)=>{workerUpdates++;return oldWorkers(...args);};
  drawResource=(...args)=>{resourceDraws++;return oldResource(...args);};
  drawBuilding=(...args)=>{buildingDraws++;return oldBuilding(...args);};
  groundMatchesCache=(...args)=>{terrainScans++;return oldScan(...args);};
  townOutline=(...args)=>{outlines++;return oldOutline(...args);};
`);
client.frame(0);
assert.equal(client.run('hitTargets.filter(h=>h.type==="building").length'),2,'Hall markers remain selectable in the overview');
assert.equal(client.run('hitTargets.filter(h=>h.type==="player").map(h=>h.townId).sort().join(",")'),'friend,own','All main characters have selectable overview markers');
assert.equal(client.run('overviewCache.water.has("28,28")'),true,'Water survives simplified terrain rendering');
assert.equal(client.run('workerUpdates+resourceDraws+buildingDraws'),0,'Overview never prepares or draws animated detail');
assert.equal(client.run('ClientScene.queue.jobs.size'),0,'Entering overview cancels suspended animation preparation');
assert.equal(client.run('JSON.stringify(world)===beforeState'),true,'Overview preparation leaves authoritative snapshots intact');
const painted=client.paints;
for(let i=1;i<=20;i++)client.frame(i*16);
assert.equal(client.paints,painted,'An unchanged overview does not clear or redraw the canvas');
assert.equal(client.run('terrainScans'),0,'Idle frames avoid terrain scans');
assert.equal(client.run('outlines'),2,'Town geometry is prepared only once');
client.run(`
  const ownSummary=overviewCache.towns.get('own'),friendSummary=overviewCache.towns.get('friend'),ground=groundCache.surface;
  world=JSON.parse(JSON.stringify(world));
  world.towns.friend.villagers[0].job='building';
  world.towns.friend.buildings[1].construction={progress:5};
`);
client.frame(336);
assert.equal(client.paints,painted,'Animation and construction progress snapshots do not redraw the overview');
assert.equal(client.run('overviewCache.towns.get("friend")===friendSummary'),true,'Unchanged footprints reuse their summary');
client.run(`
  world=JSON.parse(JSON.stringify(world));
  world.towns.friend.buildings.push({kind:'acre',x:55,y:35,construction:{progress:0}});
  world.towns.friend.roads.push({x:90,y:90});
`);
client.frame(352);
assert.equal(client.run('overviewCache.towns.get("own")===ownSummary'),true,'A friend building preserves the other town summary');
assert.equal(client.run('overviewCache.towns.get("friend")!==friendSummary'),true,'Friend construction updates its town summary');
assert.equal(client.run('Math.max(...overviewCache.towns.get("friend").outline.map(p=>p[0]))'),56,'Acres and construction footprints extend the hull; roads do not');
assert.equal(client.run('groundCache.surface===ground'),true,'Building changes preserve cached terrain');
assert.equal(client.run('outlines'),3,'Only the affected town rebuilds its outline');
client.run(`
  world=JSON.parse(JSON.stringify(world));world.towns.friend.name='Renamed';
  world.towns.friend.buildings[0].x=44;world.towns.friend.buildings.splice(2,1);
`);
client.frame(368);
assert.equal(client.run('overviewCache.towns.get("friend").name'), 'Renamed');
assert.equal(client.run('overviewCache.towns.get("friend").hall.x'),44,'Hall moves update the marker');
assert.equal(client.run('Math.max(...overviewCache.towns.get("friend").outline.map(p=>p[0]))'),50,'Removal contracts the outline');
client.run(`
  const renamedOutline=overviewCache.towns.get('friend').outline;
  world=JSON.parse(JSON.stringify(world));world.towns.friend.name='New name';
`);
client.frame(376);
assert.equal(client.run('overviewCache.towns.get("friend").outline===renamedOutline'),true,'Renaming reuses the existing outline geometry');
const beforePan=client.paints;
client.run('camera.x+=1;');client.frame(384);
assert.equal(client.paints,beforePan+1,'Camera input redraws promptly');
assert.equal(client.run('groundCache.surface===ground'),true,'Small pans reuse terrain artwork');
client.run('buildingHover={townId:"friend",index:0};');client.frame(400);
assert.equal(client.paints,beforePan+2,'Hover redraws lightweight feedback');
client.run(`
  const hallHit=hitTargets.find(h=>h.townId==='friend');
  canvas.onclick({clientX:(hallHit.left+hallHit.right)/2,clientY:(hallHit.top+hallHit.bottom)/2,button:0});
`);
assert.equal(client.run('viewed'), 'friend','Marker clicks select the marker town');
assert.equal(client.run('selection.type'), 'building');
client.frame(416);
client.run(`
  const oldTerrain=groundCache.surface;
  world=JSON.parse(JSON.stringify(world));world.shared_map.terrain[30]='b'.repeat(120);
`);
client.frame(432);
assert.equal(client.run('groundCache.surface!==oldTerrain'),true,'Visible terrain updates repaint blocks');
client.run('camera.zoom=1;');client.frame(448);
assert.equal(client.run('workerUpdates>0&&buildingDraws>0'),true,'Detailed zoom resumes workers and buildings from current state');
client.run(`
  world={world_id:'other',shared_map:{map_size:120,terrain,map_resources:[],npcs:[]},towns:{own:town('Elsewhere',30)}};
  cameraTown='other/shared';camera={x:30,y:30,zoom:.35};viewed='own';selection=null;
`);
client.frame(464);
assert.equal(client.run('overviewCache.worldId'), 'other');
assert.equal(client.run('overviewCache.towns.has("friend")'),false,'World switches discard old summaries');
client.run(`
  world={world_id:'chunks',shared_map:{chunked:true,chunk_size:40,map_size:480,terrain_seed:2,
    visible_chunks:['0,0'],terrain_chunks:{'0,0':Array(40).fill('g'.repeat(40))},resource_chunks:{}},towns:{own:town('Chunk town',30)}};
  hydrateMapChunks(world);cameraTown='chunks/shared';camera={x:30,y:30,zoom:.35};
`);
client.frame(480);
client.run(`
  const loadedTerrain=groundCache.surface;
  world={...world,shared_map:{...world.shared_map,visible_chunks:['0,0','1,0'],
    terrain_chunks:{'1,0':Array(40).fill('i'.repeat(40))},resource_chunks:{}}};
  hydrateMapChunks(world);
`);
client.frame(496);
assert.equal(client.run('world.shared_map.terrain[30][45]'), 'i','Missing terrain hydrates into the overview');
assert.equal(client.run('groundCache.surface!==loadedTerrain'),true,'New visible chunks refresh overview terrain');
assert.equal(client.run('chunkTerrainCache.size<=128'),true,'Overview preserves bounded chunk caching');
const beforeResize=client.paints;
client.run('canvas.getBoundingClientRect=()=>({left:0,top:0,width:720,height:500});');
client.frame(512);
assert.equal(client.paints,beforeResize+1,'Resizing redraws the overview at the new dimensions');
client.frame(528);
assert.equal(client.paints,beforeResize+1,'The resized overview returns to idle cache reuse');
assert.equal(client.run('JSON.stringify(townOutline([{x:1,y:1}]))'), '[[1,1],[2,1],[2,2],[1,2]]','A hall-only town encloses the whole tile');
client.run(`
  world={world_id:'founders',shared_map:{map_size:120,terrain,map_resources:[],npcs:[]},
    towns:{own:town('Home',30),friend:town('Neighbor',45),traveler:town('Traveler',35)}};
  world.towns.traveler.buildings=[];
  cameraTown='founders/shared';camera={x:30,y:30,zoom:.35};viewed='own';selection=null;
`);
client.frame(544);
assert.equal(client.run('hitTargets.filter(h=>h.type==="player").length'),3,'Founders without a town hall also appear');
const beforeMovement=client.paints;
client.run(`
  const founderGround=groundCache.surface,founderOutline=overviewCache.towns.get('friend').outline;
  const oldFriendLeft=hitTargets.find(h=>h.type==='player'&&h.townId==='friend').left;
  updateRemotePlayers({world_id:world.world_id,sampled_at:1,players:{friend:{...world.towns.friend.player,x:51}}});
`);
client.frame(560);
assert.equal(client.paints,beforeMovement+1,'Remote founder movement redraws the overview');
assert.equal(client.run('hitTargets.find(h=>h.type==="player"&&h.townId==="friend").left>oldFriendLeft'),true,'Remote markers track updated positions');
assert.equal(client.run('groundCache.surface===founderGround&&overviewCache.towns.get("friend").outline===founderOutline'),true,'Founder movement reuses cached ground and town geometry');
client.run(`
  const hit=hitTargets.find(h=>h.type==='player'&&h.townId==='friend');
  canvas.onclick({clientX:(hit.left+hit.right)/2,clientY:(hit.top+hit.bottom)/2,button:0});
`);
assert.equal(client.run('selection.type'), 'player','Overview founder markers open character inspection');
assert.equal(client.run('viewed'), 'friend');
client.frame(576);
const beforeOwnMovement=client.paints;
client.run(`
  const oldOwnLeft=hitTargets.find(h=>h.type==='player'&&h.townId==='own').left;
  playerView.following=false;
  playerView.target={...playerView.target,x:playerView.x+1};
`);
client.frame(592);
assert.equal(client.paints,beforeOwnMovement+1,'Local founder movement redraws the overview');
assert.equal(client.run('hitTargets.find(h=>h.type==="player"&&h.townId==="own").left>oldOwnLeft'),true,'Your marker follows your displayed position while viewing another town');
client.run(`world={...world,towns:{own:world.towns.own,friend:world.towns.friend}};`);
client.frame(608);
assert.equal(client.run('hitTargets.some(h=>h.type==="player"&&h.townId==="traveler")'),false,'Removed founders leave no stale overview markers');
console.log('Cached map overview checks passed');
