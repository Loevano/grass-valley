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
 buildings:[{kind:'hall',x:5,y:5},{kind:'storehouse',x:2,y:2,level:4,storage_allocations:{wood:100,food:100,wool:50}}],
 villagers:[{name:'Ada',job:'idle',woodcutting:0,farming:0,fishing:0}],map_resources:[]};
 world={world_id:'test',towns:{own:town,friend:town}};identity={town_id:'own'};viewed='own';draw=()=>{};
 const commands=[];act=data=>commands.push(data);
 inspect('building',1);
 const fields=()=>panelNodes().filter(c=>c.tag==='label').map(c=>c.children[0]);
 check(fields().length===10,'Storage offers all resource allocation categories');
 check(resourceStorage(town,'wool')===50,'Wool capacity reflects its allocation');
 check(resourceStorage(town,'fish')===200,'Edible goods use the food allocation');
 const wool=fields().find(c=>c.attributes['aria-label']==='Wool storage allocation');
 wool.value='80';wool.oninput();render();
 check(fields().find(c=>c.attributes['aria-label']==='Wool storage allocation').value==='80','Polling preserves edited allocations');
 panelNodes().find(c=>c.textContent==='Save storage allocations').onclick();
 check(commands.at(-1).action==='allocate_storage'&&commands.at(-1).allocations.wool===80,'Allocation form submits numeric capacities');
 check(panelNodes().find(c=>c.tag==='button'&&c.textContent?.startsWith('Upgrade to level 5')),'Storage can upgrade beyond level 3');
 town.buildings[1].level=5;render();
 check($('selection-title').textContent==='Warehouse','Level 5 storage is named Warehouse');
 check(panelNodes().find(c=>c.tag==='button'&&c.textContent?.startsWith('Fully upgraded')).disabled,'Warehouse is the final storage upgrade');
 viewed='friend';inspect('building',1);
 check(fields().every(c=>c.disabled),'Visitors cannot change allocations');
 check(!panelNodes().some(c=>c.textContent==='Save storage allocations'),'Visitors cannot save allocations');
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
