// Run with: node tests/test_animations.js
const animation = typeof WorkerAnimation==='undefined'?require('../web/animations.js'):WorkerAnimation;
function check(condition,message){if(!condition)throw Error(message);}
const town={buildings:[{kind:'hall',x:5,y:5},{kind:'woodcutter',x:2,y:2}],
  map_resources:[{kind:'tree',x:0,y:2},{kind:'water',x:3,y:3},{kind:'rock',x:3,y:4}]};
const worker={job:'woodcutting',work_target:town.map_resources[0]};
let state=animation.plan(town,worker,0);
check(state.phase==='walking','Workers start by walking to the source');
check(state.path.every(p=>!state.blocked.has(`${p.x},${p.y}`)),'Paths avoid occupied tiles');
const initialX=state.x;
animation.advance(state,.1,true);
check(state.x!==initialX||state.y!==8,'Walking moves smoothly between tiles');
check(state.x%1!==0||state.y%1!==0,'Movement interpolates rather than teleporting');
for(let i=0;i<200&&state.path.length;i++)animation.advance(state,.1,true);
check(state.phase==='working','Arrival starts the work animation');
check(Math.abs(state.x-worker.work_target.x)+Math.abs(state.y-worker.work_target.y)===1,'Workers stand beside the source');
const elapsed=state.elapsed;
for(let i=0;i<50;i++)animation.advance(state,.1,false);
check(state.phase==='working'&&state.elapsed===elapsed,'Full storage pauses gathering');
for(let i=0;i<33;i++)animation.advance(state,.1,true);
check(state.phase==='returning','Gathering starts a delivery trip');
for(let i=0;i<200&&state.path.length;i++)animation.advance(state,.1,true);
check(state.phase==='depositing','Return trip reaches the workplace');
for(let i=0;i<7;i++)animation.advance(state,.1,true);
check(state.phase==='walking','Workers walk back for another gathering cycle');
const changed=animation.plan(town,{job:'idle'},0,state);
check(!changed.source&&changed.phase!=='working','Changing jobs cancels gathering');
const enclosure=new Set(['0,1','1,0']);
check(animation.route({x:0,y:0},{x:5,y:5},enclosure)===null,'Unreachable routes are detected');
const farm={buildings:[{kind:'farm',x:6,y:2}],map_resources:[{kind:'water',x:9,y:2}]};
state=animation.plan(farm,{job:'farming',work_target:farm.map_resources[0]},0);
for(let i=0;i<300&&state.path.length;i++)animation.advance(state,.1,true);
check(state.phase==='working'&&state.home.kind==='farm','Farmers walk to water and use the matching farm');
