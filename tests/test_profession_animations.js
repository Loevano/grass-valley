// Run with: node tests/test_profession_animations.js
const assert=require('assert'),fs=require('fs');
const animation=require('../web/animations.js');
const source=fs.readFileSync('web/app.js','utf8'),marks=[];
const ctx=new Proxy({}, {get(target,key){
  if(key in target)return target[key];
  return (...args)=>marks.push([key,...args,target.fillStyle,target.strokeStyle]);
}});
const draw=new Function('ctx','polygon','point','tileW','tileH','npcColors',
  source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+'\nreturn drawVillager;'
)(ctx,(points,color)=>marks.push(['polygon',points,color]),(x,y)=>[(x-y)*23,(x+y)*11.5],46,23,{});
const appearances=[];
for(const [job,kind] of Object.entries({husbandry:'barn',woodworking:'sawpit',milling:'mill',preserving:'smokehouse',crafting:'workshop',breeding:'fish_pond',orcharding:'orchard',baking:'bakery',butchery:'butcher'})){
  const building={kind,x:5,y:5};
  const town={map_size:12,buildings:[{kind:'hall',x:8,y:8},building],map_resources:[]};
  // Host targets are independent snapshots of the assigned workplace.
  const villager={job,building_index:1,work_target:{...building}};
  const state=animation.plan(town,villager,0);
  assert(state.stationary,job+' works at its own workplace');
  for(let i=0;i<200&&state.path.length;i++)animation.advance(state,.1,true);
  assert.strictEqual(state.phase,'working');
  assert.strictEqual(state.home,building);
  assert.strictEqual(Math.abs(state.x-5)+Math.abs(state.y-5),1,'Worker stands beside the workplace');
  function render(active=true){marks.length=0;ctx.fillStyle='';ctx.strokeStyle='';draw({kind:'villager',index:0,x:state.x,y:state.y,state,villager,active});return JSON.stringify(marks);}
  const first=render();appearances.push(first);
  animation.advance(state,.23,true);
  assert.notStrictEqual(render(),first,job+' moves its own tools and materials');
  const elapsed=state.elapsed,paused=render(false);
  animation.advance(state,2,false);
  assert.strictEqual(state.elapsed,elapsed);
  assert.strictEqual(render(false),paused,job+' stops when production is paused');
  animation.advance(state,3.2,true);
  assert.strictEqual(state.phase,'depositing',job+' deposits at the same workplace');
  const delivery=render();animation.advance(state,.2,true);
  assert.notStrictEqual(render(),delivery,job+' visibly deposits its output');
  animation.advance(state,.5,true);
  assert.strictEqual(state.phase,'working',job+' resumes without walking to a farm');
  assert.strictEqual(state.path.length,0);
  const resting=animation.plan(town,{job:'idle'},0,state);
  assert(!resting.stationary&&!resting.source,'Unassigning cancels the profession animation');
}
assert.strictEqual(new Set(appearances).size,9,'All nine professions show distinct actions');
console.log('Profession animation checks passed');
