// Run with: node tests/test_render_cache.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const WorkerAnimation=require('../web/animations.js');
const elements=new Map(),surfaces=[],fills=[],images=[],copies=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
function context(surface,offscreen){
  return new Proxy({canvas:surface,fill(){if(offscreen)surface.paints++;else fills.push(this.fillStyle);},
    drawImage(...args){if(offscreen)copies.push({args,operation:this.globalCompositeOperation});else images.push(args);}}, {get:(target,key)=>key in target?target[key]:()=>{}});
}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},
  createElement(tag){if(tag!=='canvas')return element();const surface={paints:0};surface.getContext=()=>surface.ctx ||= context(surface,true);surfaces.push(surface);return surface;},querySelectorAll:()=>[]};
const canvas=documentMock.getElementById('world');canvas.getContext=()=>context(canvas,false);
let width=960,height=660,now=0;
canvas.getBoundingClientRect=()=>({left:0,top:0,width,height});
const windowMock={devicePixelRatio:2,addEventListener(){}};
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation','assert','surfaces','fills','images','copies','resize','performance','advanceTime',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  const town={map_size:120,terrain:Array(120).fill('g'.repeat(120)),map_resources:[{kind:'tree',x:31,y:31}],
    buildings:[{kind:'hall',x:30,y:30}],villagers:[{name:'Ada',job:'idle'}],food:40,wood:40,level:1};
  world={world_id:'cache-test',towns:{own:town}};viewed='own';cameraTown='cache-test/own';camera={x:30,y:30,zoom:1};
  const groundSurfaces=()=>surfaces.filter(surface=>![...resourceSprites.values()].includes(surface));
  draw();
  assert.equal(groundSurfaces().length,1);const surface=groundCache.surface,painted=surface.paints;
  assert(painted>0,'Ground is painted into the cache');
  assert(!fills.includes('#68865a'),'Ground is composited instead of painted on the main canvas');
  draw();assert.equal(surface.paints,painted,'Standing still reuses ground');
  camera.x+=1;draw();assert.equal(surface.paints,painted,'Small camera movements reuse padded ground');
  assert.equal(images.filter(args=>args[0]===surface).at(-1)[1],-160-tileW/2,'Cached ground follows horizontal camera translation');
  assert.equal(images.filter(args=>args[0]===surface).at(-1)[2],-160-tileH/2,'Cached ground follows vertical camera translation');
  assert(hitTargets.some(t=>t.type==='resource'&&t.index===0),'Resource selection indices survive caching');
  world=JSON.parse(JSON.stringify(world));draw();
  assert.equal(surface.paints,painted,'Polling unchanged terrain preserves ground');
  camera.x-=1;draw();
  let beforePan=surface.paints;
  camera.x+=4;camera.y-=4;draw();
  assert.equal(copies.length,1,'Panning reuses overlapping ground pixels');
  assert.equal(copies[0].operation,'copy','Transparent map edges replace old pixels');
  assert.equal(copies[0].args[5],0,'Leftward ground movement copies to the left edge');
  assert.equal(copies[0].args[1],432,'Copy source starts at an integer device pixel');
  assert(surface.paints-beforePan<painted*.3,'Panning paints only the exposed strip');
  beforePan=surface.paints;camera.x-=4;camera.y+=4;draw();
  assert.equal(copies.length,2,'Reverse panning also reuses ground');
  assert.equal(copies[1].args[5],432,'Rightward ground movement offsets the destination');
  assert(surface.paints-beforePan<painted*.3,'Reverse panning paints only the exposed strip');
  beforePan=surface.paints;camera.x+=7;camera.y+=7;draw();
  assert.equal(copies.at(-1).args[2],378,'Vertical panning shifts the source rows');
  assert(surface.paints-beforePan<painted*.3,'Vertical panning paints only the exposed strip');
  camera.x+=7;draw();
  const diagonal=copies.at(-1).args;
  assert(diagonal[1]>0&&diagonal[2]>0,'Diagonal panning copies both axes');
  camera.x+=.123;camera.y+=7.321;draw();
  assert(copies.at(-1).args.slice(1).every(Number.isInteger),'Fractional camera positions copy whole device pixels');
  camera.x-=21.123;camera.y-=21.321;draw();
  camera.x+=10;draw();assert(surface.paints>painted,'Leaving the padded viewport repaints ground');
  let previous=surface.paints;camera.zoom=.35;draw();assert(surface.paints>previous,'Zoom repaints ground');
  previous=surface.paints;draw();assert.equal(surface.paints,previous,'Repeated zoomed-out frames reuse ground');
  camera.zoom=.36;draw();
  assert.equal(surface.paints,previous,'Small zoom steps scale cached ground during the gesture');
  const zoomImage=images.filter(args=>args[0]===surface).at(-1),ratio=tileW/groundCache.tileW;
  const cachedX=160+groundCache.originX,cachedY=160+groundCache.originY+15*groundCache.tileW;
  const live=point(30,30);
  assert(Math.abs(zoomImage[1]+cachedX*ratio-live[0])<1e-6,'Scaled ground aligns with live objects horizontally');
  assert(Math.abs(zoomImage[2]+cachedY*ratio-live[1])<1e-6,'Scaled ground aligns with live objects vertically');
  advanceTime(151);draw();
  assert(surface.paints>previous,'Ground repaints at the final resolution after zoom settles');
  assert.equal(groundCache.tileW,tileW,'The settled zoom uses a sharp ground texture');
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
`);
run(documentMock,windowMock,{getItem:()=>null},()=>0,()=>{},WorkerAnimation,assert,surfaces,fills,images,copies,()=>{width=720;height=500;},{now:()=>now},ms=>{now+=ms;});
console.log('Render cache checks passed');
