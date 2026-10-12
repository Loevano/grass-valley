const assert=require('node:assert/strict');
const Scene=require('../web/scene.js');
const Animation=require('../web/animations.js');
const finish=steps=>{let r;do{r=steps.next();}while(!r.done);return r.value;};
const base=new Set(['2,0','2,1','2,2']);
const overlay=new Scene.Overlay(base);
const start={x:0,y:0},target={x:4,y:0};
let probes=0;
const has=overlay.has.bind(overlay);overlay.has=key=>{probes++;return has(key);};
const route=(destination=target,adjacent=false,bounds=null)=>Animation.route(start,destination,overlay,12,adjacent,bounds);
const original=route();assert.equal(original.length,10,'Shortest detour around a wall');
const count=probes;
const consumed=route();consumed[0].x=99;consumed.shift();
assert.deepEqual(route(),original);assert.equal(probes,count,'Warm queries perform no collision searches');
assert.deepEqual(Animation.route(start,target,new Scene.Overlay(base),12,false),original,'Other workers reuse the same geometry');
overlay.delete('2,0');assert.equal(route().length,4,'Worker exceptions invalidate cached detours');
assert(base.has('2,0'),'Worker exceptions preserve authoritative occupancy');
assert.equal(route(target,true).length,3,'Adjacent and exact arrival use separate entries');
assert.equal(route({x:0,y:0}).length,0,'Empty paths are cached');
assert.equal(route(new Set(['0,2','6,0'])).length,2,'Choose the nearest reachable water bank');
assert.equal(route(new Set(['6,0'])).length,6,'Changed banks cannot reuse the previous route');
overlay.add('1,0');overlay.add('0,1');assert.equal(route(),null);
const failedCount=probes;assert.equal(route(),null);assert.equal(probes,failedCount,'Unreachable routes are cached too');
overlay.delete('1,0');assert(route(),'Opening a blocked route retries the search');
assert.equal(route(target,false,{minX:0,maxX:3,minY:0,maxY:11}),null,'Changed bounds invalidate cached paths');
const mutable=new Set(['0,1','1,0']);
assert.equal(Animation.route(start,target,mutable,12,false),null);mutable.delete('1,0');
assert(Animation.route(start,target,mutable,12,false),'Plain mutable collision sets never reuse stale results');

const crowded=new Scene.Overlay(new Set());
for(let i=0;i<258;i++)Animation.route(start,{x:i%20,y:Math.floor(i/20)},crowded,20,false);
let evictionProbes=0;const crowdedHas=crowded.has.bind(crowded);
crowded.has=key=>{evictionProbes++;return crowdedHas(key);};
Animation.route(start,{x:1,y:0},crowded,20,false);
assert(evictionProbes>0,'Old routes are evicted when the cache fills');

function snapshot(terrain,resources=[],world_id='cache'){
  return {world_id,towns:{own:{map_size:12,buildings:[{kind:'hall',x:5,y:5}],villagers:[],
    terrain,map_resources:resources,npcs:[]}}};
}
const initial=finish(Scene.prepare(snapshot(Array(12).fill('g'.repeat(12))),null));
const initialBase=initial.regions.get('own').blocked;
const warm=Animation.route(start,target,new Scene.Overlay(initialBase),12,false);assert.equal(warm.length,4);
const supply=finish(Scene.prepare(snapshot(Array(12).fill('g'.repeat(12))),initial));
assert.strictEqual(supply.regions.get('own').blocked,initialBase,'Unchanged snapshots retain cached navigation');
const changed=finish(Scene.prepare(snapshot(['gv'+'g'.repeat(10),...Array(11).fill('g'.repeat(12))]),supply));
assert.equal(Animation.route(start,target,changed.regions.get('own').routing.navigation.overlay(),12,false).length,6,
  'Terrain edits rebuild navigation and discard stale routes');
const obstruction=finish(Scene.prepare(snapshot(Array(12).fill('g'.repeat(12)),[{kind:'tree',x:1,y:0}]),changed));
assert.equal(Animation.route(start,target,obstruction.regions.get('own').routing.navigation.overlay(),12,false).length,6,
  'New obstacles rebuild navigation');
const felled=finish(Scene.prepare(snapshot(Array(12).fill('g'.repeat(12))),obstruction));
assert.equal(Animation.route(start,target,felled.regions.get('own').routing.navigation.overlay(),12,false).length,4,
  'Felled trees restore the shortest route');
console.log('Shortest route cache, invalidation, isolation, and eviction checks passed');
