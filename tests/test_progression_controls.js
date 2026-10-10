const fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8');
const animation=new Function(fs.readFileSync('web/animations.js','utf8')+';return WorkerAnimation;')();
const elements=new Map(),listeners={};
function element(tag){return {tag,children:[],style:{},attributes:{},classList:{toggle(){}},append(...items){this.children.push(...items);},replaceChildren(){this.children=[];},setAttribute(k,v){this.attributes[k]=v;},addEventListener(){},focus(){},showModal(){},close(){}};}
const document={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvas=document.getElementById('world');canvas.getContext=()=>new Proxy({},{get:()=>()=>{}});canvas.getBoundingClientRect=()=>({width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation',
 source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
 function check(ok,message){if(!ok)throw Error(message);}
 const town={name:'Town',wood:100,food:40,coins:20,level:1,goods:{herbs:2,fodder:6},player:{x:5,y:7,facing:1,fishing:21},
 buildings:[{kind:'hall',x:5,y:5},{kind:'barn',x:3,y:5,animals:{chicken:2}},{kind:'farm',x:3,y:7},{kind:'acre',x:4,y:7,farm_index:2}],
 villagers:[{name:'Ada',job:'idle',woodcutting:10}],map_resources:[{kind:'tree',x:6,y:7,supply:4,supply_max:8,regeneration:.5}]};
 world={world_id:'test',towns:{own:town,friend:town},progression:{buildings:{barn:{level:2,wood:25}},crops:{vegetables:1,wheat:2,fodder:3},animals:{chicken:{level:2,coins:3,output:'eggs'},sheep:{level:3,coins:6,output:'wool'}},upgrades:{},recipes:{}}};
 identity={town_id:'own'};viewed='own';draw=()=>{};
 const commands=[];act=data=>commands.push(data);
 render();
 const barnButton=[...$('buildings').children].find(b=>b.buildingKind==='barn');
 check(barnButton.disabled&&barnButton.textContent.includes('Hall 2'),'Locked building explains its required hall level');
 town.level=2;render();check(!barnButton.disabled,'Hall unlock enables an affordable building');
 inspect('building',1);
 const buy=label=>$('selection-details').children.find(b=>b.tag==='button'&&b.textContent.startsWith(label));
 check(!buy('Buy chicken').disabled&&buy('Buy sheep').disabled,'Animal choices respect hall unlocks');
 buy('Buy chicken').onclick();check(commands.at(-1).action==='buy_animal'&&commands.at(-1).index===1,'Animal purchase targets the selected barn');
 inspect('building',3);
 const crops=$('selection-details').children.find(b=>b.tag==='select'&&b.attributes['aria-label']==='Crop');
 check(crops.children.find(b=>b.value==='fodder').disabled,'Locked crops cannot be selected');
 crops.value='wheat';crops.onchange();check(commands.at(-1).crop==='wheat'&&commands.at(-1).index===3,'Crop selector sends the selected acre');
 inspect('resource',0);check($('selection-details').children.some(c=>c.children?.[0]?.textContent==='4.0 / 8'),'Sources show their shared supply');
 inspect('player',0);check($('selection-details').children.some(c=>c.className==='player-skills'),'Founder shows saved skill tiles');
 town.player.activity={job:'woodcutting',kind:'tree',x:6,y:7};town.player.activity_status='Working';
 let hero=playerSprite(town,0);check(hero.active&&hero.state.phase==='working'&&hero.villager.job==='woodcutting','Founder action uses the woodcutting animation');
 town.player.activity_status='Waiting for supply to regenerate';hero=playerSprite(town,0);check(!hero.active&&hero.state.phase==='idle','Paused founder stops the working animation');
 check(buildingUsable({map_resources:[{kind:'water',x:5,y:0}]},{kind:'fishing_hut',x:0,y:0,profession_upgrades:['boat']}),'Boat extends usability beyond the original three steps');
 town.buildings.push({kind:'home',x:2,y:5,profession_upgrades:['timber_frame']});inspect('building',4);
 check($('selection-details').children.some(c=>c.children?.[0]?.textContent==='+4'),'Timber framing increases displayed housing');
 viewed='friend';inspect('building',1);check(!buy('Buy chicken'),'Visitors cannot purchase animals');
 `);
run(document,{devicePixelRatio:1,addEventListener(k,v){listeners[k]=v;}},{getItem:()=>null},()=>0,()=>{},animation);
console.log('Progression controls passed');
