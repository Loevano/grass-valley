const fs=require('fs'),assert=require('assert');
const elements=new Map(),listeners={};
function element(){return {children:[],open:false,classList:{add(){}},append(child){this.children.push(child);},
  replaceChildren(){this.children=[];},setAttribute(key,value){this[key]=value;},addEventListener(name,fn){this[name]=fn;},showModal(){this.open=true;},close(){this.open=false;}};}
const $=id=>{if(!elements.has(id))elements.set(id,element());return elements.get(id);};
const world={world_id:'first',towns:{one:{name:'Willow'},two:{name:'Oak'}}},identity={town_id:'one'};
const document={createElement:element,activeElement:null},window={addEventListener(name,fn){listeners[name]=fn;}};
let quest=null,requests=[],resolveMove,clock=0,polls=0;
const request=async(path,data)=>{
  if(path==='/api/lanterns'){polls++;return {world_id:world.world_id,quest};}
  requests.push(data);
  if(data.action==='lanterns_walk')await new Promise(resolve=>resolveMove=resolve);
  if(data.action==='leave_lanterns')quest.members.one.inside=false;
  return {ok:true};
};
const api=new Function('$','document','window','world','identity','keybindings','request','refresh','clearWalk','setInterval','performance',
  fs.readFileSync('web/lanterns.js','utf8')+';return {openLanterns,pollLanterns,renderLanterns,closeLanterns};')
  ($,document,window,world,identity,{up:'w',down:'s',left:'a',right:'d'},request,async()=>{},()=>{},()=>{},{now:()=>clock});
const flush=()=>new Promise(resolve=>setImmediate(resolve));
const key=(key,extra={})=>listeners.keydown({key,preventDefault(){},...extra});
(async()=>{
  api.openLanterns();await flush();assert.equal($('lanterns-board').children.length,65);
  key('w');assert.equal(requests.length,0);
  quest={members:{one:{inside:true,x:4,y:2,hits:0},two:{inside:true,x:1,y:2,hits:1}},playing:true,completed:false,
    hazards:[{x:4,y:2,state:'warning'},{x:8,y:3,state:'strike'}],message:'Dodge!'};
  await api.pollLanterns();assert.equal($('lanterns-board').children[2*13+4].textContent,'!');
  assert.equal($('lanterns-board').children[3*13+8].textContent,'×');
  key('w',{ctrlKey:true});assert.equal(requests.length,0);
  key('w');key('d');assert.equal(requests.length,1,'Single movement request in flight');
  assert.deepEqual(requests[0],{action:'lanterns_walk',dx:0,dy:-1,world_id:'first'});
  resolveMove();await flush();key('w');assert.equal(requests.length,1,'Movement throttled');
  quest.playing=false;await api.pollLanterns();clock=200;key('w');assert.equal(requests.length,1,'Partner departure pauses movement');
  assert([...$('lanterns-directions').children].every(b=>b.disabled));
  await api.closeLanterns();assert(!$('lanterns-dialog').open);assert(!quest.members.one.inside);
  assert.equal(requests[1].action,'leave_lanterns','Closing leaves before dismissing');
  api.openLanterns();await flush();world.world_id='second';await api.pollLanterns();assert(!$('lanterns-dialog').open);
  console.log('Falling Lanterns UI checks passed');
})().catch(e=>{console.error(e);process.exitCode=1;});
