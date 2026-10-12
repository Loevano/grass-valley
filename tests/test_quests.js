const fs=require('fs'),assert=require('assert');
const elements=new Map(),listeners={};
function element(){return {children:[],open:false,classList:{add(){}},append(child){this.children.push(child);},
  replaceChildren(){this.children=[];},setAttribute(key,value){this[key]=value;},showModal(){this.open=true;},close(){this.open=false;}};}
const $=id=>{if(!elements.has(id))elements.set(id,element());return elements.get(id);};
const document={createElement:element,activeElement:null};
const window={addEventListener(name,callback){listeners[name]=callback;}};
const world={world_id:'first',towns:{one:{name:'Willowbrook'},two:{name:'Oakstead'}},
  quarry_catalog:{layout:['###########','#....#....#','#.P..#.SLM#','#....G....#','#....#.I.T#','#....#....#','###########'],reward:20,keepsake:'Quarry lantern stone'}};
const identity={town_id:'one'},keybindings={up:'w',left:'a',down:'s',right:'d'};
let clears=0,requests=[],resolveRequest;
const request=(path,data)=>{requests.push({path,data});return new Promise(resolve=>{resolveRequest=resolve;});};
const refresh=async()=>{},clearWalk=()=>clears++;
const api=new Function('$','document','window','world','identity','keybindings','request','refresh','clearWalk',
  fs.readFileSync('web/quarry.js','utf8')+';return {openQuarry,renderQuarry,quarryCommand};')
  ($,document,window,world,identity,keybindings,request,refresh,clearWalk);
const key=(value,extra={})=>listeners.keydown({key:value,preventDefault(){},...extra});

(async()=>{
  api.openQuarry();
  assert.equal(clears,1);assert($('quarry-dialog').open);
  assert.equal($('quarry-board').children.length,77);
  assert(!$('quarry-enter').hidden);assert($('quarry-leave').hidden);
  assert([...$('quarry-directions').children].every(button=>button.disabled));
  key('w');assert.equal(requests.length,0,'Waiting for entry must not send movement');
  world.quarry={members:{one:{inside:true,x:7,y:2},two:{inside:true,x:2,y:2}},started:true,completed:false,sequence:[],message:'Read the inscription'};
  api.renderQuarry();
  assert($('quarry-enter').hidden);assert(!$('quarry-leave').hidden);
  assert(!$('quarry-objects').children[1].disabled,'Nearby sun rune can be touched');
  assert($('quarry-objects').children[3].disabled,'Distant moon rune cannot be touched');
  assert($('quarry-objects').children[4].disabled,'Sealed chest cannot be opened');
  assert.equal($('quarry-board').children[3*11+5].textContent,'·','Remote founder on plate opens the visible gate');
  key('w',{ctrlKey:true});key('w',{repeat:true});assert.equal(requests.length,0);
  key('w');key('d');
  assert.equal(requests.length,1,'Only one command may be pending');
  assert.deepEqual(requests[0].data,{action:'quarry_walk',dx:0,dy:-1,world_id:'first'});
  resolveRequest({ok:true});await new Promise(resolve=>setImmediate(resolve));
  $('quarry-close').onclick();assert(!$('quarry-dialog').open);
  assert(world.quarry.members.one.inside,'Closing the map preserves party membership');
  key('w');assert.equal(requests.length,1,'Closed quarry ignores puzzle keys');
  api.openQuarry();world.quarry.completed=true;api.renderQuarry();
  assert([...$('quarry-directions').children].every(button=>button.disabled));
  assert($('quarry-enter').hidden);assert($('quarry-reward').textContent.includes('Rewards received'));
  world.world_id='second';api.renderQuarry();assert(!$('quarry-dialog').open,'Switching worlds closes the old expedition');
  console.log('Quarry UI checks passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
