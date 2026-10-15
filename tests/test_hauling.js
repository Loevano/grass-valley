const assert=require('assert'),fs=require('fs');
const animation=require('../web/animations.js');
const source=fs.readFileSync('web/app.js','utf8');
const client=require('./client_harness');
const town={map_size:16,buildings:[{kind:'hall',x:1,y:2},{kind:'storehouse',x:5,y:2}],map_resources:[]};
const villager={name:'Ada',job:'hauling',hauling:20,building_index:1,
  logistics_position:{x:0,y:0},logistics_target:{kind:'delivery',x:1,y:0},
  logistics_trip:{path:[[0,0],[1,0],[2,0],[3,0]],step:0}};
for(const [practice,speed] of [[undefined,1.8],[19.99,1.8],[20,2.25],[40,2.7]]){
  const state=animation.plan(town,{...villager,hauling:practice},0);
  animation.advance(state,.2,true);
  assert(Math.abs(state.x-speed*.2)<1e-9,'Delivery animation reflects hauling speed');
}
const cardClient=client();
cardClient.run(`world={progression:{buildings:{},talent_roots:{}}};identity={town_id:'own'};viewed='own';
  skillBar=(label,value)=>({label,value});villagerActivity=()=> 'Hauling goods';`);
const card=(town,worker,index)=>cardClient.run(`villagerCard(${JSON.stringify(town)},${JSON.stringify(worker)},${index})`);
const details=card(town,villager,0);
assert(details.children.some(c=>c.label==='Hauling'&&c.value===20));
assert(details.children.some(c=>c.className==='villager-hauling'&&c.textContent.includes('100 goods per trip · 2.25 tiles/s')));
assert(card(town,{...villager,hauling:undefined},0).children.some(c=>c.label==='Hauling'&&c.value===0));
console.log('Hauling skill display and animation checks passed');

const cargoTrip={operation:'collect',path:[[0,0],[1,0],[2,0],[3,0],[4,0]],
  pickup_step:1,delivery_step:3,step:0,delivered:false,cargo:{wood:12,planks:3.5}};
const loaded={...villager,hauling:0,logistics_trip:cargoTrip};
let cargoState=animation.plan(town,loaded,0);
animation.advance(cargoState,0,true);
assert.deepStrictEqual(cargoState.cargo,{},'Haulers approach the pickup empty-handed');
animation.advance(cargoState,1,true);
assert.deepStrictEqual(cargoState.cargo,{wood:12,planks:3.5},'Show the mixed load after reaching the pickup');
animation.advance(cargoState,1,true);
assert.deepStrictEqual(cargoState.cargo,{},'Hide the load after reaching the destination');
assert(cargoState.path.length,'The empty return walk can continue');
const waiting=animation.plan(town,{...loaded,logistics_position:{x:4,y:0},
  logistics_trip:{...cargoTrip,step:4,delivery_step:4,picked_up:true,cargo:{wood:5}}},0);
animation.advance(waiting,0,true);
assert.deepStrictEqual(waiting.cargo,{wood:5},'Goods remain visible while destination storage is full');
const waitingSupply=animation.plan(town,{...loaded,logistics_position:{x:3,y:0},
  logistics_trip:{...cargoTrip,step:3,picked_up:true,cargo:{wood:5}}},0);
animation.advance(waitingSupply,10,true);
assert.strictEqual(waitingSupply.x,3,'A worker holding undelivered supplies waits at the destination');
assert.deepStrictEqual(waitingSupply.cargo,{wood:5});
waitingSupply.transportHolding=false;
animation.advance(waitingSupply,1,true);
assert.strictEqual(waitingSupply.x,4,'The return walk resumes once the host confirms delivery');
assert.deepStrictEqual(waitingSupply.cargo,{});
const cargoText=v=>card(town,v,0).children.find(c=>c.className==='villager-cargo').textContent;
assert.strictEqual(cargoText(loaded),'Collecting: 12 Wood · 3.5 Planks');
assert.strictEqual(cargoText({...loaded,logistics_trip:{...cargoTrip,step:2}}),'Collecting: 12 Wood · 3.5 Planks','Path progress alone does not prove pickup');
assert.strictEqual(cargoText({...loaded,logistics_trip:{...cargoTrip,step:2,picked_up:true}}),'Carrying: 12 Wood · 3.5 Planks');
assert.strictEqual(cargoText({...loaded,logistics_trip:{...cargoTrip,step:3,delivered:true}}),'Carrying: nothing');
assert.strictEqual(cargoText({...loaded,job:'milling',logistics_trip:{...cargoTrip,operation:'supply',picked_up:true,cargo:{wheat:8}}}),'Carrying: 8 Wheat','Producers show carried supplies');
assert.strictEqual(cargoText({...villager,job:'farming',logistics_trip:null,pending_output:{wheat:4}}),'Carrying: 4 Wheat','Gatherers show pending harvests');
assert.strictEqual(cargoText({...villager,job:'idle',logistics_trip:null}),'Carrying: nothing');
const paints=[],labels=[];
const ctx={fillRect(){paints.push(this.fillStyle);},strokeRect(){},strokeText(){},fillText(text){labels.push(text);}};
const drawCargo=new Function('ctx','goodName',source.slice(source.indexOf('function drawHaulerCargo('),source.indexOf('// Tools and materials are drawn'))+'return drawHaulerCargo;')(ctx,key=>key);
drawCargo({wood:12,fish:3.5,wool:2});
assert(paints.includes('#997044')&&paints.includes('#8ebbbd')&&paints.includes('#eee0bd'),'Basket contents distinguish the resource types');
assert.deepStrictEqual(labels,['12 wood · 3.5 fish · 2 wool'],'Map labels identify every carried good and quantity');
console.log('Hauler cargo display checks passed');
const frameHarness=new Function('WorkerAnimation',`
  let world,viewed='own',workerTown=null;
  const workers=new Map();
  ${source.slice(source.indexOf('function animatedVillagers('),source.indexOf('let npcClock='))}
  return {frame(town,seconds){world={world_id:'cargo-test',towns:{own:town}};
    return animatedVillagers(town,seconds)[0].state;}};
`)(animation);
const animatedTown={...town,villagers:[{...loaded,production_status:'Hauling outputs',
  logistics_trip:{...cargoTrip,id:1}}]};
let first=frameHarness.frame(animatedTown,0);
frameHarness.frame(animatedTown,1);
assert.deepStrictEqual(first.cargo,{wood:12,planks:3.5});
animatedTown.villagers[0].logistics_trip={...cargoTrip,id:2,cargo:{fish:4}};
assert.strictEqual(frameHarness.frame(animatedTown,0),first,'Finish the visible previous load before starting a new trip');
assert.deepStrictEqual(first.transportCargo,{wood:12,planks:3.5},'A newer trip cannot overwrite the load still on screen');
frameHarness.frame(animatedTown,5);
const second=frameHarness.frame(animatedTown,0);
assert.notStrictEqual(second,first,'Identical routes get a fresh animation for each trip');
assert.deepStrictEqual(second.transportCargo,{fish:4});
console.log('Repeated hauling trip checks passed');
