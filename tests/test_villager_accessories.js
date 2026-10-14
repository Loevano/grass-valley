// Run with: node tests/test_villager_accessories.js
const assert=require('assert');
const source=require('fs').readFileSync('web/app.js','utf8');
const marks=[];
const ctx=new Proxy({
  fillRect(...args){marks.push({color:this.fillStyle,args});},
  arc(...args){marks.push({color:this.strokeStyle,args});},
  ellipse(...args){marks.push({color:this.strokeStyle,args});},
}, {get:(target,key)=>key in target?target[key]:()=>{}});
const {gear,draw}=new Function('ctx','polygon','point','tileW','tileH','npcColors',
  source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function draw(seconds=0)'))+
  '\nreturn {gear:villagerAccessories,draw:drawVillager};'
)(ctx,(points,color)=>marks.push({color,points}),()=>[0,0],46,23,{f:['#416951','#70906a']});
function sprite(skills={},job='idle',kind='villager',phase='idle'){
  return {kind,index:0,x:0,y:0,biome:'f',active:true,villager:{job,...skills},
    state:{path:[],phase,clock:0,facing:1}};
}
const flags=['axe','blouse','woodcuttingMaster','sickle','hat','farmingMaster','pouch','scarf','fishingMaster'];
const none=Object.fromEntries(flags.map(key=>[key,false]));
function earned(b){const result=gear(b);return Object.fromEntries(flags.map(key=>[key,result[key]]));}
assert.deepStrictEqual(earned(sprite()),none);
const skilled={woodcutting:20,farming:20,fishing:20,baking:20};
for(const [skill,tool,clothing] of [['woodcutting','axe','blouse'],['farming','sickle','hat'],['fishing','pouch','scarf']]){
  for(const level of [0,1,4,5,9,10,19,20,21]){
    const expected={...none,[tool]:level>=5,[clothing]:level>=10,[`${skill}Master`]:level>=20};
    assert.deepStrictEqual(earned(sprite({...skilled,[skill]:level},skill)),expected,`${skill} level ${level} uses only current practice`);
    assert.deepStrictEqual(earned(sprite({[skill]:level},'baking')),none,'Bakers never carry previous profession gear');
  }
}
for(const kind of ['player','npc']){
  const result=gear(sprite(skilled,'farming',kind));
  assert.deepStrictEqual(earned(sprite(skilled,'farming',kind)),none);
  assert.strictEqual(result.outfit,null,'Other characters keep their own appearance');
}
function rendered(b){marks.length=0;ctx.strokeStyle=undefined;ctx.fillStyle=undefined;draw(b);return marks.slice();}
const has=(output,color)=>output.some(mark=>mark.color===color);
assert(!has(rendered(sprite({farming:0},'farming')),'#ccb779'));
assert(!has(rendered(sprite({farming:9},'farming')),'#ccb779'));
assert(has(rendered(sprite({farming:10},'farming')),'#ccb779'));
assert(rendered(sprite({farming:5},'farming')).some(mark=>mark.color==='#dce5df'&&mark.args[2]===4),'Farmer carries a curved sickle');
for(const skills of [{farming:5},{worker_tool:'fine_sickle'},{worker_tool:'steel_sickle'},{worker_tool:'masterwork_sickle'}]){
  for(const [stage,farmAction] of ['plowing','sowing','watering','harvesting'].entries()){
    for(const confirmed of [false,true]){
      const farmer=sprite(skills,'farming','villager','working');
      Object.assign(farmer.state,confirmed?{fieldCycle:true,farmAction,elapsed:.5}:{elapsed:stage*2+.5});
      const output=rendered(farmer);
      assert.strictEqual(has(output,'#dce5df'),stage===3,`${farmAction} shows a sickle only for harvesting`);
      if(stage===3){
        assert.strictEqual(output.filter(mark=>mark.color==='#dce5df'&&mark.args[2]===5).length,1,'Harvesting holds one sickle');
        assert(!output.some(mark=>mark.color==='#dce5df'&&mark.args[2]===4),'No spare sickle hangs at the waist');
        assert(!output.some(mark=>mark.color==='#e5c77f'),'No floating stand of grain is added beside the tool');
      }
      if(stage===0)assert(has(output,'#aab9b4'),'Plowing retains the hoe');
      if(stage===1)assert(has(output,'#efd59a'),'Sowing retains the seeds');
    }
  }
}
assert(!has(rendered(sprite({woodcutting:9},'woodcutting')),'#b8c6ad'));
assert(has(rendered(sprite({woodcutting:10},'woodcutting')),'#b8c6ad'));
assert(has(rendered(sprite({woodcutting:5},'woodcutting')),'#e4ece7'));
assert(has(rendered(sprite({fishing:5},'fishing')),'#806448'));
assert(has(rendered(sprite({fishing:10},'fishing')),'#6baeb5'));
for(const [skill,color] of [['woodcutting','#e4ece7'],['farming','#dce5df'],['fishing','#806448']]){
  assert(!has(rendered(sprite({[skill]:4},skill)),color));
  assert(!has(rendered(sprite({[skill]:19},skill)),'#e8bd55'));
  assert(has(rendered(sprite({[skill]:20},skill)),'#e8bd55'));
}
const jobs=['building','hauling','husbandry','woodworking','milling','preserving','crafting','breeding','orcharding','baking','butchery'];
for(const job of jobs){
  const beginner=rendered(sprite(skilled,job)),basic=rendered(sprite({...skilled,[job]:5},job));
  const experienced=rendered(sprite({...skilled,[job]:10},job)),master=rendered(sprite({...skilled,[job]:20},job));
  assert(gear(sprite({},job)).outfit,`${job} has its own outfit`);
  assert.notDeepStrictEqual(basic,beginner,`${job} earns its tool at level 5`);
  assert.notDeepStrictEqual(experienced,basic,`${job} upgrades clothing at level 10`);
  assert(!has(experienced,'#e8bd55'));
  assert(has(master,'#e8bd55'),`${job} earns gold accents at level 20`);
}
for(const phase of ['idle','walking','working','returning','depositing']){
  const b=sprite(skilled,'baking','villager',phase);
  if(phase==='walking')b.state.path=[{x:1,y:0}];
  const output=rendered(b);
  for(const color of ['#f2e6cd','#fff1d8','#e8bd55'])assert(has(output,color),`Baker outfit remains visible while ${phase}`);
  for(const color of ['#ccb779','#dce5df','#b8c6ad','#6baeb5'])assert(!has(output,color),`Baker never wears another profession's gear while ${phase}`);
}
assert.strictEqual(gear(sprite(skilled,'idle')).outfit,null,'Unassigned villagers wear ordinary clothing');
assert.deepStrictEqual(earned(sprite(skilled,'idle')),none,'Unassigned villagers put away profession gear');
assert(!gear(sprite({worker_tool:'masterwork_sickle'},'baking')).sickle,'Equipped tool cannot override profession');
assert(gear(sprite({worker_tool:'fine_sickle'},'farming')).sickle,'Matching equipped tool is visible even for beginners');
assert(has(rendered(sprite(skilled,'idle','player')),'#d6b85d'));
assert(has(rendered(sprite(skilled,'idle','npc')),'#416951'));
console.log('Villager accessory checks passed');
