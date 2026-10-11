const assert=require('assert'),fs=require('fs');
const animation=require('../web/animations.js');
const source=fs.readFileSync('web/app.js','utf8');
const town={map_size:16,buildings:[{kind:'hall',x:1,y:2},{kind:'storehouse',x:5,y:2}],map_resources:[]};
const villager={name:'Ada',job:'hauling',hauling:20,building_index:1,
  logistics_position:{x:0,y:0},logistics_target:{kind:'delivery',x:1,y:0},
  logistics_trip:{path:[[0,0],[1,0],[2,0],[3,0]],step:0}};
for(const [practice,speed] of [[undefined,1.8],[19.99,1.8],[20,2.25],[40,2.7]]){
  const state=animation.plan(town,{...villager,hauling:practice},0);
  animation.advance(state,.2,true);
  assert(Math.abs(state.x-speed*.2)<1e-9,'Delivery animation reflects hauling speed');
}
const element=tag=>({tag,children:[],append(child){this.children.push(child);},setAttribute(){}});
const world={progression:{buildings:{},talent_roots:{}}};
const helpers=source.slice(source.indexOf('function workerEfficiency('),source.indexOf('async function playerAction('));
const toolSource=source.slice(source.indexOf('const workerTools='),source.indexOf('const equipmentItems='));
const cardSource=source.slice(source.indexOf('function villagerCard('),source.indexOf('function villagerRow('));
const card=new Function('document','world','identity','viewed','buildingLevel','goodName','professionJobs','skillBar','villagerActivity','jobRequirement',helpers+toolSource+cardSource+'return villagerCard;')(
  {createElement:element},world,{town_id:'own'},'own',()=>1,key=>key,{},
  (label,value)=>({label,value}),()=> 'Hauling goods',()=> '');
const details=card(town,villager,0);
assert(details.children.some(c=>c.label==='Hauling'&&c.value===20));
assert(details.children.some(c=>c.className==='villager-hauling'&&c.textContent.includes('100 goods per trip · 2.25 tiles/s')));
assert(card(town,{...villager,hauling:undefined},0).children.some(c=>c.label==='Hauling'&&c.value===0));
console.log('Hauling skill display and animation checks passed');
