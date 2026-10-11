const assert=require('assert'),fs=require('fs');
const animation=require('../web/animations.js');
const source=fs.readFileSync('web/app.js','utf8');
const harness=new Function('WorkerAnimation',`
  let world,viewed='own',workerTown=null;
  const workers=new Map();
  ${source.slice(source.indexOf('function animatedVillagers('),source.indexOf('let npcClock='))}
  return {frame(town,seconds){world={world_id:'delivery-test',towns:{own:town}};
    return animatedVillagers(town,seconds)[0].state;}};
`)(animation);
const farm={kind:'farm',x:8,y:3},field={kind:'acre',x:8,y:4,farm_index:1};
let town={map_size:16,buildings:[{kind:'hall',x:2,y:2},farm,field],
  map_resources:[{kind:'water',x:10,y:4}],villagers:[{
    name:'Bram',job:'farming',building_index:1,work_target:field,
    construction_position:{x:8,y:4},production_status:'Working',
  }]};
let state=harness.frame(town,0);
animation.advance(state,4,true);
animation.advance(state,.1,true);
assert(!Number.isInteger(state.x),'Start the delivery while Bram is between tiles');
const before={x:state.x,y:state.y};
const path=[[8,4],[7,4],[6,4],[5,4],[4,4],[3,4],[2,4],[2,3],
  [2,4],[3,4],[4,4],[5,4],[6,4],[7,4],[8,4]];
let bram=town.villagers[0];
bram.production_status='Hauling outputs';
bram.logistics_trip={operation:'collect',workplace:farm,storage:town.buildings[0],
  path,step:5,delivered:false};
bram.logistics_position={x:3,y:4};
bram.logistics_target={kind:'delivery',x:2,y:4};
state=harness.frame(town,0);
assert.deepStrictEqual({x:state.x,y:state.y},before,'Starting delivery preserves the visible position');
assert(state.path.some(p=>p.x===2&&p.y===3),'Animation includes the storage visit');
const delivery=state;
const visited=new Set(),stages=new Set();
for(let frame=0;frame<350;frame++){
  const previous={x:state.x,y:state.y};
  if(frame===30){
    town=JSON.parse(JSON.stringify(town));bram=town.villagers[0];
    bram.logistics_trip.step=10;bram.logistics_trip.delivered=true;
    bram.logistics_position={x:4,y:4};bram.logistics_target={kind:'delivery',x:5,y:4};
    assert.strictEqual(harness.frame(town,0),delivery,'Host progress preserves the full animated route');
  }
  if(frame===60){
    delete bram.logistics_trip;delete bram.logistics_target;
    bram.logistics_position={x:8,y:4};bram.production_status='Working';
    assert.strictEqual(harness.frame(town,0),delivery,'Finish the visible delivery before resuming field work');
  }
  state=harness.frame(town,.1);
  assert(Math.hypot(state.x-previous.x,state.y-previous.y)<=.180001,'Polling never teleports Bram');
  if(Number.isInteger(state.x)&&Number.isInteger(state.y))visited.add(`${state.x},${state.y}`);
  if(!state.transport&&state.phase==='working')stages.add(Math.min(3,Math.floor(state.elapsed/2)));
}
assert(visited.has('2,3'),'Bram walks all the way to storage');
assert.deepStrictEqual([...stages].sort(),[0,1,2,3],'Bram resumes the complete farming cycle');

const previous={x:state.x,y:state.y};
bram.job='idle';bram.work_target=null;
state=harness.frame(town,0);
assert.deepStrictEqual({x:state.x,y:state.y},previous,'Reassignment preserves the visible position');
assert(!state.transport&&!state.farming,'Reassignment cancels delivery and farming');
console.log('Delivery animation checks passed');
