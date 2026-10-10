// Run with: node tests/test_render_cache.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const WorkerAnimation=require('../web/animations.js');
const elements=new Map(),surfaces=[],fills=[],images=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
function context(surface,offscreen){
  return new Proxy({canvas:surface,fill(){if(offscreen)surface.paints++;else fills.push(this.fillStyle);},
    drawImage(...args){images.push(args);}}, {get:(target,key)=>key in target?target[key]:()=>{}});
}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},
  createElement(tag){if(tag!=='canvas')return element();const surface={paints:0};surface.getContext=()=>surface.ctx ||= context(surface,true);surfaces.push(surface);return surface;},querySelectorAll:()=>[]};
const canvas=documentMock.getElementById('world');canvas.getContext=()=>context(canvas,false);
let width=960,height=660;
canvas.getBoundingClientRect=()=>({left:0,top:0,width,height});
const windowMock={devicePixelRatio:2,addEventListener(){}};
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation','assert','surfaces','fills','images','resize',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  const town={map_size:120,terrain:Array(120).fill('g'.repeat(120)),map_resources:[{kind:'tree',x:31,y:31}],
    buildings:[{kind:'hall',x:30,y:30}],villagers:[{name:'Ada',job:'idle'}],food:40,wood:40,level:1};
  world={world_id:'cache-test',towns:{own:town}};viewed='own';cameraTown='cache-test/own';camera={x:30,y:30,zoom:1};
  draw();
  assert.equal(surfaces.length,1);const surface=surfaces[0],painted=surface.paints;
  assert(painted>0,'Ground is painted into the cache');
  assert(!fills.includes('#68865a'),'Ground is composited instead of painted on the main canvas');
  draw();assert.equal(surface.paints,painted,'Standing still reuses ground');
  camera.x+=1;draw();assert.equal(surface.paints,painted,'Small camera movements reuse padded ground');
  assert.equal(images.at(-1)[1],-160-tileW/2,'Cached ground follows horizontal camera translation');
  assert.equal(images.at(-1)[2],-160-tileH/2,'Cached ground follows vertical camera translation');
  assert(hitTargets.some(t=>t.type==='resource'&&t.index===0),'Resource selection indices survive caching');
  world=JSON.parse(JSON.stringify(world));draw();
  assert.equal(surface.paints,painted,'Polling unchanged terrain preserves ground');
  camera.x+=10;draw();assert(surface.paints>painted,'Leaving the padded viewport repaints ground');
  let previous=surface.paints;camera.zoom=.35;draw();assert(surface.paints>previous,'Zoom repaints ground');
  previous=surface.paints;draw();assert.equal(surface.paints,previous,'Repeated zoomed-out frames reuse ground');
  window.devicePixelRatio=1;draw();assert(surface.paints>previous,'Retina scale changes repaint ground');
  previous=surface.paints;resize();draw();assert(surface.paints>previous,'Window resizing repaints ground');
  panel='build';fills.length=0;draw();assert(fills.includes('#e4c98022'),'Build range remains above cached terrain');
  panel=null;fills.length=0;draw();assert(!fills.includes('#e4c98022'),'Closing Build clears overlays without leaving marks');
  world=JSON.parse(JSON.stringify(world));world.towns.own.terrain[30]='b'.repeat(120);draw();
  assert.equal(surfaces.length,2,'Terrain changes discard stale ground');
  world.towns.friend={...world.towns.own,terrain:Array(120).fill('s'.repeat(120)),map_resources:[]};
  viewed='friend';draw();assert.equal(surfaces.length,3,'Switching separate maps discards stale ground');
  world=JSON.parse(JSON.stringify(world));world.world_id='another-world';draw();
  assert.equal(surfaces.length,4,'Changing worlds invalidates ground');
`);
run(documentMock,windowMock,{getItem:()=>null},()=>0,()=>{},WorkerAnimation,assert,surfaces,fills,images,()=>{width=720;height=500;});
console.log('Render cache checks passed');
