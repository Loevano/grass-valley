// Run with: node tests/test_worker_highlights.js
const assert=require('node:assert/strict');
const client=require('./client_harness')();
client.run(`
  const terrain=Array(30).fill('g'.repeat(30));
  const town={map_size:30,terrain,map_resources:[],npcs:[],roads:[],level:1,wood:65,food:40,coins:20,
    buildings:[{kind:'hall',x:3,y:3},{kind:'woodcutter',x:5,y:5},{kind:'woodcutter',x:8,y:5},
      {kind:'farm',x:6,y:7},{kind:'acre',x:7,y:7,farm_index:3},{kind:'acre',x:8,y:7,farm_index:2}],
    villagers:[{name:'Ada',job:'woodcutting',building_index:1},
      {name:'Bram',job:'woodcutting',building_index:1},{name:'Cora',job:'woodcutting',building_index:2},
      {name:'Dara',job:'farming',building_index:4},{name:'Erin',job:'farming',building_index:5}]};
  world={world_id:'highlights',shared_map:{map_size:30,terrain,map_resources:[],npcs:[]},
    towns:{own:town,friend:{...town,buildings:[],villagers:town.villagers.slice(0,1)}}};
  viewed='own';identity={town_id:'own'};
  const sprites=Object.entries(world.towns).flatMap(([townId,t])=>t.villagers.map((villager,index)=>({
    kind:'villager',townId,villager,index,x:4+index,y:9,active:false,
    state:{path:[],phase:'idle',clock:0,facing:1}})));
  frameActors={world,villagers:sprites,hero:null,otherPlayers:[]};
  const snapshot=JSON.stringify(world);
  let highlighted=[],strokes=[],path=[],outlines=[];
  ctx.drawImage=(surface,x,y,width,height)=>{
    if(surface===workerOutlineCanvas)outlines.push({x,y,width,height});
  };
  ctx.beginPath=()=>{path=[];};ctx.moveTo=(x,y)=>path.push([x,y]);ctx.lineTo=(x,y)=>path.push([x,y]);
  ctx.stroke=function(){strokes.push({color:this.strokeStyle,width:this.lineWidth,path:path.slice()});};
  const originalDrawVillager=drawVillager;
  drawVillager=(sprite,marked,target)=>{
    if(marked)highlighted.push(sprite.townId+'/'+sprite.index);
    originalDrawVillager(sprite,marked,target);
  };
  function paint(){
    highlighted=[];strokes=[];outlines=[];animationFrameRunning=true;draw();animationFrameRunning=false;
    return highlighted.slice();
  }
  paint();
`);
assert.deepEqual(Array.from(client.run('highlighted')),[],'Unselected buildings do not mark workers');
client.run(`
  const hit=hitTargets.find(h=>h.type==='building'&&h.index===1&&h.townId==='own');
  canvas.onclick({clientX:(hit.left+hit.right)/2,clientY:hit.top+2});
  paint();
`);
assert.deepEqual(Array.from(client.run('highlighted')),['own/0','own/1'],
  'Clicking a building marks all its assigned workers and excludes other workplaces and towns');
assert(client.run(`
  const [px,py]=point(sprites[0].x,sprites[0].y),scale=tileW/46;
  outlines.length>0&&outlines.some(o=>o.x<px-10*scale&&o.x+o.width>px+10*scale
    &&o.y<py+tileH*.65-23*scale&&o.y+o.height>py+tileH*.65+3*scale)
    &&!strokes.some(stroke=>stroke.color==='#fff0b1'&&stroke.width===2&&stroke.path.length===2);
`),'Assigned workers receive a silhouette outline around the whole pose without an overhead line');
assert.deepEqual(Array.from(client.run('sprites[0].x+=.5;paint();')),['own/0','own/1'],
  'Moving workers retain the highlight');
assert.deepEqual(Array.from(client.run("inspect('building',2);paint();")),['own/2'],
  'Changing the selected building replaces the worker highlights');
assert.deepEqual(Array.from(client.run("inspect('building',3);paint();")),['own/3'],
  'Selecting a farm includes workers assigned to its linked acres');
assert.deepEqual(Array.from(client.run("inspect('building',4);paint();")),['own/3'],
  'Selecting an acre marks its own worker');
assert.deepEqual(Array.from(client.run("inspect('villager',0);paint();")),[],
  'Selecting an individual villager clears building worker indicators');
assert.deepEqual(Array.from(client.run("inspect('building',1);openPanel(null);paint();")),[],
  'Closing the selection clears the worker indicators');
assert(client.run('outlines.length===0'),'Clearing selection removes the outline artwork');
assert(client.run('JSON.stringify(world)===snapshot'),'Highlighting never mutates authoritative state');
console.log('Building worker highlight checks passed');
