// Run with Node.js or JavaScriptCore's jsc shell.
const read=path=>typeof readFile==='function'?readFile(path):require('fs').readFileSync(path,'utf8');
const source=read('web/app.js');
const elements=new Map(),events={};
function element(){return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(key,callback){events[key]=callback;},showModal(){},close(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>new Proxy({}, {get:()=>()=>{}});
canvasMock.getBoundingClientRect=()=>({left:0,top:0,width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','events',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  function check(condition,message){if(!condition)throw Error(message);}
  viewed='own';world={world_id:'test',towns:{own:{map_size:120,buildings:[],map_resources:[],villagers:[]}}};
  animatedVillagers=()=>[];draw();
  let prevented=false;
  function wheel(deltaX,deltaY,altKey=false,deltaMode=0,ctrlKey=false){events.wheel({deltaX,deltaY,altKey,deltaMode,ctrlKey,clientX:600,clientY:380,preventDefault(){prevented=true;}});}
  const before={...camera};wheel(54,27);
  check(prevented&&camera.zoom===before.zoom,'Regular scrolling pans and prevents page scrolling');
  check(camera.x!==before.x,'Scrolling moves the camera');
  const tile=tileAt({clientX:600,clientY:380});
  const zoom=camera.zoom;wheel(0,-100,true);
  check(camera.zoom>zoom,'Option-scroll zooms in');
  const anchored=tileAt({clientX:600,clientY:380});
  check(tile.x===anchored.x&&tile.y===anchored.y,'Zoom keeps the tile under the cursor anchored');
  wheel(0,-10000,true);check(camera.zoom===2.5,'Zoom has an upper limit');
  wheel(0,10000,true);check(camera.zoom===.35,'Zoom has a lower limit');
  camera={x:60,y:60,zoom:1};draw();
  const pinchTile=tileAt({clientX:600,clientY:380});
  prevented=false;wheel(0,-100,false,0,true);draw();
  check(prevented&&camera.zoom>1,'Touchpad pinch zooms in and prevents browser zoom');
  const pinchAnchored=tileAt({clientX:600,clientY:380});
  check(pinchTile.x===pinchAnchored.x&&pinchTile.y===pinchAnchored.y,'Pinch keeps the tile under the cursor anchored');
  const pinchZoom=camera.zoom;wheel(0,100,false,0,true);draw();
  check(camera.zoom<pinchZoom,'Touchpad pinch zooms out');
  wheel(0,-10000,false,0,true);check(camera.zoom===2.5,'Pinch respects the upper zoom limit');
  wheel(0,10000,false,0,true);check(camera.zoom===.35,'Pinch respects the lower zoom limit');
  wheel(100000,100000);check(camera.x>=0&&camera.x<120&&camera.y>=0&&camera.y<120,'Panning stays inside the map');
  camera={x:80,y:80,zoom:1};draw();
  const [px,py]=point(80,80),large=tileAt({clientX:px,clientY:py+tileH/2});
  check(large.x===80&&large.y===80,'Tile picking works beyond the old map bounds');
  viewed='friend';draw();check(camera.x===5.5&&camera.zoom===1,'Changing towns resets the camera');
`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},events);
if(typeof print==='function')print('Map navigation checks passed');else console.log('Map navigation checks passed');
