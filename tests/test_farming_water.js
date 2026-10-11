const assert=require('assert');
const animation=require('../web/animations.js');
const farm={kind:'farm',x:2,y:2},field={kind:'acre',x:2,y:3,farm_index:0};
const town={map_size:16,buildings:[farm,field],map_resources:[{kind:'water',x:12,y:3}]};
function arrive(state){
  for(let i=0;i<500&&state.path.length;i++)animation.advance(state,.1,true);
}
let state=animation.plan(town,{job:'farming',building_index:1,work_target:field},0);
assert.equal(state.home,farm,'A worker assigned directly to an acre uses its parent farm');
arrive(state);
assert.equal(state.phase,'working');
animation.advance(state,2,true);
assert.equal(state.phase,'working','Sowing stays at the field');
assert.deepEqual({x:state.x,y:state.y},{x:field.x,y:field.y});
animation.advance(state,2,true);
assert.equal(state.phase,'fetching_water','Only the watering stage starts a water trip');
assert.equal(state.elapsed,4,'Water travel preserves crop progress');
assert(state.path.every(tile=>!state.blocked.has(`${tile.x},${tile.y}`)));
arrive(state);
assert.equal(state.phase,'collecting_water');
assert.equal(Math.abs(state.x-12)+Math.abs(state.y-3),1,'Farmer fills the can from the bank');
animation.advance(state,1,false);
assert.equal(state.waterElapsed,0,'Paused work does not fill the can');
animation.advance(state,.7,true);
assert.equal(state.phase,'returning_water');
arrive(state);
assert.equal(state.phase,'working');
assert(state.waterReady);
assert.equal(state.elapsed,4);
assert.deepEqual({x:state.x,y:state.y},{x:field.x,y:field.y},'Water is carried back to the same field');
animation.advance(state,4,true);
assert(['returning','depositing'].includes(state.phase),'Watering and harvesting finish before grain delivery');
arrive(state);
animation.advance(state,.7,true);
arrive(state);
assert.equal(state.elapsed,0);
assert(!state.waterReady,'Each crop cycle requires fresh water');

state=animation.plan({...town,map_resources:[]},{job:'farming',work_target:field},0);
arrive(state);
animation.advance(state,2,true);
assert.equal(state.phase,'working','Missing water does not prevent tilling or sowing');
animation.advance(state,2,true);
assert.equal(state.phase,'waiting_water','Missing water pauses the watering stage');
animation.advance(state,20,true);
assert.equal(state.elapsed,4,'Cannot skip watering when no water is reachable');

const enclosed={kind:'water',x:5,y:3};
state=animation.plan({...town,map_resources:[enclosed,...town.map_resources],buildings:[farm,field,
  ...[{x:4,y:3},{x:6,y:3},{x:5,y:2},{x:5,y:4}].map(tile=>({kind:'home',...tile}))]},
  {job:'farming',work_target:field},0);
arrive(state);
animation.advance(state,4,true);
assert.deepEqual(state.waterSource,town.map_resources[0],'Farmer finds farther water when the nearest bank is blocked');
assert.equal(state.phase,'fetching_water');
assert(!animation.plan(town,{job:'idle'},0,state).farming,'Changing jobs cancels the water cycle');
console.log('Farming water checks passed');
