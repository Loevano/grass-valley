const assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const app=require('./client_harness')();
app.run(`
  const town={name:'Barn gates',level:3,map_size:28,sandbox:true,wood:100,food:40,coins:20,
    player:{x:7,y:12,facing:1},villagers:[],map_resources:[],npcs:[],roads:[],
    terrain:Array(28).fill('g'.repeat(28)),buildings:[{kind:'hall',x:5,y:5},{kind:'barn',x:12,y:12,animals:{chicken:1}}]};
  world={world_id:'barn-gates',towns:{own:town}};viewed='own';identity={town_id:'own'};
  const barn=town.buildings[1],original=JSON.stringify(world),commands=[];
  act=async command=>commands.push(command);
  inspect('building',1);
`);
assert.equal(app.run(`$('selection-fence').hidden`),false);
assert.equal(app.run(`$('selection-fence-orientation').children.length`),4);
app.run(`$('selection-fence-orientation').value='2';$('selection-fence-orientation').onchange();`);
assert.equal(app.run(`JSON.stringify(commands[0])`),JSON.stringify({action:'set_fence_orientation',index:1,orientation:2}));
assert.equal(app.run(`JSON.stringify(world)`),app.run('original'),'Controls and drawing never mutate authoritative snapshots');
app.run(`inspect('building',0);`);
assert.equal(app.run(`$('selection-fence').hidden`),true,'Other building inspectors hide orientation');
app.run(`identity={town_id:'friend'};inspect('building',1);`);
assert.equal(app.run(`$('selection-fence').hidden`),true,'Visitors cannot rotate another town’s gate');
app.run(`identity={town_id:'own'};`);
const layouts=JSON.parse(execFileSync('python3',['-c',`
import json, barns
result=[]
for x,y in [(12,12),(0,0),(27,27),(0,12),(27,12)]:
    for orientation in range(4):
        actual,a,b,segments=barns.layout(x,y,28,orientation)
        result.append([actual,a,b,segments])
print(json.dumps(result))
`],{encoding:'utf8',env:{...process.env,PYTHONPATH:'server'}}));
const browser=JSON.parse(app.run(`JSON.stringify([[12,12],[0,0],[27,27],[0,12],[27,12]].flatMap(([x,y])=>[0,1,2,3].map(orientation=>{
  const p=barnPen({x,y},28,orientation);return [p.orientation,[p.a.x,p.a.y],[p.end.x,p.end.y],p.segments];
})))`));
assert.deepEqual(browser,layouts,'Browser geometry exactly matches host geometry, including map edges');
app.run(`
  let position={x:8.1,y:12};
  for(let i=0;i<8;i++)position=predictPlayerStep(town,position,1,0);
`);
assert(app.run(`position.x<=8.32`),'Predicted movement stops before solid rails');
app.run(`position={x:16.2,y:13.5};for(let i=0;i<8;i++)position=predictPlayerStep(town,position,-1,0);`);
assert(app.run(`position.x<15`),'Prediction crosses the automatic gate');
app.run(`
  barnFenceSprites.states=new Map();
  let gates=barnFenceSprites([barn],town,[],0),gate=gates.find(p=>p.kind==='barn_gate');
`);
assert.equal(app.run('gate.opening'),0);
app.run(`
  const passer={kind:'villager',x:14.5,y:13,state:{path:[{x:14.5,y:14},{x:14,y:14}]}};
  for(let i=0;i<10;i++)gate=barnFenceSprites([barn],town,[passer],.1).find(p=>p.kind==='barn_gate');
`);
assert.equal(app.run('gate.opening'),0,'A keeper walking parallel to the gate must leave it closed');
app.run(`for(let i=0;i<3;i++)gate=barnFenceSprites([barn],town,[{kind:'villager',x:16,y:13.5,state:{path:[{x:15,y:13.5}]}}],.1).find(p=>p.kind==='barn_gate');`);
assert.equal(app.run('gate.opening'),1,'Gate opens smoothly before people cross');
assert(app.run(`gate.start.y<gate.pen.a.y-2`),'The open leaf slides beside the existing fence');
app.run(`for(let i=0;i<10;i++)gate=barnFenceSprites([barn],town,[passer],.1).find(p=>p.kind==='barn_gate');`);
assert.equal(app.run('gate.opening'),0,'The gate closes after a crossing even while a keeper passes nearby');
app.run(`
  const blocked=new Set(['12,12']);blocked.fences=barnFenceSegments([barn],28);
  const path=WorkerAnimation.route({x:7,y:12},{x:11,y:12},blocked,28,false,null,null,'smooth',true);
`);
assert(app.run(`path.some(p=>p.x>=16)`),'Browser route planning uses the gate');
assert(app.run(`path.every((p,i)=>!barnFenceCrossed(i?path[i-1]:{x:7,y:12},p,blocked.fences))`),'No corner or rail shortcuts');
assert.equal(app.run(`JSON.stringify(world)`),app.run('original'),'Gate animation and collision do not mutate snapshots');
app.run(`
  const hut={kind:'fishing_hut',x:22,y:22};
  const fishingTown={...town,buildings:[...town.buildings,hut],map_resources:[{kind:'water',x:24,y:22}]};
  const sailor={x:24,y:22,home:hut,source:{kind:'water',x:24,y:22},path:[],phase:'walking',
    sailing:{berth:{x:24,y:22},dock:{x:23,y:22},water:new Set(['24,22']),seaBounds:{minX:24,maxX:24,minY:22,maxY:22}}};
  WorkerAnimation.refreshNavigation(sailor,fishingTown,{});
`);
assert(app.run(`sailor.sailing.seaBlocked.fences.length>0`),'Refreshing a boat route preserves fence collision');
app.run(`{
  const check=(condition,message)=>{if(!condition)throw Error(message);};
  for(let orientation=0;orientation<4;orientation++){
    const pen=barnPen({...barn,fence_orientation:orientation},28);
    const at=(normal,along=1)=>({x:pen.a.x+pen.nx*normal+pen.tx*along,y:pen.a.y+pen.ny*normal+pen.ty*along});
    const person=(normal,along,route,kind='villager')=>({...at(normal,along),kind,state:{path:route.map(([n,t])=>at(n,t))}});
    for(const side of [-1,1]){
      check(!barnGatePassage(person(side,1,[[side,2],[side*1.5,2]]),pen),'Passing on either side must not open orientation '+orientation);
      check(!barnGatePassage(person(side,1,[]),pen),'Standing near the entrance must not open it');
      check(!barnGatePassage(person(side,1,[[side*1.5,1]]),pen),'Walking away must not open it');
      check(barnGatePassage(person(side,1,[[-side,1]]),pen),'Entry and exit must open orientation '+orientation);
    }
    check(!barnGatePassage(person(1,2.4,[[-1,2.4]]),pen),'A route crossing beyond the doorway must not trigger it');
    check(!barnGatePassage(person(1,1,[[1,4],[-1,4],[-1,1]]),pen),'A distant future crossing must not open it while passing');
    check(barnGatePassage(person(.05,1,[]),pen),'Keep the gate open for someone standing in the doorway');
    check(barnGatePassage(person(1,1,[[.9,1]],'player'),pen),'Short founder input steps must open the gate before arrival');
    check(!barnGatePassage(person(1,1,[[1,1.1]],'player'),pen),'A founder walking parallel must not open it');
    const crossing=person(-1,1,[[-.95,1]]);
    crossing.state.playbackQueue=[{index:0,time:0,samples:[at(-1),at(-.95),at(-.5),at(.5),at(1)]}];
    check(barnGatePassage(crossing,pen),'Read upcoming confirmed waypoints, not only the next short sample');
    crossing.state.path=[];
    check(!barnGatePassage(crossing,pen),'Working keepers must not open the gate for movement after their pause');
    const passing=person(-1,1,[[-1,1.5]]);
    passing.state.playbackQueue=[{index:1,time:1,samples:[at(1),at(-1),at(-1,1.5),at(-1,2)]}];
    check(!barnGatePassage(passing,pen),'Already played crossings must not reopen the gate');
    const local=person(1,1,[],'npc'),destination=at(-1);
    local.state.path=[[destination.x,destination.y]];
    check(barnGatePassage(local,pen),'Locals use coordinate-array waypoints');
    const remote=person(1,1,[],'player');remote.state.waypoints=[at(.5),at(-.5)];
    check(barnGatePassage(remote,pen),'Remote founder waypoint queues must open the entrance');
  }
}`);
assert.equal(app.run(`JSON.stringify(world)`),app.run('original'),'Passage checks preserve authoritative snapshots');
console.log('Barn fence, orientation, animation and host/browser geometry checks passed');
