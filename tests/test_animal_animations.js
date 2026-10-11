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
const animals=sprites([barn],0);
assert.equal(animals.length,12,'Every animal appears, including upgraded barn capacity');
for(const species of Object.keys(barn.animals))assert.equal(animals.filter(a=>a.species===species).length,3);
assert.equal(sprites([{...barn,animals:{}}],0).length,0,'Empty barns have no animals');
assert.equal(sprites([{...barn,kind:'farm'}],0).length,0,'Other workplaces have no barn animals');
assert.equal(sprites([{...barn,construction:{progress:0}}],0).length,0,'New construction hides animals');
assert.equal(sprites([{...barn,construction:{upgrade:true}}],0).length,12,'Upgrading retains the herd');
assert.equal(sprites([{...barn,animals:{chicken:4}}],0).length,4,'Purchases and breeding appear from the current stock');
assert.equal(sprites([],0).length,0,'Demolished barns leave no animals');
let farthest=0;
for(let time=0;time<32;time+=.1)for(const animal of sprites([barn],time)){
  const distance=Math.abs(animal.x-barn.x)+Math.abs(animal.y-barn.y);
  farthest=Math.max(farthest,distance);
  assert(distance<2.5,'Animals remain close to their own barn');
}
assert(farthest>2,'Animals roam beyond the barn tile');
const obstacle={kind:'home',x:6,y:5};
const landscape={map_size:12,map_resources:[{kind:'water',x:5,y:6},{kind:'tree',x:4,y:5}],
  terrain:Array.from({length:12},()=>Array(12).fill('g'))};
landscape.terrain[4][5]='v';
for(let time=0;time<32;time+=.1)for(const animal of sprites([barn,obstacle],time,landscape)){
  assert.equal(Math.round(animal.x),barn.x,'An enclosed herd stays at its barn');
  assert.equal(Math.round(animal.y),barn.y,'Animals avoid buildings, resources, and void terrain');
  assert(!animal.walking,'Enclosed animals rest rather than walking in place');
}
for(let time=0;time<32;time+=.1)for(const animal of sprites([{...barn,x:0,y:0}],time,{map_size:3})){
  assert(animal.x>=0&&animal.y>=0&&animal.x<3&&animal.y<3,'Edge barns keep their herd inside map bounds');
}
const openLandscape={...landscape,terrain:Array(12).fill('g'.repeat(12))};
const blockedTiles=new Set(['6,5','5,6','4,5']);
let previous=null;
for(let time=0;time<32;time+=.1){
  const herd=sprites([barn,obstacle],time,openLandscape);
  for(const [index,animal] of herd.entries()){
    assert(!blockedTiles.has(`${Math.round(animal.x)},${Math.round(animal.y)}`),'Trips go around occupied neighboring tiles');
    if(previous)assert(Math.hypot(animal.x-previous[index].x,animal.y-previous[index].y)<.08,'Trips remain continuous through turns and rest transitions');
  }
  previous=herd;
}
const chunked={chunked:true,map_size:12,terrain:{5:{5:'g'}}};
assert(sprites([barn],4,chunked).every(a=>!a.walking),'Animals avoid unloaded map chunks');
const lone={...barn,animals:{chicken:1}};
const resting=sprites([lone],0)[0],moving=sprites([lone],4)[0],later=sprites([lone],4.1)[0];
assert(moving.walking,'Animals enter a walking phase');
assert(Math.hypot(later.x-moving.x,later.y-moving.y)>0,'Walking interpolates inside the yard');
assert(Math.hypot(later.x-moving.x,later.y-moving.y)<.08,'Walking moves smoothly');
assert(resting.feeding,'Animals also peck or graze');
const returned=sprites([lone],16)[0];
assert.deepEqual([returned.x,returned.y],[resting.x,resting.y],'Animals return to the barn after each trip');
function render(animal){marks.length=0;draw(animal);return JSON.stringify(marks);}
assert.notEqual(render(moving),render(later),'Legs and body animate between frames');
assert.notEqual(render(resting),render(moving),'Feeding and walking have different poses');
assert.equal(new Set(['chicken','sheep','pig','cow'].map(species=>render({...moving,species}))).size,4,'Each species has distinct artwork');
assert.equal(JSON.stringify(barn),original,'Animation leaves authoritative stock and production untouched');
console.log('Animal animation checks passed');
