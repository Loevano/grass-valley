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
  return (async()=>{
    function check(condition,message){if(!condition)throw Error(message);}
    const town={name:'Our town',wood:65,food:40,coins:20,level:1,harvested:0,
      buildings:[{kind:'hall',x:5,y:5},{kind:'farm',x:6,y:2},{kind:'home',x:2,y:2}],
      map_resources:[{kind:'water',x:9,y:2}],
      villagers:[{name:'Ada',job:'idle',woodcutting:0,farming:0},{name:'Bea',job:'farming',building_index:1,woodcutting:0,farming:7}]};
    world={world_id:'test',towns:{own:town,friend:town}};identity={town_id:'own'};viewed='own';
    const commands=[];act=async data=>commands.push(data);
    const controls=()=>$('selection-details').children;
    const button=label=>controls().find(c=>c.tag==='button'&&c.textContent.startsWith(label)&&!c.textContent.startsWith('Upgrade to level'));
    const workerRows=()=>controls().find(c=>c.className==='building-workers').children;
    inspect('building',1);
    check(controls().some(c=>c.children?.[0]?.textContent==='1 / 4'),'Farm shows occupied and total spaces');
    check(workerRows().length===1&&workerRows()[0].children[0].children[0].textContent==='Bea','Building lists its assigned worker compactly');
    check(workerRows()[0].children[0].children[1].textContent==='Farming · Lv 7','Farm worker shows relevant skill level');
    check(workerRows()[0].children[1].attributes['aria-label']==='Kick Bea from Farm','Kick control names the worker and building');
    const kick=workerRows()[0].children[1];
    button('Add settler').onclick();
    check(commands.at(-1).action==='assign_settler'&&commands.at(-1).index===0&&commands.at(-1).building_index===1,'Adds selected settler to selected farm');
    kick.onclick();check(commands.at(-1).action==='remove_settler'&&commands.at(-1).index===1,'Kicks the correct worker from the building');
    workerRows()[0].children[0].onclick();check(selection.type==='villager'&&selection.index===1,'Worker name opens individual details');
    inspect('building',1);
    button('Upgrade to').onclick();check(commands.at(-1).action==='upgrade_capacity'&&commands.at(-1).index===1,'Upgrade targets selected building');
    town.villagers[0].building_index=1;render();
    check(workerRows().length===2,'Compact list includes every assigned worker');
    workerRows()[0].children[1].onclick();check(commands.at(-1).index===0,'Each kick control targets its own worker');
    inspect('building',0);check(!!button('Add settler'),'Hall supports assignment');
    town.villagers[0].building_index=0;render();check(button('Add settler').disabled,'Full building disables additions');
    check(workerRows()[0].children[0].children[1].textContent.includes('Fishing · Lv 0'),'Non-workplace assignments show all skills with zero for missing values');
    inspect('building',2);check(controls().some(c=>c.children?.[0]?.textContent==='0 / 2'),'Cottage has two spaces');
    town.wood=0;render();check(button('Upgrade to').disabled,'Upgrade disabled without enough wood');
    town.buildings[2].settler_level=3;render();check(button('Settler capacity fully').disabled,'Max capacity cannot upgrade');
    town.buildings[2].construction={progress:0,total:30};render();check(!button('Add settler'),'Construction sites cannot assign settlers');
    delete town.buildings[2].construction;viewed='friend';inspect('building',1);
    check(!button('Add settler')&&!button('Remove')&&!button('Upgrade to'),'Visitors see capacity without mutation controls');
    check(workerRows().length===1&&workerRows()[0].children.length===1,'Visitors see worker skills without kick controls');
    const before=commands.length;kick.onclick();check(commands.length===before,'Stale kick handlers cannot change a visited town');
    viewed='own';town.level=5;
    world.progression={buildings:{bakery:{job:'baking',level:4}},job_requirements:{baking:{milling:10}},talent_roots:{baking:'farming'},recipes:{},upgrades:{}};
    town.buildings.push({kind:'bakery',x:4,y:5});
    town.villagers[0].milling=9;town.villagers[1].milling=10;
    town.villagers[1].talents={farming:1};
    inspect('building',3);
    const picker=controls().find(c=>c.attributes?.['aria-label']==='Settler to add');
    check(picker.children[1].disabled&&picker.children[1].textContent.includes('Requires 10 milling'),'Unqualified settler shows required personal practice');
    check(!picker.children[0].disabled&&picker.children[0].textContent.includes('1.25× output'),'Qualified settler shows natural output bonus');
    check(picker.value==='1','Picker defaults to an eligible settler');
    button('Add settler').onclick();check(commands.at(-1).index===1&&commands.at(-1).building_index===3,'Bakery assignment targets qualified settler');
    town.villagers.push(
      {name:'Cal',job:'idle',milling:10},
      {name:'Dee',job:'idle',milling:10,talents:{farming:1}},
      {name:'Eli',job:'idle',milling:10,talents:{farming:-1}},
      {name:'Flo',job:'idle',milling:0,talents:{farming:1}}
    );
    render();
    const recommendations=controls().find(c=>c.attributes?.['aria-label']==='Settler to add');
    check(recommendations.children.map(option=>Number(option.value)).join(',')==='3,5,2,4,1,0','Unallocated settlers come first, ordered by profession talent, before reassignment candidates');
    check(recommendations.value==='3','Qualified unallocated talent match is recommended by default');
    check(recommendations.children[1].disabled,'Talent does not bypass profession requirements');
    button('Add settler').onclick();check(commands.at(-1).index===3,'Assignment uses the recommended settler original index');
    town.villagers[3].milling=0;render();
    check(controls().find(c=>c.attributes?.['aria-label']==='Settler to add').value==='2','Unallocated qualified settler beats an assigned talent match');
    town.villagers.splice(2);render();
    town.villagers[1].milling=0;render();check(button('Add settler').disabled,'No qualified settlers disables assignment');
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
