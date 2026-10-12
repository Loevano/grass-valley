// Run with: node tests/test_roads.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8'),elements=new Map(),fills=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const context=new Proxy({fill(){fills.push(this.fillStyle);}}, {get:(target,key)=>key in target?target[key]:()=>{}});
documentMock.getElementById('world').getContext=()=>context;
documentMock.getElementById('world').getBoundingClientRect=()=>({left:0,top:0,width:720,height:660});

const run=new Function('document','window','localStorage','assert','fills',source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
return (async()=>{
  const town={level:1,wood:0,coins:0,food:20,villagers:[],player:{x:5,y:7,facing:1},map_size:120,terrain:Array(120).fill('g'.repeat(120)),buildings:[{kind:'hall',x:5,y:5}],map_resources:[],roads:[{x:5,y:7}]};
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
  town.roads[0].construction={progress:9,total:10};
  assert.equal(roadSpeed(world.towns.own,town.player),1,'Unfinished shared roads do not boost walking');
  world={world_id:'roads',towns:{own:town}};
  const pending=predictPlayerStep(town,town.player,1,-1);
  assert(Math.abs(Math.hypot(pending.x-5,pending.y-7)-.3)<1e-9);
  delete town.roads[0].construction;
  world={world_id:'roads',towns:{own:town}};
  const drawBuildingOriginal=drawBuilding,previewBuildings=[];
  drawBuilding=(building,...args)=>{previewBuildings.push(building.kind);drawBuildingOriginal(building,...args);};
  tool='road';hoverTile={x:6,y:8};fills.length=0;draw();
  assert.equal(fills.filter(color=>color==='#b99c70').length,2,'Hover draws both the placed road and a road preview');
  assert.equal(fills.filter(color=>color==='#c6aa7e').length,2,'Road preview uses the same surface artwork as placed roads');
  assert.deepEqual(previewBuildings,['hall'],'Road hover never renders a house');
  assert(fills.includes('#d6d8a366'),'Valid road hover retains placement feedback');
  camera.zoom=.5;hoverTile={x:21,y:20};fills.length=0;draw();
  assert(fills.includes('#c76d6455'),'Invalid road hover retains placement feedback');
  assert.equal(fills.filter(color=>color==='#b99c70').length,2,'Invalid road hover still previews a road');
  tool='home';hoverTile={x:6,y:8};previewBuildings.length=0;draw();
  assert.deepEqual(previewBuildings,['hall','home'],'Cottage hover keeps its building artwork');
  drawBuilding=drawBuildingOriginal;hoverTile=null;
  let sent;request=async(path,data)=>{sent=data;return {};};refresh=async()=>{};render=()=>{};
  tool='road';await buildAt(6,8);
  assert.equal(sent.action,'build_road','Zero-cost road tool can place roads');
  assert.equal(sent.x,6);assert.equal(sent.y,8);
  tool=null;camera.zoom=1;await selectTile(5,7);
  assert.equal(selection.type,'road','Road tile opens its inspector');
  town.roads[0].construction={progress:3,total:10};
  renderSelection(town);
  assert(document.getElementById('selection-details').children.some(c=>c.textContent?.includes('7s of builder work remaining')));
})();`);
run(documentMock,{addEventListener(){}},{getItem:()=>null},assert,fills).then(()=>console.log('Road UI and prediction checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
const WorkerAnimation=require('../web/animations.js');
const state=()=>({x:5,y:7,path:[{x:6,y:7}],phase:'walking',clock:0});
const normal=state(),boosted=state();boosted.roads=new Set(['5,7']);
WorkerAnimation.advance(normal,.1,true);WorkerAnimation.advance(boosted,.1,true);
assert(Math.abs((boosted.x-5)/(normal.x-5)-1.5)<1e-9,'Settlers walk 50% faster on roads');
