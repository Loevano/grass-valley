const assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const client=require('./client_harness');
const catalog=JSON.parse(execFileSync('python3',['-c','import json, progression; print(json.dumps(progression.public_catalog()))'],{cwd:require('node:path').join(__dirname,'../server'),encoding:'utf8'}));
const app=client();
app.run(`
  world={progression:${JSON.stringify(catalog)}};
  const town={level:1,wood:20,food:10,goods:{},villagers:[],buildings:[
    {kind:'hall'},{kind:'storehouse',storage_enabled:{wood:true,steel:true}}
  ]};
  identity={town_id:'own'};viewed='own';
  const commands=[];act=command=>commands.push(command);
  const storagePanel=document.createElement('div');
  function nodes(node=storagePanel){return node.children.flatMap(child=>[child,...nodes(child)]);}
  function storage(index=1){
    storagePanel.replaceChildren();storagePanel.append(document.createElement('h3'));
    renderStorage(town,town.buildings[index],index,storagePanel);
    return nodes().filter(node=>node.className==='storage-row').map(row=>row.children[0].children[0].textContent);
  }
  function toggle(label){return nodes().find(node=>node.className==='storage-caption'&&node.children[0].textContent===label).children.find(node=>node.type==='checkbox');}
`);
const rows=()=>Array.from(app.run('storage()'));
assert.deepEqual(new Set(rows()),new Set(['Wood','Provisions','Vegetables','Fish','Berries','Herbs']),'A starting storehouse omits locked goods and empty categories');
assert(!app.run('nodes().some(node=>node.className==="resource-group"&&node.children[0].textContent==="Metalworking")'));
assert.equal(app.run('roomAllocations(town.buildings[1]).steel'),50,'Hidden resources retain authoritative capacity');
app.run(`toggle('Wood').checked=false;toggle('Wood').onchange();`);
assert.equal(app.run('commands.at(-1).enabled.steel'),true,'An ordinary switch preserves hidden settings');
app.run(`toggle('Wood').onclick({altKey:true});toggle('Wood').onchange();`);
assert.equal(app.run('commands.at(-1).enabled.steel'),true);
assert.equal(app.run('commands.at(-1).enabled.planks'),false,'Toggle-all only enables shown resources');
app.run(`toggle('Wood').onclick({altKey:true});toggle('Wood').onchange();`);
assert.equal(app.run('commands.at(-1).enabled.steel'),true,'Toggle-all cannot disable hidden acceptance settings');
assert.equal(app.run('commands.at(-1).enabled.wood'),false);
app.run('town.level=2;');
assert(rows().includes('Wheat')&&rows().includes('Eggs')&&rows().includes('Manure'));
assert(!rows().includes('Wool')&&!rows().includes('Planks'));
app.run('town.level=3;');
assert(rows().includes('Wool')&&rows().includes('Planks')&&rows().includes('Carrots')&&rows().includes('Fodder'));
assert(!rows().includes('Trout'),'Hall progression alone does not provide a fishing boat');
app.run(`town.buildings.push({kind:'fishing_hut',profession_upgrades:['boat']});`);
assert(rows().includes('Trout')&&!rows().includes('Salmon')&&!rows().includes('Smoked trout'));
app.run('town.level=4;');
assert(rows().includes('Flour')&&rows().includes('Bread')&&rows().includes('Tools')&&rows().includes('Meat')&&rows().includes('Hides')&&rows().includes('Smoked trout'));
assert(!rows().includes('Bread rolls')&&!rows().includes('Wooden bowls')&&!rows().includes('Milk'));
app.run(`town.buildings[2].profession_upgrades.push('improved_boat');`);
assert(rows().includes('Salmon')&&rows().includes('Smoked salmon')&&!rows().includes('Sturgeon'));
app.run(`town.level=5;town.buildings[2].profession_upgrades.push('master_boat');`);
assert(rows().includes('Sturgeon')&&rows().includes('Smoked sturgeon')&&rows().includes('Milk')&&rows().includes('Fruit')&&rows().includes('Leather'));
app.run(`town.buildings.push({kind:'bakery'});town.villagers.push({job:'baking',building_index:3,baking:5,milling:2});`);
assert(rows().includes('Bread rolls')&&!rows().includes('Herb bread'));
app.run('town.villagers[0].baking=10;');
assert(rows().includes('Herb bread'));
app.run('town.villagers[0].milling=0;');
assert(!rows().includes('Bread rolls'),'Unqualified workers cannot unlock advanced recipes');
app.run(`town.villagers[0].milling=2;town.villagers[0].job='idle';`);
assert(!rows().includes('Bread rolls'),'Recipe availability follows qualified assigned workers');
for(const [level,good] of [[6,'Iron ore'],[7,'Iron'],[8,'Saddles'],[9,'Steel'],[10,'Trade crates']]){
  app.run(`town.level=${level};`);assert(rows().includes(good),`${good} appears at its hall unlock`);
}
app.run(`town.level=1;town.buildings[1].stored_goods={steel:2};town.buildings[1].output_buffer={cakes:1};town.buildings[1].input_buffer={flour:1};town.goods={leather:3};`);
assert(rows().includes('Steel')&&rows().includes('Cakes')&&rows().includes('Flour')&&rows().includes('Leather'),'Stored, buffered, reserved, and town stock stay accessible before unlock');
app.run(`town.buildings[1].stored_goods={};town.buildings[1].output_buffer={};town.buildings[1].input_buffer={};town.goods={};`);
assert(!rows().includes('Steel'));
app.run(`town.buildings[2].profession_upgrades=[];viewed='friend';`);
assert.deepEqual(new Set(rows()),new Set(['Wood','Provisions','Vegetables','Fish','Berries','Herbs']),'Visitors use the viewed town unlocks');
assert.equal(app.run('nodes().filter(node=>node.type==="checkbox").length'),0);
app.run(`viewed='own';`);
assert(!Array.from(app.run('storage(0)')).includes('Steel'),'Hall storage uses the same filtering');
assert.deepEqual(Array.from(app.run('storage(2)')),['Fish'],'Fishing hut storage also filters locked catches');
app.run(`town.sandbox=true;`);
assert(rows().includes('Trade crates')&&rows().includes('Cakes')&&rows().includes('Sturgeon'),'Sandbox storage shows all goods');
const snapshot=app.run('JSON.stringify(town)');
rows();app.run('storage(0)');
assert.equal(app.run('JSON.stringify(town)'),snapshot,'Storage rendering never mutates host snapshots');
console.log('Storage unlock visibility and preserved settings passed');
