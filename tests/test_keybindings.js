// Run with Node.js or JavaScriptCore's jsc shell.
const read=path=>typeof readFile==='function'?readFile(path):require('fs').readFileSync(path,'utf8');
const source=read('web/app.js');
function check(condition,message){if(!condition)throw Error(message);}
function session(saved=null){
  const elements=new Map(),listeners={},storage=new Map([['lantern-settings',saved]]);
  function element(tagName){return {tagName,events:{},children:[],style:{},attributes:{},classList:{toggle(){}},
    append(...children){this.children.push(...children);for(const child of children)if(child.id)elements.set(child.id,child);},
    replaceChildren(){this.children=[];},setAttribute(key,value){this.attributes[key]=value;},
    addEventListener(type,callback){this.events[type]=callback;},focus(){},showModal(){this.open=true;},close(){this.open=false;this.events.close?.();}};}
  const document={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
  const canvas=document.getElementById('world');
  canvas.getContext=()=>new Proxy({}, {get:()=>()=>{}});
  const run=new Function('document','window','localStorage','setTimeout','clearTimeout',
    source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
    draw=()=>{};
    const town={name:'Home',wood:50,food:30,coins:5,villagers:[],buildings:[],map_resources:[{kind:'water',x:5,y:6},{kind:'tree',x:6,y:7},{kind:'berry_bush',x:4,y:7}],player:{x:5,y:7,facing:1}};
    world={world_id:'test',towns:{own:town}};identity={town_id:'own'};viewed='own';playerSprite(town,0);
    request=async()=>({player:{x:5.2,y:6.8,facing:1}});
    const actions=[];playerAction=action=>actions.push(action);
    return {get actions(){return actions;},get bindings(){return keybindings;},get locked(){return cameraLocked;},get following(){return playerView.following;},
      get camera(){return typeof camera==='undefined'?null:{...camera};},get held(){return [...walkKeys];},step:seconds=>playerSprite(town,seconds),
      pan(){if(typeof camera!=='undefined'){camera.x=30;camera.y=30;}},get panel(){return panel;}};
  `);
  const state=run(document,{addEventListener(type,callback){listeners[type]=callback;}},
    {getItem:key=>storage.get(key),setItem:(key,value)=>storage.set(key,value)},()=>0,()=>{});
  const key=(key,extra={})=>listeners.keydown({key,preventDefault(){},...extra});
  const rebind=(action,key,extra={})=>document.getElementById('binding-'+action).onkeydown({key,preventDefault(){},stopPropagation(){},...extra});
  return {state,document,storage,key,rebind,listeners};
}
(async()=>{
  const {state,document,storage,key,rebind,listeners}=session();
  key('f');key('c');key('g');check(state.actions.join(',')==='fish,chop,gather','F/C/G start the matching character activities');
  key('f',{repeat:true});key('g',{ctrlKey:true});check(state.actions.length===3,'Held actions and modified shortcuts do not repeat requests');
  check(state.locked&&state.bindings.camera==='y','Camera starts locked with Y binding');
  key('Y');check(!state.locked&&!state.following,'Y unlocks camera');
  key('y',{repeat:true});check(!state.locked,'Held Y does not toggle repeatedly');
  state.pan();key('d');state.step(.1);await Promise.resolve();await Promise.resolve();
  const hero=state.step(.1);
  check(hero.x===5.2&&!state.following&&(!state.camera||state.camera.x===30&&state.camera.y===30),'Unlocked player moves without moving camera');
  listeners.keyup({key:'d'});
  key('y');check(state.locked&&state.following&&(!state.camera||state.camera.x===hero.x&&state.camera.y===hero.y),'Relocking immediately centers player');
  key('y',{ctrlKey:true});check(state.locked,'Browser shortcuts do not toggle camera');
  document.activeElement={tagName:'INPUT'};key('y');check(state.locked,'Typing does not toggle camera');
  document.activeElement={isContentEditable:true};key('y');check(state.locked,'Editable content does not toggle camera');
  document.activeElement=null;
  document.getElementById('settings-button').onclick();
  check(state.panel===null&&document.getElementById('settings-panel').open,'Settings opens as a modal without opening a side panel');
  check(document.getElementById('settings-button').attributes['aria-expanded']==='true','Settings button reflects the open popup');
  key('y');key('w');key('f');check(state.actions.length===3,'Settings blocks gathering actions');check(state.locked&&state.held.length===0,'Settings blocks gameplay shortcuts');
  rebind('right','L');check(state.bindings.right==='l','Settings captures and normalizes a custom key');
  rebind('up','l');check(state.bindings.up==='w','Duplicate bindings are rejected');
  rebind('up','ArrowUp');check(state.bindings.up==='w','Tile navigation keys stay reserved');
  rebind('up','k',{metaKey:true});check(state.bindings.up==='w','Modified shortcuts cannot become bindings');
  rebind('camera','c');check(state.bindings.camera==='y','Gathering keys cannot be assigned twice');
  rebind('camera','v');
  document.getElementById('close-settings').onclick();
  check(!document.getElementById('settings-panel').open&&document.getElementById('settings-button').attributes['aria-expanded']==='false','Close button dismisses the popup and updates the toolbar');
  document.getElementById('help-button').onclick();
  document.getElementById('settings-button').onclick();
  key('Escape');check(state.panel==='help','Escape in Settings leaves the underlying panel intact');
  document.getElementById('settings-panel').close();
  check(document.getElementById('settings-button').attributes['aria-expanded']==='false','Native dialog dismissal updates the toolbar');
  document.getElementById('settings-button').onclick();
  document.getElementById('settings-button').onclick();
  check(!document.getElementById('settings-panel').open&&state.panel==='help','Settings toggles without changing the underlying panel');
  document.getElementById('close-panel').onclick();
  key('d');check(state.held.length===0,'Old movement binding stops working');
  key('l');check(state.held.includes('l'),'Custom movement binding works');
  key('y');check(state.locked,'Old camera binding stops working');
  key('v');check(!state.locked,'Custom camera binding works');
  const restored=session(storage.get('lantern-settings'));
  check(restored.state.bindings.right==='l'&&restored.state.bindings.camera==='v'&&!restored.state.locked,'Bindings and camera lock survive reload');
  document.getElementById('reset-keybindings').onclick();
  check(state.bindings.right==='d'&&state.bindings.camera==='y'&&state.held.length===0,'Reset restores defaults and releases held keys');
  document.getElementById('world-menu').open=true;key('y');check(!state.locked,'World menus block camera toggling');
  const oldCustom=session(JSON.stringify({keybindings:{up:'i',left:'j',down:'k',right:'l',camera:'c'}}));
  check(oldCustom.state.bindings.camera==='c'&&oldCustom.state.bindings.chop!=='c'&&oldCustom.state.bindings.fish==='f','Older custom settings retain their bindings and get unique activity keys');
  const malformed=session('{broken');check(malformed.state.bindings.camera==='y','Malformed storage falls back to defaults');
  const invalid=session(JSON.stringify({keybindings:{up:'w',left:'a',down:'s',right:'d',camera:'d'}}));
  check(invalid.state.bindings.camera==='y','Invalid stored duplicates fall back to defaults');
  if(typeof print==='function')print('Keybinding settings checks passed');else console.log('Keybinding settings checks passed');
})().catch(error=>{if(typeof print==='function'){print(String(error));print(error.stack);quit(1);}else{console.error(error);process.exitCode=1;}});
