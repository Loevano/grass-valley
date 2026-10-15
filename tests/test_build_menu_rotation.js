const assert=require('node:assert/strict');
const app=require('./client_harness')();
app.run(`
  const town={name:'Placement',level:10,map_size:32,sandbox:true,wood:100,food:40,coins:20,
    player:{x:3,y:9,facing:1},villagers:[],map_resources:[],npcs:[],roads:[],
    terrain:Array(32).fill('g'.repeat(32)),buildings:[{kind:'hall',x:5,y:5,footprint:[3,3]}]};
  world={world_id:'build-turns',towns:{own:town}};viewed='own';identity={town_id:'own'};
  const choose=kind=>[...$('buildings').children].flatMap(group=>group.children).find(button=>button.buildingKind===kind).onclick();
  const face=orientation=>{$('build-orientation').value=String(orientation);$('build-orientation').onchange();};
  const commands=[];request=async(path,data)=>commands.push(data);refresh=async()=>{};
  openPanel('build');
`);
assert.equal(app.run(`$('build-facing').hidden`),true,'Choose a building before choosing its direction');
app.run(`choose('sawpit');`);
assert.equal(app.run(`$('build-facing').hidden`),false);
assert.equal(app.run(`$('build-orientation').value`),'0');
const before=app.run('JSON.stringify(world)');
app.run(`face(1);`);
assert.deepEqual(JSON.parse(app.run(`JSON.stringify(placementBuilding({x:18,y:18},'sawpit'))`)),
  {kind:'sawpit',x:18,y:18,orientation:1,footprint:[2,3]});
assert.equal(app.run('JSON.stringify(world)'),before,'Choosing direction does not mutate host snapshots');
app.run(`town.map_resources=[{kind:'rock',x:18,y:20}];buildTile={x:18,y:18};face(0);`);
assert.equal(app.run(`placementValid(town,{x:18,y:18},'sawpit')`),true,'Default footprint avoids the third row');
app.run(`face(1);`);
assert.equal(app.run(`placementValid(town,{x:18,y:18},'sawpit')`),false,'Turned preview reserves the third row');
assert.equal(app.run('buildTile'),null,'Changing direction clears a now-invalid placement');
app.run(`town.map_resources=[];choose('home');`);
assert.equal(app.run(`$('build-orientation').value`),'1','The direction stays selected across building choices');
for(const tool of ['road','move','destroy']){
  app.run(tool==='road'?`choose('road');`:`$('${tool}').onclick();`);
  assert.equal(app.run(`$('build-facing').hidden`),true,`${tool} does not offer new-building rotation`);
}
app.run(`choose('sawpit');building=true;render();face(3);`);
assert.equal(app.run(`$('build-orientation').disabled`),true);
assert.equal(app.run('buildOrientation'),1,'An in-flight placement keeps its direction');
app.run(`building=false;town.buildings.push({kind:'sawpit',x:12,y:12,orientation:2,footprint:[3,2]});$('move').onclick();moveIndex=1;`);
assert.equal(app.run(`placementBuilding({x:18,y:18},'sawpit').orientation`),2,'Move previews use the existing building’s direction');
app.run(`choose('sawpit');`);
app.run(`(async()=>{
  await buildAt(18,18);
  if(commands[0].orientation!==1)throw Error('Build requests must carry the preview direction');
  await buildAt(22,18);
  if(commands[1].orientation!==1)throw Error('Repeated placements retain the chosen direction');
  choose('road');await buildAt(18,18);
  if('orientation' in commands[2]||commands[2].action!=='build_road')throw Error('Road commands do not carry building facing');
})()`).then(()=>console.log('Build menu direction, footprint previews, repeated placement, and command checks passed'))
  .catch(error=>{console.error(error);process.exitCode=1;});
