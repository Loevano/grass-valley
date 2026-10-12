// Run with: node tests/test_camera_worker_layout.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const animation=require('../web/animations.js');
let plans=0;
const WorkerAnimation={...animation,plan(...args){plans++;return animation.plan(...args);}};
new Function('WorkerAnimation','assert','planCount',`
  let world,viewed='own',workerTown=null;
  const workers=new Map();
  ${source.slice(source.indexOf('function mapRoads('),source.indexOf('function playerBoatLevel('))}
  ${source.slice(source.indexOf('function animatedVillagers('),source.indexOf('let npcClock='))}
  const terrain={};
  for(let y=900;y<=1020;y++){terrain[y]={};for(let x=900;x<=1020;x++)terrain[y][x]='g';}
  const town={chunked:1,map_size:1920,town_origin:{x:960,y:960},terrain,
    buildings:[{kind:'hall',x:960,y:960}],map_resources:[],npcs:[],
    villagers:[{job:'idle',production_status:'Working'},{job:'idle',production_status:'Working'}]};
  world={world_id:'camera-layout',shared_map:{},towns:{own:town}};
  const frame=()=>{
    const town=world.towns.own,buildings=Object.values(world.towns).flatMap(t=>t.buildings);
    return animatedVillagers(town,0,'own',townWorkerLayout(town,buildings,town.map_resources));
  };
  const states=frame().map(sprite=>sprite.state);assert.equal(planCount(),2);
  assert.strictEqual(states[0].roads,states[1].roads,'Workers share the snapshot road index');
  const originalRoads=states[0].roads;
  frame();assert.strictEqual(states[0].roads,originalRoads,'Animation frames reuse the road index');
  const snapshot=()=>{world=JSON.parse(JSON.stringify(world));return world.towns.own;};
  snapshot().map_resources.push({kind:'tree',x:1100,y:1100});
  frame().forEach((sprite,i)=>assert.strictEqual(sprite.state,states[i],'Distant camera resources preserve worker paths'));
  assert.equal(planCount(),2);
  snapshot().buildings.push({kind:'home',x:1101,y:1101});frame();
  assert.equal(planCount(),2,'Distant buildings cannot invalidate local paths');
  snapshot().map_resources.push({kind:'rock',x:964,y:964});frame();
  assert.equal(planCount(),4,'Nearby obstacles replan worker paths');
  snapshot().terrain[965][965]='v';frame();
  assert.equal(planCount(),6,'Nearby terrain becoming unavailable replans paths');
  snapshot().terrain[965][965]='g';frame();
  assert.equal(planCount(),8,'Newly loaded nearby ground replans paths');
  snapshot().map_resources[1].supply=5;frame();
  assert.equal(planCount(),8,'Resource supplies do not invalidate worker paths');
  snapshot().buildings.push({kind:'home',x:967,y:967,construction:{progress:0}});frame();
  assert.equal(planCount(),10,'Nearby construction sites update the path layout');
  snapshot().buildings.at(-1).construction.progress=10;frame();
  assert.equal(planCount(),10,'Construction progress preserves the path layout');
  snapshot().buildings.at(-1).construction=null;frame();
  assert.equal(planCount(),12,'Construction completion updates the path layout');
  snapshot().roads=[{x:961,y:962},{x:962,y:962,construction:{progress:0}}];
  const updated=frame();
  assert(updated[0].state.roads.has('961,962'),'Road snapshots update the shared index');
  assert(!updated[0].state.roads.has('962,962'),'Unfinished roads do not speed workers');
  assert.equal(planCount(),14,'Completed roads update worker route costs');
  snapshot().roads[1].construction.progress=9;frame();
  assert.equal(planCount(),14,'Road construction progress preserves existing routes');
  delete snapshot().roads[1].construction;frame();
  assert.equal(planCount(),16,'Road completion replans worker routes');
  snapshot().roads.shift();frame();
  assert.equal(planCount(),18,'Road removal replans worker routes');
  snapshot().roads.push({x:1100,y:1100});frame();
  assert.equal(planCount(),18,'Distant roads cannot invalidate local routes');
`)(WorkerAnimation,assert,()=>plans);
console.log('Camera exploration worker layout checks passed');
