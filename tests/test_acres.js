// Run with: node tests/test_acres.js
const source=typeof readFile==='function'?readFile('web/app.js'):require('fs').readFileSync('web/app.js','utf8');
const elements=new Map();
function element(){
  return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},
    replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){},showModal(){},close(){}};
}
const context=new Proxy({}, {get:()=>()=>{}});
const documentMock={getElementById(id){
  if(!elements.has(id))elements.set(id,element());
  return elements.get(id);
},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>context;
canvasMock.getBoundingClientRect=()=>({left:0,top:0,width:720,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','animation',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
 function panelNodes(node=$('selection-details')){return Array.from(node.children || []).flatMap(child=>[child,...panelNodes(child)]);}

  return (async()=>{
    function check(condition,message){if(!condition)throw Error(message);}
    const town={name:'Farm town',wood:65,food:40,coins:20,level:1,harvested:0,
      buildings:[{kind:'hall',x:5,y:5},{kind:'farm',x:6,y:2}],
      map_resources:[{kind:'water',x:9,y:2}],villagers:[]};
    world={world_id:'test',towns:{own:town,friend:JSON.parse(JSON.stringify(town))}};
    identity={town_id:'own'};viewed='own';render();
    check(![...$('buildings').children].flatMap(group=>[...group.children]).filter(button=>button.buildingKind).some(b=>b.textContent.startsWith('Acre')),'Acres are built through a farm');
    inspect('building',1);
    const button=panelNodes().find(b=>b.textContent==='Build acre · 5 wood');
    check(button&&!button.disabled,'Farm provides acre construction');
    button.onclick();check(tool==='acre'&&acreFarmIndex===1,'Construction remembers selected farm');
    check(placementValid(town,{x:6,y:3},'acre'),'Nearby open field is valid');
    check(!placementValid(town,{x:2,y:2},'acre'),'Distant fields are invalid');
    const commands=[];
    request=async(path,data)=>{commands.push(data);town.buildings.push({kind:data.kind,x:data.x,y:data.y,farm_index:data.farm_index});town.wood-=5;};
    refresh=async()=>render();
    await selectTile(2,2);check(commands.length===0,'Invalid range sends no build request');
    await selectTile(6,3);
    check(commands.length===1&&commands[0].farm_index===1&&commands[0].kind==='acre','Build sends selected farm');
    check(town.wood===60&&tool==='acre','Build spends wood and allows more acres');
    $('cancel').onclick();inspect('building',2);
    check($('selection-title').textContent==='Acre','Crop field can be inspected');
    town.wood=0;inspect('building',1);
    check(panelNodes().find(b=>b.textContent==='Build acre · 5 wood').disabled,'Acre button requires wood');
    viewed='friend';inspect('building',1);
    check(!panelNodes().some(b=>b.textContent==='Build acre · 5 wood'),'Visitors cannot build acres');
    const field={kind:'acre',x:6,y:3,farm_index:1};
    const farmTown={map_size:12,buildings:[town.buildings[0],town.buildings[1],field],map_resources:town.map_resources};
    const state=animation.plan(farmTown,{job:'farming',work_target:field},0);
    check(state.home===farmTown.buildings[1]&&state.source===field,'Farmers deliver crops to the associated farm');
    for(let i=0;i<300&&state.path.length;i++)animation.advance(state,.1,true);
    check(state.phase==='working','Farmer reaches the crop field');
    viewed='own';tool='move';moveIndex=1;
    check(placementValid(town,{x:6,y:3},'farm'),'Farm moves can overlap the old linked acre footprint');
    const before=JSON.stringify(town.buildings);
    town.map_resources.push({kind:'rock',x:7,y:4});
    check(!placementValid(town,{x:7,y:3},'farm'),'A blocked acre destination invalidates the farm preview');
    town.map_resources.pop();
    check(JSON.stringify(town.buildings)===before,'Farm placement previews preserve authoritative building positions');
    town.buildings.push({kind:'home',x:7,y:4});
    check(!placementValid(town,{x:7,y:3},'farm'),'Unrelated buildings block the acre destinations');
    town.buildings.pop();
    world.shared_map={};
    world.towns.friend.buildings=[];
    world.towns.friend.player={x:7,y:4};
    check(!placementValid(town,{x:7,y:3},'farm'),'A visiting player on an acre destination blocks the farm preview');
  })();`);
const animation=require('../web/animations.js');
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},animation)
  .then(()=>console.log('Farm acre checks passed'))
  .catch(error=>{console.error(error);process.exitCode=1;});
