// Run with: node tests/test_building_visual_upgrades.js
const assert=require('assert'),fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8');
const elements=new Map(),marks=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const context=new Proxy({}, {get(target,key){
  if(key in target)return target[key];
  return (...args)=>marks.push([key,...args,target.fillStyle,target.strokeStyle,target.lineWidth]);
}});
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvas=documentMock.getElementById('world');canvas.getContext=()=>context;
canvas.getBoundingClientRect=()=>({left:0,top:0,width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','assert','marks','WorkerAnimation',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  const town={level:1,map_size:12,buildings:[],map_resources:[{kind:'tree',x:6,y:5},{kind:'water',x:5,y:6}],villagers:[]};
  identity={town_id:'own'};viewed='own';world={world_id:'test',towns:{own:town}};
  function appearance(building,owner=town){
    marks.length=0;ctx.fillStyle='';ctx.strokeStyle='';ctx.lineWidth=1;
    drawBuilding(building,owner);return JSON.stringify(marks);
  }
  for(const kind of ['home','storehouse','woodcutter','farm','fishing_hut','barn','sawpit','mill','smokehouse','workshop','fish_pond','orchard','market','bakery','butcher','tannery','mine','smelter','stable','saddler','steelworks','tradehouse']){
    const building={kind,x:5,y:5},original=JSON.stringify(building);
    const base=appearance(building),second=appearance({...building,level:2}),third=appearance({...building,level:3});
    assert.notStrictEqual(base,second,kind+' changes at level 2');
    assert.notStrictEqual(second,third,kind+' changes at level 3');
    assert.strictEqual(JSON.stringify(building),original,'Rendering does not mutate saved buildings');
    if(kind!=='acre')assert.notStrictEqual(base,appearance({...building,settler_level:2}),kind+' shows upgraded settler capacity');
    const site={...building,construction:{progress:15,total:30,upgrade:{level:2}}};
    assert.strictEqual(appearance(site),appearance({...site,construction:{...site.construction,upgrade:{level:3}}}),'Pending target levels do not render as completed');
    assert.notStrictEqual(appearance(site),second,'Completion replaces scaffolding with upgraded artwork');
  }
  const acre={kind:'acre',x:5,y:5};
  assert.strictEqual(appearance(acre),appearance({...acre,level:3,settler_level:3}),'Old acre upgrades have no visual effect');
  const herd=barnAnimalSprites([{kind:'stable',x:5,y:5,animals:{horse:2},animal_growth:{horse:[0,300]}}],0,town);
  assert.equal(herd.length,2,'Stable horses appear beside their workplace');
  assert(herd[0].size<herd[1].size,'A growing foal is smaller than an adult');
  herd.forEach(drawBarnAnimal);
  paintResource(ctx,{kind:'wild_horse',x:7,y:7},false,true);
  assert.strictEqual(buildingHeight({kind:'home',settler_level:3},town),buildingHeight({kind:'home'},town),'Capacity markers do not masquerade as structural upgrades');
  const silhouettes=['barn','sawpit','mill','smokehouse','workshop','fish_pond','orchard','market','bakery','butcher'].map(kind=>appearance({kind,x:5,y:5}));
  assert.strictEqual(new Set(silhouettes).size,10,'Advanced workplaces have distinct artwork');
  assert.notStrictEqual(appearance({kind:'mill',x:5,y:5}),(()=>{marks.length=0;drawBuilding({kind:'mill',x:5,y:5},town,{elapsed:1});return JSON.stringify(marks);})(),'Active mill sails rotate');
  const cottage={kind:'home',x:5,y:5,level:4};
  assert.notStrictEqual(appearance(cottage),appearance({...cottage,level:3}),'Level 4 cottages have full stone walls');
  assert.strictEqual(buildingVisualLevel(cottage,town),4,'Level 4 cottage artwork is not capped at 3');
  const hall={kind:'hall',x:5,y:5};
  const halls=[1,2,3,4,5].map(level=>appearance(hall,{...town,level}));
  assert.strictEqual(new Set(halls).size,5,'Each completed hall level has distinct artwork');
  assert.strictEqual(appearance(hall,{...town,level:1,upgrade_remaining:10}),halls[0],'Hall keeps its completed level while upgrading');
  assert.notStrictEqual(appearance({...hall,settler_level:2}),halls[0],'Hall capacity upgrades add windows');
  assert.notStrictEqual(appearance({...hall,settler_level:2}),appearance({...hall,settler_level:3}),'Both hall capacity upgrades are visible');
  for(const kind of ['hall','mill','sawpit','workshop']){
    const expanded={kind,x:5,y:5,footprint:kind==='sawpit'?[3,2]:[2,2]},saved=JSON.stringify(expanded);
    assert.notStrictEqual(appearance(expanded),appearance({...expanded,footprint:[1,1]}),kind+' uses its enlarged lot for its workplace');
    assert.strictEqual(JSON.stringify(expanded),saved,'Workplace artwork preserves the authoritative footprint');
  }
  const warmWindows=building=>JSON.parse(appearance(building)).filter(mark=>mark[0]==='fillRect'&&mark[5]==='#edc77d');
  assert.deepStrictEqual(warmWindows({...hall,footprint:[2,2]})[0].slice(3,5),warmWindows(hall)[0].slice(3,5),'Wider halls retain the original human-sized windows');
  for(const [kind,upgrade] of [['home','timber_frame'],['woodcutter','axe'],['woodcutter','tools'],['sawpit','saw'],['fishing_hut','nets'],['fishing_hut','wool_nets'],['fishing_hut','boat']]){
    const building={kind,x:5,y:5};
    assert.notStrictEqual(appearance(building),appearance({...building,profession_upgrades:[upgrade]}),upgrade+' adds visible equipment');
    assert.notStrictEqual(appearance({...building,level:3}),appearance({...building,level:3,profession_upgrades:[upgrade]}),upgrade+' stays visible on upgraded buildings');
  }
  const fishingHut={kind:'fishing_hut',x:5,y:5};
  const boatTiers=[['boat'],['boat','improved_boat'],['boat','improved_boat','master_boat']].map(profession_upgrades=>appearance({...fishingHut,profession_upgrades}));
  assert.strictEqual(new Set(boatTiers).size,3,'Each fishing boat tier has visible upgrades');
  town.map_resources.push({kind:'water',x:4,y:6});
  town.buildings=[{kind:'fishing_hut',x:4,y:5,profession_upgrades:['boat']},
    {kind:'fishing_hut',x:5,y:4,profession_upgrades:['boat']}];
  const docked=fishingBoatSprites([['own',town]],town.map_resources);
  assert.strictEqual(docked.length,2,'Each hut owns its own visible boat');
  assert.notDeepStrictEqual([docked[0].x,docked[0].y],[docked[1].x,docked[1].y],'Boats use separate nearby berths');
  assert(docked.every(b=>town.map_resources.some(r=>r.kind==='water'&&r.x===b.x&&r.y===b.y)),'Boats float on water');
  assert.strictEqual(JSON.stringify(docked),JSON.stringify(fishingBoatSprites([['own',JSON.parse(JSON.stringify(town))]],town.map_resources)),'Berths remain stable after reload');
  const sailor={townId:'own',x:4.5,y:6,state:{boating:true,home:town.buildings[0]}};
  const sailing=fishingBoatSprites([['own',town]],town.map_resources,[sailor]);
  assert.deepStrictEqual([sailing[0].x,sailing[0].y],[sailor.x,sailor.y],'Boat follows its fisher continuously on water');
  assert.strictEqual(sailing[0].passenger,sailor,'Fisher rides in the hut-owned boat');
  assert(!sailing[1].passenger,'Another hut keeps its own docked boat');
  town.buildings[0].profession_upgrades.push('improved_boat');
  const secondSailor={...sailor,x:5,y:6},rodFisher={...sailor,state:{...sailor.state,boating:false}};
  const crewBoat=fishingBoatSprites([['own',town]],town.map_resources,[sailor,secondSailor,rodFisher])[0];
  assert.deepStrictEqual(crewBoat.crew,[sailor,secondSailor],'Every boat crew member rides aboard; rod fishers remain on the dock');
  const almostInteger={...sailor,x:4.999999999,y:6.000000001};
  assert.strictEqual(fishingBoatSprites([['own',town]],town.map_resources,[almostInteger])[0].passenger,almostInteger,'Playback roundoff does not detach a fisherman from the boat');
  const dock={x:4,y:6},routeBerth={x:5,y:6};
  const dockFisher={...sailor,x:4,y:6,state:{...sailor.state,sailing:{dock,berth:routeBerth}}};
  const waitingBoat=fishingBoatSprites([['own',town]],town.map_resources,[dockFisher])[0];
  assert.deepStrictEqual([waitingBoat.x,waitingBoat.y],[5,6],'Boat waits one water tile away from the dock');
  assert(!waitingBoat.passenger,'A fisher on the wooden dock has not boarded');
  const stepping={...dockFisher,x:4.5};
  assert(!fishingBoatSprites([['own',town]],town.map_resources,[stepping])[0].passenger,'Boat waits while the fisher steps aboard');
  const riding={...dockFisher,x:5};
  assert.strictEqual(fishingBoatSprites([['own',town]],town.map_resources,[riding])[0].passenger,riding,'Fisher boards at the adjacent berth');
  const berth={x:5,y:6};
  const boarding={...sailor,x:5,y:5.75,state:{...sailor.state,sailing:{berth}}};
  const boardingBoat=fishingBoatSprites([['own',town]],town.map_resources,[boarding,secondSailor])[0];
  assert.deepStrictEqual([boardingBoat.x,boardingBoat.y],[5,6],'Boat waits at the actual route berth during boarding');
  assert(!boardingBoat.passenger,'Boarding waits until the fisher reaches water instead of switching halfway ashore');
  const boarded={...boarding,y:6};
  const boardedBoat=fishingBoatSprites([['own',town]],town.map_resources,[boarded,secondSailor])[0];
  assert.strictEqual(boardedBoat.passenger,boarded,'Fisher boards at the berth without relocating the boat');
  const leavingBoat=fishingBoatSprites([['own',town]],town.map_resources,[{...boarding,y:5.99},sailor])[0];
  assert.deepStrictEqual([leavingBoat.x,leavingBoat.y],[5,6],'Unloading leaves the boat at its berth rather than jumping to another crew member');
  assert(!leavingBoat.passenger);
  assert.deepStrictEqual(leavingBoat.crew,[sailor],'A crew member awaiting the captain remains inside the docked boat');
  town.buildings[0].profession_upgrades.pop();

  assert(!fishingBoatSprites([['own',town]],town.map_resources,[{...sailor,x:4,y:5}])[0].passenger,'Boat stays docked while its fisher walks on land');
  const crowded=fishingBoatSprites([['own',town]], [{kind:'water',x:5,y:6}]);
  assert(crowded.every(b=>Math.floor(b.x+.5)===5&&Math.floor(b.y+.5)===6),'Shared berths keep every boat on the water tile');
  const boatLooks=[1,2,3].map(level=>{marks.length=0;drawFishingBoat({...docked[0],level});return JSON.stringify(marks);});
  assert.strictEqual(new Set(boatLooks).size,3,'Full-size boat tiers look different');
  draw();
  assert(draw.piers.length===2&&draw.piers.every(pier=>pier.length>1),'Unstaffed boat huts keep their visible docks');
  assert(docked.every(b=>{const [px,py]=point(b.x,b.y);return hitTargets.some(hit=>hit.type==='building'&&hit.index===b.objectIndex&&hit.townId==='own'&&hit.left<=px&&hit.right>=px&&hit.top<=py+tileH/2&&hit.bottom>=py+tileH/2);}), 'Clicking a visible boat opens its own hut');
  town.buildings[0].construction={progress:0,total:30,upgrade:{profession_upgrades:['boat','improved_boat']}};
  assert.strictEqual(fishingBoatSprites([['own',town]],town.map_resources)[0].level,1,'Owned boat stays visible at its current tier during upgrades');
  assert.strictEqual(fishingBoatSprites([['own',{...town,buildings:[{kind:'fishing_hut',x:4,y:5}]}]],town.map_resources).length,0,'Unpurchased huts have no boat');
  const compactSawpit={kind:'sawpit',x:4,y:5,footprint:[1,1]};
  const compactBottom=Math.max(...JSON.parse(appearance(compactSawpit)).filter(mark=>mark[0]==='fillRect').map(mark=>mark[2]+mark[4]));
  world={world_id:'sawpit-selection',towns:{own:{...town,buildings:[compactSawpit]}}};draw();
  assert(hitTargets.find(hit=>hit.type==='building'&&hit.index===0).bottom>=compactBottom,'Legacy sawpit selection includes the lower saw handle and pit rim');
  const visitor={...town,level:5,buildings:[hall],map_resources:[]};
  town.buildings=[{kind:'home',x:4,y:5,level:3}];
  world={world_id:'test',shared_map:town,town_sites:[{x:5,y:5}],towns:{own:town,visitor}};
  draw();
  const target=hitTargets.find(hit=>hit.type==='building'&&hit.townId==='visitor');
  const [,py]=point(hall.x,hall.y);
  assert(target&&target.top<=py+tileH/2-(buildingHeight(hall,visitor)+30)*tileW/46,'Visitor hall roof uses its own level and remains selectable');
  buildingEffects.length=0;
  commandEffect({action:'upgrade_capacity',index:0});
  assert.strictEqual(buildingEffects[0].kind,'build','Capacity upgrades trigger construction feedback');
  buildingEffects.length=0;
  commandEffect({action:'move',index:0,x:6,y:6});
  assert.deepStrictEqual(buildingEffects.map(e=>e.kind),['dust','move'],'Moves mark the old and new sites');
  assert.strictEqual(buildingEffects[1].x,6);
  drawBuildingEffects(.4);const effectAge=buildingEffects[0].age;
  drawBuildingEffects(0);assert.strictEqual(buildingEffects[0].age,effectAge,'UI redraws do not advance effects');
  drawBuildingEffects(1);assert.strictEqual(buildingEffects.length,0,'Effects expire');
  town.buildings[0].construction={progress:29,total:30};
  const finished={...town,buildings:[{kind:'home',x:4,y:5,level:3}]};
  updateBuildingEffects({world_id:'test',towns:{own:finished}});
  assert.strictEqual(buildingEffects[0].kind,'complete','Polled construction completion triggers feedback');
  updateBuildingEffects({world_id:'other',towns:{}});
  assert.strictEqual(buildingEffects.length,0,'Switching worlds clears feedback');
  console.log('Building visual upgrade checks passed');
`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},assert,marks,require('../web/animations.js'));
