// Run with: node tests/test_butcher_animation.js
const assert=require('assert'),fs=require('fs');
const animation=require('../web/animations.js');
const source=fs.readFileSync('web/app.js','utf8'),marks=[];
const ctx=new Proxy({}, {get(target,key){
  if(key in target)return target[key];
  return (...args)=>marks.push([key,...args,target.fillStyle,target.strokeStyle]);
}});
const draw=new Function('ctx','polygon','point','tileW','tileH','npcColors',
  source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+'return drawVillager;'
)(ctx,(points,color)=>marks.push(['polygon',points,color]),(x,y)=>[(x-y)*23,(x+y)*11.5],46,23,{});
const villager={job:'butchery',butchery:2};
const state={x:5,y:6,path:[],clock:2,elapsed:0,facing:1,source:{kind:'butcher',x:5,y:5},
  butcherAnimal:'pig',butcherPickup:{x:12,y:11}};
function render(phase,elapsed=0,species='pig'){
  marks.length=0;ctx.fillStyle='';ctx.strokeStyle='';
  draw({kind:'villager',index:0,x:state.x,y:state.y,villager,active:true,
    state:{...state,phase,elapsed,butcherAnimal:species,path:['walking','dragging'].includes(phase)?[{x:6,y:6}]:[]}});
  return JSON.stringify(marks);
}
const walking=render('walking'),finding=render('finding_animal'),dragging=render('dragging');
assert(!walking.includes('#702d32')&&!finding.includes('#702d32')&&!dragging.includes('#702d32'),'Blood appears only during processing at the shop');
assert(finding.includes('#bca17b')&&dragging.includes('#bca17b'),'Pickup and dragging show the rope');
assert(dragging.includes('["rotate",-0.65'),'Dragged animals lie on their side');
assert.notEqual(walking,finding,'The butcher stops beside the animal to find it');
assert.notEqual(finding,dragging,'Dragging has a distinct animal pose');
const first=render('working',0),later=render('working',7);
assert(later.includes('#702d32')&&later.includes('#9b3b40'),'Butchering leaves a shaded blood pool');
assert.notEqual(first,later,'Cleaver strokes and blood pool advance with the work');
assert.equal(new Set(['pig','cow','sheep','chicken'].map(species=>render('dragging',0,species))).size,4,'The selected species is visibly transported');
assert(!render('working',7,null).includes('#702d32'),'Recipe work without an animal has no blood pool');
const samples=[
  {x:12,y:11,time:0,phase:'finding_animal',elapsed:0,butcherAnimal:'pig',butcherPickup:{x:12,y:11}},
  {x:12,y:11,time:1.2,phase:'dragging',elapsed:0,butcherAnimal:'pig'},
  {x:11,y:11,time:2.2,phase:'working',elapsed:0,butcherAnimal:'pig'},
  {x:11,y:11,time:3,phase:'working',elapsed:.8,butcherAnimal:'pig'},
];
const host={...samples[0],task:['butcher_animal','first'],version:1,playback:samples};
const snapshot=JSON.stringify(host),visible={x:12,y:11,clock:0};
animation.playback(visible,host,0);
assert.equal(visible.butcherAnimal,'pig','Reload starts at the confirmed reserved animal');
animation.playback(visible,{...host,version:2},1.7);
assert.equal(visible.phase,'dragging');
assert.equal(visible.x,11.5,'Animal and butcher follow confirmed movement together');
assert.equal(visible.butcherAnimal,'pig');
animation.playback(visible,{...host,version:2},.7);
assert.equal(visible.phase,'working');
assert(Math.abs(visible.elapsed-.2)<1e-9);
const completed={...host,task:['recipe'],version:3,playback:[
  {x:11,y:11,time:0,phase:'working',elapsed:0},
  {x:11,y:11,time:3,phase:'working',elapsed:3},
]};
animation.playback(visible,completed,1);
assert.equal(visible.butcherAnimal,undefined,'Completing or cancelling the animal task clears its artwork');
assert.equal(JSON.stringify(host),snapshot,'Playback never changes authoritative snapshots');
console.log('Butcher animation checks passed');
