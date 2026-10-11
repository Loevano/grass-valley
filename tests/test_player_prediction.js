// Run with: node tests/test_player_prediction.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8'),elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
documentMock.getElementById('world').getContext=()=>new Proxy({}, {get:()=>()=>{}});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','assert',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  return (async()=>{
    const town={player:{x:5,y:7,facing:1},map_size:12,terrain:Array(12).fill('g'.repeat(12)),buildings:[],map_resources:[]};
    world={world_id:'prediction',towns:{own:town}};identity={town_id:'own'};viewed='own';
    const screenDistance=(x,y)=>Math.hypot(x*23-y*23,x*11.5+y*11.5);
    for(const [dx,dy] of [[1,-1],[-1,1],[1,1],[-1,-1],[1,0],[-1,0],[0,1],[0,-1]]){
      const target=predictPlayerStep(town,town.player,dx,dy);
      assert(Math.abs(screenDistance(target.x-5,target.y-7)-.3*Math.SQRT2*23)<1e-9,
        'Every predicted direction covers the same screen distance per command');
      playerView=null;playerSprite(town,0);playerView.target=target;
      const sprite=playerSprite(town,1/60);
      assert(Math.abs(screenDistance(sprite.x-5,sprite.y-7)-3/60*Math.SQRT2*23)<1e-9,
        'Every animated direction covers the same screen distance per frame');
    }
    town.player.boat=true;town.player.boat_level=3;
    town.map_resources=[{kind:'water',x:5,y:7}];movementMap=null;
    const sailing=predictPlayerStep(town,town.player,1,-1);
    assert(Math.abs(sailing.x-5-.3/Math.SQRT2*1.5)<1e-9,'Boat speed is predicted on water');
    town.map_resources=[];movementMap=null;
    assert(Math.abs(predictPlayerStep(town,town.player,1,-1).x-5-.3/Math.SQRT2)<1e-9,'Boat upgrades keep normal land speed');
    playerView=null;
    playerSprite(town,0);
    let release,calls=0;
    request=()=>{calls++;return new Promise(resolve=>{release=resolve;});};
    walkKeys.add('d');let hero=playerSprite(town,1/60);
    assert(hero.x>5&&hero.y<7,'Walking responds on the first display frame before the host replies');
    let previous=hero.x;
    for(let frame=0;frame<5;frame++){
      hero=playerSprite(town,1/60);assert(hero.x>previous,'The predicted step advances on each frame');previous=hero.x;
    }
    for(let frame=0;frame<120;frame++)hero=playerSprite(town,1/60);
    assert.equal(calls,1,'Delayed replies cannot flood movement commands');
    assert(Math.hypot(hero.x-5,hero.y-7)<=1.500001,'Prediction remains bounded during a stalled connection');
    release({player:{x:5,y:7,facing:1}});clearWalk();
    for(let i=0;i<6;i++)await Promise.resolve();
    hero=playerSprite(town,0);
    assert.equal(calls,2,'Buffered input is sent as one ordered catch-up request');
    release({player:{x:5,y:7,facing:1}});
    for(let i=0;i<6;i++)await Promise.resolve();
    for(let frame=0;frame<40;frame++)hero=playerSprite(town,.1);
    assert.equal(hero.x,5);assert.equal(hero.y,7,'Host rejection corrects a predicted step');
    // Low-latency walking should sustain the host\'s 10 Hz cadence at 60 FPS.
    calls=0;let host={x:5,y:7,facing:1};
    request=async()=>{calls++;host={x:host.x+.21213203435596426,y:host.y-.21213203435596426,facing:1};return {player:host};};
    walkKeys.add('d');previous=hero.x;
    for(let frame=0;frame<60;frame++){
      hero=playerSprite(town,1/60);assert(hero.x>previous,'Walking does not pause between acknowledged steps');previous=hero.x;
      for(let i=0;i<6;i++)await Promise.resolve();
    }
    assert.equal(calls,11,'Movement sends immediately and at each 100 ms boundary');
    clearWalk();
    // Periodic host stalls must not interrupt the walking animation or camera.
    town.player={x:5,y:7,facing:1};playerView=null;playerSprite(town,0);
    host={...town.player};let clock=0,pending=null,maxBatch=0;
    request=(path,data)=>{
      const steps=data.steps || [{dx:data.dx,dy:data.dy}];maxBatch=Math.max(maxBatch,steps.length);
      for(const step of steps)host=predictPlayerStep(town,host,step.dx,step.dy);
      return new Promise(resolve=>{pending={resolve,player:{...host},due:clock+((clock<.11||clock>.5&&clock<.61)?.25:.02)};});
    };
    walkKeys.add('d');previous=5;
    for(let frame=0;frame<60;frame++){
      clock+=1/60;
      if(pending&&clock>=pending.due){pending.resolve({player:pending.player});pending=null;}
      for(let i=0;i<6;i++)await Promise.resolve();
      hero=playerSprite(town,1/60);
      assert(hero.x>previous,'250 ms host delays preserve continuous walking at frame '+frame);previous=hero.x;
      assert.equal(camera.x,hero.x,'The camera follows buffered walking');
    }
    clearWalk();
    assert(maxBatch>1&&maxBatch<=5,'Delayed input catches up in bounded batches');
    // Drain the final acknowledged and unsent input before the next scenario.
    request=async(path,data)=>{
      for(const step of data.steps || [{dx:data.dx,dy:data.dy}])host=predictPlayerStep(town,host,step.dx,step.dy);
      return {player:{...host}};
    };
    if(pending){pending.resolve({player:pending.player});pending=null;}
    for(let frame=0;frame<10;frame++){
      for(let i=0;i<6;i++)await Promise.resolve();
      playerSprite(town,.1);
    }
    // Buffered turns retain their order after the original command is acknowledged.
    town.player={x:5,y:7,facing:1};playerView=null;playerSprite(town,0);host={...town.player};
    const sent=[];
    request=(path,data)=>{
      const steps=data.steps || [{dx:data.dx,dy:data.dy}];sent.push(steps);
      for(const step of steps)host=predictPlayerStep(town,host,step.dx,step.dy);
      return new Promise(resolve=>{release=()=>resolve({player:{...host}});});
    };
    walkKeys.add('d');playerSprite(town,.1);playerSprite(town,.1);
    walkKeys.delete('d');walkKeys.add('s');playerSprite(town,.1);clearWalk();
    release();for(let i=0;i<6;i++)await Promise.resolve();
    playerSprite(town,0);
    assert.deepEqual(sent[1],[{dx:1,dy:-1},{dx:1,dy:1}],'Catch-up preserves the turn before key release');
    release();for(let i=0;i<6;i++)await Promise.resolve();
    assert.equal(playerView.steps.length,0,'Released input drains after acknowledgement');
    assert.deepEqual(playerView.target,host,'Acknowledgement reconciles to the host position');
    // Uneven display frames must not introduce pauses between movement commands.
    for(const keys of [['w'],['s'],['d'],['w','d']]){
      town.player={x:5,y:7,facing:1};playerView=null;playerSprite(town,0);
      let host={...town.player},elapsed=0,previous={...town.player};calls=0;
      request=async(path,data)=>{calls++;host=predictPlayerStep(town,host,data.dx,data.dy);return {player:{...host}};};
      keys.forEach(key=>walkKeys.add(key));
      for(let frame=0;frame<45;frame++){
        const seconds=[.016,.023,.014,.019][frame%4];elapsed+=seconds;
        const sprite=playerSprite(town,seconds);
        const distance=screenDistance(sprite.x-previous.x,sprite.y-previous.y);
        assert(Math.abs(distance-seconds*3*Math.SQRT2*23)<1e-8,
          'Uneven frames keep a steady walking speed for '+keys.join('+')+' at frame '+frame);
        assert(Math.abs(camera.x-sprite.x)<1e-9&&Math.abs(camera.y-sprite.y)<1e-9,
          'The locked camera follows every movement frame');
        previous={x:sprite.x,y:sprite.y};
        for(let i=0;i<6;i++)await Promise.resolve();
      }
      assert.equal(calls,Math.floor((elapsed+1e-6)/.1)+1,'Uneven frames preserve the 10 Hz movement cadence');
      clearWalk();
    }
    const blockedTown={...town,map_resources:[{kind:'water',x:4,y:3}]};
    world={world_id:'obstacles',towns:{own:blockedTown}};
    assert.equal(predictPlayerStep(blockedTown,{x:3.3,y:3,facing:1},1,0).x,3.3,'Prediction respects water collision radius');
    const forest={...blockedTown,map_resources:[{kind:'tree',x:4,y:3}]};world={world_id:'forest',towns:{own:forest}};
    assert(predictPlayerStep(forest,{x:3.3,y:3,facing:1},1,0).x>3.3,'Trees remain walkable');
    world={world_id:'shared',shared_map:{},towns:{own:forest,friend:{buildings:[{kind:'hall',x:4,y:3}]}}};
    assert.equal(predictPlayerStep(forest,{x:3.3,y:3,facing:1},1,0).x,3.3,'Other towns buildings block predicted movement');
    assert.equal(predictPlayerStep(forest,{x:11.3,y:3,facing:1},1,0).x,11.3,'Prediction stays inside map boundaries');
    const voidTown={...town,terrain:Array(12).fill('ggggvggggggg')};world={world_id:'outline',towns:{own:voidTown}};
    assert.equal(predictPlayerStep(voidTown,{x:3.3,y:3,facing:1},1,0).x,3.3,'Prediction stays inside irregular terrain');
  })();`);
run(documentMock,{addEventListener(){}},{getItem:()=>null},()=>0,()=>{},assert)
  .then(()=>console.log('Player prediction checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
