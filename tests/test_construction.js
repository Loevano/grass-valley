const assert=require('assert'),fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8');
const animation=require('../web/animations.js');
const elements=new Map(),fills=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){},showModal(){},close(){}};}
const context=new Proxy({}, {get:()=>()=>{}});
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvas=documentMock.getElementById('world');canvas.getContext=()=>context;
canvas.getBoundingClientRect=()=>({left:0,top:0,width:720,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation','assert','fills',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  function panelNodes(node=$('selection-details')){return Array.from(node.children || []).flatMap(child=>[child,...panelNodes(child)]);}

  const site={kind:'home',x:6,y:6,construction:{progress:0,total:30}};
  const town={name:'Builders',wood:40,food:40,coins:20,level:1,harvested:0,
    buildings:[{kind:'hall',x:5,y:5},site],map_resources:[],villagers:[
      {name:'Ada',job:'idle',woodcutting:0,farming:0,fishing:0,construction_target:{kind:'home',x:6,y:6}}]};
  world={world_id:'test',towns:{own:town}};identity={town_id:'own'};viewed='own';
  inspect('building',1);
  assert(panelNodes().some(c=>c.textContent?.includes('Waiting for a builder')));
  assert(panelNodes().some(c=>c.max===30&&c.value===0),'Construction has accessible progress');
  assert(!panelNodes().some(c=>c.textContent==='+2'),'Unfinished home has no housing benefit');
  inspect('villager',0);
  const card=$('selection-details').children[0];
  assert(card.children.some(c=>c.className==='villager-activity'&&c.textContent==='Walking to construction'));
  assert(card.children.some(c=>c.children.some(o=>o.value==='building')),'Builder job is selectable');
  assert(card.children.find(c=>c.className==='villager-profile').children.some(c=>c.textContent==='Builder (automatic)'));
  const row=villagerRow(town,town.villagers[0],0);
  assert(row.children[1].children.some(c=>c.value==='idle'),'Automatic helping preserves the selected resting job');
  assert(row.children[5].children.some(c=>c.textContent==='Walking to construction'),'Overview shows the automatic construction activity');
  polygon=(_,color)=>fills.push(color);
  for(const progress of [0,12,24]){
    site.construction.progress=progress;fills.length=0;drawBuilding(site,town);
    if(progress===0)assert(!fills.includes('#b69a7044'),'Waiting site has foundations only');
    if(progress===12)assert(fills.includes('#b69a7044'),'Middle stage has timber framing');
    if(progress===24)assert(fills.includes('#80574b'),'Final stage adds the roof');
  }
  const worker=town.villagers[0];
  const state=WorkerAnimation.plan(town,worker,0);
  for(let i=0;i<100&&state.path.length;i++)WorkerAnimation.advance(state,.1,true);
  assert.strictEqual(state.phase,'working');
  WorkerAnimation.advance(state,15,true);
  assert.strictEqual(state.phase,'working','Builders remain at the site rather than delivering resources');
  worker.construction_target=null;
  assert(!WorkerAnimation.plan(town,worker,0,state).construction,'Completion releases idle helpers');
  worker.job='hauling';worker.construction_target={kind:'home',x:6,y:6};
  worker.logistics_target={x:8,y:6};worker.logistics_trip={path:[[7,6],[8,6]],cargo:{wood:5}};
  const helper=WorkerAnimation.plan(town,worker,0);
  assert(helper.construction&&!helper.transport,'Workplace helpers show construction while their delivery is paused');
  worker.construction_target=null;
  assert(WorkerAnimation.plan(town,worker,0).transport,'Delivery animation resumes after helping');
  delete worker.logistics_target;delete worker.logistics_trip;worker.job='idle';
  const surrounded={...town,buildings:[town.buildings[0],
    {kind:'farm',x:8,y:8,construction:{progress:0,total:30,upgrade:{settler_level:2}}},
    ...[[7,8],[9,8],[8,7],[8,9]].map(([x,y])=>({kind:'acre',x,y}))]};
  const builder={job:'building',construction_target:{kind:'farm',x:8,y:8}};
  const farmPlan=WorkerAnimation.plan(surrounded,builder,0);
  assert.notStrictEqual(farmPlan.phase,'waiting','Builders can reach farms surrounded by completed acres');
  assert(farmPlan.path.some(p=>surrounded.buildings.some(b=>b.kind==='acre'&&b.x===p.x&&b.y===p.y)),
    'Construction animation crosses fields to reach the farm');
  site.kind='farm';inspect('building',1);
  assert(!panelNodes().some(c=>c.textContent==='Build acre · 5 wood'),'Unfinished farms cannot add acres');
  assert(!acrePlacementValid(town,{x:6,y:7},1));
  site.kind='home';site.level=1;site.construction={progress:12,total:30,upgrade:{level:2}};
  inspect('building',1);
  assert(panelNodes().some(c=>c.textContent?.includes('Upgrading · 40% complete · 18s remaining')));
  assert(panelNodes().some(c=>c.className==='upgrade-button'&&c.disabled),'Repeated upgrades are disabled');
  site.kind='workshop';world.progression={buildings:{workshop:{}},recipes:{},upgrades:{}};
  inspect('building',1);
  assert(panelNodes().some(c=>c.textContent?.includes('18s remaining')),'Profession buildings retain upgrade progress');
  console.log('Construction UI and animation checks passed');
`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},animation,assert,fills);
