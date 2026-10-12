// Run with: node tests/test_upgrade_dialog.js
const fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8');
const animation=new Function(fs.readFileSync('web/animations.js','utf8')+';return WorkerAnimation;')();
const elements=new Map(),listeners={};
function element(tag){return {tag,children:[],style:{},attributes:{},classList:{toggle(){}},append(...items){this.children.push(...items);},replaceChildren(){this.children=[];},setAttribute(k,v){this.attributes[k]=v;},addEventListener(name,handler){(this.listeners ||= {})[name]=handler;},focus(){},showModal(){this.open=true;},close(){this.open=false;this.listeners?.close?.();}};}
const document={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvas=document.getElementById('world');canvas.getContext=()=>new Proxy({},{get:()=>()=>{}});canvas.getBoundingClientRect=()=>({width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation',
 source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
 function check(ok,message){if(!ok)throw Error(message);}
 function textOf(node){return (node.textContent || '')+node.children.map(textOf).join(' ');}
 const town={level:2,wood:100,food:40,coins:20,goods:{},villagers:[{woodcutting:10}],player:{},buildings:[{kind:'hall',x:5,y:5},{kind:'home',x:6,y:5},{kind:'woodcutter',x:7,y:5}]};
 world={world_id:'test',towns:{own:town},hall_ready:{own:true},hall_upgrades:[{level:3,wood:60,seconds:60,coins:40,milestone:'Develop two professions'}],progression:{buildings:{sawpit:{label:'Sawpit',level:3}},crops:{carrots:3},animals:{sheep:{level:3}},upgrades:{axe:{label:'Improved axe',building:'woodcutter',level:2,skill:'woodcutting',xp:10,wood:10,coins:5}}}};
 identity={town_id:'own'};viewed='own';
 const commands=[];act=data=>commands.push(data);
 showUpgrade({action:'upgrade_building',index:1});
 check($('upgrade-dialog').open&&!commands.length,'Opening preview sends no command');
 check($('upgrade-cost').textContent==='20 wood','Building preview shows exact cost');
 check($('upgrade-time').textContent.includes('30 seconds'),'Building preview shows builder time');
 check($('upgrade-benefits').children.some(row=>textOf(row).includes('4 spaces')),'Building preview shows new housing');
 check($('upgrade-title').textContent==='Upgrade Cottage'&&$('upgrade-description').textContent==='Level 1 → 2','Building heading separates name and level change');
 check($('upgrade-location').textContent==='(6, 5)','Coordinates remain visible');
 check($('confirm-upgrade').textContent==='Upgrade for 20 wood','Confirmation shows exact spending');
 check($('upgrade-requirements').hidden&&$('upgrade-requirements-label').hidden&&$('upgrade-error').hidden,'Empty requirements and errors are hidden');
 check(!canControlPlayer(),'Modal blocks movement');
 $('cancel-upgrade').onclick();check(!pendingUpgrade&&!commands.length&&!$('upgrade-dialog').open,'Cancel discards pending upgrade');
 showUpgrade({action:'upgrade_capacity',index:1});
 check(textOf($('upgrade-benefits').children[0]).includes('2 → 4'),'Capacity preview shows before and after');
 $('confirm-upgrade').onclick();$('confirm-upgrade').onclick();
 check(commands.length===1&&commands[0].action==='upgrade_capacity'&&commands[0].index===1,'Confirmation sends once to the selected building');
 showUpgrade({action:'profession_upgrade',index:2,upgrade:'axe'});
 check(textOf($('upgrade-benefits').children[0]).includes('25% wood'),'Equipment preview explains effect');
 check($('confirm-upgrade').textContent==='Upgrade for 10 wood + 5 coins','Equipment confirmation shows every resource');
 check($('upgrade-cost').textContent.includes('5 coins')&&$('upgrade-requirements').textContent.includes('10 woodcutting practice'),'Equipment shows resources and practice');
 town.coins=0;renderUpgradeDialog();$('confirm-upgrade').onclick();
 check(!$('upgrade-error').hidden,'Blocked upgrades show the reason');
 check($('confirm-upgrade').disabled&&commands.length===1,'Resource changes prevent spending from stale popup');
 town.coins=20;renderUpgradeDialog();check($('upgrade-error').hidden,'Resolved errors disappear');$('confirm-upgrade').onclick();
 check(commands[1].upgrade==='axe','Equipment confirmation preserves upgrade key');
 showUpgrade({action:'upgrade'});
 check($('upgrade-benefits').children.some(row=>textOf(row).includes('40 coins')),'Hall preview shows reward');
 check($('upgrade-benefits').children.some(row=>textOf(row).includes('Sawpit')),'Hall preview shows unlocks');
 check($('upgrade-cost').textContent==='60 wood','Hall reward is not counted as a cost');
 check($('upgrade-requirements').textContent.includes('Develop two professions'),'Hall preview shows milestone');
 world.hall_ready.own=false;renderUpgradeDialog();check($('confirm-upgrade').disabled,'Hall milestone is rechecked');
 world.hall_ready.own=true;$('confirm-upgrade').onclick();
 check(commands[2].action==='upgrade'&&!('index' in commands[2]),'Hall keeps the server command contract');
 town.buildings.push({kind:'storehouse',x:8,y:5});
 const capacities=[50,75,100,150,200,250,300,350];
 for(let level=1;level<8;level++){
   town.buildings[3].level=level;showUpgrade({action:'upgrade_building',index:3});
   const row=$('upgrade-benefits').children[0];
   check(row.children[0].textContent==='Capacity per resource'&&row.children[1].textContent===capacities[level-1]+' → '+capacities[level]&&row.children[2].textContent==='+'+(capacities[level]-capacities[level-1]),'Every storage tier shows compact before/after and gain');
   $('cancel-upgrade').onclick();
 }
 showUpgrade({action:'upgrade_building',index:1});town.buildings.splice(1,1);
 $('confirm-upgrade').onclick();check(commands.length===3&&$('confirm-upgrade').disabled,'Removed buildings cannot redirect an upgrade');
 $('upgrade-dialog').close();check(!pendingUpgrade,'Escape/native close clears pending command');
 showUpgrade({action:'upgrade_building',index:1});world.world_id='another';
 $('confirm-upgrade').onclick();check(commands.length===3,'World changes invalidate preview');
`);
run(document,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},animation);
console.log('Upgrade dialog checks passed');
