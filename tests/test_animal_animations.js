// Run with: node tests/test_animal_animations.js
const assert=require('assert'),fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8'),marks=[];
const ctx=new Proxy({}, {get(target,key){
  if(key in target)return target[key];
  return (...args)=>marks.push([key,...args,target.fillStyle,target.strokeStyle]);
}});
const {sprites,draw}=new Function('ctx','point','tileW','tileH',
  source.slice(source.indexOf('function barnAnimalSprites('),source.indexOf('function draw(seconds=0)'))+
  'return {sprites:barnAnimalSprites,draw:drawBarnAnimal};'
)(ctx,(x,y)=>[(x-y)*23,(x+y)*11.5],46,23);
const barn={kind:'barn',x:5,y:5,level:3,animals:{chicken:3,sheep:3,pig:3,cow:3}};
const original=JSON.stringify(barn);
const Scene=require('../web/scene.js');
const cachedSprites=new Function('ClientScene',
  source.slice(source.indexOf('function barnAnimalSprites('),source.indexOf('function drawBarnAnimal('))+
  'return barnAnimalSprites;'
)(Scene);
const animals=sprites([barn],0);
assert.equal(animals.length,12,'Every animal appears, including upgraded barn capacity');
const young=sprites([{...barn,animals:{pig:1},animal_growth:{pig:[0]}}],0)[0];
const growingBarn={...barn,animals:{pig:1},animal_growth:{pig:[150]}};
const growthBefore=JSON.stringify(growingBarn);
assert.equal(sprites([growingBarn],0)[0].size,young.size,'A host growth update does not jump visual size');
const between=sprites([growingBarn],1.5)[0],halfGrown=sprites([growingBarn],3)[0];
assert(young.size<between.size&&between.size<halfGrown.size,'Size increases smoothly between confirmed ages');
assert.equal(sprites([growingBarn],30)[0].size,halfGrown.size,'Visual growth stops at the confirmed age');
assert.equal(JSON.stringify(growingBarn),growthBefore,'Smoothing leaves authoritative growth untouched');
const adultBarn={...barn,animals:{pig:1},animal_growth:{pig:[300]}};
assert.equal(sprites([adultBarn],30)[0].size,halfGrown.size,'Reaching adulthood also blends from the current size');
const adult=sprites([adultBarn],33)[0];
assert(young.size<halfGrown.size&&halfGrown.size<adult.size,'Fed growth gradually increases visual size');
assert.equal(sprites([{...barn,animals:{pig:1},animal_growth:{pig:[0]}}],33)[0].size,young.size,'A replacement young animal starts small');
const timedSprites=new Function('world',
  source.slice(source.indexOf('function barnAnimalSprites('),source.indexOf('function drawBarnAnimal('))+'return barnAnimalSprites;'
)({shared_map:{},progression:{animals:{chicken:{growth_seconds:30},sheep:{growth_seconds:60},pig:{growth_seconds:120},cow:{growth_seconds:180}}}});
for(const [species,duration] of [['chicken',30],['sheep',60],['pig',120],['cow',180]]){
  const growing={...barn,animals:{[species]:1},animal_growth:{[species]:[duration/2]}};
  const original=JSON.stringify(growing),half=timedSprites([growing],0)[0];
  assert(Math.abs(half.size-.6)<1e-9,'Each species reaches half size on its own growth timer');
  growing.animal_growth[species]=[0];
  assert.equal(timedSprites([growing],1)[0].size,.4,'Automatic replacement resets visible size');
  assert.equal(JSON.stringify({...growing,animal_growth:{[species]:[duration/2]}}),original,'Rendering preserves confirmed ages');
}
for(const species of Object.keys(barn.animals))assert.equal(animals.filter(a=>a.species===species).length,3);
assert.equal(sprites([{...barn,animals:{}}],0).length,0,'Empty barns have no animals');
assert.equal(sprites([{...barn,kind:'farm'}],0).length,0,'Other workplaces have no barn animals');
assert.equal(sprites([{...barn,construction:{progress:0}}],0).length,0,'New construction hides animals');
assert.equal(sprites([{...barn,construction:{upgrade:true}}],0).length,12,'Upgrading retains the herd');
assert.equal(sprites([{...barn,animals:{chicken:4}}],0).length,4,'Purchases and breeding appear from the current stock');
assert.equal(sprites([],0).length,0,'Demolished barns leave no animals');
let farthest=0;
for(const animate of [sprites,cachedSprites])for(let time=0;time<32;time+=.1)for(const animal of animate([barn],time)){
  const distance=Math.abs(animal.x-barn.x)+Math.abs(animal.y-barn.y);
  farthest=Math.max(farthest,distance);
  assert(Math.max(Math.abs(animal.x-barn.x),Math.abs(animal.y-barn.y))<3.5,'Animals remain inside their barn fence');
  assert(Math.round(animal.x)!==barn.x||Math.round(animal.y)!==barn.y,'Walking and grazing avoid the barn footprint');
}
assert(farthest>6,'Animals reach the corners of the full fenced range');
for(const animate of [sprites,cachedSprites])for(let time=0;time<40;time+=.5){
  for(const animal of animate([{...barn,level:10,animals:{chicken:40}}],time))
    assert(Math.max(Math.abs(animal.x-barn.x),Math.abs(animal.y-barn.y))<3.5,'Large herds stay inside the fence');
  for(const animal of animate([{...barn,kind:'stable',animals:{horse:4}}],time))
    assert(Math.abs(animal.x-barn.x)+Math.abs(animal.y-barn.y)<2.5,'Stable roaming retains its original range');
}
const obstacle={kind:'home',x:6,y:5};
const landscape={map_size:12,map_resources:[{kind:'water',x:5,y:6},{kind:'tree',x:4,y:5}],
  terrain:Array.from({length:12},()=>Array(12).fill('g'))};
landscape.terrain[4][5]='v';
for(const animate of [sprites,cachedSprites]){
  assert.equal(animate([barn,obstacle],4,landscape).length,0,'Enclosed herds stay sheltered instead of overlapping the barn');
  assert.equal(animate([barn,obstacle],4,{...landscape,map_resources:[]}).length,12,'Animals appear when a side opens');
}
for(const animate of [sprites,cachedSprites])for(let time=0;time<32;time+=.1)for(const animal of animate([{...barn,x:0,y:0}],time,{map_size:3})){
  assert(animal.x>=0&&animal.y>=0&&animal.x<3&&animal.y<3,'Edge barns keep their herd inside map bounds');
}
const openLandscape={...landscape,terrain:Array(12).fill('g'.repeat(12))};
const blockedTiles=new Set(['5,5','6,5','5,6','4,5']);
for(const animate of [sprites,cachedSprites]){
  let previous=null;
  for(let time=0;time<32;time+=.1){
    const herd=animate([barn,obstacle],time,openLandscape);
    assert.equal(herd.length,12,'One accessible side still lets the entire herd roam');
    for(const [index,animal] of herd.entries()){
      assert(!blockedTiles.has(`${Math.round(animal.x)},${Math.round(animal.y)}`),'Trips go around occupied neighboring tiles');
      if(previous)assert(Math.hypot(animal.x-previous[index].x,animal.y-previous[index].y)<.08,'Trips remain continuous through turns and rest transitions');
    }
    previous=herd;
  }
  const isolated={...openLandscape,map_resources:[...openLandscape.map_resources,
    {kind:'rock',x:4,y:4},{kind:'rock',x:6,y:4},{kind:'rock',x:5,y:3}]};
  const restingHerd=animate([barn,obstacle],4,isolated);
  assert.equal(restingHerd.length,12,'A single open tile can hold a resting herd');
  assert(restingHerd.every(a=>!a.walking&&Math.round(a.x)===5&&Math.round(a.y)===4),'Isolated outdoor animals rest without crossing obstacles');
}
const chunked={chunked:true,map_size:12,terrain:{5:{5:'g'}}};
for(const animate of [sprites,cachedSprites])assert.equal(animate([barn],4,chunked).length,0,'Animals avoid unloaded map chunks');
const lone={...barn,animals:{chicken:1}};
const resting=sprites([lone],0)[0],moving=sprites([lone],4)[0],later=sprites([lone],4.1)[0];
assert(moving.walking,'Animals enter a walking phase');
assert(Math.hypot(later.x-moving.x,later.y-moving.y)>0,'Walking interpolates inside the yard');
assert(Math.hypot(later.x-moving.x,later.y-moving.y)<.08,'Walking moves smoothly');
assert(resting.feeding,'Animals also peck or graze');
const returned=Array.from({length:160},(_,i)=>sprites([lone],12+i*.5)[0])
  .find(a=>!a.walking&&a.x===resting.x&&a.y===resting.y);
assert(returned,'Animals return to their outdoor resting spot after each trip');
function render(animal){marks.length=0;draw(animal);return JSON.stringify(marks);}
assert.notEqual(render(moving),render(later),'Legs and body animate between frames');
assert.notEqual(render(resting),render(moving),'Feeding and walking have different poses');
assert.equal(new Set(['chicken','sheep','pig','cow'].map(species=>render({...moving,species}))).size,4,'Each species has distinct artwork');
assert.equal(JSON.stringify(barn),original,'Animation leaves authoritative stock and production untouched');
const shelter={...barn,kind:'village_hut',village:'Sunwell',animals:{horse:2,chicken:2}};
const shelterBefore=JSON.stringify(shelter),stock=sprites([shelter],4);
assert.equal(stock.filter(a=>a.species==='horse').length,2,'Caravan shelters show their horses');
assert.equal(stock.filter(a=>a.species==='chicken').length,2,'Village shelters support mixed herds');
assert(stock.some(a=>a.walking),'Village animals roam between feeding stops');
assert.equal(JSON.stringify(shelter),shelterBefore,'Village stock remains authoritative');
const combined=new Function(source.slice(source.indexOf('function villageAnimalBuildings('),source.indexOf('function barnAnimalSprites('))+'return villageAnimalBuildings;')();
const buildings=[barn],data={villageBarns:[shelter]};
assert.strictEqual(combined(buildings,data),combined(buildings,data),'Village herds retain animal routing caches across frames');
assert.deepEqual(combined(buildings,data),[barn,shelter]);
console.log('Animal animation checks passed');
