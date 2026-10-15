// Orchard planting, ownership, linked movement, and host-owned tree artwork.
const assert=require('assert'),fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8'),elements=new Map(),marks=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(...children){this.children.push(...children);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){},showModal(){},close(){}};}
const context=new Proxy({}, {get(target,key){return key in target?target[key]:(...args)=>marks.push([key,...args,target.fillStyle,target.strokeStyle]);}});
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvas=documentMock.getElementById('world');canvas.getContext=()=>context;canvas.getBoundingClientRect=()=>({left:0,top:0,width:720,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','assert','marks',source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  function nodes(node=$('selection-details')){return Array.from(node.children || []).flatMap(child=>[child,...nodes(child)]);}
  return (async()=>{
    const town={name:'Orchard',wood:65,food:40,coins:20,level:5,harvested:0,map_size:16,terrain:Array(16).fill('g'.repeat(16)),
      buildings:[{kind:'hall',x:5,y:5},{kind:'orchard',x:8,y:6}],map_resources:[],villagers:[]};
    identity={town_id:'own'};viewed='own';world={world_id:'test',towns:{own:town,friend:JSON.parse(JSON.stringify(town))}};render();
    assert(![...$('buildings').children].flatMap(group=>[...group.children]).some(b=>b.buildingKind==='orchard_tree'),'Planting starts at the owning orchard');
    inspect('building',1);
    const plant=nodes().find(b=>b.textContent==='Plant fruit tree · 5 wood');assert(plant&&!plant.disabled);
    plant.onclick();assert.equal(tool,'orchard_tree');assert.equal(orchardIndex,1);
    assert(placementValid(town,{x:9,y:5},'orchard_tree'));assert(!placementValid(town,{x:12,y:6},'orchard_tree'));
    const commands=[];
    request=async(path,data)=>{commands.push(data);town.buildings.push({...data,tree:{stage:'sapling',elapsed:0,fruit:8}});town.wood-=5;};refresh=async()=>render();
    await buildAt(12,6);assert.equal(commands.length,0);
    await buildAt(9,5);assert.equal(commands.length,1);assert.equal(commands[0].orchard_index,1);assert.equal(commands[0].kind,'orchard_tree');assert.equal(tool,'orchard_tree');
    $('cancel').onclick();inspect('building',2);
    assert.equal($('selection-title').textContent,'Fruit tree');assert($('selection-level').hidden);assert(!$('selection-move').hidden);assert(!$('selection-destroy').hidden);
    assert(nodes().some(b=>b.textContent==='Owning orchard'));assert(nodes().some(b=>b.textContent?.includes('Sapling growing')));
    assert(!nodes().some(b=>b.textContent==='Add worker'||b.textContent==='Upgrade'));
    assert.equal(settlerCapacity({...town.buildings[2],settler_level:3}),0);
    assert.equal(buildingStorageSize(town.buildings[2]),0,'Fruit trees add no town storage');
    assert.equal(upgradePreview({action:'upgrade_building',index:2},town,town.buildings[2]),null);
    assert.equal(upgradePreview({action:'upgrade_capacity',index:2},town,town.buildings[2]),null);
    tool='move';moveIndex=2;assert(!placementValid(town,{x:12,y:6},'orchard_tree'),'Individual trees stay within their orchard');
    $('cancel').onclick();
    const appearance=[];
    for(const stage of ['sapling','blossom','fruit','ripe','harvested']){
      const tree={kind:'orchard_tree',x:9,y:5,orchard_index:1,tree:{stage,elapsed:12,fruit:8}},before=JSON.stringify(tree);
      marks.length=0;drawBuilding(tree,town);appearance.push(JSON.stringify(marks));assert.equal(JSON.stringify(tree),before,'Drawing preserves host state');
    }
    assert.equal(new Set(appearance).size,5,'Every tree stage has distinct artwork');
    inspect('building',1);assert.equal($('selection-destroy').textContent,'Destroy orchard & trees · No refund');
    nodes().find(b=>b.textContent?.startsWith('Tree · 9, 5')).onclick();assert.equal(selection.index,2);
    inspect('building',1);town.wood=0;render();assert(nodes().find(b=>b.textContent==='Plant fruit tree · 5 wood').disabled);
    town.sandbox=true;render();assert(!nodes().find(b=>b.textContent==='Plant fruit tree · Free').disabled);
    viewed='friend';inspect('building',1);assert(!nodes().some(b=>b.textContent?.startsWith('Plant fruit tree')));
    viewed='own';tool='move';moveIndex=1;assert(placementValid(town,{x:9,y:6},'orchard'));
    const before=JSON.stringify(town.buildings);town.map_resources.push({kind:'rock',x:10,y:5});assert(!placementValid(town,{x:9,y:6},'orchard'));assert.equal(JSON.stringify(town.buildings),before);
  })();`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},assert,marks).then(()=>console.log('Orchard UI checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
