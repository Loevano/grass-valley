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
 for(const [kind,capacities] of [['farm',[480,1080,1920]],['acre',[10,10,10]]]){
   for(const [index,capacity] of capacities.entries()){
     const building={kind,level:index+1};
     for(const crop of ['vegetables','wheat','carrots','fodder'])check(roomAllocations(building)[crop]===capacity,'Farm crop capacity follows building level');
     check(buildingStorageSize(building)===4*capacity+10*(kind==='acre'?1:index+1),'Farm storage total matches item allowances');
   }
 }
 const commands=[];act=data=>commands.push(data);
 inspect('building',1);
 const fields=()=>panelNodes().filter(c=>c.tag==='input'&&c.type==='checkbox'&&!c.attributes['aria-label']?.startsWith('Accept all '));
 const categoryToggle=title=>panelNodes().find(c=>c.attributes['aria-label']==='Accept all '+title+' resources');
 const meters=()=>panelNodes().filter(c=>c.tag==='meter');
 const button=text=>panelNodes().find(c=>c.tag==='button'&&c.textContent===text);
 const toggle=key=>fields().find(c=>c.attributes['aria-label']==='Accept '+key);
 check(meters().length===28&&fields().length===28,'Storage has meters and resource switches');
 check(meters().find(c=>c.attributes['aria-label']==='Fish stored').value===10&&meters().find(c=>c.attributes['aria-label']==='Smoked fish stored').value===4,'Food types have separate item counts');
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
 const groups=panelNodes().filter(c=>c.className==='resource-group');
 const baking=groups.find(c=>c.children[0].textContent==='Baking');
 check(baking&&panelNodes(baking).some(c=>c.textContent==='Bread')&&panelNodes(baking).some(c=>c.textContent==='Wheat'),'Storage groups ingredients and foods by trade');
 check(categoryToggle('Baking').indeterminate&&!categoryToggle('Baking').checked,'Partially enabled categories show a mixed checkbox');
 const beforeCategory=commands.length,fishBefore=toggle('Fish').checked;
 categoryToggle('Baking').checked=true;categoryToggle('Baking').onchange();
 check(commands.length===beforeCategory+1&&['wheat','flour','bread'].every(key=>commands.at(-1).enabled[key]),'Category selection enables its resources with one command');
 check(['Wheat','Flour','Bread'].every(key=>toggle(key).checked)&&!categoryToggle('Baking').indeterminate,'Category selection updates child switches and clears mixed state');
 categoryToggle('Baking').checked=false;categoryToggle('Baking').onchange();
 check(commands.length===beforeCategory+2&&['wheat','flour','bread'].every(key=>!commands.at(-1).enabled[key]),'Category deselection disables all of its resources with one command');
 check(!categoryToggle('Baking').checked&&!categoryToggle('Baking').indeterminate&&toggle('Fish').checked===fishBefore,'Category changes preserve other categories');
 toggle('Bread').checked=true;toggle('Bread').onchange();
 check(categoryToggle('Baking').indeterminate&&!categoryToggle('Baking').checked,'Individual changes update the category mixed state');
 town.buildings[1].storage_enabled=commands.at(-1).enabled;render();
 check(categoryToggle('Baking').indeterminate,'Category mixed state survives host refresh');
 toggle('Bread').checked=false;toggle('Bread').onchange();
 check(!commands.at(-1).enabled.bread&&commands.at(-1).enabled.fish,'Disabling bread preserves the fish switch');
 const beforeAll=commands.length;
 toggle('Wood').onclick({altKey:true});toggle('Wood').checked=true;toggle('Wood').onchange();
 check(commands.length===beforeAll+1&&Object.values(commands.at(-1).enabled).every(Boolean)&&fields().every(c=>c.checked),'Option-click enables all with one command');
 check(categoryToggle('Baking').checked&&!categoryToggle('Baking').indeterminate,'Toggle-all updates category checkboxes');
 toggle('Wood').onclick({altKey:true});toggle('Wood').checked=false;toggle('Wood').onchange();
 check(commands.length===beforeAll+2&&Object.values(commands.at(-1).enabled).every(enabled=>!enabled)&&fields().every(c=>!c.checked),'Repeated Option-click disables all with one command');
 town.buildings[1].storage_enabled=commands.at(-1).enabled;render();
 check(fields().every(c=>!c.checked),'Host-confirmed switches remain disabled');
 toggle('Planks').onclick({altKey:true});toggle('Planks').checked=true;toggle('Planks').onchange();
 check(commands.length===beforeAll+3&&Object.values(commands.at(-1).enabled).every(Boolean)&&fields().every(c=>c.checked),'Option-click on another switch enables all again');
 toggle('Wood').onclick({altKey:false});toggle('Wood').checked=false;toggle('Wood').onchange();
 check(!commands.at(-1).enabled.wood&&commands.at(-1).enabled.bread,'Normal click after Option-click only changes one resource');
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
 check(meters().length===28&&!fields().length&&!categoryToggle('Baking'),'Visitors see storage meters without switches');
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
 check(meters().length===4&&meters().some(c=>c.attributes['aria-label']==='Trout stored'),'Fishing huts have separate storage for each fish species');
 check(roomAllocations(town.buildings[4]).tools===undefined,'Old unrelated allocations are ignored');
 check(resourceStorage(town,'fish')===500&&resourceStorage(town,'bread')===450,'Level-8 warehouse and hall accept all food; the hut adds fish storage only');
 check(panelNodes().some(c=>c.textContent?.includes('Tools from previous storage')),'Legacy unrelated goods remain visible');
 check(!button('Allocate')&&roomAllocations(town.buildings[4]).fish===50,'Fishing huts ignore old allocations and have fixed food capacity');
 town.buildings.push({kind:'mill',x:1,y:3});inspect('building',5);
 check(meters().length===2,'Mill offers wheat and flour storage');
 check(!button('Allocate')&&!fields().length,'Mill capacity cannot be edited');
 check(roomAllocations(town.buildings[5]).wheat===80&&roomAllocations(town.buildings[5]).flour===80,'Mill buffers two minutes of base wheat and flour');
 for(const [kind,base] of [['mill',160],['bakery',100]]){
   for(let level=1;level<=3;level++){
     const building={kind,level},scale=1+.25*(level-1);
     check(buildingStorageSize(building)===base*scale,'Grain storage scales independently of workers');
     check(roomAllocations(building).flour===(kind==='mill'?80:40)*scale,'Flour allowances scale with storage levels');
     check(upgradeBenefit(kind,level,building).includes('storage spaces')&&!upgradeBenefit(kind,level,building).includes('crops / worker'),'Grain upgrades explain storage benefits');
   }
 }
 town.buildings[5].level=2;inspect('building',5);
 check(panelNodes().some(c=>c.className==='storage-note'&&c.textContent.includes('Separate input buffer: 0 / 200')),'Mill panel explains its upgraded input buffer');
 check(!panelNodes().some(c=>['Storage spaces','Capacity by resource'].includes(c.textContent)),'Storage capacity is not repeated inside Production');
 town.buildings[5].level=1;
 inspect('building',0);check(!button('Allocate'),'Hall capacity cannot be edited');
 world.progression={fixed_storage:{mill:{wheat:35,flour:15}},workplace_storage_sizes:{mill:90},storage_upgrade_kinds:['mill']};
 check(roomAllocations(town.buildings[5]).wheat===35,'Client uses host fixed capacities');
 check(buildingStorageSize({...town.buildings[5],level:2})===112.5,'Client uses host buffer sizes and upgrade kinds');
 delete world.progression;
 const smoke={kind:'smokehouse',x:1,y:3,output_buffer:{smoked_trout:2,smoked_sturgeon:3}};
 const smokePanel=document.createElement('div');smokePanel.append(document.createElement('h3'));renderStorage(town,smoke,6,smokePanel);
 check(panelNodes(smokePanel).some(c=>c.attributes?.['aria-label']==='Smoked trout stored'&&c.value===2),'Smokehouse displays stored smoked trout');
 check(panelNodes(smokePanel).some(c=>c.attributes?.['aria-label']==='Smoked sturgeon stored'&&c.value===3),'Smokehouse displays stored smoked sturgeon');
 check(resourceCategories(['sturgeon','smoked_sturgeon','smoked_trout']).every(([title])=>title==='Fishing'),'Smoked species stay in Fishing');
 check(marketSaleGoods({}).includes('smoked_sturgeon'),'Market includes smoked species');
 world.progression={recipes:{smokehouse:[{fish:1,wood:.25},'smoked_fish',1]},production_seconds:{smoked_fish:3}};
 const smokeDescription=productionDescription(smoke);
 check(smokeDescription.includes('1 fish + 0.25 wood')&&smokeDescription.includes('3s batch')&&smokeDescription.includes('+0.5 food and +1 coin per fish'),'Smokehouse explains conversion and added value');
 delete world.progression;
 const catalogFoods={...foodGoods(),bread_rolls:1.75};
 world.progression={edible:catalogFoods,storage_keys:[...storageKeys,'bread_rolls'],
   storage_goods:{storehouse:[...storageGoods.storehouse,'bread_rolls']},
   sellable_goods:['bread','bread_rolls','fish','smoked_fish','wooden_bowls','unknown_good'],
   profession_recipes:{bakery:[{output:'bread_rolls'}]}};
 $('selection-details').replaceChildren();$('selection-details').append(document.createElement('h3'));
 renderStorage(town,town.buildings[1],1);
 check(toggle('Bread rolls')&&meters().find(c=>c.attributes['aria-label']==='Bread rolls stored').max===350,'Host catalog foods have independent capacity and switches');
 const categorized=resourceCategories(['bread','wheat','flour','fish','smoked_fish','wood','planks','wooden_bowls','unknown_good','bread']);
 check(categorized.find(([title])=>title==='Baking')[1].join(',')==='wheat,flour,bread','Baking includes the whole chain in order');
 check(categorized.find(([title])=>title==='Fishing')[1].join(',')==='fish,smoked_fish','Fishing includes raw and smoked fish');
 check(categorized.find(([title])=>title==='Timber')[1].join(',')==='wood,planks,wooden_bowls','Timber includes crafted goods');
 check(categorized.flatMap(([,keys])=>keys).length===9&&categorized.at(-1)[0]==='Other resources','Unknown resources remain visible and duplicates appear once');
 const sales=document.createElement('div');renderMarketSales(town,sales);
 const editor=sales.children[0],saleGroups=editor.children.filter(c=>c.className==='resource-group');
 check(saleGroups.map(c=>c.children[0].textContent).join(',')==='Baking,Fishing,Timber,Other resources','Market sales use the same categories');
 const saleNodes=panelNodes(editor),breadCap=saleNodes.find(c=>c.id==='sale-cap-bread');
 check(!saleNodes.some(c=>['Enabled','Disabled'].includes(c.textContent)),'Automatic sales use checkboxes without duplicate enabled labels');
 breadCap.value=17;renderMarketSales(town,document.createElement('div'));
 check(renderMarketSales.editor===editor&&breadCap.value===17,'Grouped market editors preserve drafts on refresh');
 const breadSale=saleNodes.find(c=>c.id==='sale-enabled-bread'),breadForm=saleNodes.find(c=>c.children.some(child=>child.children?.includes(breadSale)));
 breadForm.reportValidity=()=>true;breadSale.checked=true;breadSale.onchange();
 check(commands.at(-1).action==='configure_market_sale'&&commands.at(-1).resource==='bread'&&commands.at(-1).cap===17,'Grouped sales retain resource-specific commands');
 viewed='friend';const commandCount=commands.length;breadSale.onchange();
 check(commands.length===commandCount,'Reattached market controls cannot change another town');viewed='own';
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
