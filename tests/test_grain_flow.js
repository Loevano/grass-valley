const assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const client=require('./client_harness');
const catalog=JSON.parse(execFileSync('python3',['-c','import json, progression; print(json.dumps(progression.public_catalog()))'],{cwd:require('node:path').join(__dirname,'../server'),encoding:'utf8'}));
const app=client();
app.run(`world={progression:${JSON.stringify(catalog)},towns:{}};`);
app.run(`
  const town={level:6,wood:500,goods:{planks:100,tools:50},coins:20,player:{},
    villagers:[{job:'milling',building_index:0,farming:10,milling:5,talents:{}},
               {job:'baking',building_index:1,milling:2,baking:5,talents:{}},
               {job:'idle'}],
    buildings:[{kind:'mill',x:5,y:5,profession_upgrades:['masterwork_millstones']},
               {kind:'bakery',x:7,y:5,profession_upgrades:['masterwork_oven'],grain_flow:{rates:{bread:0}}}]};
  world.towns.own=town;identity={town_id:'own'};viewed='own';
  function textOf(node){return (node.textContent || '')+' '+node.children.map(textOf).join(' ');}
  function productionPanel(item,index){
    const target=document.createElement('div'),upgrades=document.createElement('div'),details=document.createElement('div'),boats=document.createElement('div'),unlocked=document.createElement('div');
    renderProfession(town,item,index,target,upgrades,details,boats,unlocked);return textOf(target);
  }
`);
assert.equal(app.run('grainProduction(town.buildings[0]).amount'),320);
assert.equal(app.run('grainProduction(town.buildings[1]).feeds'),320);
assert.equal(app.run('settlerCapacity(town.buildings[0])'),1);
assert.equal(app.run('roomAllocations(town.buildings[1]).bread'),640);
const summaryLabels=/Village grain flow|Current meal demand|Delivered baked food|Feeds \d|Ingredient demand at capacity|Recent delivered supply|Capacity with assigned worker|Equipment capacity with one neutral worker/;
app.run(`town.buildings.push({kind:'hall',x:5,y:3},{kind:'farm',x:3,y:5});`);
const before=app.run('JSON.stringify(town)');
for(let index=0;index<4;index++)assert.doesNotMatch(app.run(`productionPanel(town.buildings[${index}],${index})`),summaryLabels);
assert.equal(app.run('JSON.stringify(town)'),before,'Rendering cannot mutate authoritative snapshots');
assert.match(app.run('productionPanel(town.buildings[1],1)'),/flour.*wood.*bread/,'Bakery recipe details remain available');
app.run('town.buildings[1].grain_flow.rates.bread=240;');
assert.doesNotMatch(app.run('productionPanel(town.buildings[1],1)'),summaryLabels,'Legacy delivery data does not restore the summaries');
app.run(`town.buildings[1].recipe='cakes';`);
assert.equal(app.run('grainProduction(town.buildings[1]).amount'),160);
assert.equal(app.run('grainProduction(town.buildings[1]).feeds'),480);
app.run(`town.buildings.push({kind:'bakery',recipe:'herb_bread',profession_upgrades:['larger_oven']});
  town.villagers.push({job:'baking',building_index:4,milling:2,baking:3,talents:{}});`);
assert.match(app.run('productionPanel(town.buildings[1],1)'),/flour.*eggs.*milk.*wood.*cakes/);
app.run(`town.buildings[1].profession_upgrades=['larger_oven'];town.buildings[1].recipe='bread';`);
const preview=app.run(`upgradePreview({action:'profession_upgrade',upgrade:'double_hearth_oven'},town,town.buildings[1])`);
assert.equal(preview.ready,true);
assert(preview.benefits.some(text=>text.includes('Feeds 80 → 160')));
assert(preview.benefits.some(text=>text.includes('80 flour')&&text.includes('160 flour')));
assert.equal(preview.seconds,30);
app.run('town.level=4;');
assert.equal(app.run(`upgradePreview({action:'profession_upgrade',upgrade:'double_hearth_oven'},town,town.buildings[1]).ready`),false);
app.run(`town.level=6;town.villagers[1].job='idle';`);
assert.doesNotMatch(app.run('productionPanel(town.buildings[1],1)'),summaryLabels);
console.log('Grain summary removal, recipes, storage, upgrade previews, and snapshot isolation passed');
