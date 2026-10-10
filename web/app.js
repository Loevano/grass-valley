const $ = id => document.getElementById(id);
const canvas = $('world'), ctx = canvas.getContext('2d');
let identities = {}, identity = null;
try {
  identities = JSON.parse(localStorage.getItem('lantern-identities') || '{}') || {};
  const legacy = JSON.parse(localStorage.getItem('lantern-identity') || 'null');
  if(legacy && !identities.legacy) identities.legacy = legacy;
} catch { identities = {}; }
let world = {towns:{}}, viewed = null, tool = null, moveIndex = null;
let buildTile = null, building = false, hoverTile = null;
let acreFarmIndex = null;
let panel = null, selection = null, hitTargets = [], keyboardTile = null;
let catalog = {worlds:[], can_manage:false}, menuStarted = false, refreshing = null, menuBusy = false, deletingWorld = null;
let tileW = 46, tileH = 23, originX = 0, originY = 0;
const workers = new Map();
let workerTown = null, lastFrame = null;
const walkKeys = new Set();
const defaultBindings={up:'w',left:'a',down:'s',right:'d',camera:'y'};
const bindingLabels={up:'Walk up',left:'Walk left',down:'Walk down',right:'Walk right',camera:'Toggle camera lock'};
let keybindings={...defaultBindings}, cameraLocked=true;
try{
  const saved=JSON.parse(localStorage.getItem('lantern-settings') || 'null');
  if(saved?.keybindings && Object.keys(defaultBindings).every(action=>/^[a-z0-9]$/.test(saved.keybindings[action]))
    && new Set(Object.values(saved.keybindings)).size===Object.keys(defaultBindings).length){
    keybindings=Object.fromEntries(Object.keys(defaultBindings).map(action=>[action,saved.keybindings[action]]));
  }
  if(typeof saved?.cameraLocked==='boolean')cameraLocked=saved.cameraLocked;
}catch{}
function saveSettings(){
  try{localStorage.setItem('lantern-settings',JSON.stringify({keybindings,cameraLocked}));}
  catch{notice('Settings apply for this session. Browser storage is unavailable.');}
}
function setCameraLock(locked){
  cameraLocked=locked;
  if(playerView && viewed===identity?.town_id){
    playerView.following=locked;
    if(locked&&typeof camera!=='undefined'){camera.x=playerView.x;camera.y=playerView.y;}
  }
  saveSettings();renderSettings();
}
function renderSettings(){
  $('camera-lock-button').textContent=`Camera lock: ${cameraLocked?'On':'Off'}`;
  $('camera-lock-button').setAttribute('aria-pressed',String(cameraLocked));
  for(const action of Object.keys(defaultBindings))$('binding-'+action).value=keybindings[action].toUpperCase();
  $('world').setAttribute('aria-label',`Town grid. Use ${['up','left','down','right'].map(action=>keybindings[action].toUpperCase()).join(', ')} to walk. Press ${keybindings.camera.toUpperCase()} to toggle camera lock. Use arrow keys and Enter to inspect tiles.`);
  $('movement-hint').textContent=`Click map · ${['up','left','down','right'].map(action=>keybindings[action].toUpperCase()).join('/')} to walk · ${keybindings.camera.toUpperCase()} camera lock`;
}
for(const action of Object.keys(defaultBindings)){
  const label=document.createElement('label');label.textContent=bindingLabels[action];label.setAttribute('for','binding-'+action);
  const input=document.createElement('input');input.id='binding-'+action;input.readOnly=true;
  input.setAttribute('aria-describedby','keybindings-help');
  input.onkeydown=event=>{
    if(event.key==='Tab'||event.key==='Escape')return;
    event.preventDefault();event.stopPropagation();
    const key=event.key.toLowerCase();
    if(event.ctrlKey||event.metaKey||event.altKey||!/^[a-z0-9]$/.test(key)){
      $('settings-message').textContent='Choose a letter or number without modifier keys.';return;
    }
    if(Object.keys(keybindings).some(other=>other!==action&&keybindings[other]===key)){
      $('settings-message').textContent='That key is already assigned. Choose another key.';return;
    }
    clearWalk();keybindings[action]=key;saveSettings();renderSettings();$('settings-message').textContent='Keybinding saved.';
  };
  $('keybindings').append(label,input);
}
$('camera-lock-button').onclick=()=>setCameraLock(!cameraLocked);
$('reset-keybindings').onclick=()=>{clearWalk();keybindings={...defaultBindings};saveSettings();renderSettings();$('settings-message').textContent='Default keybindings restored.';};
renderSettings();
let playerView = null, walkingRequest = false, walkCooldown = 0;
function clearWalk(){walkKeys.clear();walkCooldown=0;}
function canControlPlayer(){
  return viewed===identity?.town_id && !!world.towns[viewed]?.player && !tool
    && !$('world-menu').open && !$('delete-world-dialog').open
    && panel!=='settings' && !document.activeElement?.isContentEditable
    && !['INPUT','TEXTAREA','SELECT','BUTTON'].includes(document.activeElement?.tagName);
}
window.addEventListener('keyup',event=>walkKeys.delete(event.key.toLowerCase()));
window.addEventListener('blur',clearWalk);
document.addEventListener?.('visibilitychange',clearWalk);
function playerSprite(town,seconds){
  if(!town?.player)return null;
  const key=`${world.world_id}/${viewed}`;
  if(playerView?.key!==key){
    playerView={...town.player,key,target:{...town.player},clock:0,following:cameraLocked&&viewed===identity?.town_id};clearWalk();
    if(playerView.following&&typeof camera!=='undefined'){camera.x=playerView.x;camera.y=playerView.y;}
  }
  const s=playerView;
  if(viewed!==identity?.town_id)s.target={...town.player};
  if(!canControlPlayer())clearWalk();
  const horizontal=Number(walkKeys.has(keybindings.right))-Number(walkKeys.has(keybindings.left));
  const vertical=Number(walkKeys.has(keybindings.down))-Number(walkKeys.has(keybindings.up));
  const dx=Math.sign(horizontal+vertical),dy=Math.sign(vertical-horizontal);
  walkCooldown=Math.max(0,walkCooldown-seconds);
  if((dx||dy)&&!walkingRequest&&walkCooldown===0){
    walkingRequest=true;walkCooldown=.1;
    request('/api/command',{action:'walk',dx,dy,world_id:world.world_id}).then(result=>{
      if(playerView===s){s.target=result.player;town.player={...result.player};}
    }).catch(error=>{clearWalk();notice(error.message);}).finally(()=>{walkingRequest=false;});
  }
  const distance=Math.hypot(s.target.x-s.x,s.target.y-s.y);
  const travel=Math.min(1,distance?seconds*3.6/distance:0);
  s.x+=(s.target.x-s.x)*travel;s.y+=(s.target.y-s.y)*travel;
  s.facing=s.target.facing;s.clock+=seconds;s.path=distance>.001?[s.target]:[];s.phase='idle';
  if(viewed===identity?.town_id&&s.following&&(dx||dy||distance>.001)){
    if(typeof camera!=='undefined'){camera.x=s.x;camera.y=s.y;}
  }
  return {kind:'player',x:s.x,y:s.y,index:1,state:s,villager:{job:'idle'},active:false};
}
const labels = {hall:'Town hall',home:'Cottage',woodcutter:'Woodcutter',farm:'Farm',storehouse:'Storehouse',acre:'Acre'};
const costs = {home:20,woodcutter:15,farm:15,storehouse:25,acre:5};
const colors = {hall:'#a88861',home:'#c5ad80',woodcutter:'#947b55',farm:'#bc9b5c',storehouse:'#8c9b8b'};
// Small vector illustrations share the map's earth, sage, and amber palette.
const iconPaths = {
  wood:'<path fill="#94704c" d="M4 9 18 4l9 7-14 6z"/><path fill="#705337" d="M4 9v12l9 7V17z"/><path fill="#cda875" d="M13 17 27 11v12l-14 5z"/><path d="m17 18 6-3m-6 7 6-3"/>',
  food:'<path d="M16 28V5m0 11-7-6m7 12 7-6"/><path fill="#d9bd70" d="m16 7-5-5-2 5 7 7 7-7-2-5zm-7 3-5 1 2 6 10 5zm14 6 5 1-2 6-10 5z"/>',
  coins:'<ellipse fill="#b88d45" cx="16" cy="18" rx="11" ry="12"/><ellipse fill="#e4c980" cx="16" cy="14" rx="11" ry="11"/><path d="M16 7v14m4-11h-6l-2 4 8 2-2 4h-6"/>',
  villagers:'<circle fill="#dfbc91" cx="12" cy="10" r="5"/><circle fill="#c99f79" cx="23" cy="12" r="4"/><path fill="#7899a2" d="M3 28v-6a9 9 0 0 1 18 0v6z"/><path fill="#b9866b" d="M21 18a7 7 0 0 1 9 7v3h-6v-6z"/>',
  home:'<path fill="#c5ad80" d="M7 14h18v15H7z"/><path fill="#80574b" d="m3 15 13-12 13 12z"/><path fill="#f0cc80" d="M10 19h5v5h-5z"/><path fill="#4a4838" d="M19 20h4v9h-4z"/>',
  hall:'<path fill="#ac8e65" d="M5 13h22v16H5z"/><path fill="#5a747b" d="M2 14 16 5l14 9z"/><path d="M16 5V1"/><path fill="#e4c980" d="M17 1h9l-3 5h-6zM8 18h5v6H8zm11 0h5v6h-5z"/>',
  woodcutter:'<path fill="#947b55" d="M5 15h22v14H5z"/><path fill="#80574b" d="m2 16 14-11 14 11z"/><path fill="#cda875" d="M4 24h17v5H4z"/><path d="m20 27 7-17"/><path fill="#bacac5" d="m23 13 3-7 5 6z"/>',
  farm:'<path fill="#80764a" d="m2 19 15-8 13 8-15 10z"/><path d="m8 20 12-6m-8 9 12-6m-8 9 12-6"/><path fill="#d9bd70" d="M8 12h4v8H8zm8-6h4v9h-4zm8 6h4v8h-4z"/>',
  storehouse:'<path fill="#8c9b8b" d="M4 13h24v16H4z"/><path fill="#647876" d="M2 14 16 4l14 10z"/><path fill="#ad895c" d="M8 18h8v11H8zm10 4h8v7h-8z"/><path d="m8 18 8 11m0-11L8 29m10-7 8 7"/>',
  tree:'<path fill="#775b3e" d="M14 16h4v14h-4z"/><path fill="#56814c" d="m16 2 13 22H3z"/><path fill="#719451" d="m16 2 9 15H7z"/>',
  water:'<path fill="#6ba2af" d="M16 2C12 10 5 15 5 21a11 11 0 0 0 22 0c0-6-7-11-11-19z"/><path d="M10 21q0 6 7 6"/>',
  acre:'<path fill="#80764a" d="m2 19 15-8 13 8-15 10z"/><path d="m8 20 12-6m-8 9 12-6m-8 9 12-6"/><path fill="#d9bd70" d="M8 12h4v8H8zm8-6h4v9h-4zm8 6h4v8h-4z"/>',
  rock:'<path fill="#89928a" d="m3 25 3-15 12-7 11 12-3 12-13 3z"/><path fill="#b1baa8" d="m6 10 12-7 11 12-14 3z"/>'
};
function illustration(kind){
  const span=document.createElement('span');span.className='illustration';span.setAttribute('aria-hidden','true');
  span.innerHTML=`<svg viewBox="0 0 32 32" fill="none" stroke="#594e39" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">${iconPaths[kind] || iconPaths.villagers}</svg>`;
  return span;
}
function fact(value,label){
  const card=document.createElement('div');card.className='selection-fact';
  const number=document.createElement('strong');number.textContent=value;card.append(number);
  const caption=document.createElement('small');caption.textContent=label;card.append(caption);return card;
}
function skillBar(label,value){
  const row=document.createElement('div');row.className='skill-row';
  const caption=document.createElement('small');caption.textContent=`${label} · ${value}`;row.append(caption);
  const meter=document.createElement('meter');meter.min=0;meter.max=Math.max(10,Math.ceil((value+1)/10)*10);meter.value=value;
  meter.setAttribute('aria-label',`${label}: ${value}; scale to ${meter.max}`);row.append(meter);return row;
}
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
  clearWalk();
  panel=next;selection=null;tool=null;moveIndex=null;buildTile=null;hoverTile=null;
  render();
}
for(const name of ['build','villagers','valley','help','settings']){
  $(name+'-button').onclick=()=>openPanel(panel===name?null:name);
}
$('close-panel').onclick=()=>{openPanel(null);canvas.focus();};
$('find-player-button').onclick=()=>{
  const town=world.towns[identity?.town_id];
  if(!town)return;
  if(!town.player){notice('Restart the Python host, then refresh this page to load your character.');return;}
  viewed=identity.town_id;keyboardTile=null;openPanel(null);
  setCameraLock(true);
  if(typeof camera!=='undefined'){camera.x=playerView.x;camera.y=playerView.y;camera.zoom=1;}
  draw();canvas.focus();notice(`Your gold-clad villager is marked You. Use ${['up','left','down','right'].map(action=>keybindings[action].toUpperCase()).join('/')} to walk.`);
};
window.addEventListener('keydown',event=>{
  const key=event.key.toLowerCase();
  if(key===keybindings.camera&&!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    event.preventDefault();setCameraLock(!cameraLocked);draw();notice(`Camera lock ${cameraLocked?'on':'off'}.`);
  }
  if(['up','left','down','right'].some(action=>keybindings[action]===key)&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    event.preventDefault();walkKeys.add(key);
  }
  if(event.key==='Escape' && !$('world-menu').open && !$('delete-world-dialog').open){
    openPanel(null);canvas.focus();
  }
});
function inspect(type,index){
  panel=type==='villager'&&panel==='villagers'?'villagers':'selection';selection={type,index};tool=null;moveIndex=null;buildTile=null;hoverTile=null;render();
}
function villagerCard(town,v,index,withLink=false){
  const div=document.createElement('div');div.className='villager';
  const name=document.createElement(withLink?'button':'strong');name.textContent=v.name;
  if(withLink){name.className='villager-name';name.onclick=()=>inspect('villager',index);}div.append(name);
  div.append(skillBar('Woodcutting',v.woodcutting));div.append(skillBar('Farming',v.farming));
  const resource=v.job==='woodcutting'?'wood':'food';
  const full=town[resource]>=100+100*town.buildings.filter(b=>b.kind==='storehouse').length;
  const activity=document.createElement('small');activity.textContent=v.job==='idle'?'Resting':v.work_target?(full?`Paused: ${resource} storage full. Spend ${resource} or build a storehouse.`:v.job==='woodcutting'?'Gathering at the forest':'Tending crops by the water'):'Waiting for a workplace near '+(v.job==='woodcutting'?'trees':'water');div.append(activity);
  const select=document.createElement('select');select.setAttribute('aria-label',`Job for ${v.name}`);
  for(const [job,label] of [['idle','Resting'],['woodcutting','Woodcutting'],['farming','Farming']]){const option=document.createElement('option');option.value=job;option.textContent=label;select.append(option);}
  select.value=v.job;select.disabled=viewed!==identity?.town_id;select.onchange=()=>act({action:'job',index,job:select.value});div.append(select);
  return div;
}
function villagerRow(town,v,index){
  const expanded=panel==='villagers'&&selection?.type==='villager'&&selection.index===index;
  const row=document.createElement('div');row.className='villager-row';
  const button=document.createElement('button');button.className='villager-summary';
  button.setAttribute('aria-expanded',String(expanded));button.setAttribute('aria-controls',`villager-details-${index}`);
  const name=document.createElement('strong');name.textContent=v.name;button.append(name);
  const job=document.createElement('span');job.className='muted';job.textContent={idle:'Resting',woodcutting:'Woodcutting',farming:'Farming'}[v.job];button.append(job);
  const arrow=document.createElement('span');arrow.setAttribute('aria-hidden','true');arrow.textContent=expanded?'▾':'▸';button.append(arrow);
  button.onclick=()=>{selection=expanded?null:{type:'villager',index};render();$('villagers').children[index].children[0].focus();};
  row.append(button);
  const details=document.createElement('div');details.id=`villager-details-${index}`;details.hidden=!expanded;
  if(expanded){const card=villagerCard(town,v,index);card.children[0].hidden=true;details.append(card);}
  row.append(details);return row;
}
function renderSelection(town){
  $('selection-details').replaceChildren();$('hall-details').hidden=true;$('selection-move').hidden=true;$('selection-destroy').hidden=true;
  if(!selection)return;
  if(!town){selection=null;panel=null;return;}
  if(selection.type==='player'){
    $('selection-title').textContent=viewed===identity?.town_id?'You':'Town founder';
    const p=document.createElement('p');
    p.textContent=viewed===identity?.town_id?'Your adventuring villager. Click the map and use your movement keys to explore. Open Settings to change keybindings. Your position is saved with this world.':'This is the town owner’s adventuring villager.';
    $('selection-details').append(p);return;
  }
  const item=selection.type==='npc'?town.npcs?.[selection.index]:selection.type==='villager'?town.villagers[selection.index]:selection.type==='building'?town.buildings[selection.index]:town.map_resources[selection.index];
  if(!item){selection=null;panel=null;return;}
  if(selection.type==='npc'){
    $('selection-title').textContent=item.name;
    const role=document.createElement('p');role.textContent=item.role;
    const dialogue=document.createElement('p');dialogue.textContent='“'+item.dialogue+'”';
    $('selection-details').append(illustration('villagers'));$('selection-details').append(role);$('selection-details').append(dialogue);return;
  }
  if(selection.type==='villager'){
    $('selection-title').textContent=item.name;$('selection-details').append(villagerCard(town,item,selection.index));return;
  }
  $('selection-details').append(illustration(item.kind));
  const p=document.createElement('p');
  if(selection.type==='resource'){
    const swamp=town.terrain?.[item.y]?.[item.x]==='b';
    $('selection-title').textContent=(swamp?{tree:'Swamp willow',water:'Swamp water',rock:'Rock outcrop'}:{tree:'Forest',water:'Water',rock:'Rock outcrop'})[item.kind];
    p.textContent={tree:'Renewable wood source. Place a woodcutter within 3 horizontal or vertical tile steps, then assign a villager to woodcutting.',water:'Renewable source for farming. Place a farm within 3 horizontal or vertical tile steps, then assign a villager to farming.',rock:'This outcrop occupies its tile. Mining is not available yet.'}[item.kind];
  }else{
    $('selection-title').textContent=labels[item.kind];
    if(item.kind==='home')$('selection-details').append(fact('+2','Villager spaces'));
    if(item.kind==='storehouse')$('selection-details').append(fact(100+100*town.buildings.filter(b=>b.kind==='storehouse').length,'Capacity per resource'));
    if(item.kind==='hall')$('selection-details').append(fact(town.level,'Hall level'));
    p.textContent={hall:'The heart of your settlement. Harvest wood to unlock your first upgrade.',home:'Room for 2 additional villagers, up to 16 in the town. New residents need available housing and sufficient food.',storehouse:`Adds 100 storage for each resource. Town capacity: ${100+100*town.buildings.filter(b=>b.kind==='storehouse').length} wood and food.`,woodcutter:'A workplace for gathering wood from nearby trees.',farm:'A workplace for growing food near water. Build acres nearby for farmers to tend.',acre:'A crop field tended by farmers from its farm. Its farm needs water within 3 tile steps to produce food.'}[item.kind];
    if(item.kind==='hall'){
      $('hall-details').hidden=false;
      $('hall-progress').value=Math.min(30,town.harvested);
      $('progress').textContent=`Level ${town.level} · Timber roots: ${Math.min(30,town.harvested)}/30 wood harvested`;
      $('upgrade').disabled=viewed!==identity?.town_id || town.level>=2 || town.harvested<30 || town.wood<40;
    }
    $('selection-move').hidden=viewed!==identity?.town_id;
    $('selection-move').textContent=town.coins?'Move building · 1 coin':'Move building · Free';
    $('selection-move').onclick=()=>{const index=selection.index;openPanel('build');tool='move';moveIndex=index;render();};
    $('selection-destroy').hidden=viewed!==identity?.town_id || item.kind==='hall';
    $('selection-details').append(p);
    if(item.kind==='farm'&&viewed===identity?.town_id){
      const button=document.createElement('button');button.textContent='Build acre · 5 wood';button.disabled=town.wood<costs.acre;
      button.onclick=()=>{acreFarmIndex=selection.index;openPanel('build');tool='acre';moveIndex=null;buildTile=null;hoverTile=null;render();};
      $('selection-details').append(button);
    }
    if(item.kind==='woodcutter'||item.kind==='farm'){
      const assigned=town.villagers.map((v,i)=>({v,i,state:workers.get(i)})).filter(w=>w.v.job===(item.kind==='woodcutter'?'woodcutting':'farming')&&w.state?.home?.x===item.x&&w.state?.home?.y===item.y);
      const status=document.createElement('p');status.className='workplace-status';
      const nearby=town.map_resources.some(r=>r.kind===(item.kind==='woodcutter'?'tree':'water')&&Math.abs(r.x-item.x)+Math.abs(r.y-item.y)<=3);
      status.textContent=!nearby?'Needs a source within 3 tile steps.':!assigned.length?'Source nearby. Assign villagers to start work.':`${assigned.length} assigned worker${assigned.length===1?'':'s'}.`;
      $('selection-details').append(status);
      for(const w of assigned)$('selection-details').append(villagerCard(town,w.v,w.i,true));
    }
  }
  if(selection.type==='resource')$('selection-details').append(p);
}
for(const [kind,cost] of Object.entries(costs).filter(([kind])=>kind!=='acre')){const button=document.createElement('button');button.textContent=`${labels[kind]} · ${cost} wood`;button.append(illustration(kind));button.onclick=()=>{panel='build';selection=null;tool=kind;moveIndex=null;buildTile=null;hoverTile=null;render();};$('buildings').append(button);}
$('move').onclick=()=>{panel='build';selection=null;tool='move';moveIndex=null;buildTile=null;hoverTile=null;render();};
$('cancel').onclick=()=>{tool=null;moveIndex=null;buildTile=null;hoverTile=null;render();};
async function buildAt(x,y){
  const town=world.towns[viewed];
  if(building || !costs[tool] || viewed!==identity?.town_id || !town || town.wood<costs[tool])return;
  const selected={x,y},kind=tool;
  if(kind==='acre'&&!acrePlacementValid(town,selected,acreFarmIndex)){notice('Place acres within 3 tile steps of their farm.');return;}
  building=true;render();
  try{
    await request('/api/command',{action:'build',kind,...selected,...(kind==='acre'?{farm_index:acreFarmIndex}:{}),world_id:world.world_id});
    if(tool===kind&&buildTile?.x===x&&buildTile?.y===y){buildTile=null;hoverTile=null;}
    await refresh();
  }catch(e){notice(e.message);await refresh();}
  finally{building=false;render();}
}
$('upgrade').onclick=()=>act({action:'upgrade'});
$('selection-destroy').onclick=async()=>{
  const town=world.towns[viewed],selected=selection,button=$('selection-destroy');
  if(button.disabled || viewed!==identity?.town_id || selected?.type!=='building'
    || !town?.buildings[selected.index] || town.buildings[selected.index].kind==='hall')return;
  button.disabled=true;
  try{
    await request('/api/command',{action:'destroy',index:selected.index,world_id:world.world_id});
    if(selection===selected)openPanel(null);
    await refresh();
  }catch(e){notice(e.message);await refresh();}
  finally{button.disabled=false;}
};
async function refresh(){
  if(refreshing)return refreshing;
  refreshing=(async()=>{
    const next=await request('/api/world');
    if(next.world_id!==world.world_id){tool=null;moveIndex=null;buildTile=null;hoverTile=null;viewed=null;selection=null;panel=null;keyboardTile=null;}
    world=next;identity=identities[world.world_id] || null;
    catalog=await request('/api/worlds');
    if(!world.towns[viewed]){viewed=identity?.town_id || Object.keys(world.towns)[0];selection=null;panel=null;tool=null;moveIndex=null;buildTile=null;hoverTile=null;keyboardTile=null;}
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
  for(const name of ['selection','villagers','valley','help','settings'])$(name+'-panel').hidden=panel!==name;
  $('side-panel').hidden=!panel && $('join').hidden;
  $('close-panel').hidden=!panel;
  $('game').classList.toggle('panel-open',!$('side-panel').hidden);
  for(const name of ['build','villagers','valley','help','settings']){
    $(name+'-button').setAttribute('aria-expanded',String(panel===name));
    $(name+'-button').classList.toggle('selected',panel===name);
  }
  $('build-button').disabled=!own || viewed!==identity?.town_id;
  $('find-player-button').disabled=!own;
  $('villagers-button').disabled=!town;
  $('move').textContent=own?.coins?'Move a building · 1 coin':'Move a building · Free';
  $('mode-hint').textContent=tool==='move'?(moveIndex===null?'Moving: click a building. Escape to cancel.':'Moving: click open grass to move here. Escape to cancel.'):tool?`Building ${labels[tool].toLowerCase()}: hover over open grass and left-click to build. Escape to cancel.`:'Click a building, villager, or resource to learn more.';
  $('view-title').textContent=town?.name || 'A place to call home';
  $('resources-empty').hidden=!!own;$('resource-totals').hidden=!own;
  if(own){
    $('resource-wood').textContent=Math.floor(own.wood);
    $('resource-food').textContent=Math.floor(own.food);
    $('resource-coins').textContent=own.coins;
    $('resource-villagers').textContent=own.villagers.length;
  }
  if(buildTile && (!town || town.buildings.some(b=>b.x===buildTile.x&&b.y===buildTile.y) || town.map_resources.some(r=>r.x===buildTile.x&&r.y===buildTile.y)))buildTile=null;
  for(const button of $('buildings').children)button.classList.toggle('selected',button.textContent===`${labels[tool]} · ${costs[tool]} wood`);
  if(own){$('town-name').textContent=own.name;$('hint').textContent=tool==='move'?(moveIndex===null?'Click a building to move it.':'Click open grass.'):tool?(building?'Building…':own.wood<costs[tool]?'Not enough wood.':`Hover over open grass, then left-click to build a ${labels[tool].toLowerCase()}.`):'Choose a building, then hover over an open tile.';}
  $('towns').replaceChildren();
  for(const [id,t] of Object.entries(world.towns)){const b=document.createElement('button');b.textContent=t.name+(id===identity?.town_id?' · Yours':'');b.classList.toggle('selected',id===viewed);b.onclick=()=>{viewed=id;tool=null;moveIndex=null;buildTile=null;hoverTile=null;selection=null;panel=null;keyboardTile=null;render();};$('towns').append(b);}
  $('villagers').replaceChildren();
  if(town)for(const [i,v] of town.villagers.entries())$('villagers').append(villagerRow(town,v,i));
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
    if(world.towns[viewed]?.terrain?.[r.y]?.[r.x]==='b'){
      ctx.strokeStyle='#665f40';ctx.lineWidth=3;ctx.beginPath();ctx.moveTo(0,-7);ctx.lineTo(-10,-29);ctx.moveTo(0,-12);ctx.lineTo(10,-34);ctx.stroke();
      ctx.fillStyle='#64784c';ctx.beginPath();ctx.ellipse(0,-34,20,14,0,0,Math.PI*2);ctx.fill();
      ctx.fillStyle='#819064';ctx.beginPath();ctx.ellipse(-5,-38,13,9,0,0,Math.PI*2);ctx.fill();
      ctx.strokeStyle='#728459';ctx.lineWidth=2;
      for(let branch=-16;branch<=16;branch+=8){ctx.beginPath();ctx.moveTo(branch,-34);ctx.quadraticCurveTo(branch-4,-19,branch+2,-8+Math.abs(branch)/3);ctx.stroke();}
      ctx.restore();return;
    }
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
let npcClock=0;
const npcColors={g:['#a97b58','#c7a06d'],f:['#416951','#70906a'],m:['#b29b4a','#e2cc80'],
  s:['#be986a','#e6c79a'],r:['#717b88','#acb4bd'],b:['#586b4b','#a1a474']};
function npcSprites(town,seconds){
  npcClock+=seconds;
  return (town?.npcs || []).map((npc,index)=>({...npc,kind:'npc',index,villager:{job:'idle'},active:false,
    state:{path:[],phase:'idle',clock:npcClock+index*.7,facing:index%2?1:-1}}));
}
function villagerAccessories(b){
  // Earned gear stays visible across jobs: basic tools at 1, clothing at 5.
  const v=b.kind==='villager'?b.villager:{};
  return {axe:v.woodcutting>=1,blouse:v.woodcutting>=5,
    sickle:v.farming>=1,hat:v.farming>=5,pouch:v.fishing>=1,scarf:v.fishing>=5};
}
function drawVillager(b){
  const accessories=villagerAccessories(b);
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
  ctx.fillStyle=b.kind==='npc'?npcColors[b.biome][0]:b.kind==='player'?'#d6b85d':['#7899a2','#b9866b','#a0a16b','#947ca0','#c6aa62'][b.index%5];ctx.fillRect(-4,-13,8,10);
  if(accessories.blouse){
    polygon([[-4,-13],[4,-13],[7,-9],[5,-7],[3,-10],[4,-3],[-4,-3],[-3,-10],[-5,-7],[-7,-9]],'#b8c6ad');
    polygon([[-3,-13],[0,-10],[3,-13]],'#eee0bd');
    ctx.strokeStyle='#6c8068';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(0,-10);ctx.lineTo(0,-4);ctx.stroke();
  }
  ctx.fillStyle=['#e7c68c','#c99f79','#ad795c'][b.index%3];ctx.beginPath();ctx.arc(0,-17,4,0,Math.PI*2);ctx.fill();
  ctx.fillStyle=['#584638','#aa7846','#363c36','#d3bc8c'][b.index%4];ctx.beginPath();ctx.arc(0,-18,4,Math.PI,Math.PI*2);ctx.fill();
  if(b.index%2)ctx.fillRect(-4,-18,2,7);
  if(accessories.hat){polygon([[-6,-19],[0,-23],[6,-19]],'#ccb779');ctx.fillStyle='#ccb779';ctx.fillRect(-7,-19,14,2);}
  if(b.kind==='npc'){
    const accent=npcColors[b.biome][1];
    if(b.biome==='b'||b.biome==='f')polygon([[-5,-18],[0,-25],[5,-18]],accent);
    else{ctx.fillStyle=accent;ctx.fillRect(-7,-21,14,2);ctx.fillRect(-4,-25,8,4);}
    ctx.fillStyle=accent;ctx.fillRect(-4,-12,8,2);
    ctx.fillStyle='#8b704b';ctx.fillRect(5,-7,5,6);
  }
  ctx.fillStyle='#ded2a6';ctx.fillRect(-4,-6,8,1);
  if(accessories.axe){
    ctx.strokeStyle='#a78454';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(-6,0);ctx.lineTo(-8,-10);ctx.stroke();
    polygon([[-8,-11],[-13,-12],[-14,-7],[-7,-7]],'#e4ece7','#6b7d79');
  }
  if(accessories.sickle){
    ctx.strokeStyle='#a78454';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(6,0);ctx.lineTo(7,-6);ctx.stroke();
    ctx.strokeStyle='#dce5df';ctx.lineWidth=1.8;ctx.beginPath();ctx.arc(7,-9,4,-Math.PI/2,Math.PI/2);ctx.stroke();
  }
  if(accessories.pouch){
    ctx.fillStyle='#806448';ctx.fillRect(-3,-5,6,5);
    ctx.strokeStyle='#d5dfd8';ctx.lineWidth=1;ctx.beginPath();ctx.arc(0,-3,1.5,0,Math.PI);ctx.stroke();
  }
  if(accessories.scarf){
    ctx.fillStyle='#6baeb5';ctx.fillRect(-4,-14,8,2);ctx.fillRect(2,-12,2,5);
  }
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
  const town=world.towns[viewed],hero=playerSprite(town,seconds);
  tileW=Math.min(54,(rect.width-30)/12);tileH=tileW/2;originX=rect.width/2;originY=Math.max(140,(rect.height-12*tileH)/2);
  if(typeof camera==='undefined'&&hero&&playerView.following){
    originX=rect.width/2-(hero.x-hero.y)*tileW/2;
    originY=rect.height/2-(hero.x+hero.y)*tileH/2-tileH/2;
  }
  hitTargets=[];
  const resources=town?.map_resources || [];
  const water=new Set(resources.filter(r=>r.kind==='water').map(r=>`${r.x},${r.y}`));
  // Ground thickness appears only at the actual valley boundary.
  const terrainSize=town?.map_size || 12;
  const preview=hoverTile || keyboardTile;
  const previewKind=costs[tool]?tool:tool==='move'&&moveIndex!==null?town?.buildings[moveIndex]?.kind:null;
  const sourceKind=previewKind==='woodcutter'?'tree':previewKind==='farm'?'water':null;
  const reachable=new Set(preview&&sourceKind?resources.filter(r=>r.kind===sourceKind&&Math.abs(r.x-preview.x)+Math.abs(r.y-preview.y)<=3).map(r=>`${r.x},${r.y}`):[]);
  for(let x=0;x<12;x++)for(let y=0;y<12;y++){
    const [px,py]=point(x,y),wet=water.has(`${x},${y}`);
    const variation=(x*17+y*31+x*y*7)%7;
    const diamond=[[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]];
    const biome=town?.terrain?.[y]?.[x] || 'g';
    if(biome==='v')continue;
    const land=(tx,ty)=>tx>=0&&ty>=0&&tx<terrainSize&&ty<terrainSize&&town?.terrain?.[ty]?.[tx]!=='v';
    if(!land(x+1,y))polygon([diamond[1],diamond[2],[diamond[2][0],diamond[2][1]+12],[diamond[1][0],diamond[1][1]+12]],'#344b35');
    if(!land(x,y+1))polygon([diamond[2],diamond[3],[diamond[3][0],diamond[3][1]+12],[diamond[2][0],diamond[2][1]+12]],'#405438');
    const palette={g:['#68865a','#69875b','#668358'],f:['#506f4a','#55764c','#5a7b50'],
      m:['#83945e','#899b65','#80915b'],s:['#b4a575','#bdad7d','#b8a878'],r:['#8b8d7c','#969785','#858978'],b:['#59674b','#626f50','#536247']};
    const neighbors=[[x,y-1],[x+1,y],[x,y+1],[x-1,y]];
    const shores=neighbors.map(([tx,ty])=>!water.has(`${tx},${ty}`));
    const depth=shores.filter(Boolean).length;
    const waterColor=biome==='b'?(depth?'#718565':'#4e6857'):(depth?'#6299a1':'#447d91');
    polygon(diamond,wet?waterColor:palette[biome][variation%3],tool?'#91a37455':null);
    if(biome==='b'&&variation%3===0){
      ctx.strokeStyle='#9b9f69';ctx.lineWidth=tileW/46;
      for(let reed=0;reed<3;reed++){const rx=px+(reed-1)*tileW*.07,ry=py+tileH*.7;ctx.beginPath();ctx.moveTo(rx,ry);ctx.lineTo(rx+(reed-1)*tileW*.025,ry-tileH*.4);ctx.stroke();}
      if(wet){ctx.fillStyle='#a3ad74';ctx.beginPath();ctx.ellipse(px+tileW*.17,py+tileH*.55,tileW*.065,tileH*.075,0,0,Math.PI*2);ctx.fill();}
    }
    if(!wet&&variation===0){ctx.fillStyle='#8fa574';ctx.fillRect(px-5*tileW/46,py+tileH*.5,2*tileW/46,2*tileW/46);ctx.fillStyle='#d7c78a';ctx.fillRect(px+4*tileW/46,py+tileH*.65,2*tileW/46,2*tileW/46);}
    if(!wet&&(x===0||y===0||x===terrainSize-1||y===terrainSize-1)&&variation%2===0){ctx.strokeStyle='#9caf77';ctx.lineWidth=tileW/46;ctx.beginPath();ctx.moveTo(px-6*tileW/46,py+tileH*.65);ctx.lineTo(px-9*tileW/46,py+tileH*.4);ctx.moveTo(px-6*tileW/46,py+tileH*.65);ctx.lineTo(px-4*tileW/46,py+tileH*.35);ctx.stroke();}
    if(reachable.has(`${x},${y}`))polygon(diamond,'#b8dcad44','#c6dfaa');
    if(preview?.x===x&&preview?.y===y&&previewKind&&town){const valid=placementValid(town,preview,previewKind);ctx.save();ctx.lineWidth=2;polygon(diamond,valid?'#d6d8a366':'#c76d6455',valid?'#eee5ab':'#f3a295');ctx.restore();}
    const selected=selection?.type==='building'?town?.buildings[selection.index]:selection?.type==='resource'?resources[selection.index]:null;
    if((selected?.x===x&&selected?.y===y)||(!previewKind&&keyboardTile?.x===x&&keyboardTile?.y===y)){
      ctx.save();ctx.lineWidth=3;polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],'#fff0b133','#fff0b1');ctx.restore();
    }
    if(wet){
      ctx.strokeStyle='#b4d7ce99';ctx.lineWidth=tileW/46;
      for(let edge=0;edge<4;edge++)if(shores[edge]){
        ctx.beginPath();ctx.moveTo(...diamond[edge]);ctx.lineTo(...diamond[(edge+1)%4]);ctx.stroke();
      }
      const ripple=((x*73856093)^(y*19349663)^(town?.terrain_seed || 0))>>>0;
      if(ripple%5===0){
        const offset=(ripple%13-6)*tileW/70;
        ctx.beginPath();ctx.moveTo(px-tileW*.14+offset,py+tileH*.5);
        ctx.quadraticCurveTo(px+offset,py+tileH*.38,px+tileW*.12+offset,py+tileH*.55);ctx.stroke();
      }
    }
  }
  const buildings=town?.buildings || [{kind:'hall',x:5,y:5}];
  const villagers=animatedVillagers(town,seconds);
  const objects=[...buildings,...resources.filter(r=>r.kind!=='water'),...villagers,...npcSprites(town,seconds),...(hero?[hero]:[])];
  for(const b of objects.sort((a,b)=>(a.x+a.y)-(b.x+b.y))){
    const [px,py]=point(b.x,b.y),scale=tileW/46;
    if(b.kind==='player'){
      const baseY=py+tileH*.65;
      ctx.strokeStyle='#ffe8a0';ctx.lineWidth=2;ctx.beginPath();ctx.ellipse(px,baseY,10*scale,5*scale,0,0,Math.PI*2);ctx.stroke();
      drawVillager(b);
      ctx.fillStyle='#fff0b1';ctx.font=`bold ${12*scale}px sans-serif`;ctx.textAlign='center';
      ctx.fillText(viewed===identity?.town_id?'You':'Founder',px,baseY-31*scale);
      hitTargets.push({type:'player',index:0,left:px-10*scale,right:px+10*scale,top:baseY-27*scale,bottom:baseY+5*scale});
      continue;
    }
    const character=b.kind==='villager'||b.kind==='npc';
    const type=b.kind==='npc'?'npc':b.kind==='villager'?'villager':resources.includes(b)?'resource':'building';
    const index=character?b.index:type==='resource'?resources.indexOf(b):buildings.indexOf(b);
    const centerX=px+(character?(b.index%3-1)*3:0);
    const baseY=py+(character?tileH*.65+Math.floor(b.index/3):tileH/2);
    hitTargets.push({type,index,left:centerX-(character?9*scale:tileW*.45),right:centerX+(character?9*scale:tileW*.45),top:baseY-(b.kind==='tree'?70:b.kind==='hall'?75:character?26:(b.kind==='farm'||b.kind==='acre')?0:45)*scale,bottom:baseY+(character?5*scale:tileH/2)});
    if(character&&selection?.type===type&&selection.index===b.index){
      ctx.fillStyle='#fff0b144';ctx.strokeStyle='#fff0b1';ctx.lineWidth=2;ctx.beginPath();ctx.ellipse(centerX,baseY,12*scale,6*scale,0,0,Math.PI*2);ctx.fill();ctx.stroke();
    }
    if(b.kind==='tree'||b.kind==='rock'){drawResource(b);continue;}
    if(character){
      drawVillager(b);
      continue;
    }
    drawBuilding(b);
  }
  if(preview && previewKind && town && town.terrain?.[preview.y]?.[preview.x]!=='v' && viewed===identity?.town_id){
    const valid=placementValid(town,preview,previewKind);
    ctx.save();ctx.globalAlpha=valid ? .65 : .35;drawBuilding({...preview,kind:previewKind});ctx.restore();
  }
}
function acrePlacementValid(town,tile,index){
  const farm=town.buildings[index];
  return farm?.kind==='farm'&&Math.abs(tile.x-farm.x)+Math.abs(tile.y-farm.y)<=3;
}
function placementValid(town,tile,kind){
  return town.terrain?.[tile.y]?.[tile.x]!=='v'
    && (kind!=='acre'||acrePlacementValid(town,tile,tool==='move'?town.buildings[moveIndex]?.farm_index:acreFarmIndex))
    && !town.buildings.some(b=>b.x===tile.x&&b.y===tile.y)
    && !(town.npcs || []).some(n=>n.x===tile.x&&n.y===tile.y)
    && !town.map_resources.some(r=>r.x===tile.x&&r.y===tile.y)
    && (tool==='move' || town.wood>=costs[kind]);
}
function drawBuilding(b){
  if(b.kind==='acre'){
    const [x,y]=point(b.x,b.y);
    polygon([[x,y+tileH*.1],[x+tileW*.4,y+tileH*.5],[x,y+tileH*.9],[x-tileW*.4,y+tileH*.5]],'#80764a','#ab9561');
    for(let row=0;row<4;row++)for(let crop=0;crop<4;crop++){
      const [px,py]=point(b.x-.3+row*.2,b.y-.3+crop*.2);
      ctx.strokeStyle=(row+crop)%2?'#d9bd70':'#b8b458';ctx.lineWidth=2*tileW/46;
      ctx.beginPath();ctx.moveTo(px,py+tileH/2);ctx.lineTo(px,py+tileH/2-5*tileW/46);ctx.stroke();
    }
    return;
  }
  if(b.kind==='farm'){
    const [x,y]=point(b.x,b.y),w=tileW*.38;
    ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(x,y+tileH*.7,w+6,9,0,0,Math.PI*2);ctx.fill();
    for(let r=0;r<4;r++){const [px,py]=point(b.x+r*.18,b.y+.1);polygon([[px,py+4],[px+10,py+9],[px-7,py+17],[px-17,py+12]],r%2?'#b8b458':'#80764a');}
    return;
  }
  const [x,top]=point(b.x,b.y),y=top+tileH/2,s=tileW/46;
  const w=tileW*(b.kind==='storehouse' ? .43 : .38),depth=w*tileH/tileW,h=(b.kind==='hall'?42:b.kind==='woodcutter'?23:27)*s;
  // Wall bases share the tile's ground plane and its isometric slope.
  ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(x+8*s,y+depth*.6,w+8*s,depth+2*s,0,0,Math.PI*2);ctx.fill();
  polygon([[x-w,y],[x,y+depth],[x,y+depth-h],[x-w,y-h]],colors[b.kind]);
  polygon([[x,y+depth],[x+w,y],[x+w,y-h],[x,y+depth-h]],'#857659');
  polygon([[x-w,y],[x,y+depth],[x,y+depth-3*s],[x-w,y-3*s]],'#797665');
  polygon([[x,y+depth],[x+w,y],[x+w,y-3*s],[x,y+depth-3*s]],'#5e6255');
  polygon([[x-w-3*s,y-h],[x,y-h-15*s],[x+w+3*s,y-h],[x,y-h+depth+1*s]],b.kind==='hall'?'#5a747b':'#80574b');
  ctx.fillStyle='#3d4438';ctx.fillRect(x-6*s,y-h+13*s,5*s,9*s);
  // Warm windows and a hanging lantern; no new ambient animation.
  ctx.save();ctx.shadowColor='#f1c779';ctx.shadowBlur=7*s;ctx.fillStyle='#edc77d';
  ctx.fillRect(x+5*s,y-h+12*s,5*s,6*s);ctx.fillRect(x-12*s,y-h+9*s,3*s,4*s);ctx.restore();
  ctx.fillStyle='#594d38';ctx.fillRect(x-12*s,y-h+10*s,3*s,s);
  ctx.fillRect(x-22*s,y-15*s,2*s,18*s);ctx.fillRect(x-22*s,y-15*s,7*s,2*s);
  ctx.save();ctx.shadowColor='#f1c779';ctx.shadowBlur=9*s;ctx.fillStyle='#edc77d';ctx.fillRect(x-18*s,y-12*s,4*s,5*s);ctx.restore();
  if(b.kind==='home'){
    ctx.fillStyle='#85745f';ctx.fillRect(x+7*s,y-h-15*s,5*s,13*s);
    ctx.fillStyle='#b9a18a';ctx.fillRect(x+6*s,y-h-16*s,7*s,3*s);
  }
  if(b.kind==='woodcutter'){
    for(let i=0;i<3;i++){ctx.fillStyle='#705337';ctx.fillRect(x-18*s+i*5*s,y+3*s,5*s,6*s);ctx.fillStyle='#cda875';ctx.beginPath();ctx.arc(x-16*s+i*5*s,y+6*s,2*s,0,Math.PI*2);ctx.fill();}
    polygon([[x-20*s,y-h],[x-7*s,y-h-7*s],[x+9*s,y-h+1*s],[x-4*s,y-h+9*s]],'#9b825a');
  }
  if(b.kind==='storehouse'){
    ctx.fillStyle='#4c594e';ctx.fillRect(x-3*s,y-h+11*s,11*s,16*s);
    for(let i=0;i<2;i++){const cx=x+9*s+i*6*s,cy=y+1*s-i*3*s;ctx.fillStyle='#ac895d';ctx.fillRect(cx,cy,6*s,7*s);polygon([[cx,cy],[cx+6*s,cy+7*s],[cx+6*s,cy],[cx,cy+7*s]],'#c4a477');}
    ctx.fillStyle='#526c69';ctx.fillRect(x-14*s,y-h-3*s,28*s,2*s);
  }
  if(b.kind==='hall'){polygon([[x+11*s,y-h+6*s],[x+17*s,y-h+3*s],[x+17*s,y-h+22*s],[x+14*s,y-h+19*s],[x+11*s,y-h+25*s]],'#d6c478');}
  if(b.kind==='hall'){ctx.fillStyle='#d6c478';ctx.fillRect(x,y-h-30*s,2*s,17*s);polygon([[x+2*s,y-h-30*s],[x+16*s,y-h-26*s],[x+2*s,y-h-21*s]],'#d6c478');}
}
async function selectTile(x,y){
  const town=world.towns[viewed];
  if(!town || x<0||y<0||x>=12||y>=12)return;
  if(town.terrain?.[y]?.[x]==='v')return;
  const npc=(town.npcs || []).findIndex(n=>n.x===x&&n.y===y);
  const resource=town.map_resources.findIndex(r=>r.x===x&&r.y===y);
  const index=town.buildings.findIndex(b=>b.x===x&&b.y===y);
  if(!tool){
    if(npc>=0)inspect('npc',npc);
    else if(index>=0)inspect('building',index);
    else if(resource>=0)inspect('resource',resource);
    else openPanel(null);
    return;
  }
  if(viewed!==identity?.town_id)return;
  if(npc>=0){notice('A local is standing here. Choose another tile.');return;}
  if(resource>=0){notice('Choose open grass, away from resources.');return;}
  if(tool==='move'){
    if(moveIndex===null){if(index<0){notice('Click the tile beneath a building.');return;}moveIndex=index;render();}
    else{await act({action:'move',index:moveIndex,x,y});moveIndex=null;render();}
  }else{
    if(building)return;
    if(index>=0){notice('Choose open grass, away from existing buildings.');return;}
    if(town.wood<costs[tool]){notice('Not enough wood.');return;}
    await buildAt(x,y);
  }
}
function tileAt(event){
  const rect=canvas.getBoundingClientRect(),dx=event.clientX-rect.left-originX,dy=event.clientY-rect.top-originY;
  return {x:Math.floor(dx/tileW+dy/tileH),y:Math.floor(dy/tileH-dx/tileW)};
}
canvas.onmousemove=event=>{
  const town=world.towns[viewed],tile=tileAt(event);
  buildTile=costs[tool]&&viewed===identity?.town_id&&town&&tile.x>=0&&tile.y>=0&&tile.x<12&&tile.y<12
    &&(tool!=='acre'||acrePlacementValid(town,tile,acreFarmIndex))&&town.terrain?.[tile.y]?.[tile.x]!=='v'&&!(town.npcs || []).some(n=>n.x===tile.x&&n.y===tile.y)&&!town.buildings.some(b=>b.x===tile.x&&b.y===tile.y)&&!town.map_resources.some(r=>r.x===tile.x&&r.y===tile.y)?tile:null;
  hoverTile=(costs[tool]||tool==='move'&&moveIndex!==null)&&viewed===identity?.town_id&&town&&tile.x>=0&&tile.y>=0&&tile.x<(town.map_size || 12)&&tile.y<(town.map_size || 12)?tile:null;
  draw();
};
canvas.onmouseleave=()=>{buildTile=null;hoverTile=null;draw();};
canvas.onclick=async event=>{
  if(event.button!==undefined&&event.button!==0)return;
  canvas.focus?.();
  const rect=canvas.getBoundingClientRect(),px=event.clientX-rect.left,py=event.clientY-rect.top;
  keyboardTile=null;
  if(!tool){
    const hit=[...hitTargets].reverse().find(h=>px>=h.left&&px<=h.right&&py>=h.top&&py<=h.bottom);
    if(hit){inspect(hit.type,hit.index);return;}
  }
  const tile=tileAt(event);
  await selectTile(tile.x,tile.y);
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
