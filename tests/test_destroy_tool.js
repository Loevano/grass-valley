const assert=require('node:assert/strict');
const fs=require('node:fs');
const client=require('./client_harness.js')();

assert.match(fs.readFileSync('web/index.html','utf8'),/id="cancel"[^]*?<button id="destroy" class="danger" aria-pressed="false">Destroy · No refund<\/button><\/section>/);

client.run(`(async()=>{
  const assert=(value,message)=>{if(!value)throw Error(message);};
  const town={name:'Demolition',wood:65,food:40,coins:20,level:1,harvested:0,map_size:120,
    buildings:[{kind:'hall',x:3,y:3},{kind:'home',x:5,y:5},{kind:'wooden_bench',x:8,y:5}],
    roads:[{x:7,y:7}],map_resources:[{kind:'tree',x:6,y:5}],villagers:[]};
  world={world_id:'destroy-test',shared_map:{},towns:{own:town,friend:{...town,buildings:[{kind:'home',x:10,y:10}],roads:[{x:11,y:11}]}}};
  identity={town_id:'own'};viewed='own';
  const commands=[];let release,fail=false,refreshed=false;
  request=async(path,data)=>{commands.push(data);await new Promise(resolve=>release=resolve);if(fail)throw Error('Rejected destruction');};
  refresh=async options=>{
    assert(options.afterCommand,'Destruction requires a snapshot newer than any pending poll');
    refreshed=true;
    if(!fail){const data=commands.at(-1);if(data.action==='destroy')town.buildings.splice(data.index,1);else town.roads=[];}
    render();
  };
  $('destroy').onclick();
  assert(tool==='destroy'&&panel==='build'&&selection===null,'Toggle activates persistent destroy mode');
  assert($('mode-hint').textContent.includes('No refund')&&canvas.style.cursor==='crosshair','Destroy mode has clear feedback');
  await selectTile(3,3);assert(commands.length===0,'Town hall remains protected');
  await selectTile(6,5);assert(commands.length===0,'Natural resources remain protected');
  await selectTile(10,10);await selectTile(11,11);assert(commands.length===0,'Other towns buildings and roads remain protected');
  viewed='friend';await selectTile(10,10);assert(commands.length===0,'Visitor mode cannot destroy');viewed='own';

  hitTargets=[{type:'building',index:1,townId:'friend',left:0,right:10,top:0,bottom:10}];
  await canvas.onclick({clientX:5,clientY:5});assert(commands.length===0,'Sprite clicks preserve ownership');
  hitTargets=[{type:'resource',index:0,left:0,right:10,top:0,bottom:10}];
  await canvas.onclick({clientX:5,clientY:5});assert(commands.length===0,'A foreground tree cannot destroy the building behind it');
  hitTargets=[{type:'building',index:1,townId:'own',left:0,right:10,top:0,bottom:10}];
  const pending=canvas.onclick({clientX:5,clientY:5});
  await canvas.onclick({clientX:5,clientY:5});
  assert(commands.length===1&&destroying,'Repeated pending clicks send one command');
  assert(commands[0].action==='destroy'&&commands[0].index===1&&commands[0].world_id==='destroy-test','Sprite click targets the clicked building');
  assert(town.buildings.length===3,'Client does not mutate authoritative buildings');
  release();await pending;
  assert(refreshed&&!destroying&&tool==='destroy'&&panel==='build','Mode stays active after host confirmation');
  const second=selectTile(8,5);assert(commands.at(-1).index===1,'Next destruction uses refreshed building indices');release();await second;

  fail=true;const failed=selectTile(7,7);release();await failed;
  assert($('notice').textContent==='Rejected destruction'&&tool==='destroy'&&!destroying&&town.roads.length===1,'Rejected removal shows its error and permits retry');
  fail=false;hitTargets=[];
  const [roadX,roadY]=point(7,7),road=canvas.onclick({clientX:roadX,clientY:roadY+tileH/2});
  assert(commands.at(-1).action==='remove_road'&&commands.at(-1).x===7&&commands.at(-1).y===7,'Ground clicks remove roads');release();await road;
  assert(town.roads.length===0&&tool==='destroy','Road removal keeps destroy active');

  $('destroy').onclick();assert(tool===null&&canvas.style.cursor==='','Clicking the toggle again turns it off');
  $('destroy').onclick();$('cancel').onclick();assert(tool===null,'Cancel ends destroy mode');
  $('destroy').onclick();$('move').onclick();assert(tool==='move','Choosing move replaces destroy mode');
  $('destroy').onclick();openPanel(null);assert(tool===null,'Closing the panel ends destroy mode');
})()`).then(()=>console.log('Destroy tool checks passed')).catch(error=>{console.error(error);process.exitCode=1;});
