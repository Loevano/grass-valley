const assert=require('assert');
const fs=require('fs');
const {execFileSync}=require('child_process');
const source=fs.readFileSync('web/app.js','utf8');
const progression=JSON.parse(execFileSync('python3',['-c','import json, progression; print(json.dumps(progression.public_catalog()))'],{cwd:require('path').join(__dirname,'../server'),encoding:'utf8'}));
const world={progression};
const document={createElement(tag){return {tag,children:[],attributes:{},append(...items){this.children.push(...items);},replaceChildren(...items){this.children=items;},setAttribute(key,value){this.attributes[key]=value;}};}};
const commands=[];
const context=new Function('world','document','identity','viewed','act',
  source.slice(source.indexOf('const goodName='),source.indexOf('function renderResourceList('))+
  source.slice(source.indexOf('function costText('),source.indexOf('function itemCard('))+
  source.slice(source.indexOf('function jobRequirement('),source.indexOf('function workerEfficiency('))+
  source.slice(source.indexOf('function selectedProfessionRecipe('),source.indexOf('function renderProfession('))+
  'return {renderRecipes,productionDescription};');
function panel(town,item,index,owner=true){
  const target=document.createElement('div');
  const ui=context(world,document,{town_id:'own'},owner?'own':'visitor',data=>commands.push(data));
  ui.renderRecipes(town,item,index,target);
  return {chooser:target.children[0],note:target.children[1],description:ui.productionDescription(item)};
}
function recipeFacts(note){
  const rows=note.children[0].children;
  return Object.fromEntries(rows.flatMap((row,index)=>index%2?[]:[[row.textContent,rows[index+1].textContent]]));
}
for(const [kind,recipes] of Object.entries(progression.profession_recipes)){
  const job=progression.buildings[kind].job,index=3,item={kind};
  const worker={job,building_index:index,...progression.job_requirements[job]};
  const town={villagers:[worker,{job,building_index:4,[job]:100}],player:{[job]:100}};
  let {chooser,description}=panel(town,item,index);
  assert.strictEqual(chooser.value,recipes[0].output);
  assert.strictEqual(chooser.children.length,recipes.length,'Every catalog recipe is offered, including crafted worker tools');
  assert(!chooser.children[0].disabled);
  assert(chooser.children.slice(1).every(option=>option.disabled));
  assert.equal(chooser.children[0].textContent.toLowerCase(),recipes[0].output.replaceAll('_',' ')+' · lv 1');
  assert.equal(chooser.children[1].textContent.toLowerCase(),recipes[1].output.replaceAll('_',' ')+` · lv ${recipes[1].level} · locked`);
  assert.strictEqual(recipeFacts(panel(town,item,index).note).Produces,`${recipes[0].amount} ${recipes[0].output.replaceAll('_',' ')}`);
  assert(description.includes(recipes[0].output.replaceAll('_',' ')));
  for(const [recipeIndex,recipe] of recipes.entries()){
    if(recipeIndex===0)continue;
    const level=recipe.level;
    worker[job]=level-.001;
    assert(panel(town,item,index).chooser.children[recipeIndex].disabled);
    worker[job]=level;
    chooser=panel(town,item,index).chooser;
    assert(!chooser.children[recipeIndex].disabled);
    chooser.value=recipe.output;chooser.onchange();
    assert.deepStrictEqual(commands.at(-1),{action:'set_recipe',index,recipe:recipe.output});
    item.recipe=recipe.output;
    description=panel(town,item,index).description;
    const note=recipeFacts(panel(town,item,index).note);
    const seconds=progression.production_seconds[recipe.output];
    assert.strictEqual(note['Batch time'],`${seconds}s`);
    assert.strictEqual(note.Practice,`+${((kind==='butcher'?0.1:0.08)*seconds/3).toFixed(2)} per batch`);
    assert(description.includes(`/ ${seconds}s batch`));
    assert(description.includes(`${((kind==='butcher'?0.1:0.08)*seconds/3).toFixed(2)} practice / batch`));
    assert.strictEqual(note.Produces,`${recipe.amount} ${recipe.output.replaceAll('_',' ')}`);
    assert.strictEqual(note.Requires,'Worker level '+level+'+');
    assert(description.includes('Recipe level '+level));
    assert(description.includes(recipe.output.replaceAll('_',' ')));
  }
  const before=commands.length;
  chooser=panel(town,item,index,false).chooser;
  assert(chooser.disabled);chooser.onchange();assert.strictEqual(commands.length,before);
  worker[job]=0;
  chooser=panel(town,item,index).chooser;
  assert.strictEqual(chooser.value,recipes.at(-1).output);
  assert(chooser.children.at(-1).disabled);
}
console.log('Production recipe and crafted-tool selector checks passed');
