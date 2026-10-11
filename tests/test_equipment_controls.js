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
`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},assert);
console.log('Equipment control checks passed');
