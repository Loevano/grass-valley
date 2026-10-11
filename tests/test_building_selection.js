// Run with Node.js, or JavaScriptCore's jsc shell.
const source=typeof readFile==='function'?readFile('web/app.js'):require('fs').readFileSync('web/app.js','utf8');
const elements=new Map();
function element(){
  return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},
    replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){},showModal(){},close(){}};
}
const fills=[];
const context=new Proxy({fill(){fills.push(this.fillStyle);}}, {get:(target,key)=>key in target?target[key]:()=>{}});
const documentMock={getElementById(id){
  if(!elements.has(id))elements.set(id,element());
  return elements.get(id);
},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>context;
canvasMock.getBoundingClientRect=()=>({left:0,top:0,width:720,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','fills',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  return (async()=>{
    function check(condition,message){if(!condition)throw Error(message);}
    const town={map_size:120,name:'Test town',wood:65,food:40,coins:20,level:1,harvested:0,
      buildings:[{kind:'hall',x:5,y:5}],map_resources:[{kind:'tree',x:0,y:0}],villagers:[]};
    world={world_id:'test',towns:{own:town,friend:{...town,name:'Friend'}}};
    identity={town_id:'own',token:'test'};viewed='own';
    const commands=[];
    let fail=false,release;
    request=async(path,data)=>{
      commands.push(data);
      await new Promise(resolve=>{release=resolve;});
      if(fail)throw Error('Tile no longer available');
      town.buildings.push({kind:data.kind,x:data.x,y:data.y});town.wood-=costs[data.kind];
    };
    refresh=async()=>render();
    function pointer(x,y,button=0){const [px,py]=point(x,y);return {clientX:px,clientY:py+tileH/2,button};}
    function click(x,y,button=0){return canvas.onclick(pointer(x,y,button));}
    function hover(x,y){canvas.onmousemove(pointer(x,y));draw();} // Paint the next animation frame.
    [...$('buildings').children].flatMap(group=>[...group.children]).filter(button=>button.buildingKind)[0].onclick();
    check(tool==='home','Choosing a cottage activates its tool');
    camera={x:15,y:15,zoom:.5};draw();
    fills.length=0;hover(21,20);
    check(buildTile===null&&fills.includes('#c76d6455'),'Beyond the hall range previews red');
    check(fills.includes('#e4c98022'),'Build panel highlights the hall building area');
    await click(21,20);
    check(commands.length===0&&$('notice').textContent.includes('within 30 tile steps'),'Out-of-range clicks explain the limit without submitting');
    check(placementValid(town,{x:20,y:20},'home'),'Thirty combined steps is inside the range');
    town.level=2;
    check(placementValid(town,{x:21,y:20},'home'),'Upgraded hall expands client placement range');
    town.level=1;
    fills.length=0;inspect('building',0);
    check(fills.includes('#e4c98022'),'Inspecting the hall highlights its building range');
    fills.length=0;openPanel(null);
    check(!fills.includes('#e4c98022'),'Closing the panel clears the range overlay');
    [...$('buildings').children].flatMap(group=>[...group.children]).filter(button=>button.buildingKind)[0].onclick();
    hover(2,3);
    check(buildTile.x===2&&buildTile.y===3,'Hover previews the tile under the cursor');
    check(commands.length===0&&town.wood===65,'Hover does not build or spend wood');
    hover(4,3);
    check(buildTile.x===4,'Moving the cursor updates the preview');
    canvas.onmouseleave();check(buildTile===null,'Leaving the map clears the preview');
    hover(4,3);await click(4,3,2);
    check(commands.length===0,'Right click does not build');
    const pending=click(2,3);
    check(building,'Left click starts construction immediately');
    await click(4,3);
    check(commands.length===1,'Repeated clicks cannot submit while building');
    release();await pending;
    check(commands[0].action==='build'&&commands[0].kind==='home'&&commands[0].x===2&&commands[0].y===3,'Click builds on the clicked tile rather than the previous hover');
    check(town.wood===45&&tool==='home','Success spends wood and keeps the building type selected');
    fills.length=0;hover(5,5);check(buildTile===null,'Existing buildings cannot be previewed');
    check(hoverTile.x===5&&fills.includes('#c76d6455'),'Occupied buildings show a red placement preview');
    render();check(hoverTile.x===5,'Refresh preserves blocked preview under a stationary pointer');
    hover(0,0);check(buildTile===null,'Resources cannot be previewed');
    hover(-1,2);check(buildTile===null,'Tiles outside the map cannot be previewed');
    await click(5,5);await click(0,0);await click(-1,2);
    check(commands.length===1,'Occupied and out-of-bounds clicks send no command');
    town.wood=0;fills.length=0;hover(1,2);
    check(fills.includes('#c76d6455'),'Unaffordable placements show a red preview');
    await click(1,2);
    check(commands.length===1,'Insufficient wood sends no command');
    check($('notice').textContent==='Not enough wood.','Insufficient wood displays a useful message');
    town.wood=45;render();fail=true;hover(1,2);
    const rejected=click(1,2);release();await rejected;
    check(!building&&buildTile.x===1,'A rejected build allows retrying the hovered tile');
    check($('notice').textContent==='Tile no longer available','A rejected build displays its error');
    fail=false;const retry=click(1,2);release();await retry;
    check(town.wood===25&&buildTile===null,'Retry builds and clears the occupied preview');
    hover(2,2);town.buildings.push({kind:'home',x:2,y:2});render();
    check(buildTile===null,'Refresh clears a tile that becomes occupied');
    hover(3,2);$('cancel').onclick();
    check(buildTile===null&&tool===null,'Cancel clears the preview and building type');
    hover(3,2);check(buildTile===null,'Hover without a building type has no build preview');
    [...$('buildings').children].flatMap(group=>[...group.children]).filter(button=>button.buildingKind)[0].onclick();hover(3,2);[...$('buildings').children].flatMap(group=>[...group.children]).filter(button=>button.buildingKind)[1].onclick();
    check(buildTile===null,'Changing building types clears the preview');
    hover(3,2);$('move').onclick();
    check(buildTile===null&&tool==='move','Move clears construction preview');
    [...$('buildings').children].flatMap(group=>[...group.children]).filter(button=>button.buildingKind)[0].onclick();hover(3,2);$('towns').children[1].onclick();
    check(viewed==='friend'&&buildTile===null,'Visiting another town clears the preview');
    tool='home';hover(3,3);await click(3,3);
    check(buildTile===null&&commands.length===3,'Visiting another town cannot preview or build');
  })();`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},fills)
  .then(()=>{if(typeof print==='function')print('Building selection checks passed');else console.log('Building selection checks passed');})
  .catch(error=>{if(typeof print==='function'){print(error.stack);quit(1);}else{console.error(error);process.exitCode=1;}});
