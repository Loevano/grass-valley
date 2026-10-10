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
    const town={name:'Our town',wood:65,food:40,coins:20,level:1,harvested:30,
      buildings:[{kind:'hall',x:5,y:5},{kind:'woodcutter',x:1,y:0}],
      map_resources:[{kind:'tree',x:0,y:0},{kind:'water',x:3,y:8},{kind:'rock',x:0,y:2}],
      villagers:[{name:'Ada',job:'woodcutting',woodcutting:8,farming:2,work_target:{kind:'tree',x:0,y:0}}]};
    const friend=JSON.parse(JSON.stringify(town));friend.name='Friend';friend.level=2;friend.harvested=12;
    world={world_id:'test',towns:{own:town,friend}};identity={town_id:'own'};viewed='own';
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
    town.wood=0;render();check($('upgrade').disabled,'Hall upgrade requires enough wood');town.wood=65;
    await clickTile(3,8);
    check(selection.type==='resource'&&$('selection-title').textContent==='Water','Water tile opens resource information');
    await clickSprite('resource',0);
    check($('selection-title').textContent==='Forest','Clicking treetop selects resource');
    await clickTile(0,2);
    check($('selection-title').textContent==='Rock outcrop','Rock opens mining availability information');
    await clickSprite('villager',0);
    check(selection.type==='villager'&&$('selection-title').textContent==='Ada','Moving villager is selectable');
    const job=$('selection-details').children[0].children.find(child=>child.tag==='select');
    job.value='farming';job.onchange();
    check(commands[0].action==='job'&&commands[0].index===0&&commands[0].job==='farming','Inspector assigns the selected villager job');
    draw(.2);render();check(selection.index===0&&$('selection-title').textContent==='Ada','Selection follows animation and refresh');
    inspect('building',1);
    check($('selection-details').children.some(child=>child.textContent==='1 assigned worker.'),'Workplace lists its assigned workers');
    $('selection-move').onclick();
    check(tool==='move'&&moveIndex===1&&panel==='build','Inspector Move starts with the selected building');
    await clickTile(8,8);
    check(commands[1].action==='move'&&commands[1].index===1&&commands[1].x===8,'Movement sends the selected building and destination');
    $('buildings').children[0].onclick();const [hoverX,hoverY]=point(8,8);canvas.onmousemove({clientX:hoverX,clientY:hoverY+tileH/2});
    listeners.keydown({key:'Escape'});
    check(panel===null&&tool===null&&buildTile===null&&$('side-panel').hidden,'Escape closes panels and cancels placement');
    $('world-menu').open=true;$('help-button').onclick();listeners.keydown({key:'Escape'});
    check(panel==='help','Escape respects the world dialog');$('world-menu').open=false;
    $('valley-button').onclick();$('towns').children[1].onclick();
    check(viewed==='friend'&&panel===null&&$('build-button').disabled,'Visiting resets panels and disables Build');
    await clickSprite('building',0);
    check($('progress').textContent.includes('Level 2')&&$('progress').textContent.includes('12/30'),'Visitor sees the visited hall progress');
    check($('upgrade').disabled&&$('selection-move').hidden,'Visitor cannot upgrade or move buildings');
    await clickSprite('villager',0);
    check($('selection-details').children[0].children.find(child=>child.tag==='select').disabled,'Visitor job controls are disabled');
    $('towns').children[0].onclick();$('villagers-button').onclick();
    town.villagers.push({name:'Bea',job:'idle',woodcutting:1,farming:3});render();
    check($('villagers').children.length===2,'Compact overview includes every villager');
    check($('villagers').children.every(row=>row.children[1].hidden),'Roster starts with collapsed details');
    $('villagers').children[0].children[0].onclick();
    check(selection.type==='villager'&&panel==='villagers','Roster selection expands in place');
    check(!$('villagers').children[0].children[1].hidden&&$('villagers').children[1].children[1].hidden,'Only selected villager expands');
    render();check(!$('villagers').children[0].children[1].hidden,'Refresh preserves expanded villager');
    const rosterJob=$('villagers').children[0].children[1].children[0].children.find(child=>child.tag==='select');
    rosterJob.value='idle';rosterJob.onchange();check(commands.at(-1).index===0&&commands.at(-1).job==='idle','Expanded roster assigns the correct job');
    $('villagers').children[1].children[0].onclick();
    check($('villagers').children[0].children[1].hidden&&!$('villagers').children[1].children[1].hidden,'Selecting another villager collapses the previous one');
    $('villagers').children[1].children[0].onclick();check(selection===null&&$('villagers').children.every(row=>row.children[1].hidden),'Clicking selected row collapses it');
    viewed='friend';render();$('villagers').children[0].children[0].onclick();
    check($('villagers').children[0].children[1].children[0].children.find(child=>child.tag==='select').disabled,'Expanded roster respects visitor restrictions');
    viewed='own';town.villagers.pop();render();
    $('close-panel').onclick();check($('side-panel').hidden,'Close button closes inspector');
    canvas.events.keydown({key:'Enter',preventDefault(){}});
    check(selection.type==='building'&&selection.index===0,'Keyboard Enter inspects the default tile');
    canvas.events.keydown({key:'ArrowUp',preventDefault(){}});
    check(keyboardTile.x===4&&keyboardTile.y===5,'Keyboard arrows navigate tile selection');
    town.villagers=[];inspect('villager',0);
    check(panel===null&&$('side-panel').hidden,'Missing selected object closes inspector safely');
    identity=null;panel=null;render();check(!$('join').hidden,'New settlers see the join form');
    $('help-button').onclick();check($('join').hidden&&!$('help-panel').hidden,'Join form gives way to requested panel');
  })();`);
run(documentMock,{devicePixelRatio:1,addEventListener(key,callback){listeners[key]=callback;}},{getItem:()=>null},()=>0,()=>{},animation,listeners)
  .then(()=>{if(typeof print==='function')print('Selection panel checks passed');else console.log('Selection panel checks passed');})
  .catch(error=>{if(typeof print==='function'){print(String(error));print(error.stack);quit(1);}else{console.error(error);process.exitCode=1;}});
