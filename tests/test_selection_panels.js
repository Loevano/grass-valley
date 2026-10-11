// Run with Node.js or JavaScriptCore's jsc shell.
const read=path=>typeof readFile==='function'?readFile(path):require('fs').readFileSync(path,'utf8');
const source=read('web/app.js');
const animation=new Function(read('web/animations.js')+';return WorkerAnimation;')();
const elements=new Map(),listeners={};
function element(tag){
  return {tag,children:[],style:{},attributes:{},events:{},classList:{toggle(name,enabled){this[name]=enabled;}},
    append(child){this.children.push(child);},replaceChildren(){this.children=[];},
    setAttribute(key,value){this.attributes[key]=value;},
    addEventListener(key,callback){this.events[key]=callback;},focus(){},showModal(){this.open=true;},close(){this.open=false;this.events.close?.();}};
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
    const town={name:'Our town',wood:65,food:40,coins:20,level:1,harvested:30,
      buildings:[{kind:'hall',x:5,y:5},{kind:'woodcutter',x:1,y:0}],
      map_resources:[{kind:'tree',x:0,y:0},{kind:'water',x:3,y:8},{kind:'rock',x:0,y:2}],
      villagers:[{name:'Ada',job:'woodcutting',woodcutting:8,farming:2,work_target:{kind:'tree',x:0,y:0}}]};
    const friend=JSON.parse(JSON.stringify(town));friend.name='Friend';friend.level=2;friend.harvested=12;
    world={world_id:'test',hall_ready:{own:true,friend:false},hall_upgrades:[{level:2,harvested:30,wood:40,seconds:30,coins:30},{level:3,harvested:90,wood:80,seconds:60,coins:60}],towns:{own:town,friend}};identity={town_id:'own'};viewed='own';
    const commands=[];act=async data=>{commands.push(data);};
    function clickTile(x,y){const [px,py]=point(x,y);return canvas.onclick({clientX:px,clientY:py+tileH/2});}
    async function clickSprite(type,index){const hit=hitTargets.find(h=>h.type===type&&h.index===index);await canvas.onclick({clientX:(hit.left+hit.right)/2,clientY:hit.top+3});}
    render();
    check($('side-panel').hidden&&!$('resource-totals').hidden,'Start with the map and resource bar visible');
    for(const name of ['build','villagers','valley','help']){
      $(name+'-button').onclick();
      check(panel===name&&!$('side-panel').hidden,'Toolbar opens '+name);
      for(const other of ['selection','villagers','valley','help'])check($(other+'-panel').hidden===(other!==name),'Only requested panel is visible');
      check($('controls').hidden===(name!=='build'),'Build controls only appear in Build');
      $(name+'-button').onclick();check($('side-panel').hidden,'Toolbar toggles panel closed');
    }
    await clickSprite('building',0);
    check(selection.type==='building'&&$('selection-title').textContent==='Town hall','Clicking the roof selects a building');
    check(!$('hall-details').hidden&&!$('upgrade').disabled,'Owner sees available hall upgrade');
    check($('selection-destroy').hidden,'Town hall has no destroy button');
    function rangeFact(){return panelNodes().find(child=>child.children?.[1]?.textContent==='Building range');}
    check(rangeFact().children[0].textContent==='10 tile steps','Hall panel displays its current building range');
    town.level=2;render();
    check(rangeFact().children[0].textContent==='15 tile steps','Hall panel reflects the expanded range');
    town.level=1;render();
    town.wood=0;render();check(!$('upgrade').disabled,'Hall preview remains available without wood');
    $('upgrade').onclick();check($('upgrade-dialog').open&&$('confirm-upgrade').disabled,'Unaffordable hall opens popup with confirmation disabled');
    $('confirm-upgrade').onclick();check(!commands.length,'Unaffordable hall sends no command');$('cancel-upgrade').onclick();town.wood=65;
    town.level=2;town.harvested=90;town.wood=80;render();
    check(!$('upgrade').disabled&&$('upgrade').title.includes('level 3'),'Level 2 owners can upgrade again');
    check($('hall-progress').hidden&&$('progress').textContent.includes('60 coins'),'Next milestone and reward use host requirements');
    world.hall_ready.own=false;render();check(!$('upgrade').disabled,'Hall preview remains available before its milestone');
    $('upgrade').onclick();check($('upgrade-dialog').open&&$('confirm-upgrade').disabled&&!$('upgrade-error').hidden,'Unmet hall milestone blocks confirmation and shows a reason');
    $('confirm-upgrade').onclick();check(!commands.length,'Unmet hall milestone sends no command');$('cancel-upgrade').onclick();
    world.hall_ready.own=true;town.wood=79;render();check(!$('upgrade').disabled,'Later hall preview remains available before its wood cost is met');
    town.wood=80;town.upgrade_remaining=57;render();
    check($('upgrade').disabled&&$('upgrade').title.includes('57s remaining'),'Pending upgrade shows timer and prevents repeat clicks');
    town.upgrade_remaining=0;town.level=10;render();
    check($('upgrade').disabled&&$('hall-progress').hidden&&$('progress').textContent.includes('planned'),'Maximum level hides milestone and disables upgrade');
    town.level=1;town.harvested=30;town.wood=65;render();
    check($('progress').textContent.includes('Level 1'),'Returning to a hall restores its development requirements');
    await clickTile(3,8);
    check(selection.type==='resource'&&$('selection-title').textContent==='Water','Water tile opens resource information');
    await clickSprite('resource',0);
    check($('selection-title').textContent==='Forest','Clicking treetop selects resource');
    await clickTile(0,2);
    check($('selection-title').textContent==='Rock outcrop','Rock opens mining availability information');
    await clickSprite('villager',0);
    check(selection.type==='villager'&&$('selection-title').textContent==='Ada','Moving villager is selectable');
    const profile=$('selection-details').children[0].children.find(child=>child.tag==='dl');
    function profileValue(label){const i=profile.children.findIndex(child=>child.textContent===label);return profile.children[i+1].textContent;}
    check(profileValue('Name')==='Ada'&&profileValue('Job')==='Woodcutting','Villager popup labels name and job');
    check(profileValue('House type')==='Town hall · shared shelter','Town without cottages shows shared shelter');
    check(profileValue('Talent')==='No special talent','Missing talents have a readable fallback');
    town.buildings.push({kind:'home',x:7,y:7,level:2});town.villagers[0].talents={woodcutting:1,farming:-1};render();
    const updatedProfile=$('selection-details').children[0].children.find(child=>child.tag==='dl');
    check(updatedProfile.children.some(child=>child.textContent==='Shared cottages · level 2'),'Housing shows completed cottage levels');
    check(updatedProfile.children.some(child=>child.textContent==='Woodcutting'),'Profile shows the positive talent');
    town.buildings.pop();
    const job=$('selection-details').children[0].children.find(child=>child.tag==='select');
    job.value='farming';job.onchange();
    check(commands[0].action==='job'&&commands[0].index===0&&commands[0].job==='farming','Inspector assigns the selected villager job');
    draw(.2);render();check(selection.index===0&&$('selection-title').textContent==='Ada','Selection follows animation and refresh');
    await clickSprite('villager',0);
    check(selection===null&&panel===null&&$('side-panel').hidden,'Clicking the selected villager toggles the popup closed');
    await clickSprite('villager',0);
    check(selection.type==='villager'&&panel==='selection','Clicking again reopens the villager popup');
    inspect('building',1);
    check(!$('selection-destroy').hidden,'Owner can destroy a regular building');
    check(panelNodes().some(child=>child.textContent==='1 assigned worker.'),'Workplace lists its assigned workers');
    check($('selection-details').children[0].children[0].textContent==='Storage','Storage appears first');
    const workerSection=$('selection-details').children[1];
    check(workerSection.children[0].textContent==='Workers','Workers appear below storage');
    const assignment=workerSection.children.find(child=>child.className==='worker-assignment');
    check(assignment.children[0].tag==='select'&&assignment.children[1].textContent==='Add worker','Worker picker and add action share one row');
    check(workerSection.children.at(-1).className==='workplace-status','Working status follows assignment');
    check(!panelNodes().some(child=>child.textContent?.includes('Food space is shared')),'Storage omits the long explanatory text');
    const upgradeActions=panelNodes().find(child=>child.className==='building-upgrade-actions');
    check(upgradeActions.children.every(child=>child.className==='upgrade-card'),'Level and capacity use consistent upgrade cards');
    check(upgradeActions.children.map(child=>child.children.at(-1).textContent).join(',')==='Lvl,Capacity','Level card stays before capacity card');
    check($('selection-level').textContent==='Lv 1','Building level is a compact header badge');
    const folds=()=>panelNodes().filter(child=>child.tag==='details');
    check(folds().every(child=>!child.open),'Secondary details start collapsed');
    const detailsFold=folds().find(child=>child.children[0].textContent==='Building details');
    detailsFold.open=true;detailsFold.events.toggle();render();
    check(folds().find(child=>child.children[0].textContent==='Building details').open,'Refresh preserves expanded details');
    inspect('building',0);inspect('building',1);
    check(folds().every(child=>!child.open),'Selecting another building resets expanded sections');
    $('selection-move').onclick();
    check(tool==='move'&&moveIndex===1&&panel==='build','Inspector Move starts with the selected building');
    await clickTile(11,10);
    check(moveIndex===1&&commands.length===1&&$('notice').textContent.includes('within 10 tile steps'),'Out-of-range moves keep selection and show the limit');
    await clickTile(8,8);
    check(commands[1].action==='move'&&commands[1].index===1&&commands[1].x===8,'Movement sends the selected building and destination');
    [...$('buildings').children].flatMap(group=>[...group.children]).filter(button=>button.buildingKind)[0].onclick();const [hoverX,hoverY]=point(8,8);canvas.onmousemove({clientX:hoverX,clientY:hoverY+tileH/2});
    listeners.keydown({key:'Escape'});
    check(panel===null&&tool===null&&buildTile===null&&$('side-panel').hidden,'Escape closes panels and cancels placement');
    $('world-menu').open=true;$('help-button').onclick();listeners.keydown({key:'Escape'});
    check(panel==='help','Escape respects the world dialog');$('world-menu').open=false;
    $('valley-button').onclick();$('towns').children[1].onclick();
    check(viewed==='friend'&&panel===null&&$('build-button').disabled,'Visiting resets panels and disables Build');
    await clickSprite('building',0);
    check($('progress').textContent.includes('Level 2')&&$('progress').textContent.includes('Still needed'),'Visitor sees the visited hall progress');
    check($('upgrade').disabled&&$('selection-move').hidden,'Visitor cannot upgrade or move buildings');
    inspect('building',1);check($('selection-destroy').hidden,'Visitor cannot destroy buildings');
    await clickSprite('villager',0);
    check($('selection-details').children[0].children.find(child=>child.tag==='select').disabled,'Visitor job controls are disabled');
    $('towns').children[0].onclick();$('villagers-button').onclick();
    town.villagers[0].talents={woodcutting:1,farming:-1};
    town.villagers.push({name:'Bea',job:'idle',woodcutting:1,farming:3});render();
    check($('side-panel').classList['villagers-overview'],'Villagers uses the wide popup layout');
    const rosterRows=()=>$('villagers').children[0].children[1].children;
    const table=$('villagers').children[0];
    check(table.tag==='table'&&table.attributes['aria-label']==='Villagers overview','Overview uses an accessible table');
    check(table.children[0].children[0].children.map(cell=>cell.textContent).join(',')==='Name,Job,Housing,Skills,Talents,Work status','Table labels comparison columns');
    check(rosterRows().length===2,'Overview includes every villager');
    for(const row of rosterRows()){
      check(row.tag==='tr'&&row.children[0].attributes.scope==='row','Each villager has a table row header');
      check(row.children[3].children[0].className==='villager-skills','Skills use a compact icon group');
      check(!row.children[1].children[1].disabled,'Every owner row has a job control');
      check(row.children[2].textContent==='Shared, lvl 1','Housing is compact in the table');
      check(row.children[5].children.length>=1,'Work status is visible in the table');
    }
    const skillIcons=rosterRows()[0].children[3].children[0].children;
    check(skillIcons.length===2,'Zero and missing skills are hidden');
    check(skillIcons[0].attributes['aria-label']==='Woodcutting · 8 practice','Skill icon has an accessible skill name and value');
    check(skillIcons[0].children[0].children[1].style.clipPath==='inset(20% 0 0)','Skill artwork fills proportionally to its meter scale');
    town.villagers[1].baking=15;render();
    const specialist=rosterRows()[1].children[3].children[0].children;
    check(specialist.some(icon=>icon.title==='Baking · 15 practice'),'Advanced skills have icons too');
    town.villagers[1].woodcutting=0;town.villagers[1].farming=0;town.villagers[1].baking=0;render();
    check(rosterRows()[1].children[3].children[0].children.length===0,'An untrained villager shows no skill icons');
    check(rosterRows()[0].children[4].children[0].textContent.includes('Weakness: Farming'),'Table preserves talent strengths and weaknesses');
    check(rosterRows()[1].children[4].children[0].textContent==='No special talent','Empty talents have a readable fallback');
    render();check(rosterRows().length===2,'Refresh keeps every row visible');
    const rosterJob=rosterRows()[1].children[1].children[1];
    rosterJob.value='woodcutting';rosterJob.onchange();check(commands.at(-1).index===1&&commands.at(-1).job==='woodcutting','Overview assigns the correct villager job');
    viewed='friend';render();
    check(rosterRows().every(row=>row.children[1].children[1].disabled),'Overview respects visitor restrictions');
    viewed='own';town.villagers.pop();render();
    listeners.keydown({key:'Escape'});check($('side-panel').hidden,'Escape closes inspector');
    check(!$('side-panel').classList['villagers-overview'],'Closing clears the wide layout');
    const residents=town.villagers;town.villagers=[];openPanel('villagers');
    check($('villagers').children[0].textContent.includes('No villagers yet'),'Empty roster explains how to welcome residents');
    town.villagers=residents;openPanel(null);
    canvas.events.keydown({key:'Enter',preventDefault(){}});
    check(selection.type==='building'&&selection.index===0,'Keyboard Enter inspects the default tile');
    canvas.events.keydown({key:'ArrowUp',preventDefault(){}});
    check(keyboardTile.x===4&&keyboardTile.y===5,'Keyboard arrows navigate tile selection');
    town.villagers=[];inspect('villager',0);
    check(panel===null&&$('side-panel').hidden,'Missing selected object closes inspector safely');
    identity=null;panel=null;render();check(!$('join').hidden,'New settlers see the join form');
    $('help-button').onclick();check($('join').hidden&&!$('help-panel').hidden,'Join form gives way to requested panel');
    identity={town_id:'own'};town.player={x:9,y:9,facing:1};viewed='own';render();
    const [playerX,playerY]=point(playerView.x,playerView.y);
    check(Math.abs(playerX-480)<1&&Math.abs(playerY+tileH/2-330)<1,'Player appears at the center of the map');
    check(hitTargets.some(hit=>hit.type==='player'),'Centered player is drawn and selectable');
    delete town.player;
    identity={town_id:'own'};viewed='own';
    let release,fail=false;
    request=async(path,data)=>{
      commands.push(data);
      await new Promise(resolve=>{release=resolve;});
      if(fail)throw Error('Destroy rejected');
      town.buildings.splice(data.index,1);
    };
    refresh=async()=>render();
    inspect('building',0);
    const before=commands.length;
    await $('selection-destroy').onclick();
    check(commands.length===before,'Town hall protection also guards the button handler');
    viewed='friend';inspect('building',1);await $('selection-destroy').onclick();
    check(commands.length===before,'Visitor handler cannot send a destroy command');
    viewed='own';inspect('building',1);fail=true;
    const rejected=$('selection-destroy').onclick();release();await rejected;
    check(selection.index===1&&town.buildings.length===2&&!$('selection-destroy').disabled,'Rejected destruction keeps selection and allows retry');
    check($('notice').textContent==='Destroy rejected','Destroy errors are shown');
    fail=false;const pending=$('selection-destroy').onclick();
    await $('selection-destroy').onclick();
    check(commands.length===before+2,'Repeated clicks cannot destroy another building while pending');
    check(commands.at(-1).action==='destroy'&&commands.at(-1).index===1&&commands.at(-1).world_id==='test','Destroy sends selected building and world');
    release();await pending;
    check(town.buildings.length===1&&selection===null&&panel===null,'Successful destruction removes building and closes selection');
    act=async data=>{commands.push(data);};
    town.buildings.push({kind:'home',x:6,y:3});town.wood=65;
    inspect('building',1);
    function buildingUpgrade(){return panelNodes().find(child=>child.tag==='button'&&child.textContent==='Lvl');}
    check(buildingUpgrade().title.includes('20 wood')&&buildingUpgrade().title.includes('4 spaces'),'Cottage upgrade previews cost and housing');
    buildingUpgrade().onclick();$('confirm-upgrade').onclick();check(commands.at(-1).action==='upgrade_building'&&commands.at(-1).index===1,'Upgrade targets the selected building');
    town.wood=19;render();check(!buildingUpgrade().disabled,'Building preview remains available without wood');
    const beforeUpgrade=commands.length;buildingUpgrade().onclick();
    check($('upgrade-dialog').open&&$('confirm-upgrade').disabled,'Unaffordable building opens popup with confirmation disabled');
    $('confirm-upgrade').onclick();check(commands.length===beforeUpgrade,'Unaffordable building sends no command');$('cancel-upgrade').onclick();
    town.wood=65;town.buildings[1].construction={progress:0,total:30};render();check(buildingUpgrade().disabled,'Unfinished buildings cannot upgrade');
    delete town.buildings[1].construction;town.buildings[1].level=3;render();check(buildingUpgrade().disabled&&buildingUpgrade().title.includes('Fully upgraded'),'Level 3 is the building limit');
    friend.buildings[1].level=1;viewed='friend';inspect('building',1);check(buildingUpgrade().disabled,'Visitor cannot upgrade buildings');
    town.buildings.push({kind:'storehouse',level:3,x:7,y:3});check(storageCapacity(town)===500+50*town.buildings.filter(b=>!['hall','storehouse','home'].includes(b.kind)&&!b.construction).length,'Client storage reflects upgraded storehouses');
    identity=null;render();
  })();`);
run(documentMock,{devicePixelRatio:1,addEventListener(key,callback){listeners[key]=callback;}},{getItem:()=>null},()=>0,()=>{},animation,listeners)
  .then(()=>{if(typeof print==='function')print('Selection panel checks passed');else console.log('Selection panel checks passed');})
  .catch(error=>{if(typeof print==='function'){print(String(error));print(error.stack);quit(1);}else{console.error(error);process.exitCode=1;}});
