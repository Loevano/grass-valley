const assert=require('assert'),fs=require('fs');
const {execFileSync}=require('child_process');
const source=fs.readFileSync('web/app.js','utf8');
const progression=JSON.parse(execFileSync('python3',['-c','import json, progression; print(json.dumps(progression.public_catalog()))'],{encoding:'utf8'}));
const document={createElement(tag){return {tag,children:[],append(...items){this.children.push(...items);},setAttribute(){}};}};
const commands=[];
const create=new Function('world','document','viewed','identity','goodName','jobRequirement','act',
  source.slice(source.indexOf('function animalAges('),source.indexOf('function renderProfession('))+
  'return {animalAges,renderAnimalButchery};');
const town={buildings:[{kind:'barn',x:3,y:6,animals:{pig:2,chicken:1},animal_growth:{pig:[0,300],chicken:[100]}},{kind:'butcher'}],
  villagers:[{job:'butchery',building_index:1,husbandry:2}]};
function panel(owner=true){
  const ui=create({progression},document,owner?'own':'visitor',{town_id:'own'},key=>key,
    (v)=>v.husbandry<2?'Requires husbandry':null,data=>commands.push(data));
  const target=document.createElement('div');ui.renderAnimalButchery(town,town.buildings[1],1,target);
  return {ui,note:target.children[0],chooser:target.children[1],button:target.children[2]};
}
let p=panel();
assert.equal(p.chooser.children.length,1,'Only mature animals appear');
assert(p.chooser.children[0].textContent.includes('1 adult → 8 meat'));
p.chooser.value=p.chooser.children[0].value;p.button.onclick();
assert.deepStrictEqual(commands.at(-1),{action:'butcher_animal',index:1,barn_index:0,animal:'pig'});
town.buildings[1].butchering={animal:'pig',amount:8,remaining:42};
p=panel();assert(p.note.textContent.includes('42s'));assert(p.button.disabled);assert(p.chooser.disabled);
const previous=commands.length;p.button.onclick();assert.equal(commands.length,previous);
town.buildings[1].butchering.remaining=0;assert(panel().note.textContent.includes('Waiting for buffer space'));
delete town.buildings[1].butchering;
town.villagers[0].husbandry=0;assert(panel().button.disabled);
town.villagers[0].husbandry=2;town.buildings[0].animal_growth.pig=[0,299];assert(panel().button.disabled);
assert(!panel(false).button,'Visitors cannot select animals');
assert.deepStrictEqual(p.ui.animalAges({animals:{cow:2}},'cow'),[300,300],'Old saves keep adults');
console.log('Animal butchery controls passed');
