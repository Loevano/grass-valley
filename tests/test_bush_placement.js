const assert=require('node:assert/strict');
const client=require('./client_harness');
const app=client();

(async()=>{
  app.run(`
    const town={name:'Bush placement',level:10,map_size:32,sandbox:true,wood:100,food:40,coins:20,
      player:{x:3,y:9,facing:1},villagers:[],npcs:[],roads:[],
      terrain:Array(32).fill('g'.repeat(32)),
      map_resources:[{kind:'berry_bush',x:13,y:13},{kind:'rock',x:15,y:13}],
      buildings:[{kind:'hall',x:5,y:5},{kind:'farm',x:12,y:12}]};
    world={world_id:'bush-placement',towns:{own:town}};
    identity={town_id:'own'};viewed='own';panel='build';
    const commands=[];
    request=async(path,data)=>{commands.push(data);return {};};
    refresh=async()=>render();
    function pointer(x,y){const [px,py]=point(x,y);return {clientX:px,clientY:py+tileH/2,button:0};}
  `);
  for(const kind of ['home','mill','road','acre']){
    app.run(`tool='${kind}';acreFarmIndex=1;moveIndex=null;canvas.onmousemove(pointer(13,13));render();`);
    assert.equal(app.run(`buildTile?.x`),13,kind+' keeps its bush preview after rendering');
    const before=app.run('commands.length');
    await app.run('canvas.onclick(pointer(13,13))');
    assert.equal(app.run('commands.length'),before+1,kind+' click submits placement on a bush');
    assert.equal(app.run('commands.at(-1).action'),kind==='road'?'build_road':'build');
    assert.equal(app.run('commands.at(-1).kind'),kind);
    assert.equal(app.run('commands.at(-1).x'),13);
    assert.equal(app.run('commands.at(-1).y'),13);
  }
  app.run(`tool='move';moveIndex=null;`);
  await app.run('canvas.onclick(pointer(12,12))');
  assert.equal(app.run('moveIndex'),1,'Click selects a building to move');
  await app.run('canvas.onclick(pointer(13,13))');
  assert.equal(app.run('commands.at(-1).action'),'move','Click submits a move onto a bush');
  assert.equal(app.run('commands.at(-1).index'),1);
  assert.equal(app.run('commands.at(-1).x'),13);
  const before=app.run('commands.length');
  app.run(`tool='home';`);
  await app.run('canvas.onclick(pointer(15,13))');
  assert.equal(app.run('commands.length'),before,'Clicking a rock still blocks building');
  assert.equal(app.run('town.map_resources.length'),2,'The client waits for host removal of bushes');
  app.run('tool=null;');
  await app.run('canvas.onclick(pointer(13,13))');
  assert.equal(app.run('selection?.type'),'resource','Bushes can still be inspected outside build mode');
  console.log('Bush placement click and preview checks passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
