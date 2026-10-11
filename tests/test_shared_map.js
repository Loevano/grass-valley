const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const WorkerAnimation=require('../web/animations.js');
const elements=new Map(),events={},drawnLabels=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(...children){this.children.push(...children);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(key,callback){events[key]=callback;},focus(){},showModal(){},close(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvas=documentMock.getElementById('world');
canvas.getContext=()=>new Proxy({}, {get(_,key){return key==='fillText'?(label)=>drawnLabels.push(label):()=>{};}});
canvas.getBoundingClientRect=()=>({left:0,top:0,width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation','assert','drawnLabels',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  function panelNodes(node=$('selection-details')){return Array.from(node.children || []).flatMap(child=>[child,...panelNodes(child)]);}

  return (async()=>{
    const landscape={map_size:120,terrain:Array(120).fill('g'.repeat(120)),map_resources:[],npcs:[],terrain_seed:42};
    const town=(name,x,y)=>({...landscape,name,level:1,wood:65,food:40,coins:20,harvested:0,town_origin:{x,y},
      player:{x,y:y+2,facing:1},buildings:[{kind:'hall',x,y}],villagers:[{name:'Ada',job:'idle',woodcutting:0,farming:0}]});
    world={world_id:'shared',name:'Shared valley',shared_map:landscape,town_sites:[{x:30,y:30}],towns:{own:town('Home',30,30),friend:town('Friend',80,30)}};
    identity={town_id:'own'};viewed='own';
    draw();
    assert.equal(camera.x,30,'Camera starts at the owning player');
    assert(workers.get('own/0').x>20,'Workers spawn by their own hall');
    assert(workers.get('friend/0').x>70,'Friend workers spawn by the friend hall');
    camera={x:55,y:30,zoom:.35};draw();
    assert(hitTargets.some(h=>h.type==='building'&&h.townId==='own'),'Own buildings are visible');
    assert(hitTargets.some(h=>h.type==='building'&&h.townId==='friend'),'Friend buildings are on the same map');
    assert(hitTargets.some(h=>h.type==='player'&&h.townId==='own'),'Own character is visible');
    assert(hitTargets.some(h=>h.type==='player'&&h.townId==='friend'),'Other character is visible');
    assert(drawnLabels.includes('Home')&&drawnLabels.includes('Friend'),'Town halls show town names');
    inspect('building',0,'friend');
    assert.equal(viewed,'friend');assert($('selection-move').hidden,'Cannot move a friend building');
    assert.equal(camera.x,55,'Inspecting a friend does not switch maps or reset camera');
    assert(canControlPlayer(),'Inspecting a friend still allows walking as yourself');
    const requests=[];
    request=async(path,data)=>{requests.push(data);return {player:{x:30.2,y:32,facing:1}};};
    walkKeys.add('d');playerSprite(world.towns.own,.1);await Promise.resolve();await Promise.resolve();
    assert.equal(requests[0].action,'walk');assert.equal(playerView.key,'shared/own','Walking controls own character');
    clearWalk();
    openPanel('build');assert.equal(viewed,'own','Build returns to your own town controls');
    assert(placementValid(world.towns.own,{x:31,y:30},'home'));
    assert(!placementValid(world.towns.own,{x:50,y:30},'home'),'Cannot build in wilderness');
    world.towns.own.level=2;
    assert(insideBuildingRange(world.towns.own,{x:45,y:30}),'Upgraded hall supports 15 steps');
    world.towns.own.buildings[0].x=35;
    assert(!insideBuildingRange(world.towns.own,{x:50,y:30}),'Moving a hall cannot expand original territory');
    for(const [level,reach] of [[3,20],[4,25],[5,30]]){
      const own=world.towns.own;own.level=level;own.buildings[0].x=30;
      assert.equal(buildingRange(own),reach);
      assert(placementValid(own,{x:30+reach,y:30},'home'),'Upgrades expand shared-map construction territory');
      own.buildings[0].x=31;
      assert(!insideBuildingRange(own,{x:31+reach,y:30}),'Moved halls remain bounded by upgraded original territory');
      own.buildings[0].x=30;
      assert(!insideBuildingRange(own,{x:30+reach,y:31}),'Diagonals count both axes at every level');
    }
    inspect('building',0,'own');
    assert(panelNodes().some(child=>child.textContent?.includes('Hall upgrade: +5 range')),'Hall explains range growth');
    draw();const friendWorker=workers.get('friend/0');
    draw();draw();assert.equal(workers.get('friend/0'),friendWorker,'Friend animation state survives rendering');
    const wire={world_id:'wire',name:'Wire world',shared_map:landscape,town_sites:[{x:30,y:30}],towns:{own:{...world.towns.own}}};
    delete wire.towns.own.terrain;delete wire.towns.own.map_resources;
    identities.wire={town_id:'own'};
    request=async path=>path==='/api/world'?wire:{worlds:[],can_manage:false};
    await refresh();assert.equal(world.towns.own.terrain,landscape.terrain,'Client hydrates the one shared landscape');
  })();`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null,setItem(){}},()=>0,()=>{},WorkerAnimation,assert,drawnLabels)
  .then(()=>console.log('Shared map checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
