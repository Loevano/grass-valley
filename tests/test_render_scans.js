// Run with: node tests/test_render_scans.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
new Function('assert','WorkerAnimation',`
  const resourceReach=b=>b.kind==='fishing_hut'?3+(b.level || 1)-1:10;
  const fishingBoatLevel=b=>(b.profession_upgrades || []).includes('boat')?1:0;
  ${source.slice(source.indexOf('function buildingUsable('),source.indexOf('function drawFarmCrops('))}
  ${source.slice(source.indexOf('function fishingBoatSprites('),source.indexOf('function drawFishingBoat('))}
  const resources=Array.from({length:50000},(_,i)=>({kind:i%2?'tree':'water',x:100+i%100,y:100+Math.floor(i/100)}));
  let visits=0;
  resources.some=predicate=>Array.prototype.some.call(resources,r=>{visits++;return predicate(r);});
  resources.filter=predicate=>Array.prototype.filter.call(resources,r=>{visits++;return predicate(r);});
  const town={map_resources:resources,buildings:[{kind:'fishing_hut',x:5,y:5,profession_upgrades:['boat']}]};
  const cutter={kind:'woodcutter',x:5,y:5};
  assert(!buildingUsable(town,cutter));
  fishingBoatSprites([['own',town]],resources);
  const water=fishingBoatSprites.water;
  water.filter=predicate=>Array.prototype.filter.call(water,r=>{visits++;return predicate(r);});
  const warm=visits;
  for(let frame=0;frame<120;frame++){
    assert(!buildingUsable(town,{...cutter}));
    assert.equal(fishingBoatSprites([['own',town]],resources)[0].x,5.7);
  }
  assert.equal(visits,warm,'Warm scrolling frames never rescan all 50,000 resources');
  resources.push({kind:'tree',x:6,y:5});
  assert(buildingUsable(town,cutter),'Resource additions invalidate workplace usability');
  assert(!buildingUsable(town,{...cutter,x:20,y:20}),'Moving a workplace updates its usability');
  town.map_resources=[{kind:'water',x:5,y:9}];
  const hut={kind:'fishing_hut',x:5,y:5,profession_upgrades:['boat']};
  assert(!buildingUsable(town,hut));
  assert(buildingUsable(town,{...hut,level:2}),'Upgrades update the cached resource reach');
  town.buildings=[hut];
  assert.equal(fishingBoatSprites([['own',town]],town.map_resources)[0].x,5.7,'Out-of-range water does not provide a berth');
  town.map_resources=[{kind:'water',x:5,y:6}];
  assert.equal(fishingBoatSprites([['own',town]],town.map_resources)[0].y,6,'World updates replace the water index');
`)(assert,require('../web/animations.js'));
console.log('Render resource scan checks passed');
