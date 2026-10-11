// Run with: node tests/test_workplace_usability.js
const assert=require('assert');
const source=require('fs').readFileSync('web/app.js','utf8');
const fills=[];
const ctx=new Proxy({}, {get:()=>()=>{}});
const {usable,draw}=new Function('ctx','polygon','point','tileW','tileH','colors',
  source.slice(source.indexOf('function buildingLevel('),source.indexOf('function storageCapacity('))+
  source.slice(source.indexOf('function buildingUsable('),source.indexOf('async function selectTile('))+
  '\nreturn {usable:buildingUsable,draw:drawBuilding};'
)(ctx,(points,color)=>fills.push(color),()=>[0,0],46,23,
  {farm:'#bc9b5c',woodcutter:'#947b55',fishing_hut:'#7899a2',home:'#c5ad80'});
for(const [kind,resource] of [['fishing_hut','water'],['woodcutter','tree']]){
  const building={kind,x:5,y:5};
  const reach=kind==='woodcutter'?10:3;
  const town={map_resources:[{kind:resource,x:5+reach,y:6}]};
  assert(!usable(town,building),'Sources beyond the workplace range are out of reach');
  fills.length=0;draw(building,town);
  assert(fills.includes('#c76d64')&&fills.includes('#a34f49'),'Unusable walls and roof are red');
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
console.log('Workplace usability checks passed');
