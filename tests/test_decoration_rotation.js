const assert=require('node:assert/strict');
const app=require('./client_harness')();
app.run(`
  const town={name:'Decorations',level:5,map_size:28,sandbox:true,wood:100,food:40,coins:20,
    player:{x:7,y:12,facing:1},villagers:[],map_resources:[],npcs:[],roads:[],
    terrain:Array(28).fill('g'.repeat(28)),buildings:[{kind:'hall',x:5,y:5},
      ...decorationKinds.map(kind=>({kind,x:12,y:12}))]};
  world={world_id:'decoration-rotation',towns:{own:town}};viewed='own';identity={town_id:'own'};
  const original=JSON.stringify(world),commands=[];
  act=async command=>commands.push(command);
`);
for(let index=1;index<=app.run('decorationKinds.length');index++){
  app.run(`inspect('building',${index});`);
  assert.equal(app.run(`$('selection-decoration').hidden`),false);
  assert.equal(app.run(`$('selection-decoration-orientation').children.length`),4);
  assert.equal(app.run(`$('selection-decoration-orientation').value`),'0','Legacy decorations face their default direction');
  app.run(`$('selection-decoration-orientation').value='3';$('selection-decoration-orientation').onchange();`);
  assert.deepEqual(JSON.parse(app.run(`JSON.stringify(commands.at(-1))`)),
    {action:'set_decoration_orientation',index,orientation:3});
}
assert.equal(app.run('JSON.stringify(world)'),app.run('original'),'Controls wait for the host instead of mutating snapshots');
app.run(`town.buildings[1].orientation=2;inspect('building',1);`);
assert.equal(app.run(`$('selection-decoration-orientation').value`),'2','Host orientation appears in the inspector');
assert.equal(app.run(`tool='move';moveIndex=1;placementBuilding({x:14,y:14},'wooden_bench').orientation`),2,'Move previews keep the saved direction');
assert.equal(app.run(`tool='wooden_bench';placementBuilding({x:14,y:14},'wooden_bench').orientation`),undefined,'New placements use the default direction');
app.run(`tool=null;moveIndex=null;`);
app.run(`inspect('building',0);`);
assert.equal(app.run(`$('selection-decoration').hidden`),true,'Ordinary buildings have no decoration controls');
app.run(`identity={town_id:'friend'};inspect('building',1);`);
assert.equal(app.run(`$('selection-decoration').hidden`),true,'Visitors cannot rotate decorations');
app.run(`identity={town_id:'own'};town.buildings[1].construction={progress:0,total:30};inspect('building',1);`);
assert.equal(app.run(`$('selection-decoration').hidden`),true,'Construction cannot be rotated');

// Actual drawing calls must show four distinct upright views, including cached redraws.
const drawings=JSON.parse(app.run(`
  const drawings=[];
  for(const kind of decorationKinds){
    const building={kind,x:12,y:12},saved=JSON.stringify(building),views=[];
    const target=new Proxy({
      beginPath(){views.push(['path']);},moveTo(x,y){views.push(['move',x,y]);},lineTo(x,y){views.push(['line',x,y]);},
      fill(){views.push(['fill',this.fillStyle]);}
    },{get:(object,key)=>key in object?object[key]:()=>{}});
    paintBuilding(building,town,null,target);
    const legacy=JSON.stringify(views);building.orientation=0;views.length=0;paintBuilding(building,town,null,target);
    if(legacy!==JSON.stringify(views))throw Error('Legacy orientation must match zero');
    const signatures=[];buildingSprites.clear();buildingSpriteBytes=0;
    for(let orientation=0;orientation<4;orientation++){
      building.orientation=orientation;const snapshot=JSON.stringify(building);views.length=0;
      paintBuilding(building,town,null,target);signatures.push(JSON.stringify(views));
      drawBuilding(building,town);const count=buildingSprites.size;drawBuilding(building,town);
      if(count!==orientation+1||buildingSprites.size!==count)throw Error('Rotation must refresh the cached sprite once');
      if(snapshot!==JSON.stringify(building))throw Error('Drawing changed authoritative decoration state');
    }
    delete building.orientation;drawBuilding(building,town);
    if(buildingSprites.size!==4||JSON.stringify(building)!==saved)throw Error('Default view must reuse the zero-orientation cache');
    drawings.push({kind,count:new Set(signatures).size});
  }
  JSON.stringify(drawings);
`));
for(const {kind,count} of drawings)assert.equal(count,4,`${kind} has four distinct views`);
console.log('Decoration controls, host ownership, four-direction art, and sprite cache checks passed');
