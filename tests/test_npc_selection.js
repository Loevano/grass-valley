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
    check(!$('selection-details').children.some(child=>child.textContent==='Swamp herbalist'||child.textContent?.includes('Look among the reeds.')),'Unknown local hides role and offer dialogue');
    check(!$('selection-details').children.some(child=>child.textContent?.startsWith('Buy ')||child.textContent?.startsWith('Accept ')),'Unknown local hides offers');
    town.player={x:6,y:7};render();
    let talk=$('selection-details').children.find(child=>child.textContent==='Talk');
    check(talk&&!talk.disabled,'Nearby unknown local offers Talk');
    let talkCommand;act=async data=>{talkCommand=data;};await talk.onclick();
    check(talkCommand.action==='interact_local'&&talkCommand.npc_index===0,'Talk discovers the selected NPC');
    town.local_visits={[localVisitKey(town.npcs[0])]:{met:true},[localVisitKey(town.npcs[1])]:{met:true}};render();
    act=async()=>{commands++;};
    check($('selection-details').children.some(child=>child.textContent==='Swamp herbalist'),'Known local shows role');
    check($('selection-details').children.some(child=>child.textContent?.includes('Look among the reeds.')),'Inspector shows dialogue');
    check($('selection-move').hidden&&$('hall-details').hidden,'NPC inspector has no building controls');
    check(!$('selection-details').children.some(child=>child.tag==='select'),'NPC inspector has no worker controls');
    camera.zoom=.35;draw(0);check(!hitTargets.some(hit=>hit.type==='npc'),'Zoomed-out map has no NPC sprites or targets');
    check(npcSprites(town,0).length===0,'Zoomed-out NPC sprite list is empty');
    camera.zoom=1;draw(0);check(hitTargets.some(hit=>hit.type==='npc'),'NPCs return on zoom in');
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
    world.local_stories={Moss:{title:'A Remedy Remembered',reward:12,keepsake:'Pressed leaf',stages:[
      {dialogue:'Help remember a remedy.',button:'Offer to help'},
      {dialogue:'Bring herbs.',costs:{herbs:3},button:'Bring the herbs'},
      {dialogue:'The traveler is recovering.'}]}};
    viewed='own';selection={type:'npc',index:0};town.player={x:6,y:7};render();
    let storyButton=$('selection-details').children.find(child=>child.textContent==='Offer to help');
    check(storyButton&&!storyButton.disabled,'Nearby NPC offers the story');
    let storyCommand;act=async data=>{storyCommand=data;};await storyButton.onclick();
    check(storyCommand.action==='advance_local_story'&&storyCommand.npc_index===0,'Story button sends the selected NPC');
    town.local_visits={[localVisitKey(town.npcs[0])]:{story_stage:1}};render();
    storyButton=$('selection-details').children.find(child=>child.textContent==='Bring the herbs');
    check(storyButton.disabled,'Story delivery is disabled without supplies');
    town.goods={herbs:3};render();
    check(!$('selection-details').children.find(child=>child.textContent==='Bring the herbs').disabled,'Supplies enable delivery');
    town.player.x=11;render();
    check($('selection-details').children.find(child=>child.textContent==='Bring the herbs').disabled,'Remote story delivery is disabled');
    selection={type:'player'};render();
    check($('selection-details').children.some(child=>child.textContent?.includes('A Remedy Remembered')&&child.textContent.includes('3 herbs')),'Character panel tracks active objectives');
    selection={type:'npc',index:0};town.local_visits[localVisitKey(town.npcs[0])].story_stage=2;render();
    check($('selection-details').children.some(child=>child.textContent?.includes('A Remedy Remembered · Completed')),'Completed story stays readable');
    check(!$('selection-details').children.some(child=>child.textContent==='Bring the herbs'),'Completed story has no repeat reward button');
    town.player.keepsakes=['Pressed leaf'];selection={type:'player'};render();
    check($('selection-details').children.some(child=>child.textContent==='Keepsake: Pressed leaf'),'Character keepsakes remain visible');
    town.npcs.push({name:'Flint',biome:'r',x:8,y:8});town.local_visits[localVisitKey(town.npcs.at(-1))]={story_stage:1};
    town.map_resources=[{kind:'rock',x:7,y:8}];town.player={x:6,y:8};selection={type:'resource',index:0};render();
    let rockButton=$('selection-details').children.find(child=>child.textContent?.startsWith('Examine markings'));
    check(rockButton&&!rockButton.disabled,'Active survey offers nearby rock inspection');await rockButton.onclick();
    check(storyCommand.action==='inspect_story_rock'&&storyCommand.x===7&&storyCommand.y===8,'Rock inspection sends coordinates');
    town.local_visits[localVisitKey(town.npcs.at(-1))].story_rocks=['7:8'];render();
    check($('selection-details').children.find(child=>child.textContent==='Outcrop markings recorded').disabled,'Recorded outcrop cannot be counted twice');
    town.npcs.push({name:'Bram',role:'Forest toolsmith',dialogue:'Better axes.',biome:'f',x:6,y:8});
    town.local_visits[localVisitKey(town.npcs.at(-1))]={met:true};selection={type:'npc',index:town.npcs.length-1};town.coins=30;render();
    let steel=$('selection-details').children.find(child=>child.textContent?.startsWith('Buy steel axe'));
    let master=$('selection-details').children.find(child=>child.textContent?.startsWith('Buy masterwork axe'));
    check(steel&&!steel.disabled&&steel.textContent.includes('+50%'),'Steel axe shows its bonus and is affordable');
    check(master.disabled&&master.textContent.includes('+75%'),'Masterwork axe shows its bonus and price gate');
    await steel.onclick();
    check(storyCommand.action==='buy_worker_tool'&&storyCommand.item==='steel_axe'&&storyCommand.npc_index===selection.index,'Tool purchase sends selected shop and item');
    town.player.x=20;render();
    check($('selection-details').children.find(child=>child.textContent?.startsWith('Buy steel axe')).disabled,'Tool shop requires proximity');
    town.player.x=6;town.coins=60;
    town.npcs.push({name:'Runa',role:'Harvest toolsmith',dialogue:'Better sickles.',biome:'m',x:6,y:8});
    town.local_visits[localVisitKey(town.npcs.at(-1))]={met:true};selection={type:'npc',index:town.npcs.length-1};render();
    master=$('selection-details').children.find(child=>child.textContent?.startsWith('Buy masterwork sickle'));
    check(master&&!master.disabled&&master.textContent.includes('+75% farming'),'Harvest shop sells stronger sickles');
    await master.onclick();check(storyCommand.item==='masterwork_sickle','Sickle purchase sends correct tool');
    town.npcs.push({name:'Fern',role:'Meadow grower',dialogue:'A spare sickle.',biome:'m',x:6,y:8});
    town.local_visits[localVisitKey(town.npcs.at(-1))]={met:true};selection={type:'npc',index:town.npcs.length-1};render();
    let gift=$('selection-details').children.find(child=>child.textContent?.startsWith('Accept fine sickle'));
    check(gift&&!gift.disabled,'Grower offers a free sickle');await gift.onclick();
    check(storyCommand.action==='claim_local_gift'&&storyCommand.npc_index===selection.index,'Sickle gift sends correct NPC');
    town.local_visits[localVisitKey(town.npcs.at(-1))]={gift_claimed:true};render();
    check($('selection-details').children.find(child=>child.textContent==='Fine sickle gift collected').disabled,'Claimed sickle cannot be claimed again');
    town.npcs.push({name:'Dara',biome:'s',x:6,y:8});
    town.local_visits[localVisitKey(town.npcs.at(-1))]={met:true};selection={type:'npc',index:town.npcs.length-1};render();
    gift=$('selection-details').children.find(child=>child.textContent?.startsWith('Accept desert clothing'));
    check(gift&&!gift.disabled&&gift.textContent.includes('Clothing gift'),'Outfitter offers wearable clothing');
    check(!$('selection-details').children.some(child=>child.textContent?.includes('NaN')),'Clothing gifts do not show worker bonuses');
    await gift.onclick();check(storyCommand.action==='claim_local_gift','Clothing uses saved gift action');
    town.npcs.push({name:'Nell',biome:'b',x:6,y:8});
    town.local_visits[localVisitKey(town.npcs.at(-1))]={met:true};selection={type:'npc',index:town.npcs.length-1};render();
    master=$('selection-details').children.find(child=>child.textContent?.startsWith('Buy masterwork fishing rod'));
    check(master&&!master.disabled&&master.textContent.includes('+75% fishing'),'Rod maker sells fishing upgrades');
    await master.onclick();check(storyCommand.item==='masterwork_fishing_rod','Fishing shop sends the correct item');
    town.npcs.push({name:'Pip',biome:'g',x:6,y:8});
    town.local_visits[localVisitKey(town.npcs.at(-1))]={met:true};selection={type:'npc',index:town.npcs.length-1};render();
    let barter=$('selection-details').children.find(child=>child.textContent==='Trade 10 wood for 5 bread');
    check(barter&&!barter.disabled,'Provisioner shows the bread exchange');
    await barter.onclick();check(storyCommand.action==='trade_local'&&storyCommand.npc_index===selection.index,'Barter sends selected trader');
    town.local_visits[localVisitKey(town.npcs.at(-1))]={trade_remaining:300};render();
    check($('selection-details').children.find(child=>child.textContent?.startsWith('Restocking')).disabled,'New trader obeys saved restocking');
    town.inventory_version=1;town.buildings[0].stored_goods={food:93};
    check(localTradeFoodRoom(town,'bread')<5,'Bread checks its weighted food capacity');
    town.buildings[0].stored_goods={food:92.5};
    check(localTradeFoodRoom(town,'bread')===5,'Exact bread capacity can accept a trade');
    delete town.inventory_version;delete town.buildings[0].stored_goods;
    town.player.worker_tools={masterwork_sickle:1};selection={type:'player'};render();
    check($('selection-details').children.some(child=>child.textContent?.includes('Masterwork sickle')&&child.textContent.includes('+75% farming')),'Inventory displays actual tier bonus');
    check(villagerAccessories({kind:'villager',villager:{job:'woodcutting',woodcutting:0,worker_tool:'steel_axe'}}).axe,'Steel axe is visible on untrained woodcutters');
    check(villagerAccessories({kind:'villager',villager:{job:'farming',farming:0,worker_tool:'masterwork_sickle'}}).sickle,'Masterwork sickle is visible on untrained farmers');
    check(!villagerAccessories({kind:'villager',villager:{job:'farming',worker_tool:'steel_axe'}}).axe,'A mismatched tool does not change profession artwork');
    selection={type:'npc',index:0};
    delete town.npcs;render();check(selection===null,'Missing selected NPC closes inspector safely');
    viewed='own';key('o');check(selection===null,'Missing NPC list is safe to interact with');
  })();`);
run(documentMock,{devicePixelRatio:1,addEventListener(type,callback){listeners[type]=callback;}},{getItem:()=>null},()=>0,()=>{},fills,listeners)
  .then(()=>{if(typeof print==='function')print('NPC selection checks passed');else console.log('NPC selection checks passed');})
  .catch(error=>{if(typeof print==='function'){print(String(error));print(error.stack);quit(1);}else{console.error(error);process.exitCode=1;}});
