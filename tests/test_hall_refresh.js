// Run with: node tests/test_hall_refresh.js
const assert=require('assert'),fs=require('fs'),vm=require('vm');

// Model attachment and reparenting: detached controls cannot be found by ID.
function element(tag='div',id=null){
  return {tag,id,parent:null,children:[],classList:{toggle(){}},
    append(child){
      assert(child,'Cannot append a missing control');
      if(child.parent)child.parent.children.splice(child.parent.children.indexOf(child),1);
      child.parent=this;this.children.push(child);
    },
    replaceChildren(){for(const child of this.children)child.parent=null;this.children=[];},
    setAttribute(){},addEventListener(){}};
}
const root=element();
function find(id,node=root){
  if(node.id===id)return node;
  for(const child of node.children){const match=find(id,child);if(match)return match;}
  return null;
}
function attach(id,parent=root){const node=element('div',id);parent.append(node);return node;}
const panel=attach('selection-panel');
for(const id of ['selection-art','selection-level','selection-title','selection-details'])attach(id,panel);
const hall=attach('hall-details',panel);
for(const id of ['progress','hall-progress','upgrade'])attach(id,hall);
for(const id of ['selection-move','selection-destroy'])attach(id,panel);
const upgradeButton=find('upgrade');
upgradeButton.onclick=()=>{};
const upgradeHandler=upgradeButton.onclick;
const town={level:1,coins:0,wood:100,villagers:[],map_resources:[],
  buildings:[{kind:'hall',x:5,y:5},{kind:'home',x:6,y:5}]};
const context={document:{createElement:element},$:find,
  world:{world_id:'test',towns:{own:town},hall_ready:{own:true},
    hall_upgrades:[{level:2,milestone:'Housing',coins:30,wood:40,seconds:30}]},
  viewed:'own',identity:{town_id:'own'},selection:{type:'building',index:0},
  costs:{},buildingName:b=>b.kind,buildingLevel:()=>1,buildingRange:()=>10,
  illustration:()=>element(),fact:()=>element(),costText:()=> '40 wood',canAfford:()=>true,
  renderSettlers(){},renderProfession(){},renderStorage(){}};
const source=fs.readFileSync('web/app.js','utf8');
vm.createContext(context);
vm.runInContext(source.slice(source.indexOf('function upgradePreview('),source.indexOf('function renderUpgradeDialog(')),context);
const start=source.includes('let buildingFolds=')?source.indexOf('let buildingFolds='):source.indexOf('function renderSelection(');
vm.runInContext(source.slice(start,source.indexOf('const buildingCategories=')),context);
function render(){context.renderSelection(town);}

render();
assert.strictEqual(find('hall-details'),hall);
assert(hall.hidden,'Legacy hall details remain hidden after selecting the hall');
assert.notStrictEqual(upgradeButton.parent,hall,'Hall upgrade is inside the visible card');
assert.strictEqual(upgradeButton.disabled,false);
// The two-second refresh rebuilds the currently selected hall.
render();render();
assert.strictEqual(find('upgrade'),upgradeButton,'Refresh preserves the upgrade control');
assert.strictEqual(upgradeButton.onclick,upgradeHandler,'Refresh preserves its action');
town.upgrade_remaining=27;
render();
assert(upgradeButton.disabled);
assert(upgradeButton.title.includes('27s remaining'),'Refresh updates upgrade progress');

context.selection={type:'building',index:1};render();
assert.strictEqual(find('hall-details'),hall,'Another building keeps hall controls attached');
assert(hall.hidden);
context.selection=null;render();
assert.strictEqual(find('upgrade'),upgradeButton,'Closing the panel preserves controls');
context.selection={type:'building',index:0};render();
assert(hall.hidden,'Legacy hall details stay hidden when reopening the hall');
assert.strictEqual(find('upgrade'),upgradeButton,'Reopening the hall restores its upgrade card');
assert.notStrictEqual(upgradeButton.parent,hall,'Upgrade control appears in its visible card');
console.log('Town hall refresh checks passed');
