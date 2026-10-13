const assert=require('node:assert/strict');
const Animation=require('../web/animations.js');
const Scene=require('../web/scene.js');
const client=require('./client_harness.js');
const finish=steps=>{let r;do{r=steps.next();}while(!r.done);return r.value;};
const start={x:0,y:0},target={x:8,y:0},roads=new Set(Array.from({length:9},(_,x)=>`${x},1`));
const overlay=new Scene.Overlay(new Set());
const route=(roadTiles=roads,goal=target,adjacent=false,mode='smooth')=>Animation.route(start,goal,overlay,12,adjacent,null,roadTiles,mode);
const expected=Array.from({length:9},(_,x)=>({x,y:1})).concat([target]);
for(const mode of ['smooth','departure','arrival'])assert.deepEqual(route(roads,target,false,mode),expected,'Longer road path wins on travel time');
assert.equal(route(new Set(Array.from({length:9},(_,x)=>`${x},2`))).length,8,'A slow road detour loses to a direct walk');
assert.deepEqual(route(roads,target,true).at(-1),{x:8,y:1},'Choose the fastest adjacent work spot');
assert.deepEqual(route(new Set(Array.from({length:6},(_,x)=>`${x},0`)),new Set(['0,4','5,0'])).at(-1),{x:5,y:0},'Choose water banks by travel time');
assert.equal(route(roads,start).length,0,'Already arrived has zero travel time');
assert.equal(Animation.route(start,target,overlay,12,false,{minX:0,maxX:11,minY:0,maxY:0},roads).length,8,'Road paths obey routing bounds');
for(const mode of ['departure','arrival','smooth']){
  const path=Animation.route({x:0,y:2},{x:2,y:1},overlay,12,true,null,new Set(['1,1']),mode);
  assert.deepEqual(path.at(-1),mode==='departure'?{x:2,y:2}:{x:1,y:1},'Arrival goals match each host movement model');
}

let probes=0;const originalHas=overlay.has.bind(overlay);overlay.has=k=>{probes++;return originalHas(k);};
assert.deepEqual(route(),expected);assert.equal(probes,0,'Warm road queries skip searching');
const copy=route();copy[0].x=99;copy.shift();assert.deepEqual(route(),expected,'Workers own copies of cached paths');
roads.clear();assert.equal(route().length,8,'Road removal invalidates cached routes');
roads.add('0,1');assert.equal(route().length,8,'An isolated road does not justify a detour');
for(let x=1;x<=8;x++)roads.add(`${x},1`);
assert.deepEqual(route(),expected,'Newly completed connections restore the faster road route');

// Compare the production heap to exhaustive relaxation across many maps.
let seed=42;const random=()=>{seed=(1664525*seed+1013904223)>>>0;return seed/2**32;};
for(const mode of ['smooth','departure','arrival'])for(let trial=0;trial<12;trial++){
  const tiles=Array.from({length:25},(_,i)=>({x:i%5,y:Math.floor(i/5)}));
  const key=p=>`${p.x},${p.y}`,blocked=new Set(),fast=new Set();
  for(const p of tiles){if(key(p)!=='0,0'&&key(p)!=='4,4'&&random()<.2)blocked.add(key(p));if(!blocked.has(key(p))&&random()<.5)fast.add(key(p));}
  const cost=(a,b)=>{
    const before=fast.has(key(a))?2:3,after=fast.has(key(b))?2:3;
    return mode==='departure'?2*before:mode==='arrival'?2*after:before+after;
  };
  const costs=new Map([['0,0',0]]);
  for(let i=0;i<25;i++)for(const a of tiles)if(costs.has(key(a))&&!blocked.has(key(a)))for(const b of tiles){
    if(!blocked.has(key(b))&&Math.abs(a.x-b.x)+Math.abs(a.y-b.y)===1)costs.set(key(b),Math.min(costs.get(key(b)) ?? Infinity,costs.get(key(a))+cost(a,b)));
  }
  const path=Animation.route(start,{x:4,y:4},blocked,5,false,null,fast,mode);
  if(!costs.has('4,4'))assert.equal(path,null);
  else{
    const points=[start,...path];
    assert.equal(points.slice(1).reduce((sum,p,i)=>sum+cost(points[i],p),0),costs.get('4,4'),'Heap returns the lowest travel cost');
  }
}
// Weighted searches remain resumable, including an expensive multi-goal query.
const steps=Animation.routeSteps(start,new Set(['119,119']),new Set(),120,false,null,new Set(['0,0']));
let yielded=0,result;do{result=steps.next();if(!result.done)yielded++;}while(!result.done);
assert(yielded>10);assert.equal(result.value.length,238);

const terrain=Array(12).fill('g'.repeat(12));
function world(roadTiles=[],shared=false){
  const town={map_size:12,terrain,map_resources:[{kind:'tree',...target}],npcs:[],
    buildings:[{kind:'hall',x:10,y:10},{kind:'woodcutter',x:10,y:9}],roads:roadTiles,
    villagers:[{name:'Ada',job:'woodcutting',building_index:1,work_target:{kind:'tree',...target},logistics_position:start}]};
  const snapshot={world_id:'road-routes',towns:{own:town}};
  if(shared)snapshot.shared_map={map_size:12,terrain,map_resources:town.map_resources,npcs:[]};
  return snapshot;
}
const sites=Array.from({length:9},(_,x)=>({x,y:1,construction:{progress:9,total:10}}));
const initial=world(sites),before=JSON.stringify(initial),first=finish(Scene.prepare(initial,null));
const slow=Animation.plan(first.regions.get('own').routing,initial.towns.own.villagers[0],0);
assert.equal(slow.path.length,7,'Unfinished roads provide no speed bonus');
assert.equal(JSON.stringify(initial),before,'Preparing routes preserves snapshots');
const completed=world(sites.map(({x,y})=>({x,y}))),second=finish(Scene.prepare(completed,first));
const fast=Animation.plan(second.regions.get('own').routing,completed.towns.own.villagers[0],0);
assert.equal(fast.path.length,9);assert.deepEqual(fast.path[0],{x:0,y:1});
assert.notEqual(second.regions.get('own').layout,first.regions.get('own').layout,'Road completion refreshes navigation costs');
assert.strictEqual(second.regions.get('own').blocked,first.regions.get('own').blocked,'Road changes preserve occupancy indexes');
function arrival(state){let time=0;while(state.path.length&&time<10){Animation.advance(state,.005,true);time+=.005;}return time;}
assert(arrival(fast)<arrival(slow),'Chosen road route actually arrives sooner at existing animation speeds');
const removed=world(),third=finish(Scene.prepare(removed,second));
assert.equal(Animation.plan(third.regions.get('own').routing,removed.towns.own.villagers[0],0).path.length,7,'Road removal replans to the direct route');
const shared=world([],true);shared.towns.friend={...shared.towns.own,roads:completed.towns.own.roads,villagers:[],buildings:[]};
const scene=finish(Scene.prepare(shared,null));
assert.equal(Animation.plan(scene.regions.get('own').routing,shared.towns.own.villagers[0],0).path.length,9,'Visitors route over another town roads');
const separate=world();separate.towns.friend={...separate.towns.own,roads:completed.towns.own.roads,villagers:[]};
assert.equal(finish(Scene.prepare(separate,null)).regions.get('own').routing.navigation.roads.size,0,'Separate maps retain their own roads');

// A road-only snapshot updates costs for future legs and preserves active paths.
const harness=client();
harness.run(`
  const finish=steps=>{let r;do{r=steps.next();}while(!r.done);return r.value;};
  world=${JSON.stringify(initial)};identity={town_id:'own'};viewed='own';
  ClientScene.commit(finish(ClientScene.prepare(world,null)));
  animatedVillagers(world.towns.own,0);ClientScene.queue.run();
  if(workers.get(0).path.length!==7)throw Error('Initial route must be direct');
  const oldState=workers.get(0);
  world=${JSON.stringify(completed)};
  ClientScene.commit(finish(ClientScene.prepare(world,ClientScene.current)));
  animatedVillagers(world.towns.own,0);ClientScene.queue.run();
  if(workers.get(0)!==oldState||workers.get(0).path.length!==7)throw Error('Road completion must preserve a valid active route');
  if(!workers.get(0).roads.has('0,1'))throw Error('Road completion must update movement speed data');
`);
console.log('Road travel costs, weighted optimality, cache invalidation, and shared route checks passed');
