const assert=require('node:assert/strict');
const client=require('./client_harness');
const app=client();
app.run(`
  const town={name:'Footprints',level:10,map_size:32,sandbox:true,wood:100,food:40,coins:20,
    player:{x:3,y:9,facing:1},villagers:[],map_resources:[],npcs:[],roads:[],
    terrain:Array(32).fill('g'.repeat(32)),buildings:[{kind:'hall',x:5,y:5,footprint:[2,2]},
      {kind:'mill',x:10,y:10,footprint:[2,2]}]};
  world={world_id:'footprints',towns:{own:town},shared_map:{map_size:32,terrain:town.terrain,map_resources:[],npcs:[]}};
  viewed='own';identity={town_id:'own'};tool='mill';panel='build';
`);
assert.equal(app.run(`placementValid(town,{x:9,y:9},'mill')`),false,'Secondary tile overlaps a saved workplace');
for(const kind of ['butcher','bakery','smokehouse','tannery']){
  assert.equal(app.run(`JSON.stringify(placementBuilding({x:13,y:13},'${kind}').footprint)`),'[2,2]',kind+' previews all four tiles');
  assert.equal(app.run(`placementValid(town,{x:9,y:9},'${kind}')`),false,kind+' rejects overlap on a secondary tile');
  assert.equal(app.run(`placementValid(town,{x:31,y:13},'${kind}')`),false,kind+' keeps its yard inside the map');
}

assert.equal(app.run(`placementValid(town,{x:13,y:13},'sawpit')`),true);
assert.equal(app.run(`placementValid(town,{x:31,y:13},'mill')`),false,'Entire footprint stays within the map');
app.run(`town.map_resources.push({kind:'rock',x:14,y:14});`);
assert.equal(app.run(`placementValid(town,{x:13,y:13},'mill')`),false,'Stock yard cannot overlap resources');
app.run(`town.map_resources=[{kind:'berry_bush',x:13,y:13},{kind:'herb_patch',x:14,y:14}];`);
for(const kind of ['home','mill','road']){
  assert.equal(app.run(`placementValid(town,{x:13,y:13},'${kind}')`),true,kind+' preview permits bushes');
}
assert.equal(app.run(`town.map_resources.length`),2,'Placement previews preserve authoritative bushes');
app.run(`tool='move';moveIndex=1;`);
assert.equal(app.run(`placementValid(town,{x:13,y:13},'mill')`),true,'Move preview permits bushes across the footprint');
app.run(`town.map_resources.push({kind:'tree',x:14,y:14});`);
assert.equal(app.run(`placementValid(town,{x:13,y:13},'mill')`),false,'Bushes do not mask other obstacles');
app.run(`town.map_resources=[];tool='move';moveIndex=1;`);
assert.equal(app.run(`placementValid(town,{x:11,y:10},'mill')`),true,'Move preview can reuse the original footprint');
assert.equal(app.run(`placementValid(town,{x:10,y:10},'mill')`),false,'Unchanged moves remain invalid');
app.run(`tool='mill';moveIndex=null;`);
assert.equal(app.run(`rangeContains({kind:'farm',x:10,y:10},{x:13,y:13},town)`),true,'Farm reach includes square corners');
assert.equal(app.run(`rangeContains({kind:'woodcutter',x:10,y:10},{x:16,y:15},town)`),false,'Woodcutter uses ten tile steps');
assert.equal(app.run(`rangeContains({kind:'fishing_hut',x:10,y:10,level:3,profession_upgrades:['master_boat']},{x:22,y:10},town)`),true,'Fishing preview includes boat reach');
app.run(`
  const sceneSteps=ClientScene.prepare(world);let prepared;
  do{prepared=sceneSteps.next();}while(!prepared.done);
  ClientScene.commit(prepared.value);
`);
for(const key of ['10,10','11,10','10,11','11,11']){
  assert.equal(app.run(`ClientScene.current.playerBuildings.has('${key}')`),true);
  assert.equal(app.run(`ClientScene.current.regions.get('own').blocked.has('${key}')`),true);
}
app.run(`
  const block=new Set(footprintTiles(town.buildings[1]).map(p=>p.x+','+p.y));
  for(const [x,y] of [[9,10],[9,11],[10,9],[11,9],[12,10],[12,11],[10,12]])block.add(x+','+y);
  const path=WorkerAnimation.route({x:11,y:14},town.buildings[1],block,32,true);
`);
assert.equal(app.run(`JSON.stringify(path.at(-1))`),'{"x":11,"y":12}','Browser routing reaches an open far edge');
assert.equal(app.run(`JSON.stringify(townOutline([town.buildings[1]]))`),'[[10,10],[12,10],[12,12],[10,12]]','Overview outlines the full footprint');
app.run(`tool=null;selectTile(11,11);`);
assert.equal(app.run(`selection.index`),1,'Stock tile selects its owning building');
app.run(`town.buildings.push({kind:'barn',x:20,y:20,construction:{progress:0}});tool='home';`);
assert.equal(app.run(`rangeContains(town.buildings[2],{x:23,y:23},town)`),true,'Barn range includes square corners');
assert.equal(app.run(`rangeContains(town.buildings[2],{x:24,y:20},town)`),false,'Barn range stops three tiles away');
assert.equal(app.run(`placementValid(town,{x:23,y:23},'home')`),false,'Barn construction reserves its full pen');
assert.equal(app.run(`placementValid(town,{x:24,y:24},'home')`),true,'Buildings can sit beyond the fence');
assert.equal(app.run(`placementValid(town,{x:16,y:20},'mill')`),false,'Secondary footprint cannot enter a pen');
assert.equal(app.run(`placementValid(town,{x:14,y:14},'barn')`),false,'New pens cannot surround existing stock tiles');
app.run(`tool='move';moveIndex=2;`);
assert.equal(app.run(`placementValid(town,{x:21,y:20},'barn')`),true,'Barn moves can reuse their old pen');
assert.equal(app.run(`placementValid(town,{x:13,y:13},'barn')`),false,'Moved pens cannot surround existing buildings');
app.run(`tool='move';moveIndex=1;`);
assert.equal(app.run(`placementValid(town,{x:16,y:20},'mill')`),false,'Moved stock tiles cannot enter a pen');
app.run(`town.buildings.pop();world.towns.friend={buildings:[{kind:'barn',x:20,y:20}]};tool='home';moveIndex=null;`);
assert.equal(app.run(`placementValid(town,{x:23,y:23},'home')`),false,'Other towns reserve their barn pens too');
console.log('Full building placement, scene collision, ranges, edge access and selection passed');
