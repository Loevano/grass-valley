// Run with: node tests/test_player_prediction.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8'),elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
documentMock.getElementById('world').getContext=()=>new Proxy({}, {get:()=>()=>{}});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','assert',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
    const town={level:2,wood:65,goods:{planks:10},player:{x:5.2,y:7,facing:1},map_size:12,
      terrain:Array(12).fill('g'.repeat(12)),buildings:[],map_resources:[{kind:'water',x:6,y:7}]};
    world={world_id:'wilderness',towns:{own:town}};identity={town_id:'own'};viewed='own';
    assert(predictPlayerStep(town,town.player,1,0).x<5.32,'Walking stops at the shore without a boat');
    town.player.boat=true;
    assert(predictPlayerStep(town,town.player,1,0).x>5.32,'Boat ownership immediately permits water prediction');
    town.map_resources=[{kind:'rock',x:6,y:7}];movementMap=null;
    assert(predictPlayerStep(town,town.player,1,0).x<5.32,'Boats preserve rock collision');
    delete town.player.boat;selection={type:'player',index:0};renderSelection(town);
    assert($('selection-details').children.some(c=>c.disabled&&c.textContent.includes('Requires hall level 3')),'Early halls show the boat unlock requirement');
    town.level=3;renderSelection(town);
    assert($('selection-details').children.some(c=>!c.disabled&&c.textContent==='Build exploration boat · 20 wood + 10 planks'),'Eligible players can build a boat');
    town.goods.planks=0;renderSelection(town);
    assert($('selection-details').children.some(c=>c.disabled&&c.textContent==='Build exploration boat · 20 wood + 10 planks'),'Boat controls respect material costs');
    town.map_resources=[{kind:'chest',x:6,y:7}];selection={type:'resource',index:0};renderSelection(town);
    assert.equal($('selection-title').textContent,'Island treasure');
    assert($('selection-details').children.some(c=>c.textContent==='Collect loot'),'Island chests offer a collection action');
    drawResource(town.map_resources[0]);
    town.player.boat=true;selection={type:'player',index:0};renderSelection(town);
    assert($('selection-details').children.some(c=>c.textContent?.includes('Walk onto water to sail')),'Owners see sailing instructions');
`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},assert);
console.log('Wilderness control checks passed');
