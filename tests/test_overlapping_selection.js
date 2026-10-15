// Run with Node.js or JavaScriptCore's jsc shell.
const source=typeof readFile==='function'?readFile('web/app.js'):require('fs').readFileSync('web/app.js','utf8');
const elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){},focus(){}};}
const context=new Proxy({}, {get:(target,key)=>key in target?target[key]:()=>{}});
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>context;
canvasMock.getBoundingClientRect=()=>({left:0,top:0,width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  return (async()=>{
    function check(condition,message){if(!condition)throw Error(message);}
    const town={name:'Forest town',wood:65,food:40,coins:20,level:1,harvested:0,
      buildings:[{kind:'hall',x:3,y:3},{kind:'woodcutter',x:5,y:5}],
      map_resources:[{kind:'tree',x:6,y:5},{kind:'tree',x:5,y:6}],villagers:[]};
    world={world_id:'test',towns:{own:town}};identity={town_id:'own'};viewed='own';
    animatedVillagers=()=>[];render();
    const [buildingX,py]=point(5,5),px=buildingX+tileW*.15,pointer={clientX:px,clientY:py};
    const overlaps=hitTargets.filter(h=>px>=h.left&&px<=h.right&&py>=h.top&&py<=h.bottom);
    check(overlaps[overlaps.length-1].type==='resource'&&overlaps.some(h=>h.type==='building'),'Foreground trees overlap the woodcutter');
    canvas.onmousemove(pointer);
    check(buildingHover===null,'A foreground tree does not highlight the building behind it');
    await canvas.onclick(pointer);
    check(selection?.type==='resource','Click follows the foreground tree instead of the building behind it');
    const tree=hitTargets.find(h=>h.type==='resource'&&h.index===0);
    await canvas.onclick({clientX:(tree.left+tree.right)/2,clientY:tree.top+2});
    check(selection?.type==='resource'&&selection.index===0,'Tree canopy outside buildings remains selectable');
    hitTargets.push({type:'npc',index:0,left:px-5,right:px+5,top:py-5,bottom:py+5});
    check(hitTargetAt(px,py).type==='npc','Foreground characters retain selection priority');
    tool='home';canvas.onmousemove({clientX:buildingX,clientY:py+tileH/2});
    check(buildingHover===null&&buildTile===null,'Construction mode still rejects occupied tiles');
    tool=null;
    world={world_id:'shared',shared_map:{map_resources:[]},towns:{own:{...town,buildings:[town.buildings[0]]},friend:{...town,buildings:[town.buildings[1]]}}};
    viewed='own';render();
    const [sharedX,sharedY]=point(5,5);
    await canvas.onclick({clientX:sharedX+tileW*.15,clientY:sharedY});
    check(viewed==='friend'&&selection?.type==='building'&&selection.index===0,'Shared-map buildings retain their correct owner and index');
  })();`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{})
  .then(()=>{if(typeof print==='function')print('Overlapping selection checks passed');else console.log('Overlapping selection checks passed');})
  .catch(error=>{if(typeof print==='function'){print(error.stack);quit(1);}else{console.error(error);process.exitCode=1;}});
