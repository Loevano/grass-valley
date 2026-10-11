// Run with Node.js or JavaScriptCore's jsc shell.
const source=typeof readFile==='function'?readFile('web/app.js'):require('fs').readFileSync('web/app.js','utf8');
const elements=new Map(),fills=[],listeners={};
function element(tag){return {tag,children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){},focus(){}};}
const context=new Proxy({fill(){fills.push(this.fillStyle);},fillRect(){fills.push(this.fillStyle);}}, {get:(target,key)=>key in target?target[key]:()=>{}});
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>context;
canvasMock.getBoundingClientRect=()=>({left:0,top:0,width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','fills','listeners',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  return (async()=>{
    function check(condition,message){if(!condition)throw Error(message);}
    const town={name:'Locals',wood:65,food:40,coins:20,level:1,harvested:0,
      buildings:[{kind:'hall',x:5,y:5}],map_resources:[],villagers:[],
      terrain:Array(12).fill('bbbbbbbbbbbb'),npcs:[
        {name:'Moss',role:'Swamp herbalist',dialogue:'Look among the reeds.',biome:'b',x:7,y:7},
        {name:'Sylva',role:'Forest ranger',dialogue:'Keep the forest healthy.',biome:'f',x:3,y:3}]};
    world={world_id:'test',towns:{own:town,friend:town}};identity={town_id:'own'};viewed='own';
    animatedVillagers=()=>[];let commands=0;act=async()=>{commands++;};render();
    check(fills.includes('#586b4b')&&fills.includes('#416951'),'NPC clothing matches swamp and forest themes');
    const hit=hitTargets.find(hit=>hit.type==='npc'&&hit.index===0);
    check(hit,'NPCs have clickable sprite bounds');
    await canvas.onclick({clientX:(hit.left+hit.right)/2,clientY:hit.top+3});
    check(selection.type==='npc'&&$('selection-title').textContent==='Moss','Clicking NPC selects it');
    check($('selection-details').children.some(child=>child.textContent==='Swamp herbalist'),'Inspector shows role');
    check($('selection-details').children.some(child=>child.textContent?.includes('Look among the reeds.')),'Inspector shows dialogue');
    check($('selection-move').hidden&&$('hall-details').hidden,'NPC inspector has no building controls');
    check(!$('selection-details').children.some(child=>child.tag==='select'),'NPC inspector has no worker controls');
    draw(.2);check($('selection-title').textContent==='Moss','Idle animation preserves selection');
    tool='home';await selectTile(7,7);check(commands===0,'Cannot build on a local');
    check(!placementValid(town,{x:7,y:7},'home'),'NPC tile has no valid build preview');
    tool=null;viewed='friend';render();await selectTile(3,3);
    check($('selection-title').textContent==='Sylva','Visitors can inspect NPCs');
    viewed='own';town.player={x:6,y:7};openPanel(null);
    const key=(key,extra={})=>listeners.keydown({key,preventDefault(){},...extra});
    walkKeys.add('w');key('I');
    check(selection?.type==='npc'&&selection.index===0&&walkKeys.size===0,'I opens nearby NPC and releases walking keys');
    town.npcs.push({...town.npcs[1],name:'Closer local',x:6,y:7.5});key('i');
    check(selection.index===2,'Interaction selects the nearest NPC');
    openPanel(null);town.player.x=11;key('i');
    check(selection===null&&$('notice').textContent==='Walk beside a local to interact.','Distant NPCs cannot be interacted with');
    town.player.x=6;
    for(const extra of [{repeat:true},{ctrlKey:true},{metaKey:true},{altKey:true}])key('i',extra);
    check(selection===null,'Held and modified interaction shortcuts are ignored');
    document.activeElement={tagName:'INPUT'};key('i');check(selection===null,'Typing does not interact');document.activeElement=null;
    $('settings-panel').open=true;key('i');check(selection===null,'Dialogs block interaction');$('settings-panel').open=false;
    tool='home';key('i');check(selection===null,'Placement blocks interaction');tool=null;
    keybindings.interact='o';key('i');check(selection===null,'Old interaction binding no longer opens NPC');key('o');
    check(selection.index===2,'Rebound interaction key opens NPC');
    openPanel(null);world.shared_map={npcs:town.npcs};viewed='friend';key('o');
    check(selection.index===2,'Shared map interaction uses the owner player while viewing another town');
    delete world.shared_map;
    delete town.npcs;render();check(selection===null,'Missing selected NPC closes inspector safely');
    viewed='own';key('o');check(selection===null,'Missing NPC list is safe to interact with');
  })();`);
run(documentMock,{devicePixelRatio:1,addEventListener(type,callback){listeners[type]=callback;}},{getItem:()=>null},()=>0,()=>{},fills,listeners)
  .then(()=>{if(typeof print==='function')print('NPC selection checks passed');else console.log('NPC selection checks passed');})
  .catch(error=>{if(typeof print==='function'){print(String(error));print(error.stack);quit(1);}else{console.error(error);process.exitCode=1;}});
