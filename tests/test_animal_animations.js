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
for(let time=0;time<20;time+=.1)for(const animal of sprites([barn],time)){
  assert(animal.x>=barn.x&&animal.x<barn.x+1&&animal.y>=barn.y&&animal.y<barn.y+1,'Animals stay on their barn tile');
  assert(animal.x+animal.y>barn.x+barn.y,'Animals sort in front of their barn');
}
const lone={...barn,animals:{chicken:1}};
const resting=sprites([lone],0)[0],moving=sprites([lone],4)[0],later=sprites([lone],4.1)[0];
assert(moving.walking,'Animals enter a walking phase');
assert(Math.hypot(later.x-moving.x,later.y-moving.y)>0,'Walking interpolates inside the yard');
assert(Math.hypot(later.x-moving.x,later.y-moving.y)<.02,'Walking moves smoothly');
assert(resting.feeding,'Animals also peck or graze');
function render(animal){marks.length=0;draw(animal);return JSON.stringify(marks);}
assert.notEqual(render(moving),render(later),'Legs and body animate between frames');
assert.notEqual(render(resting),render(moving),'Feeding and walking have different poses');
assert.equal(new Set(['chicken','sheep','pig','cow'].map(species=>render({...moving,species}))).size,4,'Each species has distinct artwork');
assert.equal(JSON.stringify(barn),original,'Animation leaves authoritative stock and production untouched');
console.log('Animal animation checks passed');
