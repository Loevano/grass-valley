// Run with: node tests/test_equipment_controls.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8'),elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
documentMock.getElementById('world').getContext=()=>new Proxy({}, {get:()=>()=>{}});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','assert',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
    const town={level:3,wood:65,coins:50,goods:{planks:10},player:{x:5.2,y:5,facing:1},map_size:12,
      terrain:Array(12).fill('g'.repeat(12)),buildings:[],map_resources:[],npcs:[{name:'Tobin',role:'Traveler',dialogue:'Hello',x:5,y:8}]};
    town.terrain[5]='ggggggbggggg';
    world={world_id:'gear',towns:{own:town}};identity={town_id:'own'};viewed='own';
    assert(predictPlayerStep(town,town.player,1,0).x<5.32,'Swamp blocks unprotected movement');
    town.player.items={swamp_boots:1};
    assert(predictPlayerStep(town,town.player,1,0).x<5.32,'Ownership alone does not unlock swamp');
    town.player.equipment={feet:'swamp_boots'};
    assert(predictPlayerStep(town,town.player,1,0).x>5.32,'Equipping immediately unlocks swamp prediction');
    assert(predictPlayerStep(town,{x:5.2,y:5},1,0).x>5.32,'Prediction uses refreshed gear even before the next walk reply');
    town.terrain[5]='ggggggsggggg';
    town.player.equipment={};
    assert(predictPlayerStep(town,town.player,1,0).x>5.32,'Sand is walkable without gear');
    town.player.equipment.body='desert_clothing';
    assert(predictPlayerStep(town,town.player,1,0).x>5.32,'Sand stays walkable with clothing');
    town.terrain[5]='ggggggbggggg';
    for(const y of [7,8]){
      town.terrain[y]='ggggggbggggg';town.player.y=y;
      assert(predictPlayerStep(town,town.player,1,0).x>5.32,'Both lighter swamp shades are walkable without boots');
    }
    town.player.equipment.feet='swamp_boots';
    town.player.y=7;
    selection={type:'player',index:0};renderSelection(town);
    assert($('selection-details').children.some(c=>c.children?.some(b=>b.textContent==='Unequip swamp boots')));
    selection={type:'npc',index:0};renderSelection(town);
    assert($('selection-details').children.some(c=>c.textContent==='Buy swamp boots · 15 coins'&&!c.disabled));
    assert($('selection-details').children.some(c=>c.textContent==='Sell swamp boots · 7 coins'&&c.disabled),'Worn last copy cannot be sold');
    town.player.x=10;renderSelection(town);
    assert($('selection-details').children.some(c=>c.textContent==='Buy swamp boots · 15 coins'&&c.disabled),'Trading requires proximity');
    town.player.x=5;town.coins=0;renderSelection(town);
    assert($('selection-details').children.some(c=>c.textContent==='Buy desert clothing · 20 coins'&&c.disabled),'Trading respects affordability');
    const sent=[];act=data=>sent.push(data);
    town.npcs[0].name='Sylva';town.player.y=8;renderSelection(town);
    const gift=$('selection-details').children.find(c=>c.textContent==='Accept fine axe · Gift for a worker');
    assert(gift&&!gift.disabled,'Nearby ranger offers axe gift');gift.onclick();
    assert.deepStrictEqual(sent.pop(),{action:'claim_local_gift',npc_index:0});
    town.local_visits={'Sylva:5:8':{gift_claimed:true}};renderSelection(town);
    assert($('selection-details').children.some(c=>c.textContent==='Fine axe gift collected'&&c.disabled));
    town.npcs[0].name='Reed';renderSelection(town);
    assert($('selection-details').children.some(c=>c.textContent==='Accept fine fishing rod · Gift for a worker'));
    town.npcs[0].name='Tobin';town.buildings=[{kind:'hall',stored_goods:{wood:65,food:40}}];town.inventory_version=1;renderSelection(town);
    const trade=$('selection-details').children.find(c=>c.textContent==='Trade 10 wood for 8 food');
    assert(trade&&!trade.disabled,'Barter enabled with wood and food storage room');trade.onclick();
    assert.deepStrictEqual(sent.pop(),{action:'trade_local',npc_index:0});
    town.local_visits['Tobin:5:8']={trade_remaining:297};renderSelection(town);
    assert($('selection-details').children.some(c=>c.textContent==='Restocking · 5 min remaining'&&c.disabled));
    town.local_visits['Tobin:5:8'].trade_remaining=0;town.buildings[0].stored_goods.food=100;renderSelection(town);
    assert($('selection-details').children.some(c=>c.textContent==='Trade 10 wood for 8 food'&&c.disabled),'Full storage blocks barter');
    town.buildings[0].stored_goods.food=40;town.player.x=20;renderSelection(town);
    assert($('selection-details').children.some(c=>c.textContent==='Trade 10 wood for 8 food'&&c.disabled),'Distant barter disabled');
    town.player.worker_tools={fine_axe:1};const worker={},target=document.createElement('div');
    renderWorkerTools(town,worker,2,target);const give=target.children[0];
    assert(give.textContent==='Give fine axe · +25% woodcutting output');give.onclick();
    assert.deepStrictEqual(sent.pop(),{action:'give_worker_tool',item:'fine_axe',index:2});
    worker.worker_tool='fine_axe';target.replaceChildren();renderWorkerTools(town,worker,2,target);
    assert(target.children[0].textContent==='Fine axe · +25% woodcutting output');target.children[0].children[0].onclick();
    assert.deepStrictEqual(sent.pop(),{action:'take_worker_tool',index:2});
    viewed='friend';target.replaceChildren();renderWorkerTools(town,worker,2,target);
    assert(target.children[0].children.length===0,'Visitors cannot take tools');
    delete worker.worker_tool;target.replaceChildren();renderWorkerTools(town,worker,2,target);
    assert(target.children.length===0,'Visitors cannot give tools');

`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},assert);
console.log('Equipment control checks passed');
