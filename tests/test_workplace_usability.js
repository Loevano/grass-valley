// Run with: node tests/test_workplace_usability.js
const assert=require('assert');
const app=require('./client_harness')();
const {usable,draw,fills}=app.run(`
  const fills=[],originalPolygon=polygon;
  polygon=(points,color,stroke,target)=>{fills.push(color);return originalPolygon(points,color,stroke,target);};
  ({usable:buildingUsable,draw:(building,town)=>paintBuilding(building,town),fills});
`);
for(const [kind,resource] of [['fishing_hut','water'],['woodcutter','tree']]){
  const building={kind,x:5,y:5};
  const reach=kind==='woodcutter'?10:3;
  const town={map_resources:[{kind:resource,x:5+reach,y:6}]};
  assert(!usable(town,building),'Sources beyond the workplace range are out of reach');
  fills.length=0;draw(building,town);
  assert(fills.includes('#c76d64')&&fills.includes('#a34f49'),'Unusable walls and roof retain the red warning palette');
  building.x=6;
  assert(usable(town,building),'Moving to the exact range boundary enables work');
  fills.length=0;draw(building,town);
  assert(!fills.includes('#c76d64')&&!fills.includes('#a34f49'),'Usable buildings regain their normal colors');
  town.map_resources=[];
  assert(!usable(town,building),'Missing sources disable workplaces');
  town.map_resources=[{kind:'rock',x:6,y:6}];
  assert(!usable(town,building),'Wrong resource types do not enable workplaces');
}
const farm={kind:'farm',x:5,y:5};
for(const town of [{map_resources:[]},{map_resources:[{kind:'water',x:20,y:20}]}]){
  assert(usable(town,farm),'Farms do not require nearby water');
  fills.length=0;draw(farm,town);
  assert(!fills.includes('#c76d64')&&!fills.includes('#a34f49'),'Farms keep their normal colors without nearby water');
}
assert(usable({map_resources:[]},{kind:'home',x:5,y:5}),'Cottages do not require nearby sources');
assert(usable({map_resources:[]},{kind:'hall',x:5,y:5}),'Town halls do not require nearby sources');
for(const level of [1,2,3])for(const boat of [false,true]){
  const hut={kind:'fishing_hut',x:5,y:5,level,profession_upgrades:boat?['boat']:[]};
  const reach=2+level+(boat?3:0);
  assert(usable({map_resources:[{kind:'water',x:5+reach-1,y:6}]},hut),'Completed hut upgrades and boats expand reach');
  assert(!usable({map_resources:[{kind:'water',x:5+reach,y:6}]},hut),'Water beyond upgraded reach remains unavailable');
}
const upgrading={kind:'fishing_hut',x:5,y:5,construction:{progress:15,total:30,upgrade:{level:2}}};
assert(!usable({map_resources:[{kind:'water',x:9,y:5}]},upgrading),'Pending levels do not expand reach');
console.log('Workplace usability checks passed');
