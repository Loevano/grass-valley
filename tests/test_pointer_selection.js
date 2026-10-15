// Real Canvas regression checks for visible-shape selection (no host or saves).
const fs=require('node:fs');
const {chromium}=require('playwright');
(async()=>{
  const browser=await chromium.launch({headless:true});
  try{
    for(const deviceScaleFactor of [1,2]){
      const page=await browser.newPage({viewport:{width:1100,height:800},deviceScaleFactor});
      const html=fs.readFileSync('web/index.html','utf8').replace(/<script\b[^>]*>[\s\S]*?<\/script>/g,'').replace(/<link\b[^>]*>/g,'');
      await page.setContent(html);
      await page.addStyleTag({content:'#world {position:fixed;left:0;top:0;width:960px;height:660px}'});
      await page.addScriptTag({content:fs.readFileSync('web/animations.js','utf8')});
      const source=fs.readFileSync('web/app.js','utf8');
      await page.addScriptTag({content:source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))});
      await page.evaluate(async()=>{
        const check=(condition,message)=>{if(!condition)throw Error(message);};
        const town={name:'Pointer test',level:1,wood:65,food:40,coins:20,harvested:0,
          buildings:[{kind:'hall',x:3,y:3},{kind:'woodcutter',x:5,y:5}],
          map_resources:[],villagers:[],roads:[]};
        world={world_id:'pointer',towns:{own:town}};identity={town_id:'own'};viewed='own';
        animatedVillagers=()=>[];render();
        const eventAt=(px,py)=>{const rect=canvas.getBoundingClientRect();return {clientX:rect.left+px,clientY:rect.top+py};};
        const click=async(px,py)=>canvas.onclick(eventAt(px,py));
        let [px,py]=point(5,5),scale=tileW/46;
        canvas.onmousemove(eventAt(px,py-25*scale));
        check(buildingHover?.index===1,'Hovering the wall highlights its building');
        await click(px,py-25*scale);
        check(selection?.type==='building'&&selection.index===1,'Visible wall selects its building');
        canvas.onmousemove(eventAt(px+20*scale,py-43*scale));
        check(buildingHover===null,'Empty roof corner does not highlight the building');
        await click(px+20*scale,py-43*scale);
        check(selection===null,'Empty roof corner does not select the building');
        // This lies on the building ground tile, outside the painted structure.
        const ground=[px,py+tileH*.9];
        const tile=tileAt(eventAt(...ground));
        check(tile.x===5&&tile.y===5,'Ground miss exercises the occupied-tile fallback');
        await click(...ground);
        check(selection===null,'Empty ground on an occupied tile does not select its building');
        town.map_resources=[{kind:'tree',x:6,y:5}];render();
        [px,py]=point(6,5);
        await click(px+20*scale,py-55*scale);
        check(selection===null,'Transparent tree-canopy corner is not selectable');
        await click(px,py-40*scale);
        check(selection?.type==='resource'&&selection.index===0,'Visible tree canopy selects the tree');
        let overlap=null,gap=null;
        const building=hitTargets.find(h=>h.type==='building'&&h.index===1);
        const tree=hitTargets.find(h=>h.type==='resource');
        for(let y=Math.ceil(Math.max(building.top,tree.top));y<Math.min(building.bottom,tree.bottom);y+=2){
          for(let x=Math.ceil(Math.max(building.left,tree.left));x<Math.min(building.right,tree.right);x+=2){
            if(!hitTargetContains(building,x,y))continue;
            if(hitTargetContains(tree,x,y))overlap ||= [x,y];else gap ||= [x,y];
          }
        }
        check(overlap&&gap,'Fixture includes both overlapping artwork and canopy gaps');
        await click(...overlap);
        check(selection?.type==='resource','Frontmost visible tree wins over the covered building');
        canvas.onmousemove(eventAt(...gap));
        check(buildingHover?.index===1,'Building remains hoverable through transparent canopy gaps');
        await click(...gap);
        check(selection?.type==='building'&&selection.index===1,'Visible building remains selectable through canopy gaps');
        town.npcs=[{name:'Moss',biome:'b',x:8,y:4}];render();
        [px,py]=point(8,4);
        await click(px-3,py+tileH*.65-16*scale);
        check(selection?.type==='npc','Visible character body remains selectable');
        await click(px-3+8*scale,py+tileH*.65-25*scale);
        check(selection===null,'Empty corner beside a character head is not selectable');
        town.roads=[{x:4,y:7}];town.map_resources.push({kind:'water',x:4,y:8});render();
        [px,py]=point(4,7);await click(px,py+tileH/2);
        check(selection?.type==='road','Visible road remains selectable');
        [px,py]=point(4,8);await click(px,py+tileH/2);
        check(selection?.type==='resource'&&selection.index===1,'Visible water remains selectable');
        keyboardTile={x:5,y:5};await selectTile(5,5);
        check(selection?.type==='building'&&selection.index===1,'Keyboard tile selection remains available');
        world={world_id:'shared-pointer',shared_map:{map_resources:[],npcs:[]},towns:{
          own:{...town,buildings:[town.buildings[0]],map_resources:[],roads:[],npcs:[]},
          friend:{...town,buildings:[town.buildings[1]],map_resources:[],roads:[],npcs:[]}}};
        viewed='own';render();
        const snapshot=JSON.stringify(world);
        [px,py]=point(5,5);await click(px,py-25*scale);
        check(viewed==='friend'&&selection?.type==='building'&&selection.index===0,'Visible shared-map building selects the correct owner and index');
        check(JSON.stringify(world)===snapshot,'Pointer picking preserves authoritative world state');
        const specialtyTown={...town,level:10,player:{x:3,y:3,facing:1},buildings:[],map_resources:[],roads:[],npcs:[]};
        world={world_id:'building-art',progression:{buildings:{},recipes:{},upgrades:{},animals:{},crops:{},edible:{}},towns:{own:specialtyTown}};viewed='own';
        const specialtyBuildings=['fishing_hut','fish_pond','mine','smelter','steelworks','storehouse','mill','market',
          'orchard','stable','saddler','tradehouse','well','orchard_tree','wooden_bench','wooden_statue'].map(kind=>({
            kind,x:5,y:5,footprint:[1,1],level:kind==='storehouse'?8:kind==='well'?1:3,settler_level:3,
            ...(kind==='orchard_tree'?{tree:{stage:'ripe',fruit:8}}:{}),
            profession_upgrades:kind==='fishing_hut'?['nets','wool_nets']:kind==='mine'?['steel_pickaxe']:[]}));
        specialtyBuildings.push({kind:'mill',x:5,y:5,footprint:[2,2],level:3},
          ...[0,12,24].map(progress=>({kind:'workshop',x:5,y:5,footprint:[2,2],construction:{progress,total:30}})));
        for(const building of specialtyBuildings){
          const kind=building.kind;specialtyTown.buildings=[building];
          const saved=JSON.stringify(world);render();
          const hit=hitTargets.find(target=>target.type==='building'&&target.index===0);
          const [x,y]=point(5,5);let visible=null;
          for(let dy=-25;dy<=8&&!visible;dy+=2)for(let dx=-16;dx<=16;dx+=2){
            const point=[x+dx*scale,y+dy*scale];
            if(hitTargetContains(hit,...point)){visible=point;break;}
          }
          check(visible,kind+' has a selectable painted surface');
          await click(...visible);
          check(selection?.type==='building'&&selection.index===0,kind+' painted surface selects its building');
          await click(hit.left+scale,hit.top+scale);
          check(selection===null,kind+' transparent corner does not select its building');
          check(JSON.stringify(world)===saved,kind+' drawing and picking preserve authoritative state');
        }
      });
      await page.close();
    }
    console.log('Pointer selection checks passed at 1× and 2× pixel density');
  }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
