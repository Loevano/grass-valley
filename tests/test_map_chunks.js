const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const helpers=source.slice(source.indexOf('let chunkTerrainWorld='),source.indexOf('const workers ='));
const run=new Function('assert',`
  const canvas={getBoundingClientRect:()=>({width:960,height:660})};
  let world={world_id:'first',shared_map:{chunked:1,chunk_size:40,map_size:1920}};
  let camera={x:960,y:960},tileW=54;
  ${helpers}
  const rows=Array(40).fill('g'.repeat(40));
  const resource={kind:'tree',x:960,y:960};
  const snapshot=(id,keys,terrain={},resources={})=>({world_id:id,shared_map:{
    chunked:1,chunk_size:40,map_size:1920,visible_chunks:keys,
    terrain_chunks:terrain,resource_chunks:resources}});
  const first=snapshot('first',['24,24'],{'24,24':rows},{'24,24':{
    version:'0123456789abcdef',map_resources:[resource],tree_stumps:[]}});
  hydrateMapChunks(first);
  assert.equal(first.shared_map.terrain[960][960],'g');
  assert.equal(first.shared_map.terrain[0]?.[0],undefined,'Unloaded tiles stay unknown');
  assert.deepEqual(first.shared_map.map_resources,[resource]);
  const cached=snapshot('first',['24,24']);
  hydrateMapChunks(cached);
  assert.equal(cached.shared_map.terrain[960][960],'g','Reuse cached terrain');
  assert.deepEqual(cached.shared_map.map_resources,[resource],'Reuse cached resources');
  const removed=snapshot('first',['24,24'],{}, {'24,24':{
    version:'fedcba9876543210',map_resources:[],tree_stumps:[]}});
  hydrateMapChunks(removed);
  assert.deepEqual(removed.shared_map.map_resources,[],'Removal replaces the chunk contents');
  for(let x=0;x<48;x++)for(let y=0;y<4;y++){
    const key=x+','+y;
    hydrateMapChunks(snapshot('first',[key],{[key]:rows},{[key]:{
      version:'0123456789abcdef',map_resources:[],tree_stumps:[]}}));
  }
  assert.equal(chunkTerrainCache.size,128,'Terrain cache is bounded');
  assert.equal(chunkResourceCache.size,128,'Resource cache is bounded');
  hydrateMapChunks(snapshot('second',['24,24']));
  assert.equal(chunkTerrainCache.size,0,'Changing worlds discards the old terrain');
  assert.equal(chunkResourceCache.size,0,'Changing worlds discards the old resources');
  assert.equal(mapChunkKeys().length,9);
  tileW=10;assert.equal(mapChunkKeys().length,49,'Zooming out has a bounded request');
  camera={x:0,y:0};assert(mapChunkKeys().every(key=>key.split(',').every(n=>Number(n)>=0)));
  camera={x:1919,y:1919};assert(mapChunkKeys().every(key=>key.split(',').every(n=>Number(n)<48)));
`);
run(assert);
const movement=source.slice(source.indexOf('function playerMovementDistance('),source.indexOf('function playerSprite('));
new Function('assert',`
  let movementMap=null;
  const world={towns:{}},roadSpeed=()=>1,biomeProtected=()=>true;
  ${movement}
  const player={x:960,y:960,facing:1};
  const town={chunked:1,map_size:1920,terrain:{},map_resources:[],buildings:[],player};
  assert.equal(predictPlayerStep(town,player,1,0).x,960,'Prediction blocks unloaded ground');
  for(let y=958;y<=962;y++){town.terrain[y]={};for(let x=958;x<=962;x++)town.terrain[y][x]='g';}
  assert(predictPlayerStep(town,player,1,0).x>960,'Loaded open terrain allows movement');
`)(assert);
const WorkerAnimation=require('../web/animations.js');
let terrainReads=0;
const terrain=new Proxy({}, {get(){terrainReads++;return new Proxy({}, {get(){return 'g';}});}});
const town={chunked:1,map_size:1920,town_origin:{x:960,y:960},terrain,map_resources:[],npcs:[],
  buildings:[{kind:'hall',x:960,y:960}]};
const worker=WorkerAnimation.plan(town,{job:'idle'},0,null);
assert(terrainReads<20000,'Worker animation plans inspect only the town area');
assert(worker.bounds,'Subsequent animation routes retain the town bounds');
assert(WorkerAnimation.route({x:960,y:960},{x:1030,y:960},new Set(),1920,false,worker.bounds)===null,
  'Unreachable animation targets cannot search the entire expanded world');
console.log('Explorable map caching checks passed');

const elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(...children){this.children.push(...children);},
  replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){},focus(){},showModal(){},close(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},
  createElement:element,querySelectorAll:()=>[]};
const canvas=documentMock.getElementById('world');
canvas.getContext=()=>new Proxy({}, {get:()=>()=>{}});
canvas.getBoundingClientRect=()=>({left:0,top:0,width:960,height:660});
new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation','assert',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  return (async()=>{
    const rows=Array(40).fill('g'.repeat(40)),resource={kind:'rock',x:970,y:970};
    let packet={world_id:'streamed',name:'Wilderness',town_sites:[{x:960,y:960}],shared_map:{
      chunked:1,chunk_size:40,map_size:1920,terrain_seed:42,npcs:[],visible_chunks:['24,24'],
      terrain_chunks:{'24,24':rows},resource_chunks:{'24,24':{version:'0123456789abcdef',map_resources:[resource],tree_stumps:[]}}},
      towns:{own:{name:'Home',level:1,wood:65,food:40,coins:20,harvested:0,town_origin:{x:960,y:960},
        player:{x:962,y:962,facing:1},buildings:[{kind:'hall',x:960,y:960}],villagers:[]}}};
    identities.streamed={town_id:'own'};
    const queries=[];
    request=async path=>{
      if(path.startsWith('/api/world?')||path==='/api/world'){
        queries.push(path);return JSON.parse(JSON.stringify(packet));
      }
      return {worlds:[],can_manage:false};
    };
    await refresh();
    assert.equal(world.towns.own.map_size,1920);
    assert.equal(world.towns.own.terrain[960][960],'g','Refresh hydrates chunk terrain onto each town');
    selection={type:'resource',index:0};
    packet.shared_map.terrain_chunks={};
    packet.shared_map.resource_chunks={'23,24':{version:'fedcba9876543210',
      map_resources:[{kind:'tree',x:950,y:970}],tree_stumps:[]}};
    packet.shared_map.visible_chunks=['23,24','24,24'];
    await refresh();
    const query=new URLSearchParams(queries[1].split('?')[1]);
    assert(query.get('known').includes('24,24'),'Refresh advertises cached terrain');
    assert(query.get('resources').includes('24,24:0123456789abcdef'),'Refresh advertises resource versions');
    assert.equal(selection.index,1,'Selection stays on the same resource when new chunks change indexes');
    assert.equal(world.towns.own.map_resources[1].kind,'rock');
    packet.world_id='different';packet.shared_map.visible_chunks=['24,24'];
    packet.shared_map.terrain_chunks={'24,24':Array(40).fill('f'.repeat(40))};
    packet.shared_map.resource_chunks={'24,24':{version:'aaaaaaaaaaaaaaaa',map_resources:[],tree_stumps:[]}};
    await refresh();
    assert.equal(world.towns.own.terrain[960][960],'f','World switches replace terrain at the same coordinates');
    assert.equal(world.towns.own.map_resources.length,0,'World switches discard old resources');
  })();`)(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null,setItem(){}},()=>0,()=>{},WorkerAnimation,assert)
  .then(()=>console.log('Explorable map refresh checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
