const assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const client=require('./client_harness');
const catalog=JSON.parse(execFileSync('python3',['-c','import json, progression; print(json.dumps(progression.public_catalog()))'],{cwd:require('node:path').join(__dirname,'../server'),encoding:'utf8'}));
const app=client();
app.run(`
  world={progression:${JSON.stringify(catalog)},towns:{}};
  const town={level:10,wood:500,goods:{},villagers:[
    {name:'Ada',job:'tanning',building_index:0,husbandry:2},
    {name:'Bea',job:'tanning',building_index:0,husbandry:2},
    {name:'Cal',job:'idle',husbandry:2}
  ],buildings:[{kind:'tannery'}]};
  world.towns.own=town;identity={town_id:'own'};viewed='own';
  const commands=[];act=command=>commands.push(command);
  const target=document.createElement('div');
  function nodes(node=target){return node.children.flatMap(child=>[child,...nodes(child)]);}
  function renderWorkerPanel(){target.replaceChildren();renderSettlers(town,town.buildings[0],0,target);}
  function addWorker(){return nodes().find(node=>node.textContent==='Add worker');}
`);

for(const [kind,spec] of Object.entries(catalog.buildings)){
  for(const level of [1,2,3]){
    const tiers=catalog.worker_capacities[kind];
    const expected=tiers?tiers[Math.min(level,tiers.length)-1]:spec.slots*level;
    assert.equal(app.run(`settlerCapacity({kind:${JSON.stringify(kind)},settler_level:${level}})`),expected,`${kind} capacity tier ${level} agrees with the host catalog`);
  }
}

const before=app.run('JSON.stringify(town)');
app.run('renderWorkerPanel();');
assert(app.run(`nodes().some(node=>node.textContent==='Workers · 2/2')`),'A fully staffed tannery combines its heading and worker count');
assert(app.run('addWorker().disabled'),'A full tannery cannot accept another worker');
assert.equal(app.run(`upgradePreview({action:'upgrade_capacity',index:0},town,town.buildings[0]).benefits[0]`),'2 → 4 worker spaces');
assert.equal(app.run('JSON.stringify(town)'),before,'Capacity rendering preserves authoritative town state');

app.run('town.buildings[0].settler_level=2;renderWorkerPanel();');
assert(app.run(`nodes().some(node=>node.textContent==='Workers · 2/4')`),'An upgraded tannery displays its new staffing limit in the heading');
assert(!app.run('addWorker().disabled'),'An upgraded tannery allows another worker');
app.run('addWorker().onclick();');
assert.deepEqual(JSON.parse(app.run('JSON.stringify(commands.at(-1))')),{action:'assign_settler',index:2,building_index:0});
console.log('Industry worker capacity checks passed');
