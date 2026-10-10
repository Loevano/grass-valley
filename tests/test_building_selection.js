// Run with Node.js, or JavaScriptCore's jsc shell.
const source=typeof readFile==='function'?readFile('web/app.js'):require('fs').readFileSync('web/app.js','utf8');
const elements=new Map();
function element(){
  return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},
    replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){},showModal(){},close(){}};
}
const context=new Proxy({}, {get:()=>()=>{}});
const documentMock={getElementById(id){
  if(!elements.has(id))elements.set(id,element());
  return elements.get(id);
},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>context;
canvasMock.getBoundingClientRect=()=>({left:0,top:0,width:720,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  return (async()=>{
    function check(condition,message){if(!condition)throw Error(message);}
    const town={name:'Test town',wood:65,food:40,coins:20,level:1,harvested:0,
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
    function click(x,y){const [px,py]=point(x,y);return canvas.onclick({clientX:px,clientY:py+tileH/2});}
    $('buildings').children[0].onclick();
    check(tool==='home','Choosing a cottage activates its tool');
    await click(2,3);
    check(buildTile.x===2&&buildTile.y===3,'Click selects the tile under the cursor');
    check(commands.length===0&&town.wood===65,'Selection does not build or spend wood');
    check(!$('confirm-build').hidden&&!$('confirm-build').disabled,'Selection enables confirmation');
    await click(4,3);
    check(buildTile.x===4,'Clicking another tile changes the selection');
    const pending=$('confirm-build').onclick();
    check(building&&$('confirm-build').disabled,'Confirmation disables the button while pending');
    await $('confirm-build').onclick();
    check(commands.length===1,'Repeated confirmation cannot submit twice');
    release();await pending;
    check(commands[0].action==='build'&&commands[0].kind==='home'&&commands[0].x===4&&commands[0].y===3,'Confirmation sends the selected building and tile');
    check(town.wood===45&&buildTile===null&&$('confirm-build').hidden,'Success spends wood and clears selection');
    await click(5,5);await click(0,0);
    check(buildTile===null,'Buildings and resources cannot be selected for construction');
    town.wood=0;await click(1,2);
    check($('confirm-build').disabled,'Insufficient wood disables confirmation');
    await $('confirm-build').onclick();
    check(commands.length===1,'Insufficient wood sends no command');
    town.wood=45;render();fail=true;
    const rejected=$('confirm-build').onclick();release();await rejected;
    check(!building&&buildTile.x===1&&!$('confirm-build').disabled,'A rejected build allows retrying the selection');
    check($('notice').textContent==='Tile no longer available','A rejected build displays its error');
    town.buildings.push({kind:'home',x:1,y:2});render();
    check(buildTile===null,'Refresh clears a tile that becomes occupied');
    await click(2,2);$('cancel').onclick();
    check(buildTile===null&&tool===null&&$('confirm-build').hidden,'Cancel clears selection');
    $('buildings').children[0].onclick();await click(2,2);$('buildings').children[1].onclick();
    check(buildTile===null,'Changing building types clears selection');
    await click(2,2);$('move').onclick();
    check(buildTile===null&&tool==='move','Move clears construction selection');
    $('buildings').children[0].onclick();await click(2,2);$('towns').children[1].onclick();
    check(viewed==='friend'&&buildTile===null&&$('confirm-build').hidden,'Visiting another town clears selection');
    await click(3,3);
    check(commands.length===2,'Visiting another town cannot build');
  })();`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{})
  .then(()=>{if(typeof print==='function')print('Building selection checks passed');else console.log('Building selection checks passed');})
  .catch(error=>{if(typeof print==='function'){print(error.stack);quit(1);}else{console.error(error);process.exitCode=1;}});
