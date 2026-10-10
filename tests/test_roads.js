// Run with: node tests/test_roads.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8'),elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
documentMock.getElementById('world').getContext=()=>new Proxy({}, {get:()=>()=>{}});

const run=new Function('document','window','localStorage','assert',source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
return (async()=>{
  const town={level:1,wood:0,coins:0,food:20,villagers:[],player:{x:5,y:7,facing:1},map_size:12,terrain:Array(12).fill('g'.repeat(12)),buildings:[{kind:'hall',x:5,y:5}],map_resources:[],roads:[{x:5,y:7}]};
  world={world_id:'roads',towns:{own:town}};identity={town_id:'own'};viewed='own';
  const boosted=predictPlayerStep(town,town.player,1,-1);
  assert(Math.abs(Math.hypot(boosted.x-5,boosted.y-7)-.45)<1e-9);
  const off=predictPlayerStep(town,{x:8,y:8},1,-1);
  assert(Math.abs(Math.hypot(off.x-8,off.y-8)-.3)<1e-9);
  playerView=null;playerSprite(town,0);playerView.target=boosted;
  const sprite=playerSprite(town,1/60);
  assert(Math.abs(Math.hypot(sprite.x-5,sprite.y-7)-4.5/60)<1e-9);
  world={...world,shared_map:{},towns:{own:{...town,roads:[]},friend:{buildings:[],roads:town.roads}}};
  assert.equal(roadSpeed(world.towns.own,town.player),1.5,'Visitors use another town roads');
  world={world_id:'roads',towns:{own:town}};
  let sent;request=async(path,data)=>{sent=data;return {};};refresh=async()=>{};render=()=>{};
  tool='road';await buildAt(6,8);
  assert.equal(sent.action,'build_road','Zero-cost road tool can place roads');
  assert.equal(sent.x,6);assert.equal(sent.y,8);
  tool=null;await selectTile(5,7);
  assert.equal(selection.type,'road','Road tile opens its inspector');
})();`);
run(documentMock,{addEventListener(){}},{getItem:()=>null},assert).then(()=>console.log('Road UI and prediction checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
const WorkerAnimation=require('../web/animations.js');
const state=()=>({x:5,y:7,path:[{x:6,y:7}],phase:'walking',clock:0});
const normal=state(),boosted=state();boosted.roads=new Set(['5,7']);
WorkerAnimation.advance(normal,.1,true);WorkerAnimation.advance(boosted,.1,true);
assert(Math.abs((boosted.x-5)/(normal.x-5)-1.5)<1e-9,'Settlers walk 50% faster on roads');
