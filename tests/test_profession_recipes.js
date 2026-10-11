const assert=require('assert');
const fs=require('fs');
const {execFileSync}=require('child_process');
const source=fs.readFileSync('web/app.js','utf8');
const progression=JSON.parse(execFileSync('python3',['-c','import json, progression; print(json.dumps(progression.public_catalog()))'],{encoding:'utf8'}));
const world={progression};
const document={createElement(tag){return {tag,children:[],attributes:{},append(...items){this.children.push(...items);},setAttribute(key,value){this.attributes[key]=value;}};}};
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
  return {chooser:target.children[0],description:ui.productionDescription(item)};
}
for(const [kind,recipes] of Object.entries(progression.profession_recipes)){
  const job=progression.buildings[kind].job,index=3,item={kind};
  const worker={job,building_index:index,...progression.job_requirements[job]};
  const town={villagers:[worker,{job,building_index:4,[job]:100}],player:{[job]:100}};
  let {chooser,description}=panel(town,item,index);
  assert.strictEqual(chooser.value,recipes[0].output);
  assert.strictEqual(chooser.children.length,5);
  assert(!chooser.children[0].disabled);
  assert(chooser.children.slice(1).every(option=>option.disabled));
  assert(description.includes(recipes[0].output.replaceAll('_',' ')));
  for(let level=2;level<=5;level++){
    worker[job]=level-.001;
    assert(panel(town,item,index).chooser.children[level-1].disabled);
    worker[job]=level;
    chooser=panel(town,item,index).chooser;
    assert(!chooser.children[level-1].disabled);
    chooser.value=recipes[level-1].output;chooser.onchange();
    assert.deepStrictEqual(commands.at(-1),{action:'set_recipe',index,recipe:recipes[level-1].output});
    item.recipe=recipes[level-1].output;
    description=panel(town,item,index).description;
    assert(description.includes('Recipe level '+level));
    assert(description.includes(recipes[level-1].output.replaceAll('_',' ')));
  }
  const before=commands.length;
  chooser=panel(town,item,index,false).chooser;
  assert(chooser.disabled);chooser.onchange();assert.strictEqual(commands.length,before);
  worker[job]=0;
  chooser=panel(town,item,index).chooser;
  assert.strictEqual(chooser.value,recipes[4].output);
  assert(chooser.children[4].disabled);
}
console.log('Five-level production recipe selector checks passed');
