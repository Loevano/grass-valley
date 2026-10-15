const assert=require('node:assert/strict');
const app=require('./client_harness')();
const catalog=JSON.parse(require('node:child_process').execFileSync('python3',['-c','import json, progression; print(json.dumps(progression.public_catalog()))'],{cwd:'server',encoding:'utf8'}));
app.run(`
  const createRotationElement=document.createElement;
  document.createElement=(...args)=>Object.assign(createRotationElement(...args),{value:''});
  const town={name:'Turns',level:10,map_size:32,sandbox:true,wood:100,food:40,coins:20,
    player:{x:3,y:9,facing:1},villagers:[],map_resources:[],npcs:[],roads:[],
    terrain:Array(32).fill('g'.repeat(32)),buildings:[{kind:'hall',x:5,y:5,footprint:[3,3]}]};
  world={world_id:'building-turns',progression:${JSON.stringify(catalog)},towns:{own:town}};viewed='own';identity={town_id:'own'};
  const commands=[];act=async command=>commands.push(command);
  const kinds=[...detailedBuildingKinds,'sawpit','barn','acre','orchard_tree',...decorationKinds];
`);
for(const kind of JSON.parse(app.run('JSON.stringify(kinds)'))){
  app.run(`town.buildings[1]={kind:'${kind}',x:12,y:12,orientation:3};inspect('building',1);`);
  assert.equal(app.run(`$('selection-decoration').hidden`),false,`${kind} offers facing`);
  assert.equal(app.run(`$('selection-decoration-orientation').value`),'3');
  const before=app.run('JSON.stringify(world)');
  app.run(`$('selection-decoration-orientation').value='1';$('selection-decoration-orientation').onchange();`);
  assert.deepEqual(JSON.parse(app.run('JSON.stringify(commands.at(-1))')),
    {action:'set_building_orientation',index:1,orientation:1});
  assert.equal(app.run('JSON.stringify(world)'),before,'Only the host changes orientation');
  app.run(`town.buildings[1].construction={progress:0,total:30};inspect('building',1);`);
  assert.equal(app.run(`$('selection-decoration').hidden`),true,`${kind} waits for construction`);
}
app.run(`town.buildings[1]={kind:'sawpit',x:12,y:12,footprint:[2,3],orientation:1};tool='move';moveIndex=1;`);
assert.deepEqual(JSON.parse(app.run(`JSON.stringify(placementBuilding({x:18,y:18},'sawpit'))`)),
  {kind:'sawpit',x:18,y:18,footprint:[2,3],orientation:1});
assert.equal(app.run(`placementValid(town,{x:18,y:18},'sawpit')`),true);
app.run(`town.map_resources.push({kind:'rock',x:18,y:20});`);
assert.equal(app.run(`placementValid(town,{x:18,y:18},'sawpit')`),false,'Added row blocks moves');
app.run(`town.map_resources=[{kind:'rock',x:20,y:18}];`);
assert.equal(app.run(`placementValid(town,{x:18,y:18},'sawpit')`),true,'Old third column is released');
app.run(`tool=null;moveIndex=null;identity={town_id:'friend'};inspect('building',1);`);
assert.equal(app.run(`$('selection-decoration').hidden`),true,'Visitors cannot turn another town’s buildings');
app.run(`
  const prepare=()=>{const steps=ClientScene.prepare(world);let next;do{next=steps.next();}while(!next.done);ClientScene.commit(next.value);};
  world.shared_map={map_size:32,terrain:town.terrain,map_resources:[],npcs:[]};
  prepare();const oldVersion=ClientScene.current.regions.get('own').layoutVersion;
  town.buildings[1]={...town.buildings[1],orientation:2,footprint:[3,2]};prepare();
  if(ClientScene.current.regions.get('own').layoutVersion===oldVersion)throw Error('Rotation must invalidate routing geometry');
  if(ClientScene.current.playerBuildings.has('12,14')||!ClientScene.current.playerBuildings.has('14,12'))throw Error('Scene uses the rotated footprint');
`);
// Cached and direct painters must both retain all four upright views.
const views=JSON.parse(app.run(`
  const views=[];
  for(const kind of [...detailedBuildingKinds,'barn','sawpit']){
    const signatures=[];buildingSprites.clear();buildingSpriteBytes=0;
    for(let orientation=0;orientation<4;orientation++){
      const dimensions=buildingFootprints[kind] || [1,1];
      const b={kind,x:12,y:12,orientation,footprint:orientation%2?[...dimensions].reverse():dimensions};
      const before=JSON.stringify(b),marks=[];
      const target=new Proxy({
        moveTo(x,y){marks.push(['move',x,y]);},lineTo(x,y){marks.push(['line',x,y]);},
        ellipse(x,y,rx,ry){marks.push(['ellipse',x,y,rx,ry]);},fill(){marks.push(['fill',this.fillStyle]);}
      },{get:(object,key)=>key in object?object[key]:()=>{}});
      paintBuilding(b,town,null,target,true);signatures.push(JSON.stringify(marks));
      drawBuilding(b,town);const count=buildingSprites.size;drawBuilding(b,town);
      if(count!==orientation+1||buildingSprites.size!==count)throw Error(kind+': cached views collide');
      if(JSON.stringify(b)!==before)throw Error(kind+': painting mutated host data');
    }
    views.push({kind,count:new Set(signatures).size});
  }
  JSON.stringify(views);
`));
for(const {kind,count} of views)assert.equal(count,4,`${kind} has four distinct views`);
console.log('All building facing controls, rotated move previews, scene collisions, and four-view sprites passed');
