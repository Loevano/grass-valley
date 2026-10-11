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
 const fields=()=>panelNodes().filter(c=>c.tag==='input'&&c.type==='range');
 const meters=()=>panelNodes().filter(c=>c.tag==='meter');
 const button=text=>panelNodes().find(c=>c.tag==='button'&&c.textContent===text);
 check(meters().length===10&&!fields().length,'Storage opens with simple meters');
 check(meters().find(c=>c.attributes['aria-label']==='Food stored').value===16,'Food meter combines weighted edible goods');
 check(meters().find(c=>c.attributes['aria-label']==='Wood stored').max===100,'Meters show stored goods against allocated space');
 button('Allocate').onclick();
 check(fields().length===10&&!meters().length,'Allocate turns meters into sliders');
 check(fields().find(c=>c.attributes['aria-label']==='Food storage allocation').min==='16','Food allocation cannot shrink below weighted stock');
 check(resourceStorage(town,'wool')===50,'Wool capacity reflects its allocation');
 check(resourceStorage(town,'fish')===200,'Edible goods use the food allocation');
 const wool=fields().find(c=>c.attributes['aria-label']==='Wool storage allocation');
 wool.value='80';wool.oninput();render();
 check(fields().find(c=>c.attributes['aria-label']==='Wool storage allocation').value==='80','Polling preserves edited allocations');
 check(panelNodes().find(c=>c.className==='storage-summary').textContent==='280 / 400 allocated · 120 free','Allocation summary updates live');
 const wood=fields().find(c=>c.attributes['aria-label']==='Wood storage allocation');
 wood.value='999';wood.oninput();
 check(wood.value==='220','Sliders cannot overallocate the shared room');
 wood.value='0';wood.oninput();check(wood.value==='20','Sliders preserve space occupied by stored goods');
 button('Cancel').onclick();
 check(meters().length===10&&!fields().length&&!commands.length,'Cancel returns to meters without submitting');
 button('Allocate').onclick();
 check(fields().find(c=>c.attributes['aria-label']==='Wool storage allocation').value==='50','Cancel discards draft allocations');
 const editedWool=fields().find(c=>c.attributes['aria-label']==='Wool storage allocation');editedWool.value='80';editedWool.oninput();
 button('Save allocations').onclick();
 check(meters().length===10&&!fields().length,'Save returns to meters');
 check(commands.at(-1).action==='allocate_storage'&&commands.at(-1).allocations.wool===80,'Allocation form submits numeric capacities');
 town.buildings[1].storage_allocations=commands.at(-1).allocations;render();
 check(meters().find(c=>c.attributes['aria-label']==='Wool stored').max===80,'Confirmed allocations update the meters');
 check(panelNodes().find(c=>c.tag==='button'&&c.textContent?.startsWith('Upgrade to level 5')),'Storage can upgrade beyond level 3');
 town.buildings[1].level=5;render();
 check($('selection-title').textContent==='Warehouse','Level 5 storage is named Warehouse');
 check(panelNodes().find(c=>c.tag==='button'&&c.textContent?.startsWith('Fully upgraded')).disabled,'Warehouse is the final storage upgrade');
 viewed='friend';inspect('building',1);
 check(meters().length===10&&!fields().length,'Visitors see storage meters');
 check(!button('Allocate')&&!button('Save allocations'),'Visitors cannot change allocations');
 viewed='own';
 town.buildings.push({kind:'woodcutter',x:3,y:3,output_buffer:{wood:2.5},stored_goods:{wood:1}});
 inspect('building',2);
 check(meters().length===1,'Woodcutters show only wood storage');
 check(meters().find(c=>c.attributes['aria-label']==='Wood stored').value===3.5,'Camp inventory combines produced and delivered wood');
 town.buildings[2].output_buffer.wood=3;render();
 check(meters().find(c=>c.attributes['aria-label']==='Wood stored').value===4,'Camp storage increases on the next host update');
 button('Allocate').onclick();check(fields().length===1,'Camp allocation offers only wood');
 button('Save allocations').onclick();
 check(Object.keys(commands.at(-1).allocations).join(',')==='wood','Camp only submits relevant allocations');
 town.buildings.push({kind:'home',x:4,y:4,storage_allocations:{wood:25,food:25}});
 inspect('building',3);
 check(!meters().length&&!fields().length&&!button('Allocate'),'Homes do not offer storage');
 town.buildings.push({kind:'fishing_hut',x:4,y:3,storage_allocations:{food:20,tools:30},stored_goods:{fish:2,tools:3}});
 inspect('building',4);
 check(meters().length===1&&meters()[0].attributes['aria-label']==='Food stored','Fishing huts only offer food storage');
 check(roomAllocations(town.buildings[4]).tools===undefined,'Old unrelated allocations are ignored');
 check(resourceStorage(town,'fish')===220&&resourceStorage(town,'bread')===200,'Food capacity respects which edible goods each building accepts');
 check(panelNodes().some(c=>c.textContent?.includes('Tools from previous storage')),'Legacy unrelated goods remain visible');
 button('Allocate').onclick();button('Save allocations').onclick();
 check(Object.keys(commands.at(-1).allocations).join(',')==='food','Fishing huts submit only relevant food allocation');
 town.buildings.push({kind:'mill',x:1,y:3});inspect('building',5);
 check(meters().length===2,'Mill offers wheat and flour storage');
 button('Allocate').onclick();
 check(fields().map(c=>c.attributes['aria-label']).join(',')==='Wheat storage allocation,Flour storage allocation','Mill controls match recipe inputs and outputs');
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
console.log('Storage allocation and hauling controls passed');
