const assert=require('assert'),fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8');
const document={createElement(tag){return {tag,children:[],append(...items){this.children.push(...items);},setAttribute(){}};}};
const town={name:'Home',level:8,goods:{saddles:1,leather:10},coins:10,player:{x:4,y:7,leading_horse:true},
  buildings:[{kind:'stable',x:4,y:6,animals:{horse:2},animal_growth:{horse:[300,0]}},{kind:'market'}]};
const friend={name:'Friend',level:5,coins:20,goods:{},buildings:[{kind:'market'}],
  trade_offers:[{id:'offer',target_town:'own',give_resource:'coins',give_amount:10,want_resource:'leather',want_amount:5}]};
const world={towns:{own:town,friend},progression:{animal_growth_seconds:300,tradable_goods:['coins','leather','planks']}};
const commands=[];
const amountOf=(town,key)=>town[key] ?? town.goods?.[key] ?? 0;
const canAfford=(town,cost)=>Object.entries(cost).every(([key,amount])=>amountOf(town,key)>=amount);
const create=new Function('document','world','identity','buildingLevel','amountOf','canAfford','goodName','act',
  source.slice(source.indexOf('function renderStable('),source.indexOf('function renderRecipes('))+
  source.slice(source.indexOf('function animalAges('),source.indexOf('function renderAnimalButchery('))+
  'return {renderStable,renderTownTrades};');
const ui=create(document,world,{town_id:'own'},b=>b.level || 1,amountOf,canAfford,key=>key,data=>commands.push(data));
function stable(){const panel=document.createElement('div');ui.renderStable(town,town.buildings[0],0,panel);return panel;}
let panel=stable(),settle=panel.children.find(b=>b.textContent==='Settle the horse you are leading');
assert(!settle.disabled);settle.onclick();assert.deepEqual(commands.pop(),{action:'stable_horse',index:0});
let ride=panel.children.find(b=>b.textContent==='Saddle an adult horse and ride');
assert(!ride.disabled);ride.onclick();assert.deepEqual(commands.pop(),{action:'ride_horse',index:0});
town.goods.saddles=0;assert(stable().children.find(b=>b.textContent==='Saddle an adult horse and ride').disabled);
town.player.saddle=true;assert(!stable().children.find(b=>b.textContent==='Saddle an adult horse and ride').disabled);
town.player.x=12;assert(stable().children.filter(b=>b.tag==='button').every(b=>b.disabled),'Stable controls require proximity');
town.player.x=4;town.player.horse=true;assert(!stable().children.find(b=>b.textContent==='Return riding horse for breeding').disabled);
panel=document.createElement('div');ui.renderTownTrades(town,panel);
const selectors=panel.children.filter(b=>b.tag==='select'),quantities=panel.children.filter(b=>b.tag==='input');
selectors[0].value='friend';selectors[1].value='leather';selectors[2].value='coins';quantities[0].value='5';quantities[1].value='10';
panel.children.find(b=>b.textContent==='Create trade offer').onclick();
assert.deepEqual(commands.pop(),{action:'create_trade',target_town:'friend',give_resource:'leather',give_amount:5,want_resource:'coins',want_amount:10});
let accept=panel.children.find(b=>b.tag==='p'&&b.children.length)?.children[0];
assert.equal(accept.textContent,'Accept trade');assert(!accept.disabled);accept.onclick();
assert.deepEqual(commands.pop(),{action:'accept_trade',source_town:'friend',offer_id:'offer'});
town.goods.leather=0;panel=document.createElement('div');ui.renderTownTrades(town,panel);
assert(panel.children.find(b=>b.tag==='p'&&b.children.length).children[0].disabled,'Accept button reflects insufficient goods');
town.trade_offers=[{id:'own-offer',target_town:'friend',give_resource:'leather',give_amount:1,want_resource:'coins',want_amount:1}];
panel=document.createElement('div');ui.renderTownTrades(town,panel);
panel.children.find(b=>b.tag==='p'&&b.children[0]?.textContent==='Cancel offer').children[0].onclick();
assert.deepEqual(commands.pop(),{action:'cancel_trade',source_town:'own',offer_id:'own-offer'});
console.log('Industry controls passed');
