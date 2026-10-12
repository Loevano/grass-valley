// Run with: node tests/test_render_cache.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const WorkerAnimation=require('../web/animations.js');
const elements=new Map(),surfaces=[],fills=[],images=[],copies=[],texts=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
function context(surface,offscreen){
  return new Proxy({canvas:surface,fill(){if(offscreen){surface.paints++;(surface.fills ||= []).push(this.fillStyle);}else fills.push(this.fillStyle);},
    drawImage(...args){if(offscreen)copies.push({args,operation:this.globalCompositeOperation});else images.push(args);},
    fillText(text){if(!offscreen)texts.push(text);}}, {get:(target,key)=>key in target?target[key]:()=>{}});
}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},
  createElement(tag){if(tag!=='canvas')return element();const surface={paints:0};surface.getContext=()=>surface.ctx ||= context(surface,true);surfaces.push(surface);return surface;},querySelectorAll:()=>[]};
const canvas=documentMock.getElementById('world');canvas.getContext=()=>context(canvas,false);
let width=960,height=660,now=0;
canvas.getBoundingClientRect=()=>({left:0,top:0,width,height});
const windowMock={devicePixelRatio:2,addEventListener(){}};
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation','assert','surfaces','fills','images','copies','resize','performance','advanceTime','texts',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  let scheduledFrames=0;
  const requestAnimationFrame=()=>scheduledFrames++;
  animate(0);
  assert.equal(scheduledFrames,1,'Startup schedules the next animation frame before a town has loaded');
  surfaces.length=0;fills.length=0;images.length=0;
  const town={map_size:120,terrain:Array(120).fill('g'.repeat(120)),map_resources:[{kind:'tree',x:31,y:31}],
    buildings:[{kind:'hall',x:30,y:30}],villagers:[{name:'Ada',job:'idle'}],food:40,wood:40,level:1};
  world={world_id:'cache-test',towns:{own:town}};viewed='own';cameraTown='cache-test/own';camera={x:30,y:30,zoom:1};
  const groundSurfaces=()=>surfaces.filter(surface=>![...resourceSprites.values(),...buildingSprites.values()].includes(surface));
  draw();
  const highHits=hitTargets.map(hit=>({...hit})),highSurface=groundCache.surface;
  const changeQuality=quality=>{$('graphics-quality').value=quality;$('graphics-quality').onchange();};
  changeQuality('low');
  assert.equal(canvas.width,720,'Low reduces the canvas resolution on Retina displays');
  assert.equal(groundCache.dpr,.75,'Low reduces cached ground resolution');
  assert.deepStrictEqual(hitTargets,highHits,'Quality preserves selection areas in CSS pixels');
  changeQuality('medium');
  assert.equal(canvas.width,960,'Medium caps the canvas at standard resolution');
  assert.equal(groundCache.dpr,1,'Medium uses standard ground resolution');
  window.devicePixelRatio=1;changeQuality('high');
  assert.equal(canvas.width,960,'High follows a standard display');
  changeQuality('low');assert.equal(canvas.width,720,'Low also scales down standard displays');
  window.devicePixelRatio=2;changeQuality('high');
  assert.equal(canvas.width,1920,'High restores Retina resolution immediately');
  assert.equal(groundCache.dpr,2,'High restores sharp ground after changing quality');
  assert.strictEqual(groundCache.surface,highSurface,'Quality changes reuse the ground surface');
  assert.equal(groundSurfaces().length,1);const surface=groundCache.surface,painted=surface.paints;
  assert(painted>0,'Ground is painted into the cache');
  assert.equal(surface.width,(960+768)*2,'Terrain is pre-rendered 384 pixels beyond each viewport edge');
  assert.equal(surface.height,(660+768)*2,'The larger buffer covers vertical scrolling too');
  assert(!fills.includes('#68865a'),'Ground is composited instead of painted on the main canvas');
  draw();assert.equal(surface.paints,painted,'Standing still reuses ground');
  camera.x+=1;draw();assert.equal(surface.paints,painted,'Small camera movements reuse padded ground');
  assert.equal(images.filter(args=>args[0]===surface).at(-1)[1],-384-tileW/2,'Cached ground follows horizontal camera translation');
  assert.equal(images.filter(args=>args[0]===surface).at(-1)[2],-384-tileH/2,'Cached ground follows vertical camera translation');
  assert(hitTargets.some(t=>t.type==='resource'&&t.index===0),'Resource selection indices survive caching');
  world=JSON.parse(JSON.stringify(world));draw();
  assert.equal(surface.paints,painted,'Polling unchanged terrain preserves ground');
  camera.x-=1;draw();
  let beforePan=surface.paints;
  camera.x+=4;camera.y-=4;draw();
  assert.equal(copies.length,1,'Panning refills the buffer before its edge reaches the viewport');
  assert.equal(copies[0].operation,'copy','Transparent map edges replace old pixels');
  assert.equal(copies[0].args[5],0,'Leftward ground movement copies to the left edge');
  assert.equal(copies[0].args[1],192,'Routine refills are limited to 96 CSS pixels');
  assert(surface.paints-beforePan<painted*.15,'Refills paint only a small exposed strip');
  const coversViewport=()=>{
    const image=images.filter(args=>args[0]===surface).at(-1);
    assert(image[1]<=0&&image[2]<=0&&image[1]+image[3]>=960&&image[2]+image[4]>=660,
      'Pre-rendered ground covers the entire viewport during fast scrolling');
  };
  for(let frame=0;frame<40;frame++){
    camera.x+=40/tileW;camera.y-=40/tileW;draw();coversViewport();
  }
  beforePan=surface.paints;
  for(let frame=0;frame<40;frame++){
    camera.x-=40/tileW;camera.y+=40/tileW;draw();coversViewport();
  }
  assert(surface.paints>beforePan,'Reverse panning replenishes the buffer');
  assert(copies.some(copy=>copy.args[5]===192),'Reverse refills shift pixels to the right');
  camera.x+=10;camera.y+=10;draw();coversViewport();
  assert(copies.at(-1).args[2]>0,'Vertical scrolling replenishes the buffer');
  camera.x+=7.123;camera.y+=7.321;draw();coversViewport();
  assert(copies.at(-1).args.slice(1).every(Number.isInteger),'Fractional camera positions copy whole device pixels');
  beforePan=surface.paints;camera.x+=30;camera.y-=30;draw();coversViewport();
  assert(surface.paints>beforePan,'Large camera jumps catch up immediately');
  camera={x:30,y:30,zoom:1};draw();
  // Placement mode retains detailed interactions even at distant zoom.
  tool='move';
  let previous=surface.paints;camera.zoom=.35;draw();assert(surface.paints>previous,'Zoom repaints ground');
  previous=surface.paints;draw();assert.equal(surface.paints,previous,'Repeated zoomed-out frames reuse ground');
  assert.equal(groundCache.dpr,1,'Distant ground uses a smaller texture on Retina displays');
  assert.equal(canvas.width,1920,'Live artwork keeps the full display resolution');
  for(const type of ['resource','building','villager'])
    assert(hitTargets.some(target=>target.type===type),'Simplified distant artwork preserves '+type+' selection');
  camera.zoom=.36;draw();
  assert.equal(surface.paints,previous,'Small zoom steps scale cached ground during the gesture');
  const zoomImage=images.filter(args=>args[0]===surface).at(-1),ratio=tileW/groundCache.tileW;
  const cachedX=384+groundCache.originX,cachedY=384+groundCache.originY+15*groundCache.tileW;
  const live=point(30,30);
  assert(Math.abs(zoomImage[1]+cachedX*ratio-live[0])<1e-6,'Scaled ground aligns with live objects horizontally');
  assert(Math.abs(zoomImage[2]+cachedY*ratio-live[1])<1e-6,'Scaled ground aligns with live objects vertically');
  advanceTime(151);draw();
  assert(surface.paints>previous,'Ground repaints at the final resolution after zoom settles');
  assert.equal(groundCache.tileW,tileW,'The settled zoom uses a sharp ground texture');
  tool=null;camera.zoom=.6;draw();previous=surface.paints;
  assert(groundCache.detailed,'Normal zoom retains ground decorations');
  assert.equal(groundCache.dpr,2,'Close-up ground restores Retina resolution');
  camera.zoom=.59;draw();
  assert(surface.paints>previous&&!groundCache.detailed,'Crossing the detail threshold repaints simpler terrain immediately');
  previous=surface.paints;camera.zoom=.6;draw();
  assert(surface.paints>previous&&groundCache.detailed,'Zooming back in restores terrain details immediately');
  window.devicePixelRatio=1;draw();assert(surface.paints>previous,'Retina scale changes repaint ground');
  previous=surface.paints;resize();draw();assert(surface.paints>previous,'Window resizing repaints ground');
  panel='build';fills.length=0;draw();assert(fills.includes('#e4c98022'),'Build range remains above cached terrain');
  panel=null;fills.length=0;draw();assert(!fills.includes('#e4c98022'),'Closing Build clears overlays without leaving marks');
  world=JSON.parse(JSON.stringify(world));world.towns.own.terrain[30]='b'.repeat(120);draw();
  assert.equal(groundSurfaces().length,2,'Terrain changes discard stale ground');
  world.towns.friend={...world.towns.own,terrain:Array(120).fill('s'.repeat(120)),map_resources:[]};
  viewed='friend';draw();assert.equal(groundSurfaces().length,3,'Switching separate maps discards stale ground');
  world=JSON.parse(JSON.stringify(world));world.world_id='another-world';draw();
  assert.equal(groundSurfaces().length,4,'Changing worlds invalidates ground');
  const beforeDistant=groundCache,paintedDistant=beforeDistant.surface.paints;
  world=JSON.parse(JSON.stringify(world));world.towns.friend.terrain[119]='b'.repeat(120);
  world.towns.friend.map_resources.push({kind:'water',x:119,y:119});draw();
  assert.strictEqual(groundCache,beforeDistant,'Distant terrain changes preserve the cached viewport');
  assert.equal(beforeDistant.surface.paints,paintedDistant,'Distant water changes do not repaint the viewport');
  world=JSON.parse(JSON.stringify(world));world.towns.friend.map_resources=[{kind:'water',x:30,y:30,supply:10}];draw();
  const waterSurface=groundCache.surface,waterPaints=waterSurface.paints,waterSurfaces=groundSurfaces().length;
  world=JSON.parse(JSON.stringify(world));world.towns.friend.map_resources[0].supply=20;draw();
  assert.equal(groundSurfaces().length,waterSurfaces,'Supply updates preserve the terrain cache');
  assert.equal(waterSurface.paints,waterPaints,'Supply updates do not repaint water');
  world=JSON.parse(JSON.stringify(world));world.towns.friend.map_resources=[];draw();
  assert.equal(groundSurfaces().length,waterSurfaces+1,'Water layout changes invalidate the terrain');
  buildingSprites.clear();buildingSpriteBytes=0;
  const cottage={kind:'home',x:30,y:30},owner={level:1,map_resources:[]};
  tileW=54;tileH=27;
  drawBuilding(cottage,owner);
  const cottageSprite=images.at(-1)[0],cottagePaints=cottageSprite.paints;
  const allocations=surfaces.length;
  for(let frame=0;frame<100;frame++)drawBuilding({...cottage,x:30+frame/100},owner);
  assert.equal(surfaces.length,allocations,'Completed buildings share artwork across frames and positions');
  assert.equal(cottageSprite.paints,cottagePaints,'Warm building frames do not repaint paths');
  drawBuilding({...cottage,level:2},owner);
  assert.notStrictEqual(images.at(-1)[0],cottageSprite,'Completed structural upgrades select new artwork');
  drawBuilding({...cottage,settler_level:2},owner);
  assert.notStrictEqual(images.at(-1)[0],cottageSprite,'Capacity upgrades select new markers');
  drawBuilding({...cottage,profession_upgrades:['timber_frame']},owner);
  assert.notStrictEqual(images.at(-1)[0],cottageSprite,'Profession upgrades select new artwork');
  const woodcutter={kind:'woodcutter',x:30,y:30};
  drawBuilding(woodcutter,owner);const unusableSprite=images.at(-1)[0];
  drawBuilding(woodcutter,{...owner,map_resources:[{kind:'tree',x:31,y:30}]});
  assert.notStrictEqual(images.at(-1)[0],unusableSprite,'Nearby resource changes update workplace colors');
  const barn={kind:'barn',x:30,y:30};
  drawBuilding(barn,owner);const emptyBarn=images.at(-1)[0];
  drawBuilding({...barn,animals:{chicken:2}},owner);
  assert.strictEqual(images.at(-1)[0],emptyBarn,'Live animals preserve the cached barn artwork');
  const animalTown={...town,buildings:[{kind:'barn',x:30,y:30,animals:{chicken:2,sheep:1,pig:1,cow:1}}],villagers:[]};
  world={world_id:'cache-test',towns:{own:animalTown}};viewed='own';cameraTown='cache-test/own';camera={x:30,y:30,zoom:1};
  const originalAnimalDraw=drawBarnAnimal,animalFrames=[];
  drawBarnAnimal=animal=>{animalFrames.push({...animal});originalAnimalDraw(animal);};
  draw(.1);
  assert.equal(animalFrames.length,5,'The live scene draws every stocked animal');
  const firstAnimals=JSON.stringify(animalFrames),herdAllocations=surfaces.length;
  animalFrames.length=0;draw(.2);
  assert.notEqual(JSON.stringify(animalFrames),firstAnimals,'The frame loop advances animal poses');
  assert.equal(surfaces.length,herdAllocations,'Animating the herd does not allocate new cached artwork');
  assert(hitTargets.some(target=>target.type==='building'&&target.index===0),'The barn remains selectable behind its animals');
  drawBarnAnimal=originalAnimalDraw;
  const hall={kind:'hall',x:30,y:30};
  drawBuilding(hall,owner);const firstHall=images.at(-1)[0];
  drawBuilding(hall,{...owner,level:5});
  assert.notStrictEqual(images.at(-1)[0],firstHall,'Hall artwork uses its owner level');
  const mill={kind:'mill',x:30,y:30};
  drawBuilding(mill,owner,{elapsed:1});const millSprite=images.at(-1)[0],millPaints=millSprite.paints;
  const oldStroke=ctx.stroke;let movingSails=0;ctx.stroke=()=>movingSails++;
  drawBuilding(mill,owner,{elapsed:2});drawBuilding(mill,owner,{elapsed:3});ctx.stroke=oldStroke;
  assert.strictEqual(images.at(-1)[0],millSprite,'Animated mill reuses its static artwork');
  assert.equal(millSprite.paints,millPaints,'Rotating sails do not repaint the building');
  assert.equal(movingSails,8,'Four sails still draw on each animated frame');
  const imageCount=images.length;
  drawBuilding({...cottage,construction:{progress:12,total:30}},owner);
  assert.equal(images.length,imageCount,'Construction progress draws directly');
  tileW=50;tileH=25;drawBuilding(cottage,owner);
  assert.strictEqual(images.at(-1)[0],cottageSprite,'Nearby zoom levels reuse the same resolution bucket');
  for(let resolution=1;resolution<=64;resolution*=2){
    tileW=46*resolution;tileH=tileW/2;
    for(let level=1;level<=5;level++)drawBuilding(hall,{...owner,level});
  }
  const buildingBytes=[...buildingSprites.values()].reduce((sum,sprite)=>sum+sprite.width*sprite.height*4,0);
  assert.equal(buildingSpriteBytes,buildingBytes,'Building eviction tracks texture memory');
  assert(buildingBytes<=buildingSpriteByteLimit&&buildingSprites.size<=128,'Building textures remain bounded at high resolution');
  const forestTrees=[];
  for(let y=28;y<=32;y++)for(let x=28;x<=32;x++)if(x!==30||y!==30)forestTrees.push({kind:'tree',x,y});
  const forestTown={map_size:120,terrain:Array(120).fill('g'.repeat(120)),
    buildings:[],villagers:[],map_resources:[...forestTrees,{kind:'rock',x:35,y:30},{kind:'rock',x:36,y:30},{kind:'chest',x:34,y:34}],level:1};
  world={world_id:'forest-test',towns:{own:forestTown}};viewed='own';cameraTown='forest-test/own';
  camera={x:30,y:30,zoom:.35};tool='move';
  const resourceDraws=[],originalDrawResource=drawResource;
  drawResource=(resource,...args)=>{resourceDraws.push(resource.kind);originalDrawResource(resource,...args);};
  draw();
  assert(!resourceDraws.includes('tree')&&!resourceDraws.includes('rock'),'Overview skips all tree and rock sprite draws');
  assert(resourceDraws.includes('chest'),'Overview keeps useful loot markers visible');
  assert(groundCache.surface.fills.includes('#3f6045'),'Trees become a forest canopy inside cached ground');
  assert.equal(resourceCache.rockJoins.size,0,'Overview avoids calculating rock geometry');
  assert(resourceCache.forest.has('30,30'),'Small gaps between neighboring trees join the forest canopy');
  const forestSurface=groundCache.surface,forestPaints=forestSurface.paints;
  draw();assert.equal(forestSurface.paints,forestPaints,'Stationary overview reuses the forest canopy');
  const originalLayout=groundMatchesCache;let layoutScans=0;
  groundMatchesCache=(...args)=>{layoutScans++;return originalLayout(...args);};
  const oldCanopy=resourceCache.forest;
  world={...world,towns:{own:{...world.towns.own}}};draw();
  assert.equal(layoutScans,0,'Metadata-only polls avoid scanning the cached ground');
  world={...world,towns:{own:{...world.towns.own,map_resources:world.towns.own.map_resources.map(r=>({...r,supply:5}))}}};
  draw();
  assert.strictEqual(resourceCache.forest,oldCanopy,'Supply-only polls preserve the forest canopy');
  assert.equal(layoutScans,0,'Supply-only polls avoid scanning unchanged ground');
  groundMatchesCache=originalLayout;
  const originalPoint=point;let projections=0;
  point=(...args)=>{projections++;return originalPoint(...args);};
  selection=null;draw();const unselectedProjections=projections;
  selection={type:'resource',index:0};projections=0;fills.length=0;draw();
  assert(projections<=unselectedProjections+2,'Selecting one resource avoids traversing all viewport tiles');
  assert(fills.includes('#fff0b133'),'The selected resource still receives its tile highlight');
  point=originalPoint;selection=null;

  const treeHit=hitTargets.find(target=>target.type==='resource'&&target.index===0);
  assert(treeHit,'Forest tiles remain selectable');
  assert(treeHit.bottom-treeHit.top<=tileH,'Invisible trees use tile-sized selection areas');
  assert(hitTargets.some(target=>target.type==='resource'&&target.index===forestTrees.length),'Hidden rocks remain selectable on their tile');
  camera.zoom=1;resourceDraws.length=0;draw();
  assert(resourceDraws.includes('tree')&&resourceDraws.includes('rock'),'Zooming in restores individual tree and rock artwork');
  assert.equal(resourceCache.rockJoins.size,2,'Zooming in restores joined rock geometry');
  camera.zoom=.35;draw();
  const oldForest=groundCache.surface;
  world=JSON.parse(JSON.stringify(world));world.towns.own.map_resources=world.towns.own.map_resources.filter(resource=>resource.kind!=='tree');
  draw();
  assert.notStrictEqual(groundCache.surface,oldForest,'Removing trees invalidates the cached forest canopy');
  assert(!groundCache.surface.fills.includes('#3f6045'),'Removed woodland leaves no stale canopy');
  drawResource=originalDrawResource;

  const labelTown={name:'Neighbor',level:1,map_size:120,terrain:Array(120).fill('g'.repeat(120)),
    buildings:[{kind:'hall',x:30,y:30}],villagers:[],map_resources:[],player:{x:31,y:30}};
  world={world_id:'labels',shared_map:{terrain:labelTown.terrain,map_size:120,map_resources:[]},towns:{friend:labelTown}};
  viewed='friend';identity=null;tool=null;cameraTown='labels/shared';camera={x:30,y:30,zoom:.35};
  playerSprite=()=>null;drawVillager=()=>{};remotePlayerSprite=(id,t)=>({...t.player,kind:'player',townId:id});
  texts.length=0;draw();
  assert.equal(texts.filter(text=>text==='Neighbor').length,1,'Overview labels the hall once and hides the founder');
  selection={type:'player',townId:'friend',index:0};texts.length=0;draw();
  assert(texts.includes('Neighbor'),'Overview retains the town marker when founder selection changes');
  selection={type:'building',index:0};texts.length=0;draw();
  assert(texts.includes('Neighbor'),'Selected overview halls retain their town name');
  selection=null;camera.zoom=1;texts.length=0;draw();
  assert.equal(texts.filter(text=>text==='Neighbor').length,2,'Close zoom restores hall and founder labels');

`);
run(documentMock,windowMock,{getItem:()=>null},()=>0,()=>{},WorkerAnimation,assert,surfaces,fills,images,copies,()=>{width=720;height=500;},{now:()=>now},ms=>{now+=ms;},texts);
console.log('Render cache checks passed');
