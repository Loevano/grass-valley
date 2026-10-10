const $ = id => document.getElementById(id);
const canvas = $('world'), ctx = canvas.getContext('2d');
let identities = {}, identity = null;
try {
  identities = JSON.parse(localStorage.getItem('lantern-identities') || '{}') || {};
  const legacy = JSON.parse(localStorage.getItem('lantern-identity') || 'null');
  if(legacy && !identities.legacy) identities.legacy = legacy;
} catch { identities = {}; }
let world = {towns:{}}, viewed = null, tool = null, moveIndex = null;
let buildTile = null, building = false;
let panel = null, selection = null, hitTargets = [], keyboardTile = null;
let catalog = {worlds:[], can_manage:false}, menuStarted = false, refreshing = null, menuBusy = false, deletingWorld = null;
let tileW = 46, tileH = 23, originX = 0, originY = 0;
const workers = new Map();
let workerTown = null, lastFrame = null;
const labels = {hall:'Town hall',home:'Cottage',woodcutter:'Woodcutter',farm:'Farm',storehouse:'Storehouse'};
const costs = {home:20,woodcutter:15,farm:15,storehouse:25};
const colors = {hall:'#a88861',home:'#c5ad80',woodcutter:'#947b55',farm:'#bc9b5c',storehouse:'#8c9b8b'};
function notice(message) { $('notice').textContent=message; $('notice').style.display='block'; clearTimeout(notice.timer); notice.timer=setTimeout(()=>$('notice').style.display='none',4500); }
async function request(path, data) {
  const response = await fetch(path,{method:data?'POST':'GET',headers:{'Content-Type':'application/json','Authorization':identity?.token || ''},body:data?JSON.stringify(data):undefined});
  const result = await response.json(); if(!response.ok) throw Error(result.error || 'Connection failed'); return result;
}
async function act(data) { try { await request('/api/command',{...data,world_id:world.world_id}); await refresh(); } catch(e) {notice(e.message);await refresh();} }
$('join-form').onsubmit=async event=>{event.preventDefault();const worldId=world.world_id;try{identity=await request('/api/join',{name:$('name').value,world_id:worldId});identities[worldId]=identity;localStorage.setItem('lantern-identities',JSON.stringify(identities));viewed=identity.town_id;await refresh();}catch(e){notice(e.message);}};
$('world-menu-button').onclick=()=>{$('world-menu-error').textContent='';$('world-menu').showModal();};
$('close-world-menu').onclick=()=>{if(world.world_id)$('world-menu').close();};
$('world-menu').addEventListener('cancel',event=>{if(!world.world_id)event.preventDefault();});
$('cancel-delete-world').onclick=()=>{$('delete-world-dialog').close();deletingWorld=null;};
$('delete-world-dialog').addEventListener('close',()=>{deletingWorld=null;});
async function manageWorld(path,data,closeMenu=false){
  if(menuBusy)return;
  menuBusy=true;
  $('world-menu-error').textContent='';$('delete-world-error').textContent='';
  for(const button of document.querySelectorAll('dialog button'))button.disabled=true;
  try{
    await request(path,data);
    if(path==='/api/worlds/delete'){
      delete identities[data.world_id];localStorage.setItem('lantern-identities',JSON.stringify(identities));
      if(data.world_id==='legacy')localStorage.removeItem('lantern-identity');
      $('delete-world-dialog').close();deletingWorld=null;
    }
    if(refreshing)await refreshing;
    await refresh();
    if(closeMenu)$('world-menu').close();
  }catch(e){$(deletingWorld?'delete-world-error':'world-menu-error').textContent=e.message;}
  finally{menuBusy=false;for(const button of document.querySelectorAll('dialog button'))button.disabled=false;$('close-world-menu').disabled=!world.world_id;}
}
$('new-world-form').onsubmit=event=>{event.preventDefault();manageWorld('/api/worlds/create',{name:$('world-name').value},true);};
$('delete-world-form').onsubmit=event=>{event.preventDefault();if(deletingWorld)manageWorld('/api/worlds/delete',{world_id:deletingWorld.id,confirmation:$('delete-world-name').value});};
function renderWorldMenu(){
  $('new-world-form').hidden=!catalog.can_manage;
  $('close-world-menu').disabled=!world.world_id;
  $('world-menu-description').textContent=catalog.can_manage?'Open a saved world or start a new one. Other worlds pause while you play.':'The host chooses the world. Join the current world below, or wait for the host to open one.';
  const signature=JSON.stringify(catalog);
  if(renderWorldMenu.signature===signature)return;
  renderWorldMenu.signature=signature;
  $('world-list').replaceChildren();
  if(!catalog.worlds.length){const p=document.createElement('p');p.textContent=catalog.can_manage?'No worlds yet. Name your first world below.':'Waiting for the host to start a world…';$('world-list').append(p);}
  for(const entry of catalog.worlds){
    const row=document.createElement('div');row.className='world-entry';
    const name=document.createElement('strong');name.textContent=entry.name;row.append(name);
    const details=document.createElement('small');details.textContent=`${entry.town_count}/4 towns · ${entry.id===catalog.active_world?'Playing now':'Saved'}`;row.append(details);
    if(catalog.can_manage || entry.id===catalog.active_world){const open=document.createElement('button');open.textContent=entry.id===catalog.active_world?'Continue playing':'Open world';open.onclick=()=>catalog.can_manage?manageWorld('/api/worlds/open',{world_id:entry.id},true):$('world-menu').close();row.append(open);}
    if(catalog.can_manage){const remove=document.createElement('button');remove.className='danger quiet';remove.textContent='Destroy world';remove.onclick=()=>{deletingWorld=entry;$('delete-world-description').textContent=`Destroy “${entry.name}”?`;$('delete-world-name').value='';$('delete-world-error').textContent='';$('delete-world-dialog').showModal();};row.append(remove);}
    $('world-list').append(row);
  }
}
function openPanel(next){
  panel=next;selection=null;tool=null;moveIndex=null;buildTile=null;
  render();
}
for(const name of ['build','villagers','valley','help']){
  $(name+'-button').onclick=()=>openPanel(panel===name?null:name);
}
$('close-panel').onclick=()=>{openPanel(null);canvas.focus();};
window.addEventListener('keydown',event=>{
  if(event.key==='Escape' && !$('world-menu').open && !$('delete-world-dialog').open){
    openPanel(null);canvas.focus();
  }
});
function inspect(type,index){
  panel='selection';selection={type,index};tool=null;moveIndex=null;buildTile=null;render();
}
function villagerCard(town,v,index,withLink=false){
  const div=document.createElement('div');div.className='villager';
  const name=document.createElement(withLink?'button':'strong');name.textContent=v.name;
  if(withLink){name.className='villager-name';name.onclick=()=>inspect('villager',index);}div.append(name);
  const skills=document.createElement('small');skills.textContent=`Woodcutting ${v.woodcutting} · Farming ${v.farming}`;div.append(skills);
  const resource=v.job==='woodcutting'?'wood':'food';
  const full=town[resource]>=100+100*town.buildings.filter(b=>b.kind==='storehouse').length;
  const activity=document.createElement('small');activity.textContent=v.job==='idle'?'Resting':v.work_target?(full?`Paused: ${resource} storage full. Spend ${resource} or build a storehouse.`:v.job==='woodcutting'?'Gathering at the forest':'Tending crops by the water'):'Waiting for a workplace near '+(v.job==='woodcutting'?'trees':'water');div.append(activity);
  const select=document.createElement('select');select.setAttribute('aria-label',`Job for ${v.name}`);
  for(const [job,label] of [['idle','Resting'],['woodcutting','Woodcutting'],['farming','Farming']]){const option=document.createElement('option');option.value=job;option.textContent=label;select.append(option);}
  select.value=v.job;select.disabled=viewed!==identity?.town_id;select.onchange=()=>act({action:'job',index,job:select.value});div.append(select);
  return div;
}
function renderSelection(town){
  $('selection-details').replaceChildren();$('hall-details').hidden=true;$('selection-move').hidden=true;
  if(!selection)return;
  if(!town){selection=null;panel=null;return;}
  const item=selection.type==='villager'?town.villagers[selection.index]:selection.type==='building'?town.buildings[selection.index]:town.map_resources[selection.index];
  if(!item){selection=null;panel=null;return;}
  if(selection.type==='villager'){
    $('selection-title').textContent=item.name;$('selection-details').append(villagerCard(town,item,selection.index));return;
  }
  const p=document.createElement('p');
  if(selection.type==='resource'){
    $('selection-title').textContent={tree:'Forest',water:'Water',rock:'Rock outcrop'}[item.kind];
    p.textContent={tree:'Renewable wood source. Place a woodcutter within 3 horizontal or vertical tile steps, then assign a villager to woodcutting.',water:'Renewable source for farming. Place a farm within 3 horizontal or vertical tile steps, then assign a villager to farming.',rock:'This outcrop occupies its tile. Mining is not available yet.'}[item.kind];
  }else{
    $('selection-title').textContent=labels[item.kind];
    p.textContent={hall:'The heart of your settlement. Harvest wood to unlock your first upgrade.',home:'Room for 2 additional villagers, up to 16 in the town. New residents need available housing and sufficient food.',storehouse:`Adds 100 storage for each resource. Town capacity: ${100+100*town.buildings.filter(b=>b.kind==='storehouse').length} wood and food.`,woodcutter:'A workplace for gathering wood from nearby trees.',farm:'A workplace for growing food near water.'}[item.kind];
    if(item.kind==='hall'){
      $('hall-details').hidden=false;
      $('progress').textContent=`Level ${town.level} · Timber roots: ${Math.min(30,town.harvested)}/30 wood harvested`;
      $('upgrade').disabled=viewed!==identity?.town_id || town.level>=2 || town.harvested<30 || town.wood<40;
    }
    $('selection-move').hidden=viewed!==identity?.town_id;
    $('selection-move').textContent=town.coins?'Move building · 1 coin':'Move building · Free';
    $('selection-move').onclick=()=>{const index=selection.index;openPanel('build');tool='move';moveIndex=index;render();};
    $('selection-details').append(p);
    if(item.kind==='woodcutter'||item.kind==='farm'){
      const assigned=town.villagers.map((v,i)=>({v,i,state:workers.get(i)})).filter(w=>w.v.job===(item.kind==='woodcutter'?'woodcutting':'farming')&&w.state?.home?.x===item.x&&w.state?.home?.y===item.y);
      const status=document.createElement('p');
      const nearby=town.map_resources.some(r=>r.kind===(item.kind==='woodcutter'?'tree':'water')&&Math.abs(r.x-item.x)+Math.abs(r.y-item.y)<=3);
      status.textContent=!nearby?'Needs a source within 3 tile steps.':!assigned.length?'Source nearby. Assign villagers to start work.':`${assigned.length} assigned worker${assigned.length===1?'':'s'}.`;
      $('selection-details').append(status);
      for(const w of assigned)$('selection-details').append(villagerCard(town,w.v,w.i,true));
    }
  }
  if(selection.type==='resource')$('selection-details').append(p);
}
for(const [kind,cost] of Object.entries(costs)){const button=document.createElement('button');button.textContent=`${labels[kind]} · ${cost} wood`;button.onclick=()=>{panel='build';selection=null;tool=kind;moveIndex=null;buildTile=null;render();};$('buildings').append(button);}
$('move').onclick=()=>{panel='build';selection=null;tool='move';moveIndex=null;buildTile=null;render();};
$('cancel').onclick=()=>{tool=null;moveIndex=null;buildTile=null;render();};
$('confirm-build').onclick=async()=>{
  const town=world.towns[viewed];
  if(building || !buildTile || !costs[tool] || viewed!==identity?.town_id || !town || town.wood<costs[tool])return;
  const selected=buildTile,kind=tool;
  building=true;render();
  try{
    await request('/api/command',{action:'build',kind,...selected,world_id:world.world_id});
    if(buildTile===selected)buildTile=null;
    await refresh();
  }catch(e){notice(e.message);await refresh();}
  finally{building=false;render();}
};
$('upgrade').onclick=()=>act({action:'upgrade'});
async function refresh(){
  if(refreshing)return refreshing;
  refreshing=(async()=>{
    const next=await request('/api/world');
    if(next.world_id!==world.world_id){tool=null;moveIndex=null;buildTile=null;viewed=null;selection=null;panel=null;keyboardTile=null;}
    world=next;identity=identities[world.world_id] || null;
    catalog=await request('/api/worlds');
    if(!world.towns[viewed]){viewed=identity?.town_id || Object.keys(world.towns)[0];selection=null;panel=null;tool=null;moveIndex=null;buildTile=null;keyboardTile=null;}
    render();renderWorldMenu();
    if(!menuStarted || !world.world_id){if(!$('world-menu').open)$('world-menu').showModal();menuStarted=true;}
  })();
  try{await refreshing;}finally{refreshing=null;}
}
function render(){
  const own=world.towns[identity?.town_id], town=world.towns[viewed];
  draw();
  renderSelection(town);
  $('join').hidden=!!own || !world.world_id || !!panel;
  $('controls').hidden=panel!=='build' || !own || viewed!==identity?.town_id;
  for(const name of ['selection','villagers','valley','help'])$(name+'-panel').hidden=panel!==name;
  $('side-panel').hidden=!panel && $('join').hidden;
  $('close-panel').hidden=!panel;
  $('game').classList.toggle('panel-open',!$('side-panel').hidden);
  for(const name of ['build','villagers','valley','help']){
    $(name+'-button').setAttribute('aria-expanded',String(panel===name));
    $(name+'-button').classList.toggle('selected',panel===name);
  }
  $('build-button').disabled=!own || viewed!==identity?.town_id;
  $('villagers-button').disabled=!town;
  $('move').textContent=own?.coins?'Move a building · 1 coin':'Move a building · Free';
  $('mode-hint').textContent=tool==='move'?(moveIndex===null?'Moving: click a building. Escape to cancel.':'Moving: click open grass to move here. Escape to cancel.'):tool?`Building ${labels[tool].toLowerCase()}: select open grass, then confirm. Escape to cancel.`:'Click a building, villager, or resource to learn more.';
  $('view-title').textContent=town?.name || 'A place to call home';
  $('resources-empty').hidden=!!own;$('resource-totals').hidden=!own;
  if(own){
    $('resource-wood').textContent=Math.floor(own.wood);
    $('resource-food').textContent=Math.floor(own.food);
    $('resource-coins').textContent=own.coins;
    $('resource-villagers').textContent=own.villagers.length;
  }
  if(buildTile && (!town || town.buildings.some(b=>b.x===buildTile.x&&b.y===buildTile.y) || town.map_resources.some(r=>r.x===buildTile.x&&r.y===buildTile.y)))buildTile=null;
  $('confirm-build').hidden=!buildTile || !costs[tool] || viewed!==identity?.town_id;
  $('confirm-build').disabled=building || !own || own.wood<costs[tool];
  $('confirm-build').textContent=building?'Building…':`Build here · ${costs[tool] || 0} wood`;
  for(const button of $('buildings').children)button.classList.toggle('selected',button.textContent===`${labels[tool]} · ${costs[tool]} wood`);
  if(own){$('town-name').textContent=own.name;$('hint').textContent=tool==='move'?(moveIndex===null?'Click a building to move it.':'Click open grass.'):tool?(buildTile?`Tile selected for a ${labels[tool].toLowerCase()}. ${own.wood<costs[tool]?'Not enough wood.': 'Choose Build here to place it, or select another tile.'}`:`Click open grass to select a tile for a ${labels[tool].toLowerCase()}.`):'Choose a building, then select an open tile.';}
  $('towns').replaceChildren();
  for(const [id,t] of Object.entries(world.towns)){const b=document.createElement('button');b.textContent=t.name+(id===identity?.town_id?' · Yours':'');b.classList.toggle('selected',id===viewed);b.onclick=()=>{viewed=id;tool=null;moveIndex=null;buildTile=null;selection=null;panel=null;keyboardTile=null;render();};$('towns').append(b);}
  $('villagers').replaceChildren();
  if(town)for(const [i,v] of town.villagers.entries())$('villagers').append(villagerCard(town,v,i,true));
  draw();
}
function point(x,y){return [originX+(x-y)*tileW/2,originY+(x+y)*tileH/2];}
function polygon(points,fill,stroke){ctx.beginPath();points.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));ctx.closePath();ctx.fillStyle=fill;ctx.fill();if(stroke){ctx.strokeStyle=stroke;ctx.stroke();}}
function drawResource(r){
  const [x,y]=point(r.x,r.y),s=tileW/46;
  ctx.save();ctx.translate(x,y);ctx.scale(s,s);
  ctx.fillStyle='#223b2955';ctx.beginPath();ctx.ellipse(0,12,16,6,0,0,Math.PI*2);ctx.fill();
  if(r.kind==='tree'){
    ctx.fillStyle='#775b3e';ctx.fillRect(-3,-14,6,27);
    const shade=(r.x+r.y)%2;
    polygon([[-17,-7],[0,-39],[17,-7],[0,0]],shade?'#355b3e':'#3e6642');
    polygon([[-14,-20],[0,-49],[14,-20],[0,-13]],shade?'#4e7847':'#56814c');
    polygon([[-10,-32],[0,-57],[10,-32],[0,-26]],'#719451');
  }else{
    polygon([[-16,10],[-11,-4],[2,-10],[16,0],[13,12],[0,17]],'#89928a','#a0aaa0');
    polygon([[-11,-4],[2,-10],[16,0],[0,5]],'#aab3a4');
    polygon([[0,5],[16,0],[13,12],[0,17]],'#707e78');
  }
  ctx.restore();
}
function animatedVillagers(town, seconds){
  const townKey=`${world.world_id}/${viewed}`;
  if(workerTown!==townKey){workers.clear();workerTown=townKey;}
  if(!town)return [];
  const layout=JSON.stringify([town.buildings,town.map_resources]);
  return town.villagers.map((villager,index)=>{
    const signature=JSON.stringify([layout,villager.job,villager.work_target]);
    let state=workers.get(index);
    if(!state || state.signature!==signature){
      state=Object.assign(WorkerAnimation.plan(town,villager,index,state),{signature});workers.set(index,state);
    }
    const resource=villager.job==='woodcutting'?'wood':'food';
    const active=town[resource]<100+100*town.buildings.filter(b=>b.kind==='storehouse').length;
    WorkerAnimation.advance(state,seconds,active);
    return {kind:'villager',villager,index,x:state.x,y:state.y,state,active};
  });
}
function drawVillager(b){
  const [px,py]=point(b.x,b.y),s=b.state;
  const walking=s.path.length>0,working=s.phase==='working'&&b.active;
  const stride=walking?Math.sin(s.clock*13):0;
  const chopping=working&&b.villager.job==='woodcutting';
  const swing=working?(Math.sin(s.clock*(chopping?5:7))+1)/2:0;
  ctx.save();ctx.translate(px+(b.index%3-1)*3,py+tileH*.65+Math.floor(b.index/3));ctx.scale(tileW/46,tileW/46);
  ctx.fillStyle='#223b2966';ctx.beginPath();ctx.ellipse(0,2,7,3,0,0,Math.PI*2);ctx.fill();
  const towardSource=s.source?(s.source.x-s.x)-(s.source.y-s.y):0;
  ctx.scale(working&&towardSource!==0?Math.sign(towardSource):s.facing,1);
  ctx.strokeStyle='#283c38';ctx.lineWidth=2.5;ctx.lineCap='round';
  ctx.beginPath();ctx.moveTo(-2,-4);ctx.lineTo(-2-stride*3,1);ctx.moveTo(2,-4);ctx.lineTo(2+stride*3,1);ctx.stroke();
  const bob=walking?Math.abs(stride)*1.2:chopping?swing*2.5:working?swing:Math.sin(s.clock*2)*.3;
  ctx.translate(0,-bob);
  ctx.fillStyle=['#658ca0','#b48065','#9a9862'][b.index%3];ctx.fillRect(-4,-13,8,10);
  ctx.fillStyle='#e7c68c';ctx.beginPath();ctx.arc(0,-17,4,0,Math.PI*2);ctx.fill();
  ctx.fillStyle='#584638';ctx.beginPath();ctx.arc(0,-18,4,Math.PI,Math.PI*2);ctx.fill();
  ctx.strokeStyle='#e7c68c';ctx.lineWidth=2;
  ctx.beginPath();ctx.moveTo(-3,-11);ctx.lineTo(chopping?6:-5-stride*2,chopping?-14+swing*5:-6);ctx.moveTo(3,-11);ctx.lineTo(7,chopping?-14+swing*5:-9-swing*4);ctx.stroke();
  if(working){
    ctx.save();ctx.translate(chopping?6:5,chopping?-13+swing*4:-11);ctx.rotate(chopping?-1+swing*2.5:-1.4+swing*2);
    ctx.strokeStyle='#a78454';ctx.lineWidth=chopping?2.5:2;ctx.beginPath();ctx.moveTo(0,chopping?4:3);ctx.lineTo(0,chopping?-16:-10);ctx.stroke();
    ctx.fillStyle='#cbd7d2';
    if(chopping)polygon([[-1,-17],[10,-18],[11,-10],[-1,-11]],'#e4ece7','#6b7d79');
    else ctx.fillRect(-1,-11,8,2);
    ctx.restore();
    if(chopping&&swing>.85){
      ctx.fillStyle='#e4c98a';for(let i=0;i<4;i++)ctx.fillRect(13+i*3,-6-i*3+swing*7,3,2);
    }
  }
  if(s.phase==='returning'||s.phase==='depositing'){
    ctx.fillStyle=b.villager.job==='woodcutting'?'#997044':'#d6c478';ctx.fillRect(2,-8,11,5);
    ctx.fillStyle='#e1c593';ctx.beginPath();ctx.arc(13,-5.5,2.5,0,Math.PI*2);ctx.fill();
  }
  ctx.restore();
}
function draw(seconds=0){
  const rect=canvas.getBoundingClientRect(),dpr=window.devicePixelRatio||1;
  const width=Math.round(rect.width*dpr),height=Math.round(rect.height*dpr);
  if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}
  ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,rect.width,rect.height);
  tileW=Math.min(54,(rect.width-30)/12);tileH=tileW/2;originX=rect.width/2;originY=Math.max(140,(rect.height-12*tileH)/2);
  hitTargets=[];
  const town=world.towns[viewed];
  const resources=town?.map_resources || [];
  const water=new Set(resources.filter(r=>r.kind==='water').map(r=>`${r.x},${r.y}`));
  for(let x=0;x<12;x++)for(let y=0;y<12;y++){
    const [px,py]=point(x,y),wet=water.has(`${x},${y}`);
    polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],wet?((x+y)%2?'#467f91':'#518d9d'):((x+y)%2?'#638457':'#6a8b5c'),wet?'#76a9ac':'#789766');
    if(buildTile?.x===x&&buildTile?.y===y&&costs[tool]&&viewed===identity?.town_id){
      ctx.save();ctx.lineWidth=3;
      polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],'#e5d58a88','#fff0b1');
      ctx.restore();
    }
    const selected=selection?.type==='building'?town?.buildings[selection.index]:selection?.type==='resource'?resources[selection.index]:null;
    if((selected?.x===x&&selected?.y===y)||(keyboardTile?.x===x&&keyboardTile?.y===y)){
      ctx.save();ctx.lineWidth=3;polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],'#fff0b133','#fff0b1');ctx.restore();
    }
    if(wet){ctx.strokeStyle='#b4d7ce99';ctx.beginPath();ctx.moveTo(px-tileW*.16,py+tileH*.48);ctx.lineTo(px+tileW*.12,py+tileH*.63);ctx.stroke();}
  }
  const buildings=town?.buildings || [{kind:'hall',x:5,y:5}];
  const villagers=animatedVillagers(town,seconds);
  const objects=[...buildings,...resources.filter(r=>r.kind!=='water'),...villagers];
  for(const b of objects.sort((a,b)=>(a.x+a.y)-(b.x+b.y))){
    const [px,py]=point(b.x,b.y),scale=tileW/46;
    const type=b.kind==='villager'?'villager':resources.includes(b)?'resource':'building';
    const index=type==='villager'?b.index:type==='resource'?resources.indexOf(b):buildings.indexOf(b);
    const centerX=px+(type==='villager'?(b.index%3-1)*3:0);
    const baseY=py+(type==='villager'?tileH*.65+Math.floor(b.index/3):tileH/2);
    hitTargets.push({type,index,left:centerX-(type==='villager'?9*scale:tileW*.45),right:centerX+(type==='villager'?9*scale:tileW*.45),top:baseY-(b.kind==='tree'?70:b.kind==='hall'?75:type==='villager'?26:b.kind==='farm'?0:45)*scale,bottom:baseY+(type==='villager'?5*scale:tileH/2)});
    if(type==='villager'&&selection?.type==='villager'&&selection.index===b.index){
      ctx.strokeStyle='#fff0b1';ctx.lineWidth=2;ctx.beginPath();ctx.ellipse(centerX,baseY,10*scale,5*scale,0,0,Math.PI*2);ctx.stroke();
    }
    if(b.kind==='tree'||b.kind==='rock'){drawResource(b);continue;}
    if(b.kind==='villager'){
      drawVillager(b);
      continue;
    }
    if(b.kind==='farm'){
      const [x,y]=point(b.x,b.y),w=tileW*.38;
      ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(x,y+tileH*.7,w+6,9,0,0,Math.PI*2);ctx.fill();
      for(let r=0;r<4;r++){const [px,py]=point(b.x+r*.18,b.y+.1);polygon([[px,py+4],[px+10,py+9],[px-7,py+17],[px-17,py+12]],r%2?'#b8b458':'#80764a');}
      continue;
    }
    const [x,top]=point(b.x,b.y),y=top+tileH/2,s=tileW/46;
    const w=tileW*.38,depth=w*tileH/tileW,h=(b.kind==='hall'?42:27)*s;
    // Wall bases share the tile's ground plane and its isometric slope.
    ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(x,y+depth*.25,w+3*s,depth+s,0,0,Math.PI*2);ctx.fill();
    polygon([[x-w,y],[x,y+depth],[x,y+depth-h],[x-w,y-h]],colors[b.kind]);
    polygon([[x,y+depth],[x+w,y],[x+w,y-h],[x,y+depth-h]],'#857659');
    polygon([[x-w,y],[x,y+depth],[x,y+depth-3*s],[x-w,y-3*s]],'#797665');
    polygon([[x,y+depth],[x+w,y],[x+w,y-3*s],[x,y+depth-3*s]],'#5e6255');
    polygon([[x-w-3*s,y-h],[x,y-h-15*s],[x+w+3*s,y-h],[x,y-h+depth+1*s]],b.kind==='hall'?'#5a747b':'#80574b');
    ctx.fillStyle='#3d4438';ctx.fillRect(x-6*s,y-h+13*s,5*s,9*s);
    if(b.kind==='hall'){ctx.fillStyle='#d6c478';ctx.fillRect(x,y-h-30*s,2*s,17*s);polygon([[x+2*s,y-h-30*s],[x+16*s,y-h-26*s],[x+2*s,y-h-21*s]],'#d6c478');}
  }
}
async function selectTile(x,y){
  const town=world.towns[viewed];
  if(!town || x<0||y<0||x>=12||y>=12)return;
  const resource=town.map_resources.findIndex(r=>r.x===x&&r.y===y);
  const index=town.buildings.findIndex(b=>b.x===x&&b.y===y);
  if(!tool){
    if(index>=0)inspect('building',index);
    else if(resource>=0)inspect('resource',resource);
    else openPanel(null);
    return;
  }
  if(viewed!==identity?.town_id)return;
  if(resource>=0){notice('Choose open grass, away from resources.');return;}
  if(tool==='move'){
    if(moveIndex===null){if(index<0){notice('Click the tile beneath a building.');return;}moveIndex=index;render();}
    else{await act({action:'move',index:moveIndex,x,y});moveIndex=null;render();}
  }else{
    if(building)return;
    if(index>=0){notice('Choose open grass, away from existing buildings.');return;}
    buildTile={x,y};render();
  }
}
canvas.onclick=async event=>{
  const rect=canvas.getBoundingClientRect(),px=event.clientX-rect.left,py=event.clientY-rect.top;
  keyboardTile=null;
  if(!tool){
    const hit=[...hitTargets].reverse().find(h=>px>=h.left&&px<=h.right&&py>=h.top&&py<=h.bottom);
    if(hit){inspect(hit.type,hit.index);return;}
  }
  const dx=px-originX,dy=py-originY;
  await selectTile(Math.floor(dx/tileW+dy/tileH),Math.floor(dy/tileH-dx/tileW));
};
canvas.addEventListener('keydown',event=>{
  const directions={ArrowUp:[-1,0],ArrowDown:[1,0],ArrowLeft:[0,1],ArrowRight:[0,-1]};
  if(directions[event.key]){
    event.preventDefault();const [dx,dy]=directions[event.key];
    keyboardTile=keyboardTile || {x:5,y:5};
    keyboardTile={x:Math.max(0,Math.min(11,keyboardTile.x+dx)),y:Math.max(0,Math.min(11,keyboardTile.y+dy))};draw();
  }else if(event.key==='Enter'||event.key===' '){event.preventDefault();const tile=keyboardTile || {x:5,y:5};selectTile(tile.x,tile.y);}
});
window.addEventListener('resize',()=>draw());
function animate(now){
  draw(lastFrame===null?0:Math.min((now-lastFrame)/1000,.1));lastFrame=now;
  requestAnimationFrame(animate);
}
requestAnimationFrame(animate);
refresh().catch(()=>notice('Cannot reach the host. Check the connection.'));
setInterval(()=>{if(document.activeElement?.tagName!=='SELECT')refresh().catch(()=>notice('Connection lost. Waiting for the host…'));},2000);
