const assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const client=require('./client_harness');
const progression=JSON.parse(execFileSync('python3',['-c','import json, progression; print(json.dumps(progression.public_catalog()))'],{cwd:'server',encoding:'utf8'}));
const app=client();
app.run(`world={progression:${JSON.stringify(progression)}};identity={town_id:'own'};viewed='own';
  const commands=[];act=data=>commands.push(data);
  const workshop={kind:'workshop',craft_queue:[]};
  const town={villagers:[{job:'crafting',woodworking:2,crafting:5,building_index:1}]};
  const target=document.createElement('div');renderRecipes(town,workshop,1,target);`);
assert.equal(app.run('labels.workshop'),'Workshop');
app.run(`target.children[0].value='furniture';target.children[0].onchange();`);
assert.equal(app.run('commands.length'),0,'Previewing never submits a request');
app.run(`target.children[2].value='5';target.children[3].onclick();`);
assert.equal(app.run('JSON.stringify(commands[0])'),JSON.stringify({action:'queue_workshop',index:1,recipe:'furniture',amount:5}));
app.run(`target.children[0].value='lanterns';target.children[0].onchange();target.children[2].value='10';target.children[3].onclick();`);
assert.equal(app.run('JSON.stringify(commands[1])'),JSON.stringify({action:'queue_workshop',index:1,recipe:'lanterns',amount:10}));
app.run(`target.children[2].oninput();target.replaceChildren();renderRecipes(town,workshop,1,target);`);
assert.equal(app.run('target.children[0].value'),'lanterns','Recipe preview survives host refresh');
assert.equal(app.run('target.children[2].value'),'10','Requested quantity survives host refresh');
for(const invalid of ['0','-1','1.5','1000','']){
  app.run(`target.children[2].value=${JSON.stringify(invalid)};target.children[2].oninput();target.children[3].onclick();`);
  assert(app.run('target.children[3].disabled'));
}
assert.equal(app.run('commands.length'),2);
app.run(`workshop.craft_queue=[{id:'bench',recipe:'furniture',amount:5,remaining:3},{id:'light',recipe:'lanterns',amount:10,remaining:10}];
  target.replaceChildren();renderRecipes(town,workshop,1,target);`);
assert.match(app.run('target.children[6].children[0].textContent'),/1\. Wooden benches · 3 \/ 5 remaining/);
assert.match(app.run('target.children[7].children[0].textContent'),/2\. Lanterns · 10 \/ 10 remaining/);
app.run('target.children[7].children[1].onclick()');
assert.equal(app.run('JSON.stringify(commands.at(-1))'),JSON.stringify({action:'cancel_workshop',index:1,request_id:'light'}));
const snapshot=app.run('JSON.stringify(workshop)');
app.run(`viewed='friend';target.replaceChildren();renderRecipes(town,workshop,1,target);
  target.children[3].onclick();target.children[6].children[1].onclick();`);
assert(app.run('target.children[2].disabled&&target.children[3].disabled&&target.children[6].children[1].disabled'));
assert.equal(app.run('commands.length'),3);
assert.equal(app.run('JSON.stringify(workshop)'),snapshot,'Rendering and commands leave host snapshots untouched');
assert(app.run("decorationKinds.includes('lantern')"));
console.log('Workshop request quantities, FIFO display, cancellation and ownership controls passed');

// All decoration recipes explain the route from crafting to placement.
app.run(`viewed='own';workshop.craft_queue=[];`);
for(const kind of progression.decorations){
  const recipe=progression.profession_recipes.workshop.find(recipe=>progression.buildings[kind][recipe.output]);
  app.run(`target.replaceChildren();renderRecipes(town,workshop,1,target);
    target.children[0].value=${JSON.stringify(recipe.output)};target.children[0].onchange();
    target.children[2].value='1';target.children[3].onclick();`);
  assert.deepEqual(JSON.parse(app.run('JSON.stringify(commands.at(-1))')),
    {action:'queue_workshop',index:1,recipe:recipe.output,amount:1});
  assert(app.run('target.children[0].children.some(option=>option.value==='+JSON.stringify(recipe.output)+'&&option.textContent.includes("Decoration"))'));
  const rows=JSON.parse(app.run('JSON.stringify(target.children[1].children[0].children.map(child=>child.textContent))'));
  assert(rows.includes(`${progression.buildings[kind].label} · Build → Decorations after delivery`));
  assert.deepEqual(JSON.parse(app.run(`JSON.stringify(materialCosts(world.progression.buildings[${JSON.stringify(kind)}]))`)),
    {[recipe.output]:1},`${kind} consumes its crafted item in the build menu`);
  assert.equal(app.run(`settlerCapacity({kind:${JSON.stringify(kind)}})`),0);
  if(['flower_box','wooden_table','signpost','garden_arch'].includes(kind))assert.equal(app.run(`iconPaths[${JSON.stringify(kind)}]===iconPaths[${JSON.stringify(recipe.output)}]`),true,
    `${kind} and its crafted item have matching icons`);
}
console.log('Every decoration has a workshop request, placement instructions, crafted-item cost, and zero worker spaces');

const palette=client();
palette.run(`
  const town={name:'Decoration palette',level:4,wood:100,food:40,coins:20,goods:{},harvested:0,
    buildings:[{kind:'hall',x:5,y:5}],map_resources:[],villagers:[],roads:[],player:{x:3,y:3,facing:1}};
  world={world_id:'decoration-palette',progression:${JSON.stringify(progression)},towns:{own:town}};
  identity={town_id:'own'};viewed='own';panel='build';render();
  const buttons=[...$('buildings').children].flatMap(group=>group.children).filter(button=>decorationKinds.includes(button.buildingKind));
`);
assert.equal(palette.run('buttons.length'),progression.decorations.length);
for(const kind of progression.decorations){
  const recipe=progression.profession_recipes.workshop.find(recipe=>progression.buildings[kind][recipe.output]);
  assert(palette.run(`buttons.find(button=>button.buildingKind===${JSON.stringify(kind)}).disabled`),'Crafted stock is required');
  assert.match(palette.run(`buttons.find(button=>button.buildingKind===${JSON.stringify(kind)}).title`),/Workshop.*delivery/);
  palette.run(`town.goods[${JSON.stringify(recipe.output)}]=1;render();`);
  assert.equal(palette.run(`buttons.find(button=>button.buildingKind===${JSON.stringify(kind)}).disabled`),false,'Delivered stock enables placement');
}
console.log('Decoration palette requires crafted stock and explains the workshop build path');
