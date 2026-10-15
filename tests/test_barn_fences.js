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
app.run(`for(let i=0;i<3;i++)gate=barnFenceSprites([barn],town,[{x:16,y:13.5}],.1).find(p=>p.kind==='barn_gate');`);
assert.equal(app.run('gate.opening'),1,'Gate opens smoothly before people cross');
assert(app.run(`gate.start.y<gate.pen.a.y-2`),'The open leaf slides beside the existing fence');
app.run(`for(let i=0;i<10;i++)gate=barnFenceSprites([barn],town,[],.1).find(p=>p.kind==='barn_gate');`);
assert.equal(app.run('gate.opening'),0,'The gate closes after people clear it');
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
console.log('Barn fence, orientation, animation and host/browser geometry checks passed');
