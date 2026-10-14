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
  function grainPanel(item,index){const target=document.createElement('div');renderGrainFlow(town,item,index,target);return textOf(target);}
`);
assert.equal(app.run('grainProduction(town.buildings[0]).amount'),320);
assert.equal(app.run('grainProduction(town.buildings[1]).feeds'),320);
assert.equal(app.run('villageFoodDemand(town)'),3.6);
assert.equal(app.run('settlerCapacity(town.buildings[0])'),1);
assert.equal(app.run('roomAllocations(town.buildings[1]).bread'),640);
assert.match(app.run('grainPanel(town.buildings[1],1)'),/Feeds 320 working villagers/);
assert.match(app.run('grainPanel(town.buildings[1],1)'),/Feeds 0 working villagers/);
assert.doesNotMatch(app.run('grainPanel(town.buildings[1],1)'),/Village grain flow|Current meal demand|Delivered baked food/);
app.run('town.buildings[1].grain_flow.rates.bread=240;');
assert.match(app.run('grainPanel(town.buildings[1],1)'),/Feeds 240 working villagers/);
const before=app.run('JSON.stringify(town)');
app.run('grainPanel(town.buildings[0],0); grainPanel(town.buildings[1],1);');
assert.equal(app.run('JSON.stringify(town)'),before,'Rendering cannot mutate authoritative snapshots');
app.run(`town.buildings[1].recipe='cakes';`);
assert.equal(app.run('grainProduction(town.buildings[1]).amount'),160);
assert.equal(app.run('grainProduction(town.buildings[1]).feeds'),480);
app.run(`town.buildings.push({kind:'bakery',recipe:'herb_bread',profession_upgrades:['larger_oven']});
  town.villagers.push({job:'baking',building_index:2,milling:2,baking:3,talents:{}});`);
assert.match(app.run('grainPanel(town.buildings[1],1)'),/160 flour \+ 160 eggs \+ 80 milk \+ 40 wood\/min/);
app.run(`town.buildings[1].profession_upgrades=['larger_oven'];town.buildings[1].recipe='bread';`);
const preview=app.run(`upgradePreview({action:'profession_upgrade',upgrade:'double_hearth_oven'},town,town.buildings[1])`);
assert.equal(preview.ready,true);
assert(preview.benefits.some(text=>text.includes('Feeds 80 → 160')));
assert(preview.benefits.some(text=>text.includes('80 flour')&&text.includes('160 flour')));
assert.equal(preview.seconds,30);
app.run('town.level=4;');
assert.equal(app.run(`upgradePreview({action:'profession_upgrade',upgrade:'double_hearth_oven'},town,town.buildings[1]).ready`),false);
app.run(`town.level=6;town.villagers[1].job='idle';`);
assert.match(app.run('grainPanel(town.buildings[1],1)'),/Assign a qualified worker/);
console.log('Grain capacity, variety, deliveries, storage, upgrade previews, and snapshot isolation passed');
