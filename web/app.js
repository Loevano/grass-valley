const $ = id => document.getElementById(id);
const canvas = $('world'), ctx = canvas.getContext('2d');
let identities = {}, identity = null;
try {
  identities = JSON.parse(localStorage.getItem('lantern-identities') || '{}') || {};
  const legacy = JSON.parse(localStorage.getItem('lantern-identity') || 'null');
  if(legacy && !identities.legacy) identities.legacy = legacy;
} catch { identities = {}; }
let world = {towns:{}}, viewed = null, tool = null, moveIndex = null;
let catalog = {worlds:[], can_manage:false}, menuStarted = false, refreshing = null, menuBusy = false, deletingWorld = null;
let tileW = 46, tileH = 23, originX = 0, originY = 0;
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
for(const [kind,cost] of Object.entries(costs)){const button=document.createElement('button');button.textContent=`${labels[kind]} · ${cost} wood`;button.onclick=()=>{tool=kind;moveIndex=null;render();};$('buildings').append(button);}
$('move').onclick=()=>{tool='move';moveIndex=null;render();};
$('cancel').onclick=()=>{tool=null;moveIndex=null;render();};
$('upgrade').onclick=()=>act({action:'upgrade'});
async function refresh(){
  if(refreshing)return refreshing;
  refreshing=(async()=>{
    const next=await request('/api/world');
    if(next.world_id!==world.world_id){tool=null;moveIndex=null;viewed=null;}
    world=next;identity=identities[world.world_id] || null;
    catalog=await request('/api/worlds');
    if(!world.towns[viewed])viewed=identity?.town_id || Object.keys(world.towns)[0];
    render();renderWorldMenu();
    if(!menuStarted || !world.world_id){if(!$('world-menu').open)$('world-menu').showModal();menuStarted=true;}
  })();
  try{await refreshing;}finally{refreshing=null;}
}
function render(){
  const own=world.towns[identity?.town_id], town=world.towns[viewed];
  $('join').hidden=!!own || !world.world_id;$('controls').hidden=!own;
  $('view-title').textContent=town?.name || 'A place to call home';
  $('resources').textContent=own?`Wood ${Math.floor(own.wood)} · Food ${Math.floor(own.food)} · Coins ${own.coins} · Villagers ${own.villagers.length}`:'Your village awaits';
  if(own){$('town-name').textContent=own.name;$('hint').textContent=tool==='move'?(moveIndex===null?'Click a building to move it.':'Click open grass.'):tool?`Click open grass to place a ${labels[tool].toLowerCase()}.`:'Choose a building, then click open grass.';$('progress').textContent=`Level ${own.level} · Timber roots: ${Math.min(30,own.harvested)}/30 wood harvested`;$('upgrade').disabled=own.level>=2;}
  $('towns').replaceChildren();
  for(const [id,t] of Object.entries(world.towns)){const b=document.createElement('button');b.textContent=t.name+(id===identity?.town_id?' · Yours':'');b.classList.toggle('selected',id===viewed);b.onclick=()=>{viewed=id;tool=null;moveIndex=null;render();};$('towns').append(b);}
  $('villagers').replaceChildren();
  if(town)for(const [i,v] of town.villagers.entries()){
    const div=document.createElement('div');div.className='villager';
    const name=document.createElement('strong');name.textContent=v.name;div.append(name);
    const skills=document.createElement('small');skills.textContent=`Woodcutting ${v.woodcutting} · Farming ${v.farming}`;div.append(skills);
    const activity=document.createElement('small');activity.textContent=v.job==='idle'?'Resting':v.work_target?(v.job==='woodcutting'?'Gathering at the forest':'Tending crops by the water'):'Waiting for a workplace near '+(v.job==='woodcutting'?'trees':'water');div.append(activity);
    const select=document.createElement('select');select.setAttribute('aria-label',`Job for ${v.name}`);
    for(const [job,label] of [['idle','Resting'],['woodcutting','Woodcutting'],['farming','Farming']]){const option=document.createElement('option');option.value=job;option.textContent=label;select.append(option);}
    select.value=v.job;select.disabled=viewed!==identity?.town_id;select.onchange=()=>act({action:'job',index:i,job:select.value});div.append(select);$('villagers').append(div);
  }
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
function draw(){
  const rect=canvas.getBoundingClientRect(),dpr=window.devicePixelRatio||1;
  canvas.width=rect.width*dpr;canvas.height=rect.height*dpr;ctx.scale(dpr,dpr);
  tileW=Math.min(54,(rect.width-30)/12);tileH=tileW/2;originX=rect.width/2;originY=Math.max(140,(rect.height-12*tileH)/2);
  const town=world.towns[viewed];
  const resources=town?.map_resources || [];
  const water=new Set(resources.filter(r=>r.kind==='water').map(r=>`${r.x},${r.y}`));
  for(let x=0;x<12;x++)for(let y=0;y<12;y++){
    const [px,py]=point(x,y),wet=water.has(`${x},${y}`);
    polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],wet?((x+y)%2?'#467f91':'#518d9d'):((x+y)%2?'#638457':'#6a8b5c'),wet?'#76a9ac':'#789766');
    if(wet){ctx.strokeStyle='#b4d7ce99';ctx.beginPath();ctx.moveTo(px-tileW*.16,py+tileH*.48);ctx.lineTo(px+tileW*.12,py+tileH*.63);ctx.stroke();}
  }
  const buildings=town?.buildings || [{kind:'hall',x:5,y:5}];
  const villagers=(town?.villagers || []).map((v,i)=>({kind:'villager',villager:v,index:i,x:v.work_target?.x ?? 3+(i%5),y:v.work_target?.y ?? 8+Math.floor(i/5)}));
  const objects=[...buildings,...resources.filter(r=>r.kind!=='water'),...villagers];
  for(const b of objects.sort((a,b)=>(a.x+a.y)-(b.x+b.y))){
    if(b.kind==='tree'||b.kind==='rock'){drawResource(b);continue;}
    if(b.kind==='villager'){
      const [px,py]=point(b.x,b.y),i=b.index,x=px+(i%3-1)*7,y=py+tileH*.7+Math.floor(i/3)*2;
      ctx.fillStyle=['#e7c68c','#b7cbd2','#d7a48d'][i%3];ctx.beginPath();ctx.arc(x,y-4,3,0,Math.PI*2);ctx.fill();ctx.fillStyle='#344b48';ctx.fillRect(x-3,y,6,8);
      if(b.villager.work_target){ctx.strokeStyle=b.villager.job==='woodcutting'?'#d4dbcb':'#d6c478';ctx.beginPath();ctx.moveTo(x+3,y+3);ctx.lineTo(x+7,y-4);ctx.stroke();}
      continue;
    }
    const [x,y]=point(b.x,b.y),w=tileW*.38,h=b.kind==='hall'?42:27;
    ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(x,y+tileH*.7,w+6,9,0,0,Math.PI*2);ctx.fill();
    if(b.kind==='farm'){for(let r=0;r<4;r++){const [px,py]=point(b.x+r*.18,b.y+.1);polygon([[px,py+4],[px+10,py+9],[px-7,py+17],[px-17,py+12]],r%2?'#b8b458':'#80764a');}continue;}
    polygon([[x-w,y],[x,y+10],[x,y+10-h],[x-w,y-h]],colors[b.kind]);
    polygon([[x,y+10],[x+w,y],[x+w,y-h],[x,y+10-h]],'#857659');
    polygon([[x-w-3,y-h],[x,y-h-15],[x+w+3,y-h],[x,y-h+11]],b.kind==='hall'?'#5a747b':'#80574b');
    ctx.fillStyle='#3d4438';ctx.fillRect(x-6,y-h+13,5,9);
    if(b.kind==='hall'){ctx.fillStyle='#d6c478';ctx.fillRect(x,y-h-30,2,17);polygon([[x+2,y-h-30],[x+16,y-h-26],[x+2,y-h-21]],'#d6c478');}
  }
}
canvas.onclick=async event=>{
  if(!tool || viewed!==identity?.town_id)return;
  const rect=canvas.getBoundingClientRect(),dx=event.clientX-rect.left-originX,dy=event.clientY-rect.top-originY;
  const x=Math.floor(dx/tileW+dy/tileH),y=Math.floor(dy/tileH-dx/tileW);
  if(x<0||y<0||x>=12||y>=12)return;
  const resource=world.towns[viewed].map_resources?.find(r=>r.x===x&&r.y===y);
  if(resource){notice(resource.kind==='tree'?'Forest: place a woodcutter within 3 tile steps to gather wood.':resource.kind==='water'?'Water: place a farm within 3 tile steps to grow food.':'Rock outcrop: choose open grass for buildings.');return;}
  if(tool==='move'){if(moveIndex===null){const index=world.towns[viewed].buildings.findIndex(b=>b.x===x&&b.y===y);if(index<0){notice('Click the tile beneath a building.');return;}moveIndex=index;render();}else{await act({action:'move',index:moveIndex,x,y});moveIndex=null;render();}}
  else await act({action:'build',kind:tool,x,y});
};
window.addEventListener('resize',draw);
refresh().catch(()=>notice('Cannot reach the host. Check the connection.'));
setInterval(()=>{if(document.activeElement?.tagName!=='SELECT')refresh().catch(()=>notice('Connection lost. Waiting for the host…'));},2000);
