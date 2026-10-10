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
const none={axe:false,blouse:false,woodcuttingMaster:false,sickle:false,hat:false,farmingMaster:false,
  pouch:false,scarf:false,fishingMaster:false};
assert.deepStrictEqual(gear(sprite()),none,'Missing skills have no earned gear');
for(const [skill,tool,clothing] of [['woodcutting','axe','blouse'],['farming','sickle','hat'],['fishing','pouch','scarf']]){
  assert.deepStrictEqual(gear(sprite({[skill]:0})),none,'Zero skill has no gear');
  for(const level of [0,1,4,5,9,10,19,20,21]){
    const expected={...none,[tool]:level>=5,[clothing]:level>=10,[`${skill}Master`]:level>=20};
    assert.deepStrictEqual(gear(sprite({[skill]:level})),expected,`${skill} level ${level}`);
    assert.deepStrictEqual(gear(sprite({[skill]:level},'fishing')),expected,'Changing jobs preserves earned gear');
  }
}
const skilled={woodcutting:20,farming:20,fishing:20};
assert(Object.values(gear(sprite(skilled))).every(Boolean),'Multiple skills combine their accessories');
for(const kind of ['player','npc'])assert.deepStrictEqual(gear(sprite(skilled,'farming',kind)),none,'Other characters keep their own appearance');
function rendered(b){marks.length=0;draw(b);return marks.slice();}
const has=(output,color)=>output.some(mark=>mark.color===color);
assert(!has(rendered(sprite({farming:0},'farming')),'#ccb779'),'Assigned beginners do not get a skilled farmer hat');
assert(!has(rendered(sprite({farming:9})),'#ccb779'),'Basic farming gear excludes the hat');
assert(has(rendered(sprite({farming:10})),'#ccb779'),'Skilled farmer wears the hat while resting');
assert(rendered(sprite({farming:5})).some(mark=>mark.color==='#dce5df'&&mark.args[2]===4),'Basic farmer carries a curved sickle');
assert(!has(rendered(sprite({woodcutting:9})),'#b8c6ad'),'Basic woodcutting gear excludes the blouse');
assert(has(rendered(sprite({woodcutting:10})),'#b8c6ad'),'Skilled woodcutter wears the blouse');
assert(has(rendered(sprite({woodcutting:5})),'#e4ece7'),'Basic woodcutter carries an axe while resting');
assert(has(rendered(sprite({fishing:5})),'#806448'),'Basic fisher carries a tackle pouch');
assert(has(rendered(sprite({fishing:10})),'#6baeb5'),'Skilled fisher wears a scarf');
for(const [skill,color] of [['woodcutting','#e4ece7'],['farming','#dce5df'],['fishing','#806448']]){
  assert(!has(rendered(sprite({[skill]:4})),color),'No earned gear below level 5');
  assert(!has(rendered(sprite({[skill]:19})),'#e8bd55'),'No gold accents below level 20');
  assert(has(rendered(sprite({[skill]:20})),'#e8bd55'),`${skill} earns gold accents at level 20`);
}
for(const phase of ['idle','walking','working','returning','depositing']){
  const b=sprite(skilled,'woodcutting','villager',phase);
  if(phase==='walking')b.state.path=[{x:1,y:0}];
  const output=rendered(b);
  for(const color of ['#b8c6ad','#ccb779','#806448','#6baeb5','#e8bd55'])assert(has(output,color),`Gear remains visible while ${phase}`);
}
assert(has(rendered(sprite(skilled,'idle','player')),'#d6b85d'),'Player keeps gold clothing');
assert(has(rendered(sprite(skilled,'idle','npc')),'#416951'),'NPC keeps biome clothing');
console.log('Villager accessory checks passed');
