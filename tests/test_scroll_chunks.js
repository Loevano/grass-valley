// Run with: node tests/test_scroll_chunks.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const animation=require('../web/animations.js');
let plans=0,paints=0;
const WorkerAnimation={...animation,plan(...args){plans++;return animation.plan(...args);}};
const elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(...children){this.children.push(...children);},
  replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},
  createElement(tag){const surface=element();if(tag==='canvas')surface.getContext=()=>new Proxy({canvas:surface,fill(){paints++;}},
    {get:(target,key)=>key in target?target[key]:()=>{}});return surface;},querySelectorAll:()=>[]};
const canvas=documentMock.getElementById('world');
canvas.getContext=()=>new Proxy({}, {get:()=>()=>{}});
canvas.getBoundingClientRect=()=>({left:0,top:0,width:640,height:480});
new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation','assert','counts',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  const rows=Array(40).fill('g'.repeat(40));
  const keys=start=>Array.from({length:25},(_,i)=>(start+Math.floor(i/5))+','+(22+i%5));
  const snapshot=(visible,updates=false)=>{
    const landscape={chunked:1,chunk_size:40,map_size:1920,visible_chunks:visible,terrain_chunks:{},resource_chunks:{},npcs:[]};
    if(updates)for(const key of visible){
      const [cx,cy]=key.split(',').map(Number),resources=[];
      for(let y=0;y<40;y+=4)for(let x=0;x<40;x+=4)resources.push({kind:'tree',x:cx*40+x,y:cy*40+y});
      landscape.terrain_chunks[key]=rows;
      landscape.resource_chunks[key]={version:key,map_resources:resources,tree_stumps:[]};
    }
    const town={level:1,wood:40,food:40,town_origin:{x:960,y:960},
      buildings:[{kind:'hall',x:960,y:960}],villagers:[{job:'idle',production_status:'Working'}]};
    const next={world_id:'scroll',shared_map:landscape,towns:{own:town}};
    hydrateMapChunks(next);Object.assign(town,landscape);return next;
  };
  viewed='own';cameraTown='scroll/shared';camera={x:960,y:960,zoom:.35};
  requestedMapView=mapChunkKeys({chunked:1,chunk_size:40,map_size:1920}).join(';');
  // Warm both areas, as after exploring and then reversing a scroll gesture.
  world=snapshot(keys(22),true);draw();
  world=snapshot(keys(24),true);draw();
  const ground=groundCache,resources=resourceCache,worker=workers.get('own/0'),before=counts();
  for(let frame=0;frame<30;frame++){
    world=snapshot(keys(frame%2?24:22));draw();
    assert.strictEqual(groundCache,ground,'Cached camera responses retain the ground surface');
    assert.strictEqual(resourceCache,resources,'Cached camera responses avoid rebuilding forest and spatial indices');
    assert.strictEqual(workers.get('own/0'),worker,'Exploration preserves worker routes and animation state');
  }
  assert.deepEqual(counts(),before,'Revisiting downloaded chunks causes no terrain painting or worker planning');
  assert(hitTargets.some(target=>target.type==='resource'),'Retained forest tiles remain selectable');
  const changed=snapshot(keys(22));
  changed.shared_map.resource_chunks={'24,24':{version:'removed',map_resources:[],tree_stumps:[]}};
  hydrateMapChunks(changed);Object.assign(changed.towns.own,changed.shared_map);world=changed;draw();
  assert.notStrictEqual(resourceCache,resources,'Changed resource chunks still update the forest and selection index');
  assert.notStrictEqual(groundCache,ground,'Tree removal repaints the visible canopy');
  assert.notStrictEqual(workers.get('own/0'),worker,'Changed local obstacles still update worker paths');
`)(documentMock,{devicePixelRatio:2,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},WorkerAnimation,assert,()=>({plans,paints}));
console.log('Zoomed-out chunk scrolling checks passed');
