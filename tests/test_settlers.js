// Run with Node.js or JavaScriptCore's jsc shell.
const read=path=>typeof readFile==='function'?readFile(path):require('fs').readFileSync(path,'utf8');
const source=read('web/app.js');
const animation=new Function(read('web/animations.js')+';return WorkerAnimation;')();
const elements=new Map(),listeners={};
function element(tag){
  return {tag,children:[],style:{},attributes:{},events:{},classList:{toggle(){}},
    append(child){this.children.push(child);},replaceChildren(){this.children=[];},
    setAttribute(key,value){this.attributes[key]=value;},
    addEventListener(key,callback){this.events[key]=callback;},focus(){},showModal(){},close(){}};
}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>new Proxy({}, {get:()=>()=>{}});
canvasMock.getBoundingClientRect=()=>({left:0,top:0,width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation','listeners',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  function panelNodes(node=$('selection-details')){return Array.from(node.children || []).flatMap(child=>[child,...panelNodes(child)]);}

  return (async()=>{
    function check(condition,message){if(!condition)throw Error(message);}
    const town={name:'Our town',wood:65,food:40,coins:20,level:1,harvested:0,
      buildings:[{kind:'hall',x:5,y:5},{kind:'farm',x:6,y:2},{kind:'home',x:2,y:2}],
      map_resources:[{kind:'water',x:9,y:2}],
      villagers:[{name:'Ada',job:'idle',woodcutting:0,farming:0},{name:'Bea',job:'farming',building_index:1,woodcutting:0,farming:7}]};
    world={world_id:'test',towns:{own:town,friend:town}};identity={town_id:'own'};viewed='own';
    const commands=[];act=async data=>commands.push(data);
    const controls=()=>panelNodes();
    const button=label=>controls().find(c=>c.tag==='button'&&c.textContent?.startsWith(label)&&!c.textContent?.startsWith('Upgrade to level'));
    const capacityUpgrade=()=>controls().find(c=>c.tag==='button'&&/^Upgrade (worker|settler) capacity:/.test(c.attributes['aria-label'] || ''));
    const workerRows=()=>controls().find(c=>c.className==='building-workers').children;
    inspect('building',1);
    check(controls().some(c=>c.children?.[0]?.textContent==='1 / 4'),'Farm shows occupied and total spaces');
    check(workerRows().length===1&&workerRows()[0].children[0].children[0].textContent==='Bea','Building lists its assigned worker compactly');
    check(workerRows()[0].children[0].children[1].textContent==='Farming · Lv 7','Farm worker shows relevant skill level');
    check(workerRows()[0].children[1].attributes['aria-label']==='Kick Bea from Farm','Kick control names the worker and building');
    const kick=workerRows()[0].children[1];
    button('Add worker').onclick();
    check(commands.at(-1).action==='assign_settler'&&commands.at(-1).index===0&&commands.at(-1).building_index===1,'Adds selected settler to selected farm');
    kick.onclick();check(commands.at(-1).action==='remove_settler'&&commands.at(-1).index===1,'Kicks the correct worker from the building');
    workerRows()[0].children[0].onclick();check(selection.type==='villager'&&selection.index===1,'Worker name opens individual details');
    inspect('building',1);
    capacityUpgrade().onclick();$('confirm-upgrade').onclick();check(commands.at(-1).action==='upgrade_capacity'&&commands.at(-1).index===1,'Upgrade targets selected building');
    town.villagers[0].building_index=1;render();
    check(workerRows().length===2,'Compact list includes every assigned worker');
    workerRows()[0].children[1].onclick();check(commands.at(-1).index===0,'Each kick control targets its own worker');
    inspect('building',0);
    check(!button('Add settler')&&!capacityUpgrade(),'Hall has no resident assignment or capacity upgrade controls');
    check(button('You'),'Hall identifies the controllable character as its resident');
    button('You').onclick();check(selection.type==='player','Hall resident opens the controllable character profile');
    town.buildings[0].settler_level=3;inspect('building',0);
    check(!button('Add settler'),'Legacy upgraded halls still reject resident assignment');
    town.villagers[0].building_index=2;
    inspect('building',2);
    check(workerRows()[0].children[0].children[1].textContent.includes('Fishing · Lv 0'),'Non-workplace assignments show all skills with zero for missing values');
    delete town.villagers[0].building_index;
    inspect('building',2);check(controls().some(c=>c.children?.[0]?.textContent==='0 / 2'),'Cottage has two spaces');
    town.villagers[0].building_index=2;
    town.wood=0;render();check(!capacityUpgrade().disabled,'Capacity preview available without enough wood');
    const beforeUpgrade=commands.length;capacityUpgrade().onclick();check($('confirm-upgrade').disabled,'Unaffordable capacity disables confirmation');
    $('confirm-upgrade').onclick();check(commands.length===beforeUpgrade,'Unaffordable capacity sends no command');$('cancel-upgrade').onclick();
    town.buildings[2].settler_level=3;render();check(!capacityUpgrade()&&controls().some(c=>c.className==='upgrade-card-status'&&c.textContent==='Maxed'),'Max capacity shows a status without an action');
    town.buildings[2].construction={progress:0,total:30};render();check(!button('Add worker'),'Construction sites cannot assign settlers');
    delete town.buildings[2].construction;viewed='friend';inspect('building',1);
    check(!button('Add worker')&&!button('Remove')&&!capacityUpgrade(),'Visitors see capacity without mutation controls');
    check(workerRows().length===1&&workerRows()[0].children.length===1,'Visitors see worker skills without kick controls');
    inspect('building',0);check(button('Town founder')&&!button('Add settler'),'Visited halls show their founder without assignment controls');
    const before=commands.length;kick.onclick();check(commands.length===before,'Stale kick handlers cannot change a visited town');
    viewed='own';town.level=5;
    world.progression={buildings:{bakery:{job:'baking',level:4}},job_requirements:{baking:{milling:10}},talent_roots:{baking:'farming'},recipes:{},upgrades:{}};
    town.buildings.push({kind:'bakery',x:4,y:5});
    town.villagers[0].milling=9;town.villagers[1].milling=10;
    town.villagers[1].talents={farming:1};
    inspect('building',3);
    const picker=controls().find(c=>c.attributes?.['aria-label']==='Settler to add');
    check(picker.children[0].disabled&&picker.children[0].textContent.includes('Requires 10 milling'),'Unqualified settler shows required personal practice');
    check(picker.children[0].textContent.includes('Resting'),'Resting cottage residents appear first and are labeled in the worker picker');
    check(!picker.children[1].disabled&&picker.children[1].textContent.includes('1.25×')&&!picker.children[1].textContent.includes('output'),'Qualified settler shows natural output bonus without the output suffix');
    check(picker.value==='1','Picker defaults to an eligible settler');
    button('Add worker').onclick();check(commands.at(-1).index===1&&commands.at(-1).building_index===3,'Bakery assignment targets qualified settler');
    town.villagers.push(
      {name:'Cal',job:'idle',milling:10},
      {name:'Dee',job:'idle',milling:10,talents:{farming:1}},
      {name:'Eli',job:'idle',milling:10,talents:{farming:-1}},
      {name:'Flo',job:'idle',milling:0,talents:{farming:1}}
    );
    render();
    const recommendations=controls().find(c=>c.attributes?.['aria-label']==='Settler to add');
    check(recommendations.children.map(option=>Number(option.value)).join(',')==='3,5,2,4,0,1','Resting settlers come first, with unallocated talent matches before cottage residents and working reassignment candidates');
    check(recommendations.value==='3','Qualified unallocated talent match is recommended by default');
    check(recommendations.children[1].disabled,'Talent does not bypass profession requirements');
    button('Add worker').onclick();check(commands.at(-1).index===3,'Assignment uses the recommended settler original index');
    town.villagers[3].milling=0;render();
    check(controls().find(c=>c.attributes?.['aria-label']==='Settler to add').value==='2','Unallocated qualified settler beats an assigned talent match');
    town.villagers.splice(2);render();
    town.villagers[1].milling=0;render();check(button('Add worker').disabled,'No qualified settlers disables assignment');
    const card=villagerCard(town,town.villagers[0],0);
    const jobs=card.children.find(c=>c.tag==='select');
    check(jobs.children.find(c=>c.value==='baking').disabled,'Villager job control also blocks unqualified baking');
    const assigned={...town.villagers[1],work_target:{kind:'water',x:9,y:2}};
    check(WorkerAnimation.plan(town,assigned,1).home===town.buildings[1],'Animation uses explicitly selected workplace');
    assigned.job='idle';assigned.building_index=2;assigned.work_target=null;
    check(WorkerAnimation.plan(town,assigned,1).home===town.buildings[2],'Resting settlers return to their assigned cottage');
  })();
`);

run(documentMock,{devicePixelRatio:1,addEventListener(key,callback){listeners[key]=callback;}},{getItem:()=>null},()=>0,()=>{},animation,listeners)
  .then(()=>console.log('Building settler controls passed'))
  .catch(error=>{console.error(error);process.exitCode=1;});
