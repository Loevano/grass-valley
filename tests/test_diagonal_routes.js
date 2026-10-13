const assert=require('node:assert/strict');
const Animation=require('../web/animations.js');
const Scene=require('../web/scene.js');
const start={x:0,y:0},target={x:3,y:3},blocked=new Scene.Overlay(new Set());
const route=(obstacles=blocked,diagonal=true)=>Animation.route(start,target,obstacles,8,false,null,null,'smooth',diagonal);
assert.equal(route(blocked,false).length,6,'Cardinal cache entries remain separate');
assert.deepEqual(route(),[{x:1,y:1},{x:2,y:2},{x:3,y:3}]);
const copy=route();copy[0].x=99;
assert.equal(route()[0].x,1,'Cached diagonal routes are immutable');
assert.deepEqual(route(new Set(['1,0']))[0],{x:0,y:1},'A blocked side prevents corner cutting');
assert.equal(route(new Set(['1,0','0,1'])),null,'Two blocked sides cannot be bypassed diagonally');
for(const mode of ['departure','arrival','smooth']){
  const path=Animation.route({x:0,y:3},{x:2,y:1},new Set(),12,true,null,new Set(['1,1']),mode,true);
  assert.deepEqual(path.at(-1),mode==='departure'?{x:2,y:2}:{x:1,y:1},'Diagonal road costs match host movement models');
}
const state={x:0,y:0,path:[{x:1,y:1}],phase:'walking',clock:0,elapsed:0,source:target};
Animation.advance(state,.2,true);
assert(Math.abs(Math.hypot(state.x,state.y)-1.8*.2)<1e-9,'Diagonal movement preserves walking speed');
assert(state.x>0&&state.y>0,'Both coordinates advance smoothly');
console.log('Diagonal villager route checks passed');
