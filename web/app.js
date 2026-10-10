const $ = id => document.getElementById(id);
const canvas = $('world'), ctx = canvas.getContext('2d');
let identity;
try { identity = JSON.parse(localStorage.getItem('lantern-identity') || 'null'); } catch { identity = null; }
let world = {towns:{}}, viewed = identity?.town_id, tool = null, moveIndex = null;
let tileW = 46, tileH = 23, originX = 0, originY = 0;
const labels = {hall:'Town hall',home:'Cottage',woodcutter:'Woodcutter',farm:'Farm',storehouse:'Storehouse'};
const costs = {home:20,woodcutter:15,farm:15,storehouse:25};
const colors = {hall:'#a88861',home:'#c5ad80',woodcutter:'#947b55',farm:'#bc9b5c',storehouse:'#8c9b8b'};
function notice(message) { $('notice').textContent=message; $('notice').style.display='block'; clearTimeout(notice.timer); notice.timer=setTimeout(()=>$('notice').style.display='none',4500); }
async function request(path, data) {
  const response = await fetch(path,{method:data?'POST':'GET',headers:{'Content-Type':'application/json','Authorization':identity?.token || ''},body:data?JSON.stringify(data):undefined});
  const result = await response.json(); if(!response.ok) throw Error(result.error || 'Connection failed'); return result;
}
async function act(data) { try { await request('/api/command',data); await refresh(); } catch(e) {notice(e.message);} }
$('join-form').onsubmit=async event=>{event.preventDefault();try{identity=await request('/api/join',{name:$('name').value});localStorage.setItem('lantern-identity',JSON.stringify(identity));viewed=identity.town_id;await refresh();}catch(e){notice(e.message);}};
for(const [kind,cost] of Object.entries(costs)){const button=document.createElement('button');button.textContent=`${labels[kind]} · ${cost} wood`;button.onclick=()=>{tool=kind;moveIndex=null;render();};$('buildings').append(button);}
$('move').onclick=()=>{tool='move';moveIndex=null;render();};
$('cancel').onclick=()=>{tool=null;moveIndex=null;render();};
$('upgrade').onclick=()=>act({action:'upgrade'});
async function refresh(){world=await request('/api/world');if(!world.towns[viewed])viewed=identity?.town_id || Object.keys(world.towns)[0];render();}
function render(){
  const own=world.towns[identity?.town_id], town=world.towns[viewed];
  $('join').hidden=!!own;$('controls').hidden=!own;
  $('view-title').textContent=town?.name || 'A place to call home';
  $('resources').textContent=own?`Wood ${Math.floor(own.wood)} · Food ${Math.floor(own.food)} · Coins ${own.coins} · Villagers ${own.villagers.length}`:'Your village awaits';
  if(own){$('town-name').textContent=own.name;$('hint').textContent=tool==='move'?(moveIndex===null?'Click a building to move it.':'Click an empty tile.'):tool?`Click a tile to place a ${labels[tool].toLowerCase()}.`:'Choose a building, then click a tile.';$('progress').textContent=`Level ${own.level} · Timber roots: ${Math.min(30,own.harvested)}/30 wood harvested`;$('upgrade').disabled=own.level>=2;}
  $('towns').replaceChildren();
  for(const [id,t] of Object.entries(world.towns)){const b=document.createElement('button');b.textContent=t.name+(id===identity?.town_id?' · Yours':'');b.classList.toggle('selected',id===viewed);b.onclick=()=>{viewed=id;tool=null;moveIndex=null;render();};$('towns').append(b);}
  $('villagers').replaceChildren();
  if(town)for(const [i,v] of town.villagers.entries()){
    const div=document.createElement('div');div.className='villager';
    const name=document.createElement('strong');name.textContent=v.name;div.append(name);
    const skills=document.createElement('small');skills.textContent=`Woodcutting ${v.woodcutting} · Farming ${v.farming}`;div.append(skills);
    const select=document.createElement('select');select.setAttribute('aria-label',`Job for ${v.name}`);
    for(const [job,label] of [['idle','Resting'],['woodcutting','Woodcutting'],['farming','Farming']]){const option=document.createElement('option');option.value=job;option.textContent=label;select.append(option);}
    select.value=v.job;select.disabled=viewed!==identity?.town_id;select.onchange=()=>act({action:'job',index:i,job:select.value});div.append(select);$('villagers').append(div);
  }
  draw();
}
function point(x,y){return [originX+(x-y)*tileW/2,originY+(x+y)*tileH/2];}
function polygon(points,fill,stroke){ctx.beginPath();points.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));ctx.closePath();ctx.fillStyle=fill;ctx.fill();if(stroke){ctx.strokeStyle=stroke;ctx.stroke();}}
function draw(){
  const rect=canvas.getBoundingClientRect(),dpr=window.devicePixelRatio||1;
  canvas.width=rect.width*dpr;canvas.height=rect.height*dpr;ctx.scale(dpr,dpr);
  tileW=Math.min(54,(rect.width-30)/12);tileH=tileW/2;originX=rect.width/2;originY=Math.max(140,(rect.height-12*tileH)/2);
  for(let x=0;x<12;x++)for(let y=0;y<12;y++){const [px,py]=point(x,y);polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],(x+y)%2?'#638457':'#6a8b5c','#789766');}
  const town=world.towns[viewed];
  const buildings=town?.buildings || [{kind:'hall',x:5,y:5}];
  for(const b of [...buildings].sort((a,b)=>(a.x+a.y)-(b.x+b.y))){
    const [x,y]=point(b.x,b.y),w=tileW*.38,h=b.kind==='hall'?42:27;
    ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(x,y+tileH*.7,w+6,9,0,0,Math.PI*2);ctx.fill();
    if(b.kind==='farm'){for(let r=0;r<4;r++){const [px,py]=point(b.x+r*.18,b.y+.1);polygon([[px,py+4],[px+10,py+9],[px-7,py+17],[px-17,py+12]],r%2?'#b8b458':'#80764a');}continue;}
    polygon([[x-w,y],[x,y+10],[x,y+10-h],[x-w,y-h]],colors[b.kind]);
    polygon([[x,y+10],[x+w,y],[x+w,y-h],[x,y+10-h]],'#857659');
    polygon([[x-w-3,y-h],[x,y-h-15],[x+w+3,y-h],[x,y-h+11]],b.kind==='hall'?'#5a747b':'#80574b');
    ctx.fillStyle='#3d4438';ctx.fillRect(x-6,y-h+13,5,9);
    if(b.kind==='hall'){ctx.fillStyle='#d6c478';ctx.fillRect(x,y-h-30,2,17);polygon([[x+2,y-h-30],[x+16,y-h-26],[x+2,y-h-21]],'#d6c478');}
  }
  if(town)town.villagers.forEach((v,i)=>{const [x,y]=point(3+(i%5),8+Math.floor(i/5));ctx.fillStyle=['#e7c68c','#b7cbd2','#d7a48d'][i%3];ctx.beginPath();ctx.arc(x,y-4,3,0,Math.PI*2);ctx.fill();ctx.fillStyle='#344b48';ctx.fillRect(x-3,y,6,8);});
}
canvas.onclick=async event=>{
  if(!tool || viewed!==identity?.town_id)return;
  const rect=canvas.getBoundingClientRect(),dx=event.clientX-rect.left-originX,dy=event.clientY-rect.top-originY;
  const x=Math.floor(dx/tileW+dy/tileH),y=Math.floor(dy/tileH-dx/tileW);
  if(x<0||y<0||x>=12||y>=12)return;
  if(tool==='move'){if(moveIndex===null){const index=world.towns[viewed].buildings.findIndex(b=>b.x===x&&b.y===y);if(index<0){notice('Click the tile beneath a building.');return;}moveIndex=index;render();}else{await act({action:'move',index:moveIndex,x,y});moveIndex=null;render();}}
  else await act({action:'build',kind:tool,x,y});
};
window.addEventListener('resize',draw);
refresh().catch(()=>notice('Cannot reach the host. Check the connection.'));
setInterval(()=>{if(document.activeElement?.tagName!=='SELECT')refresh().catch(()=>notice('Connection lost. Waiting for the host…'));},2000);
