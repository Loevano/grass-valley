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
    assert(Math.hypot(hero.x-5,hero.y-7)<=.300001,'Prediction is limited to one host step');
    release({player:{x:5,y:7,facing:1}});clearWalk();
    for(let i=0;i<6;i++)await Promise.resolve();
    hero=playerSprite(town,.1);
    assert.equal(hero.x,5);assert.equal(hero.y,7,'Host rejection corrects a predicted step');
    // Low-latency walking should sustain the host\'s 10 Hz cadence at 60 FPS.
    calls=0;let host={x:5,y:7,facing:1};
    request=async()=>{calls++;host={x:host.x+.21213203435596426,y:host.y-.21213203435596426,facing:1};return {player:host};};
    walkKeys.add('d');previous=hero.x;
    for(let frame=0;frame<60;frame++){
      hero=playerSprite(town,1/60);assert(hero.x>previous,'Walking does not pause between acknowledged steps');previous=hero.x;
      for(let i=0;i<6;i++)await Promise.resolve();
    }
    assert.equal(calls,10,'Floating-point cooldown residue does not drop movement commands');
    clearWalk();
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
