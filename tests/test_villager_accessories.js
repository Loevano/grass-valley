// Run with: node tests/test_villager_accessories.js
const assert=require('assert');
const source=require('fs').readFileSync('web/app.js','utf8');
const marks=[];
const ctx=new Proxy({
  fillRect(...args){marks.push({color:this.fillStyle,args});},
  arc(...args){marks.push({color:this.strokeStyle,args});},
}, {get:(target,key)=>key in target?target[key]:()=>{}});
const {gear,draw}=new Function('ctx','polygon','point','tileW','tileH','npcColors',
  source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+
  '\nreturn {gear:villagerAccessories,draw:drawVillager};'
)(ctx,(points,color)=>marks.push({color,points}),()=>[0,0],46,23,{f:['#416951','#70906a']});
function sprite(skills={},job='idle',kind='villager',phase='idle'){
  return {kind,index:0,x:0,y:0,biome:'f',active:true,villager:{job,...skills},
    state:{path:[],phase,clock:0,facing:1}};
}
const none={axe:false,blouse:false,sickle:false,hat:false,pouch:false,scarf:false};
assert.deepStrictEqual(gear(sprite()),none,'Missing skills have no earned gear');
for(const [skill,tool,clothing] of [['woodcutting','axe','blouse'],['farming','sickle','hat'],['fishing','pouch','scarf']]){
  assert.deepStrictEqual(gear(sprite({[skill]:0})),none,'Zero skill has no gear');
  for(const level of [1,4,5,20]){
    const expected={...none,[tool]:true,[clothing]:level>=5};
    assert.deepStrictEqual(gear(sprite({[skill]:level})),expected,`${skill} level ${level}`);
    assert.deepStrictEqual(gear(sprite({[skill]:level},'fishing')),expected,'Changing jobs preserves earned gear');
  }
}
const skilled={woodcutting:5,farming:5,fishing:5};
assert(Object.values(gear(sprite(skilled))).every(Boolean),'Multiple skills combine their accessories');
for(const kind of ['player','npc'])assert.deepStrictEqual(gear(sprite(skilled,'farming',kind)),none,'Other characters keep their own appearance');
function rendered(b){marks.length=0;draw(b);return marks.slice();}
const has=(output,color)=>output.some(mark=>mark.color===color);
assert(!has(rendered(sprite({farming:0},'farming')),'#ccb779'),'Assigned beginners do not get a skilled farmer hat');
assert(!has(rendered(sprite({farming:4})),'#ccb779'),'Basic farming gear excludes the hat');
assert(has(rendered(sprite({farming:5})),'#ccb779'),'Skilled farmer wears the hat while resting');
assert(rendered(sprite({farming:1})).some(mark=>mark.color==='#dce5df'&&mark.args[2]===4),'Basic farmer carries a curved sickle');
assert(!has(rendered(sprite({woodcutting:4})),'#b8c6ad'),'Basic woodcutting gear excludes the blouse');
assert(has(rendered(sprite({woodcutting:5})),'#b8c6ad'),'Skilled woodcutter wears the blouse');
assert(has(rendered(sprite({woodcutting:1})),'#e4ece7'),'Basic woodcutter carries an axe while resting');
assert(has(rendered(sprite({fishing:1})),'#806448'),'Basic fisher carries a tackle pouch');
assert(has(rendered(sprite({fishing:5})),'#6baeb5'),'Skilled fisher wears a scarf');
for(const phase of ['idle','walking','working','returning','depositing']){
  const b=sprite(skilled,'woodcutting','villager',phase);
  if(phase==='walking')b.state.path=[{x:1,y:0}];
  const output=rendered(b);
  for(const color of ['#b8c6ad','#ccb779','#806448','#6baeb5'])assert(has(output,color),`Gear remains visible while ${phase}`);
}
assert(has(rendered(sprite(skilled,'idle','player')),'#d6b85d'),'Player keeps gold clothing');
assert(has(rendered(sprite(skilled,'idle','npc')),'#416951'),'NPC keeps biome clothing');
console.log('Villager accessory checks passed');
