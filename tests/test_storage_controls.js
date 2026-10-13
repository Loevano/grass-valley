const fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8');
const animation=new Function(fs.readFileSync('web/animations.js','utf8')+';return WorkerAnimation;')();
const elements=new Map(),listeners={};
function element(tag){return {tag,children:[],style:{},attributes:{},classList:{toggle(){}},append(...items){this.children.push(...items);},replaceChildren(){this.children=[];},setAttribute(k,v){this.attributes[k]=v;},addEventListener(){},focus(){},showModal(){},close(){}};}
const document={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvas=document.getElementById('world');canvas.getContext=()=>new Proxy({},{get:()=>()=>{}});canvas.getBoundingClientRect=()=>({width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation',
 source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
 function panelNodes(node=$('selection-details')){return Array.from(node.children || []).flatMap(child=>[child,...panelNodes(child)]);}


 function check(ok,message){if(!ok)throw Error(message);}
 const town={name:'Town',wood:65,food:40,coins:20,level:5,goods:{},player:{x:5,y:7},
 buildings:[{kind:'hall',x:5,y:5},{kind:'storehouse',x:2,y:2,level:4,storage_allocations:{wood:100,food:100,wool:50},stored_goods:{wood:20,fish:10,smoked_fish:4,wool:5}}],
 villagers:[{name:'Ada',job:'idle',woodcutting:0,farming:0,fishing:0}],map_resources:[]};
 world={world_id:'test',towns:{own:town,friend:town}};identity={town_id:'own'};viewed='own';draw=()=>{};
 const commands=[];act=data=>commands.push(data);
 inspect('building',1);
 const fields=()=>panelNodes().filter(c=>c.tag==='input'&&c.type==='checkbox');
 const meters=()=>panelNodes().filter(c=>c.tag==='meter');
 const button=text=>panelNodes().find(c=>c.tag==='button'&&c.textContent===text);
 const toggle=key=>fields().find(c=>c.attributes['aria-label']==='Accept '+key);
 check(meters().length===10&&fields().length===10,'Storage has meters and resource switches');
 check(meters().find(c=>c.attributes['aria-label']==='Food stored').value===16,'Food meter combines weighted edible goods');
 check(meters().find(c=>c.attributes['aria-label']==='Wood stored').max===150,'Each enabled resource has full level capacity');
 check(toggle('Wood').checked&&!toggle('Planks').checked,'Old positive allocations become enabled resources');
 check(Object.values(roomEnabled({kind:'storehouse'})).every(enabled=>!enabled),'New storehouses start with resources disabled');
 check(localTradeFoodRoom({inventory_version:1,buildings:[{kind:'storehouse',storage_enabled:{wood:true,food:true},stored_goods:{wood:100}}]})===50,'Full wood storage leaves independent food room for restocking');
 check(resourceStorage(town,'wool')===150,'Wool has independent level capacity');
 check(resourceStorage(town,'fish')===250,'Food retains fixed hall capacity plus storehouse capacity');
 toggle('Planks').checked=true;toggle('Planks').onchange();
 toggle('Wood').checked=false;toggle('Wood').onchange();
 check(commands.at(-1).action==='configure_storage'&&commands.at(-1).enabled.planks&&!commands.at(-1).enabled.wood,'Switches submit enabled resources and preserve other changes');
 town.buildings[1].storage_enabled=commands.at(-1).enabled;render();
 check(!toggle('Wood').checked&&toggle('Planks').checked,'Host-confirmed switches persist');
 check(panelNodes().some(c=>c.textContent==='20 · Disabled'),'Disabled wood retains its inventory');
 check(!panelNodes().some(c=>c.tag==='input'&&c.type==='range'),'Storage has no allocation sliders');
 check(panelNodes().find(c=>c.className==='storage-summary').textContent==='150 capacity per enabled resource','Storage explains per-resource capacity');
 check(panelNodes().find(c=>c.tag==='button'&&c.title?.startsWith('Upgrade to level 5')),'Storage can upgrade beyond level 3');
 town.buildings[1].level=5;render();
 check($('selection-title').textContent==='Warehouse','Level 5 storage is named Warehouse');
 check(!panelNodes().find(c=>c.tag==='button'&&c.title?.startsWith('Upgrade to level 6')).disabled,'Warehouses can upgrade beyond level 5');
 town.buildings[1].level=8;render();
 check(panelNodes().find(c=>c.className==='storage-summary').textContent==='350 capacity per enabled resource','Level 8 warehouses show the expanded capacity');
 check(!panelNodes().some(c=>c.tag==='button'&&c.title?.startsWith('Upgrade to level'))&&panelNodes().some(c=>c.className==='upgrade-card-status'&&c.textContent==='Maxed'),'Level 8 shows maxed storage without an action');
 viewed='friend';inspect('building',1);
 check(meters().length===10&&!fields().length,'Visitors see storage meters without switches');
 check(!button('Allocate')&&!button('Save allocations'),'Visitors cannot change allocations');
 viewed='own';
 town.buildings.push({kind:'woodcutter',x:3,y:3,output_buffer:{wood:2.5},stored_goods:{wood:1}});
 inspect('building',2);
 check(meters().length===1,'Woodcutters show only wood storage');
 check(meters().find(c=>c.attributes['aria-label']==='Wood stored').value===3.5,'Camp inventory combines produced and delivered wood');
 town.buildings[2].output_buffer.wood=3;render();
 check(meters().find(c=>c.attributes['aria-label']==='Wood stored').value===4,'Camp storage increases on the next host update');
 check(!button('Allocate')&&!fields().length,'Camp capacity cannot be edited');
 check(roomAllocations(town.buildings[2]).wood===50,'Camp has 50 fixed wood spaces');
 town.buildings.push({kind:'home',x:4,y:4,storage_allocations:{wood:25,food:25}});
 inspect('building',3);
 check(!meters().length&&!fields().length&&!button('Allocate'),'Homes do not offer storage');
 town.buildings.push({kind:'fishing_hut',x:4,y:3,storage_allocations:{food:20,tools:30},stored_goods:{fish:2,tools:3}});
 inspect('building',4);
 check(meters().length===1&&meters()[0].attributes['aria-label']==='Food stored','Fishing huts only offer food storage');
 check(roomAllocations(town.buildings[4]).tools===undefined,'Old unrelated allocations are ignored');
 check(resourceStorage(town,'fish')===350&&resourceStorage(town,'bread')===300,'Food capacity respects which edible goods each building accepts');
 check(panelNodes().some(c=>c.textContent?.includes('Tools from previous storage')),'Legacy unrelated goods remain visible');
 check(!button('Allocate')&&roomAllocations(town.buildings[4]).food===50,'Fishing huts ignore old allocations and have fixed food capacity');
 town.buildings.push({kind:'mill',x:1,y:3});inspect('building',5);
 check(meters().length===2,'Mill offers wheat and flour storage');
 check(!button('Allocate')&&!fields().length,'Mill capacity cannot be edited');
 check(roomAllocations(town.buildings[5]).wheat===30&&roomAllocations(town.buildings[5]).flour===20,'Mill has fixed input and output capacities');
 inspect('building',0);check(!button('Allocate'),'Hall capacity cannot be edited');
 world.progression={fixed_storage:{mill:{wheat:35,flour:15}}};
 check(roomAllocations(town.buildings[5]).wheat===35,'Client uses host fixed capacities');
 delete world.progression;
 const transportTown={map_size:12,terrain:['g'.repeat(12)].flatMap(r=>Array(12).fill(r)),buildings:[{kind:'hall',x:5,y:5}],map_resources:[]};
 const v={job:'hauling',logistics_position:{x:2,y:2},logistics_target:{x:3,y:2}};
 const state=WorkerAnimation.plan(transportTown,v,0);
 check(state.transport&&state.path.length===1,'Host delivery targets use transport animation');
 WorkerAnimation.advance(state,2,true);
 check(state.x===3&&state.y===2,'Hauler reaches the host delivery target');
 WorkerAnimation.advance(state,10,true);
 check(state.x===3&&state.y===2&&state.phase==='waiting','Haulers wait for the next host transport update');
 `);
run(document,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},animation);
console.log('Storage switches and hauling controls passed');
