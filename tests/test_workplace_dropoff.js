const assert=require('assert'),fs=require('fs');
const animation=require('../web/animations.js');
const source=fs.readFileSync('web/app.js','utf8');
const harness=new Function('WorkerAnimation',`
  let world,viewed='own',workerTown=null;
  const workers=new Map();
  ${source.slice(source.indexOf('function animatedVillagers('),source.indexOf('let npcClock='))}
  return {frame(town,seconds){world={world_id:'dropoff-test',towns:{own:town}};
    return animatedVillagers(town,seconds)[0].state;}};
`)(animation);
const town={map_size:16,buildings:[{kind:'hall',x:5,y:5},{kind:'woodcutter',x:3,y:3}],
  map_resources:[{kind:'tree',x:3,y:1}],villagers:[{
    job:'woodcutting',building_index:1,work_target:{kind:'tree',x:3,y:1},
    production_status:'Workplace buffer full',pending_output:{wood:5},
    work_animation:{task:['woodcutting','woodcutter',3,3,'tree',3,1],
      x:3,y:2,path:[],phase:'depositing',elapsed:.4,waterReady:false,version:1,facing:1}
  }]};
let state=harness.frame(town,0);
assert.strictEqual(state.phase,'depositing');
assert.strictEqual(state.x,3);
assert.strictEqual(state.y,2);
harness.frame(town,1);
assert.strictEqual(state.phase,'depositing','Finish unloading even when the workplace buffer is full');
assert.strictEqual(state.elapsed,.65,'Wait at the drop-off until the host confirms it');
harness.frame(town,3);
assert.strictEqual(state.phase,'depositing','Polling cannot restart the next trip early');
const worker=town.villagers[0];
worker.work_animation={...worker.work_animation,version:2,phase:'walking',elapsed:0,path:[{x:2,y:2},{x:2,y:1}]};
worker.pending_output={wood:.5};
worker.production_status='Working';
state=harness.frame(town,0);
assert.strictEqual(state.phase,'walking','The host starts the next trip after crediting the delivery');
assert.strictEqual(state.path.length,2);
harness.frame(town,.1);
assert(state.x<3&&state.x>2,'The next trip moves smoothly from the drop-off tile');
console.log('Workplace drop-off animation checks passed');
