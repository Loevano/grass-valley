const assert=require('assert'),fs=require('fs');
const {execFileSync}=require('child_process');
const source=fs.readFileSync('web/app.js','utf8');
const progression=JSON.parse(execFileSync('python3',['-c','import json, progression; print(json.dumps(progression.public_catalog()))'],{cwd:require('path').join(__dirname,'../server'),encoding:'utf8'}));
const document={createElement(tag){return {tag,children:[],append(...items){this.children.push(...items);},setAttribute(){}};}};
const commands=[];
const create=new Function('world','document','viewed','identity','goodName','jobRequirement','act',
  source.slice(source.indexOf('function animalAges('),source.indexOf('function renderProfession('))+
  'return {animalAges,renderAnimalButchery};');
const town={buildings:[{kind:'barn',x:3,y:6,animals:{pig:2,chicken:1},animal_growth:{pig:[0,300],chicken:[10]}},{kind:'butcher'}],
  villagers:[{job:'butchery',building_index:1,husbandry:2}]};
function panel(owner=true){
  const ui=create({progression},document,owner?'own':'visitor',{town_id:'own'},key=>key,
    (v)=>v.husbandry<2?'Requires husbandry':null,data=>commands.push(data));
  const target=document.createElement('div');ui.renderAnimalButchery(town,town.buildings[1],1,target);
  const nodes=node=>[node,...node.children.flatMap(nodes)],all=nodes(target);
  return {ui,note:all.find(node=>node.className==='livestock-note'),chooser:all.find(node=>node.tag==='select'),button:all.find(node=>node.tag==='button'),
    preview:all.find(node=>node.className==='livestock-yield'),status:all.find(node=>node.className==='butchery-status'),progress:all.find(node=>node.tag==='progress')};
}
let p=panel();
assert.equal(p.chooser.children.length,1,'Only mature animals appear');
assert.equal(p.button.textContent,'Butcher & replace');
assert(p.preview.textContent.includes('8 meat + 1 free young pig'),'Preview includes the free replacement');
assert.equal(p.chooser.children[0].textContent,'pig · 1 ready');
assert(p.note.textContent.includes('free young animal')&&p.note.textContent.includes('immediately'),'Explain automatic replacement');
p.chooser.value=p.chooser.children[0].value;p.button.onclick();
assert.deepStrictEqual(commands.at(-1),{action:'butcher_animal',index:1,barn_index:0,animal:'pig'});
town.buildings.push({kind:'barn',x:9,y:4,animals:{pig:1,cow:1}});
p=panel();
assert.equal(p.chooser.children[0].textContent,'pig · 1 ready · Barn (3, 6)');
const otherPig=p.chooser.children.find(option=>option.value==='2:pig');
assert.equal(otherPig.textContent,'pig · 1 ready · Barn (9, 4)');
assert.equal(p.chooser.children.find(option=>option.value==='2:cow').textContent,'cow · 1 ready');
p.chooser.value='2:cow';p.chooser.onchange();assert(p.preview.textContent.includes('10 meat + 1 free young cow'));
p.chooser.value=otherPig.value;p.button.onclick();
assert.deepStrictEqual(commands.at(-1),{action:'butcher_animal',index:1,barn_index:2,animal:'pig'});
town.buildings.pop();
town.buildings[1].butchering={animal:'pig',amount:8,remaining:42};
p=panel();assert(p.status.textContent.includes('42s'));assert.equal(p.progress.value,18);assert(p.button.disabled);assert(p.chooser.disabled);
const previous=commands.length;p.button.onclick();assert.equal(commands.length,previous);
town.buildings[1].butchering.remaining=0;assert(panel().status.textContent.includes('Waiting for buffer space'));
delete town.buildings[1].butchering;
town.villagers[0].husbandry=0;assert(panel().button.disabled);
town.villagers[0].husbandry=2;town.buildings[0].animal_growth.pig=[0,119];assert(panel().button.disabled);
assert(!panel(false).button,'Visitors cannot select animals');
assert.deepStrictEqual(p.ui.animalAges({animals:{cow:2}},'cow'),[180,180],'Old saves keep adults');
console.log('Animal butchery controls passed');
