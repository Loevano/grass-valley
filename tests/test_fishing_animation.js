// Run with: node tests/test_fishing_animation.js
const assert=require('assert');
const source=require('fs').readFileSync('web/app.js','utf8');
const animation=require('../web/animations.js');
const marks=[];
const ctx=new Proxy({}, {get(target,key){
  if(key in target)return target[key];
  return (...args)=>marks.push({method:key,args,color:key==='fillRect'?target.fillStyle:target.strokeStyle});
}});
const point=(x,y)=>[(x-y)*23,(x+y)*11.5];
const draw=new Function('ctx','polygon','point','tileW','tileH','npcColors',
  source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+
  '\nreturn drawVillager;'
)(ctx,(points,color)=>marks.push({method:'polygon',points,color}),point,46,23,{});
function render(state,active=true,index=0){
  marks.length=0;
  ctx.strokeStyle=undefined;ctx.fillStyle=undefined;
  draw({kind:'villager',index,x:state.x,y:state.y,state,active,villager:{job:'fishing'}});
  return marks.slice();
}
const state={x:5,y:5,path:[],phase:'working',clock:0,elapsed:0,facing:1,source:{x:6,y:5}};
const cast=render(state);
state.elapsed=1;
const waiting=render(state);
assert.notDeepStrictEqual(cast,waiting,'Casting moves the rod and float');
for(const [dx,dy] of [[1,0],[-1,0],[0,1],[0,-1]]){
  state.source={x:state.x+dx,y:state.y+dy};
  const output=render(state,true,5);
  const ripple=output.find(mark=>mark.method==='ellipse'&&mark.color==='#c3e1dc');
  const facing=Math.sign(dx-dy);
  assert(ripple,'Waiting produces a ripple');
  assert.strictEqual(ripple.args[0],((dx-dy)*23-3)*facing,'Float reaches the selected water horizontally');
  assert.strictEqual(ripple.args[1],(dx+dy)*11.5-23*.65-1,'Float reaches the selected water vertically');
}
state.elapsed=2.7;
assert(render(state).some(mark=>mark.color==='#b8d8d5'),'Reeling lifts a visible fish');
const paused=render(state,false);
state.clock+=5;
assert.deepStrictEqual(render(state,false),paused,'Full storage holds the fishing pose without continuing to cast');
state.phase='returning';state.path=[{x:4,y:5}];
assert(render(state).some(mark=>mark.color==='#b8d8d5'),'Delivery displays the caught fish');
assert(!render(state).some(mark=>mark.color==='#c3e1dc'),'Walking back removes the water ripple');

const town={buildings:[{kind:'fishing_hut',x:6,y:2}],map_resources:[{kind:'water',x:9,y:2}]};
const worker=animation.plan(town,{job:'fishing',work_target:town.map_resources[0]},0);
for(let i=0;i<300&&worker.path.length;i++)animation.advance(worker,.1,true);
animation.advance(worker,2.7,true);
assert(render(worker).some(mark=>mark.color==='#b8d8d5'),'Worker timing reaches the catch before leaving the water');
animation.advance(worker,.6,true);
assert.strictEqual(worker.phase,'returning','Catching leads to delivery');
for(let i=0;i<300&&worker.path.length;i++)animation.advance(worker,.1,true);
assert.strictEqual(worker.phase,'depositing');
animation.advance(worker,.7,true);
for(let i=0;i<300&&worker.path.length;i++)animation.advance(worker,.1,true);
assert.strictEqual(worker.phase,'working');
assert.strictEqual(worker.elapsed,0,'Returning to the water starts a fresh cast');
console.log('Fishing animation checks passed');
