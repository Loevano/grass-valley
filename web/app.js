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
let camera = {x:5.5,y:5.5,zoom:1}, cameraTown = null;
const mapSize = town => town?.map_size || 120;
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
  if(playerView && (world.shared_map || viewed===identity?.town_id)){
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
  return (world.shared_map || viewed===identity?.town_id) && !!world.towns[identity?.town_id]?.player && !tool
    && !$('world-menu').open && !$('delete-world-dialog').open
    && panel!=='settings' && !document.activeElement?.isContentEditable
    && !['INPUT','TEXTAREA','SELECT','BUTTON'].includes(document.activeElement?.tagName);
}
window.addEventListener('keyup',event=>walkKeys.delete(event.key.toLowerCase()));
window.addEventListener('blur',clearWalk);
document.addEventListener?.('visibilitychange',clearWalk);
// Predict only the next host step; never run ahead while waiting for a reply.
let movementMap=null;
function predictPlayerStep(town,player,dx,dy){
  if(movementMap?.world!==world || movementMap.town!==town){
    const buildings=world.shared_map?Object.values(world.towns).flatMap(t=>t.buildings || []):town.buildings || [];
    movementMap={world,town,blocked:new Set([...buildings,...(town.map_resources || [])]
      .filter(b=>b.kind!=='tree').map(b=>`${b.x},${b.y}`))};
  }
  const size=town.map_size || 12,terrain=town.terrain;
  const open=(x,y)=>{
    if(x<-.32 || y<-.32 || x>size-.68 || y>size-.68)return false;
    for(let tx=Math.floor(x+.5)-1;tx<=Math.floor(x+.5)+1;tx++){
      for(let ty=Math.floor(y+.5)-1;ty<=Math.floor(y+.5)+1;ty++){
        const outside=tx>=0&&ty>=0&&tx<size&&ty<size&&terrain?.[ty]?.[tx]==='v';
        if((movementMap.blocked.has(`${tx},${ty}`)||outside)&&Math.abs(x-tx)<.68&&Math.abs(y-ty)<.68)return false;
      }
    }
    return true;
  };
  const predicted={...player},length=Math.hypot(dx,dy);
  if(length){
    for(let step=0;step<5;step++){
      const x=predicted.x+dx/length*.06;if(open(x,predicted.y))predicted.x=x;
      const y=predicted.y+dy/length*.06;if(open(predicted.x,y))predicted.y=y;
    }
    if(dx!==dy)predicted.facing=dx>dy?1:-1;
  }
  return predicted;
}
function playerSprite(town,seconds){
  if(!town?.player)return null;
  const playerTown=world.shared_map&&world.towns[identity?.town_id]?identity.town_id:viewed;
  const key=`${world.world_id}/${playerTown}`;
  if(playerView?.key!==key){
    playerView={...town.player,key,authoritative:{...town.player},target:{...town.player},clock:0,following:cameraLocked&&playerTown===identity?.town_id};clearWalk();
    if(playerView.following&&typeof camera!=='undefined'){camera.x=playerView.x;camera.y=playerView.y;}
  }
  const s=playerView;
  if(playerTown!==identity?.town_id)s.target={...town.player};
  if(!canControlPlayer())clearWalk();
  const horizontal=Number(walkKeys.has(keybindings.right))-Number(walkKeys.has(keybindings.left));
  const vertical=Number(walkKeys.has(keybindings.down))-Number(walkKeys.has(keybindings.up));
  const dx=Math.sign(horizontal+vertical),dy=Math.sign(vertical-horizontal);
  walkCooldown=Math.max(0,walkCooldown-seconds);
  if((dx||dy)&&!walkingRequest&&walkCooldown<=1e-6){
    walkingRequest=true;walkCooldown=.1;
    s.target=predictPlayerStep(town,s.authoritative,dx,dy);
    const controller=new AbortController(),timeout=setTimeout(()=>controller.abort(),1000);
    request('/api/command',{action:'walk',dx,dy,world_id:world.world_id},{signal:controller.signal}).then(result=>{
      if(playerView===s){s.authoritative={...result.player};s.target=result.player;
        const current=world.towns[playerTown];if(current)current.player={...result.player};}
    }).catch(error=>{if(playerView===s){s.target={...s.authoritative};clearWalk();notice(error.message);}})
      .finally(()=>{clearTimeout(timeout);walkingRequest=false;});
  }
  const distance=Math.hypot(s.target.x-s.x,s.target.y-s.y);
  const travel=Math.min(1,distance?seconds*3/distance:0);
  s.x+=(s.target.x-s.x)*travel;s.y+=(s.target.y-s.y)*travel;
  s.facing=s.target.facing;s.clock+=seconds;s.path=distance>.001?[s.target]:[];s.phase='idle';
  if(playerTown===identity?.town_id&&s.following&&(dx||dy||distance>.001)){
    if(typeof camera!=='undefined'){camera.x=s.x;camera.y=s.y;}
  }
  return {kind:'player',townId:playerTown,x:s.x,y:s.y,index:1,state:s,villager:{job:'idle'},active:false};
}
// Remote founders use small, frequent snapshots instead of the map refresh.
const remotePlayers=new Map();
let remoteWorld=null,remoteSample=-Infinity,refreshingPlayers=false;
function remotePlayerState(townId,player){
  if(remoteWorld!==world.world_id){remotePlayers.clear();remoteWorld=world.world_id;remoteSample=-Infinity;}
  let state=remotePlayers.get(townId);
  if(!state){
    state={...player,from:{...player},target:{...player},elapsed:.1,clock:0,path:[],phase:'idle'};
    remotePlayers.set(townId,state);
  }
  return state;
}
function updateRemotePlayers(snapshot){
  if(snapshot.world_id!==world.world_id)return;
  if(remoteWorld!==world.world_id){remotePlayers.clear();remoteWorld=world.world_id;remoteSample=-Infinity;}
  const sampledAt=snapshot.sampled_at ?? 0;
  if(sampledAt<remoteSample)return;
  for(const [id,player] of Object.entries(snapshot.players)){
    const town=world.towns[id];
    if(!town || id===identity?.town_id)continue;
    const state=remotePlayerState(id,town.player || player);
    if(player.x!==state.target.x || player.y!==state.target.y){
      state.from={x:state.x,y:state.y};state.target={...player};state.elapsed=0;
    }
    state.facing=player.facing;town.player={...player};
  }
  remoteSample=sampledAt;
}
async function refreshPlayers(){
  if(refreshingPlayers || !world.world_id || document.hidden)return;
  refreshingPlayers=true;
  const worldId=world.world_id,controller=new AbortController();
  const timeout=setTimeout(()=>controller.abort(),1000);
  try{
    const snapshot=await request('/api/players',undefined,{signal:controller.signal});
    // Same-world refreshes replace objects; update the current towns using sample order.
    if(world.world_id===worldId)updateRemotePlayers(snapshot);
  }finally{clearTimeout(timeout);refreshingPlayers=false;}
}
function remotePlayerSprite(townId,town,seconds){
  const state=remotePlayerState(townId,town.player);
  state.elapsed=Math.min(.1,state.elapsed+seconds);state.clock+=seconds;
  const progress=state.elapsed/.1;
  state.x=state.from.x+(state.target.x-state.from.x)*progress;
  state.y=state.from.y+(state.target.y-state.from.y)*progress;
  state.path=progress<1?[state.target]:[];
  return {kind:'player',townId,x:state.x,y:state.y,index:1,villager:{job:'idle'},active:false,state};
}
const labels = {hall:'Town hall',home:'Cottage',woodcutter:'Woodcutter',farm:'Farm',storehouse:'Storehouse',fishing_hut:'Fishing hut',acre:'Acre'};
function buildingLevel(building){return building.level || 1;}
function storageCapacity(town){return 100+100*town.buildings.filter(b=>b.kind==='storehouse'&&!b.construction).reduce((sum,b)=>sum+buildingLevel(b),0);}
function upgradeBenefit(kind,level){
  return kind==='home'?`${2*level} villager spaces`:kind==='storehouse'?`${100*level} storage per resource`:`+${25*(level-1)}% ${kind==='woodcutter'?'wood':kind==='fishing_hut'?'food per successful catch':kind==='acre'?'food from this field':'food'} per worker`;
}
const costs = {home:20,woodcutter:15,farm:15,storehouse:25,fishing_hut:15,acre:5};
const colors = {hall:'#a88861',home:'#c5ad80',woodcutter:'#947b55',farm:'#bc9b5c',storehouse:'#8c9b8b',fishing_hut:'#7899a2'};
// Small vector illustrations share the map's earth, sage, and amber palette.
const iconPaths = {
  wood:'<path fill="#94704c" d="M4 9 18 4l9 7-14 6z"/><path fill="#705337" d="M4 9v12l9 7V17z"/><path fill="#cda875" d="M13 17 27 11v12l-14 5z"/><path d="m17 18 6-3m-6 7 6-3"/>',
  food:'<path d="M16 28V5m0 11-7-6m7 12 7-6"/><path fill="#d9bd70" d="m16 7-5-5-2 5 7 7 7-7-2-5zm-7 3-5 1 2 6 10 5zm14 6 5 1-2 6-10 5z"/>',
  coins:'<ellipse fill="#b88d45" cx="16" cy="18" rx="11" ry="12"/><ellipse fill="#e4c980" cx="16" cy="14" rx="11" ry="11"/><path d="M16 7v14m4-11h-6l-2 4 8 2-2 4h-6"/>',
  villagers:'<circle fill="#dfbc91" cx="12" cy="10" r="5"/><circle fill="#c99f79" cx="23" cy="12" r="4"/><path fill="#7899a2" d="M3 28v-6a9 9 0 0 1 18 0v6z"/><path fill="#b9866b" d="M21 18a7 7 0 0 1 9 7v3h-6v-6z"/>',
  home:'<path fill="#c5ad80" d="M7 14h18v15H7z"/><path fill="#80574b" d="m3 15 13-12 13 12z"/><path fill="#f0cc80" d="M10 19h5v5h-5z"/><path fill="#4a4838" d="M19 20h4v9h-4z"/>',
  hall:'<path fill="#ac8e65" d="M5 13h22v16H5z"/><path fill="#5a747b" d="M2 14 16 5l14 9z"/><path d="M16 5V1"/><path fill="#e4c980" d="M17 1h9l-3 5h-6zM8 18h5v6H8zm11 0h5v6h-5z"/>',
  woodcutter:'<path fill="#947b55" d="M5 15h22v14H5z"/><path fill="#80574b" d="m2 16 14-11 14 11z"/><path fill="#cda875" d="M4 24h17v5H4z"/><path d="m20 27 7-17"/><path fill="#bacac5" d="m23 13 3-7 5 6z"/>',
  fishing_hut:'<path fill="#7899a2" d="M5 15h22v14H5z"/><path fill="#80574b" d="m2 16 14-11 14 11z"/><path d="M24 8v14q0 5-5 3"/>',
  farm:'<path fill="#bc9b5c" d="M5 15h22v14H5z"/><path fill="#80574b" d="m2 16 14-11 14 11z"/><path fill="#80764a" d="m2 19 15-8 13 8-15 10z"/><path d="m8 20 12-6m-8 9 12-6m-8 9 12-6"/><path fill="#d9bd70" d="M8 12h4v8H8zm8-6h4v9h-4zm8 6h4v8h-4z"/>',
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
async function request(path, data, options={}) {
  const response = await fetch(path,{...options,method:data?'POST':'GET',headers:{'Content-Type':'application/json','Authorization':identity?.token || ''},body:data?JSON.stringify(data):undefined});
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
  if(world.shared_map && next==='build' && world.towns[identity?.town_id])viewed=identity.town_id;
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
function inspect(type,index,townId=viewed){
  if(world.shared_map && world.towns[townId])viewed=townId;
  panel=type==='villager'&&panel==='villagers'?'villagers':'selection';selection={type,index};tool=null;moveIndex=null;buildTile=null;hoverTile=null;render();
}
function villagerCard(town,v,index,withLink=false){
  const div=document.createElement('div');div.className='villager';
  const name=document.createElement(withLink?'button':'strong');name.textContent=v.name;
  if(withLink){name.className='villager-name';name.onclick=()=>inspect('villager',index);}div.append(name);
  div.append(skillBar('Woodcutting',v.woodcutting));div.append(skillBar('Farming',v.farming));div.append(skillBar('Fishing',v.fishing || 0));
  const resource=v.job==='woodcutting'?'wood':'food';
  const full=town[resource]>=storageCapacity(town);
  const activity=document.createElement('small');activity.textContent=v.construction_target?(v.construction_arrived?'Building at the construction site':'Walking to the construction site'):v.job==='building'?'Waiting for construction':v.job==='idle'?'Resting':v.work_target?(full?`Paused: ${resource} storage full. Spend ${resource} or build a storehouse.`:v.job==='woodcutting'?'Gathering at the forest':v.job==='fishing'?'Fishing by the water':'Tending crops by the water'):'Waiting for a workplace near '+(v.job==='woodcutting'?'trees':'water');div.append(activity);
  const select=document.createElement('select');select.setAttribute('aria-label',`Job for ${v.name}`);
  for(const [job,label] of [['idle','Resting'],['building','Builder'],['woodcutting','Woodcutting'],['farming','Farming'],['fishing','Fishing']]){const option=document.createElement('option');option.value=job;option.textContent=label;select.append(option);}
  select.value=v.job;select.disabled=viewed!==identity?.town_id;select.onchange=()=>act({action:'job',index,job:select.value});div.append(select);
  return div;
}
function villagerRow(town,v,index){
  const expanded=panel==='villagers'&&selection?.type==='villager'&&selection.index===index;
  const row=document.createElement('div');row.className='villager-row';
  const button=document.createElement('button');button.className='villager-summary';
  button.setAttribute('aria-expanded',String(expanded));button.setAttribute('aria-controls',`villager-details-${index}`);
  const name=document.createElement('strong');name.textContent=v.name;button.append(name);
  const job=document.createElement('span');job.className='muted';job.textContent=v.job==='idle'&&v.construction_target?'Builder (automatic)':{idle:'Resting',building:'Builder',woodcutting:'Woodcutting',farming:'Farming',fishing:'Fishing'}[v.job];button.append(job);
  const arrow=document.createElement('span');arrow.setAttribute('aria-hidden','true');arrow.textContent=expanded?'▾':'▸';button.append(arrow);
  button.onclick=()=>{selection=expanded?null:{type:'villager',index};render();$('villagers').children[index].children[0].focus();};
  row.append(button);
  const details=document.createElement('div');details.id=`villager-details-${index}`;details.hidden=!expanded;
  if(expanded){const card=villagerCard(town,v,index);card.children[0].hidden=true;details.append(card);}
  row.append(details);return row;
}
const settlerSpaces={hall:1,home:2,storehouse:1,woodcutter:1,fishing_hut:1,farm:4,acre:1};
function settlerBuilding(town,v,index){
  if(Number.isInteger(v.building_index))return v.building_index;
  const kind={woodcutting:'woodcutter',farming:'farm',fishing:'fishing_hut'}[v.job];
  const places=town.buildings.map((b,i)=>({b,i})).filter(({b})=>b.kind===kind&&!b.construction&&buildingUsable(town,b));
  return places.length?places[index%places.length].i:null;
}
function renderSettlers(town,item,buildingIndex){
  const owner=viewed===identity?.town_id;
  const level=item.settler_level || 1,capacity=settlerSpaces[item.kind]*level;
  const assigned=town.villagers.map((v,i)=>({v,i})).filter(({v,i})=>settlerBuilding(town,v,i)===buildingIndex);
  $('selection-details').append(fact(`${assigned.length} / ${capacity}`,'Settler spaces'));
  const note=document.createElement('p');
  note.textContent=item.kind==='acre'?'Assigned settlers tend this acre using its farm’s water supply.':
    ['hall','home','storehouse'].includes(item.kind)?'Assigned settlers rest here and can still help with construction. Housing and storage benefits are automatic.':
    'Assign settlers here to work at this building.';
  $('selection-details').append(note);
  const roster=document.createElement('div');roster.className='building-workers';
  for(const {v,i} of assigned){
    const row=document.createElement('div');row.className='building-worker';
    const summary=document.createElement('button');summary.className='worker-summary';
    const name=document.createElement('strong');name.textContent=v.name;summary.append(name);
    const skills=document.createElement('small');
    const skill={woodcutter:'woodcutting',farm:'farming',acre:'farming',fishing_hut:'fishing'}[item.kind];
    const skillLabels={woodcutting:'Woodcutting',farming:'Farming',fishing:'Fishing'};
    skills.textContent=(skill?[skill]:Object.keys(skillLabels)).map(key=>`${skillLabels[key]} · Lv ${v[key] || 0}`).join(' / ');
    summary.append(skills);summary.onclick=()=>inspect('villager',i);row.append(summary);
    if(owner){
      const kick=document.createElement('button');kick.className='worker-kick danger';kick.textContent='Kick';
      kick.setAttribute('aria-label',`Kick ${v.name} from ${labels[item.kind] || 'this building'}`);
      kick.onclick=()=>{if(viewed===identity?.town_id)act({action:'remove_settler',index:i});};row.append(kick);
    }
    roster.append(row);
  }
  $('selection-details').append(roster);
  if(!owner)return;
  const chooser=document.createElement('select');chooser.setAttribute('aria-label','Settler to add');
  const available=town.villagers.map((v,i)=>({v,i})).filter(({v,i})=>settlerBuilding(town,v,i)!==buildingIndex);
  for(const {v,i} of available){const option=document.createElement('option');option.value=i;option.textContent=v.name+(settlerBuilding(town,v,i)!==null?' · reassign':'');chooser.append(option);}
  chooser.value=available.length?String(available[0].i):'';
  const add=document.createElement('button');add.textContent='Add settler';add.disabled=assigned.length>=capacity||!available.length;
  chooser.disabled=add.disabled;
  add.onclick=()=>{if(!add.disabled)act({action:'assign_settler',index:Number(chooser.value),building_index:buildingIndex});};
  $('selection-details').append(chooser);$('selection-details').append(add);
  const upgrade=document.createElement('button');upgrade.className='upgrade-button';upgrade.textContent=level>=3?'Settler capacity fully upgraded':`Upgrade to ${settlerSpaces[item.kind]*(level+1)} spaces · ${20*level} wood`;
  upgrade.disabled=level>=3||town.wood<20*level;
  upgrade.onclick=()=>{if(!upgrade.disabled)act({action:'upgrade_capacity',index:buildingIndex});};
  $('selection-details').append(upgrade);
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
    p.textContent={tree:'Renewable wood source. Place a woodcutter within 3 horizontal or vertical tile steps, then assign a villager to woodcutting.',water:'Renewable source for farming and fishing. Place a farm or fishing hut within 3 horizontal or vertical tile steps, then assign a villager to farming or fishing.',rock:'This outcrop occupies its tile. Mining is not available yet.'}[item.kind];
  }else{
    $('selection-title').textContent=labels[item.kind];
    if(item.kind==='home'&&!item.construction)$('selection-details').append(fact(`+${2*buildingLevel(item)}`,'Villager spaces'));
    if(item.kind==='storehouse')$('selection-details').append(fact(storageCapacity(town),'Capacity per resource'));
    if(item.kind==='hall'){
      $('selection-details').append(fact(town.level,'Hall level'));
      $('selection-details').append(fact(`${buildingRange(town)} tile steps`,'Building range'));
    }
    p.textContent={hall:'The heart of your settlement. Harvest wood to unlock upgrades through level 10. Each completed upgrade awards coins.',home:`Room for ${2*buildingLevel(item)} additional villagers, up to 16 in the town. New residents need available housing and sufficient food.`,storehouse:`Adds ${100*buildingLevel(item)} storage for each resource. Town capacity: ${storageCapacity(town)} wood and food.`,woodcutter:'A workplace for gathering wood from nearby trees.',farm:'A workplace for growing food near water. Build acres nearby for farmers to tend.',fishing_hut:'A workplace for catching fish for food near water.',acre:'A crop field tended by farmers from its farm. Its farm needs water within 3 tile steps to produce food.'}[item.kind];
    if(item.kind==='hall'){
      p.textContent+=` Build within ${buildingRange(town)} horizontal and vertical tile steps of the hall; diagonals count as 2.${town.level<2?' Upgrading expands the range to 15 steps.':''}`;
      $('hall-details').hidden=false;
      const upgrade=world.hall_upgrades?.find(next=>next.level===town.level+1);
      const target=upgrade?.harvested || 1;
      $('hall-progress').hidden=!upgrade;
      $('hall-progress').max=target;
      $('hall-progress').value=Math.min(target,town.harvested);
      $('hall-progress').setAttribute('aria-label',`Wood harvested: ${Math.min(target,town.harvested)} out of ${target}`);
      $('progress').textContent=upgrade
        ?`Level ${town.level} · ${town.level===1?'Timber roots':'Next level'}: ${Math.min(target,town.harvested)}/${target} wood harvested · Reward: ${upgrade.coins} coins`
        :`Level ${town.level} · Maximum hall level reached`;
      $('upgrade').textContent=town.upgrade_remaining
        ?`Upgrading to level ${town.level+1} · ${town.upgrade_remaining}s remaining`
        :upgrade?`Upgrade to level ${upgrade.level} · ${upgrade.wood} wood · ${upgrade.seconds} seconds`:'Hall fully upgraded';
      $('upgrade').disabled=!!town.upgrade_remaining || viewed!==identity?.town_id || !upgrade || town.harvested<target || town.wood<upgrade.wood;
    }
    $('selection-move').hidden=viewed!==identity?.town_id;
    $('selection-move').textContent=town.coins?'Move building · 1 coin':'Move building · Free';
    $('selection-move').onclick=()=>{const index=selection.index;openPanel('build');tool='move';moveIndex=index;render();};
    $('selection-destroy').hidden=viewed!==identity?.town_id || item.kind==='hall';
    if(item.construction){
      const c=item.construction,stage=c.progress===0?'Waiting for a builder':c.progress<c.total/3?'Foundations':c.progress<c.total*2/3?'Framing':'Finishing';
      p.textContent=`${stage} · ${Math.floor(c.progress/c.total*100)}% complete. Idle villagers help automatically; assigned Builders take priority. Benefits become available when finished.`;
      const progress=document.createElement('progress');progress.max=c.total;progress.value=c.progress;progress.setAttribute('aria-label',`${labels[item.kind]} construction`);$('selection-details').append(progress);
    }
    $('selection-details').append(p);
    if(costs[item.kind]){
      const level=buildingLevel(item),cost=costs[item.kind]*level;
      $('selection-details').append(fact(level,'Building level'));
      if(!item.construction)$('selection-details').append(fact(upgradeBenefit(item.kind,level),'Current benefit'));
      if(item.kind==='farm'||item.kind==='acre'){
        const note=document.createElement('p');note.textContent='Farm and acre production bonuses add together when tending that field.';$('selection-details').append(note);
      }
      const button=document.createElement('button');
      button.className='upgrade-button';
      button.textContent=level>=3?'Fully upgraded · Level 3':`Upgrade to level ${level+1} · ${cost} wood · ${upgradeBenefit(item.kind,level+1)}`;
      button.disabled=!!item.construction||level>=3||town.wood<cost||viewed!==identity?.town_id;
      const index=selection.index;
      button.onclick=()=>{if(!button.disabled&&viewed===identity?.town_id)act({action:'upgrade_building',index});};
      $('selection-details').append(button);
    }
    if(item.kind==='farm'&&!item.construction&&viewed===identity?.town_id){
      const button=document.createElement('button');button.textContent='Build acre · 5 wood';button.disabled=town.wood<costs.acre;
      button.onclick=()=>{acreFarmIndex=selection.index;openPanel('build');tool='acre';moveIndex=null;buildTile=null;hoverTile=null;render();};
      $('selection-details').append(button);
    }
    if(!item.construction&&(item.kind==='woodcutter'||item.kind==='farm'||item.kind==='fishing_hut')){
      const assigned=town.villagers.filter((v,i)=>settlerBuilding(town,v,i)===selection.index);
      const status=document.createElement('p');status.className='workplace-status';
      const nearby=buildingUsable(town,item);
      status.textContent=!nearby?'Needs a source within 3 tile steps.':!assigned.length?'Source nearby. Assign villagers to start work.':`${assigned.length} assigned worker${assigned.length===1?'':'s'}.`;
      $('selection-details').append(status);
    }
    if(!item.construction)renderSettlers(town,item,selection.index);
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
  if(!insideBuildingRange(town,selected)){notice(`Build within ${buildingRange(town)} tile steps of your town hall.`);return;}
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
    if(next.shared_map)for(const town of Object.values(next.towns))Object.assign(town,next.shared_map);
    world=next;identity=identities[world.world_id] || null;
    // Keep characters moving even if the lightweight endpoint is unavailable.
    updateRemotePlayers({world_id:world.world_id,sampled_at:world.players_sampled_at,
      players:Object.fromEntries(Object.entries(world.towns).filter(([,town])=>town.player).map(([id,town])=>[id,town.player]))});
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
  $('build-button').disabled=!own || (!world.shared_map && viewed!==identity?.town_id);
  $('find-player-button').disabled=!own;
  $('villagers-button').disabled=!town;
  $('move').textContent=own?.coins?'Move a building · 1 coin':'Move a building · Free';
  $('mode-hint').textContent=tool==='move'?(moveIndex===null?'Moving: click a building. Escape to cancel.':'Moving: click open grass to move here. Escape to cancel.'):tool?`Building ${labels[tool].toLowerCase()}: hover over open grass and left-click to build. Escape to cancel.`:'Click a building, villager, or resource to learn more.';
  $('view-title').textContent=(world.shared_map?world.name:town?.name) || 'A place to call home';
  $('resources-empty').hidden=!!own;$('resource-totals').hidden=!own;
  if(own){
    $('resource-wood').textContent=Math.floor(own.wood);
    $('resource-food').textContent=Math.floor(own.food);
    $('resource-coins').textContent=own.coins;
    $('resource-villagers').textContent=own.villagers.length;
  }
  if(buildTile && (!town || !insideBuildingRange(town,buildTile) || town.buildings.some(b=>b.x===buildTile.x&&b.y===buildTile.y) || town.map_resources.some(r=>r.x===buildTile.x&&r.y===buildTile.y)))buildTile=null;
  for(const button of $('buildings').children)button.classList.toggle('selected',button.textContent===`${labels[tool]} · ${costs[tool]} wood`);
  if(own){$('town-name').textContent=own.name;$('hint').textContent=tool==='move'?(moveIndex===null?'Click a building to move it.':'Click open grass.'):tool?(building?'Building…':own.wood<costs[tool]?'Not enough wood.':`Hover over open grass, then left-click to build a ${labels[tool].toLowerCase()}.`):'Choose a building, then hover over an open tile.';}
  $('towns').replaceChildren();
  for(const [id,t] of Object.entries(world.towns)){const b=document.createElement('button');b.textContent=t.name+(id===identity?.town_id?' · Yours':'');b.classList.toggle('selected',id===viewed);b.onclick=()=>{viewed=id;if(world.shared_map){const hall=t.buildings.find(b=>b.kind==='hall');camera.x=hall.x;camera.y=hall.y;}tool=null;moveIndex=null;buildTile=null;hoverTile=null;selection=null;panel=null;keyboardTile=null;render();};$('towns').append(b);}
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
    if((world.shared_map || world.towns[viewed])?.terrain?.[r.y]?.[r.x]==='b'){
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
function animatedVillagers(town, seconds, townId=viewed, frameLayout=null){
  const townKey=`${world.world_id}/${world.shared_map?'shared':viewed}`;
  if(workerTown!==townKey){workers.clear();workerTown=townKey;}
  if(!town)return [];
  const tiles=items=>items.map(({kind,x,y,farm_index,construction})=>[kind,x,y,farm_index,!!construction]);
  const layout=frameLayout ?? JSON.stringify([tiles(world.shared_map?Object.values(world.towns).flatMap(t=>t.buildings):town.buildings),tiles(town.map_resources)]);
  return town.villagers.map((villager,index)=>{
    const signature=JSON.stringify([villager.job,villager.building_index,villager.work_target?tiles([villager.work_target]):null,
      villager.construction_target?tiles([villager.construction_target]):null]);
    const workerKey=world.shared_map?`${townId}/${index}`:index;
    let state=workers.get(workerKey);
    if(!state || state.layout!==layout || state.signature!==signature){
      state=Object.assign(WorkerAnimation.plan(world.shared_map?{...town,obstacles:Object.values(world.towns).flatMap(t=>t.buildings),home_hall:town.buildings.find(b=>b.kind==='hall')}:town,villager,index,state),{signature,layout});workers.set(workerKey,state);
    }
    const resource=villager.job==='woodcutting'?'wood':'food';
    state.latestFishingCatch=villager.fishing_catch;
    const active=!!villager.construction_target || town[resource]<storageCapacity(town);
    WorkerAnimation.advance(state,seconds,active);
    return {kind:'villager',townId,villager,index,x:state.x,y:state.y,state,active};
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
  // Earned gear stays visible across jobs: tools at 5, clothing at 10, gold accents at 20.
  const v=b.kind==='villager'?b.villager:{};
  return {axe:v.woodcutting>=5,blouse:v.woodcutting>=10,woodcuttingMaster:v.woodcutting>=20,
    sickle:v.farming>=5,hat:v.farming>=10,farmingMaster:v.farming>=20,
    pouch:v.fishing>=5,scarf:v.fishing>=10,fishingMaster:v.fishing>=20};
}
function drawVillager(b){
  const accessories=villagerAccessories(b);
  const [px,py]=point(b.x,b.y),s=b.state;
  const walking=s.path.length>0,working=s.phase==='working'&&b.active;
  const stride=walking?Math.sin(s.clock*13):0;
  const chopping=working&&b.villager.job==='woodcutting';
  const fishing=s.phase==='working'&&b.villager.job==='fishing';
  const farming=s.phase==='working'&&b.villager.job==='farming';
  const constructing=working&&!!b.villager.construction_target;
  const farmStage=Math.min(3,Math.floor((s.elapsed || 0)/2));
  const farmSwing=(Math.sin((s.elapsed || 0)*Math.PI*3)+1)/2;
  const fishingTime=s.elapsed || 0;
  const cast=Math.min(fishingTime/.55,1),reel=Math.max(0,Math.min((fishingTime-2.35)/.85,1));
  const swing=working?(Math.sin(s.clock*(chopping?5:7))+1)/2:0;
  ctx.save();ctx.translate(px+(b.index%3-1)*3,py+tileH*.65+Math.floor(b.index/3));ctx.scale(tileW/46,tileW/46);
  ctx.fillStyle='#223b2966';ctx.beginPath();ctx.ellipse(0,2,7,3,0,0,Math.PI*2);ctx.fill();
  const facingTarget=fishing?(s.castTarget || s.source):s.source;
  const towardSource=facingTarget?(facingTarget.x-s.x)-(facingTarget.y-s.y):0;
  const facing=(working||fishing||farming)&&towardSource!==0?Math.sign(towardSource):s.facing;
  ctx.scale(facing,1);
  ctx.strokeStyle='#283c38';ctx.lineWidth=2.5;ctx.lineCap='round';
  ctx.beginPath();ctx.moveTo(-2,-4);ctx.lineTo(-2-stride*3,1);ctx.moveTo(2,-4);ctx.lineTo(2+stride*3,1);ctx.stroke();
  const bob=walking?Math.abs(stride)*1.2:fishing?reel*1.5:farming?farmSwing*(farmStage===0?2:1):chopping?swing*2.5:working?swing:Math.sin(s.clock*2)*.3;
  ctx.translate(0,-bob);
  ctx.fillStyle=b.kind==='npc'?npcColors[b.biome][0]:b.kind==='player'?'#d6b85d':['#7899a2','#b9866b','#a0a16b','#947ca0','#c6aa62'][b.index%5];ctx.fillRect(-4,-13,8,10);
  if(accessories.blouse){
    polygon([[-4,-13],[4,-13],[7,-9],[5,-7],[3,-10],[4,-3],[-4,-3],[-3,-10],[-5,-7],[-7,-9]],'#b8c6ad');
    polygon([[-3,-13],[0,-10],[3,-13]],'#eee0bd');
    ctx.strokeStyle='#6c8068';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(0,-10);ctx.lineTo(0,-4);ctx.stroke();
    if(accessories.woodcuttingMaster){
      ctx.fillStyle='#e8bd55';ctx.fillRect(-4,-4,8,1);ctx.fillRect(-1,-9,2,2);
    }
  }
  ctx.fillStyle=['#e7c68c','#c99f79','#ad795c'][b.index%3];ctx.beginPath();ctx.arc(0,-17,4,0,Math.PI*2);ctx.fill();
  ctx.fillStyle=['#584638','#aa7846','#363c36','#d3bc8c'][b.index%4];ctx.beginPath();ctx.arc(0,-18,4,Math.PI,Math.PI*2);ctx.fill();
  if(b.index%2)ctx.fillRect(-4,-18,2,7);
  if(accessories.hat){polygon([[-6,-19],[0,-23],[6,-19]],'#ccb779');ctx.fillStyle='#ccb779';ctx.fillRect(-7,-19,14,2);}
  if(accessories.farmingMaster){ctx.fillStyle='#e8bd55';ctx.fillRect(-5,-20,10,1);}
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
    if(accessories.fishingMaster){ctx.fillStyle='#e8bd55';ctx.fillRect(-1,-4,2,2);}
  }
  if(accessories.scarf){
    ctx.fillStyle='#6baeb5';ctx.fillRect(-4,-14,8,2);ctx.fillRect(2,-12,2,5);
  }
  ctx.strokeStyle='#e7c68c';ctx.lineWidth=2;
  const farmHandX=farmStage===1?7+farmSwing*8:7;
  const farmHandY=farmStage===0?-8+farmSwing*4: farmStage===2?-8:-9-farmSwing*3;
  ctx.beginPath();ctx.moveTo(-3,-11);ctx.lineTo(chopping?6:-5-stride*2,chopping?-14+swing*5:-6);ctx.moveTo(3,-11);ctx.lineTo(farming?farmHandX:7,chopping?-14+swing*5:fishing?-10-reel*3:farming?farmHandY:-9-swing*4);ctx.stroke();
  if(constructing){
    const handY=-9-swing*4;
    ctx.strokeStyle='#a78454';ctx.lineWidth=2;
    ctx.beginPath();ctx.moveTo(7,handY);ctx.lineTo(10,handY-8);ctx.stroke();
    ctx.fillStyle='#b9c3bd';ctx.fillRect(6,handY-11,9,4);
  }else if(fishing){
    // Cast into the selected water tile, then lift the catch before delivery.
    const [waterX,waterY]=s.castTarget?point(s.castTarget.x+.5,s.castTarget.y+.5):s.workSpot?point(s.source.x+.35,s.source.y+.35):s.source?point(s.source.x,s.source.y):[px+tileW*.6,py+tileH*.65];
    const surfaceX=(waterX-px-(b.index%3-1)*3)/(tileW/46)*facing;
    const surfaceY=(waterY-py-tileH*.65-Math.floor(b.index/3))/(tileW/46)+bob;
    const tipX=7+cast*13-reel*8,tipY=-34+cast*9-reel*9;
    const floatX=tipX+(surfaceX-tipX)*cast*(1-reel);
    const floatY=tipY+(surfaceY-tipY)*cast*(1-reel)+Math.sin(fishingTime*9)*cast*(1-reel);
    ctx.strokeStyle='#a78454';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(7,-10-reel*3);ctx.lineTo(tipX,tipY);ctx.stroke();
    ctx.strokeStyle='#d5dfd8';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(tipX,tipY);ctx.lineTo(floatX,floatY);ctx.stroke();
    if(cast===1&&reel===0){
      ctx.strokeStyle='#c3e1dc';ctx.beginPath();ctx.ellipse(surfaceX,surfaceY,4+Math.sin(fishingTime*6)*1.5,2,0,0,Math.PI*2);ctx.stroke();
    }
    ctx.fillStyle='#df785e';ctx.fillRect(floatX-2,floatY-2,4,3);
    ctx.fillStyle='#f3e5bc';ctx.fillRect(floatX-2,floatY+1,4,2);
    if(reel>0&&s.fishingCaught!==false){
      polygon([[floatX,floatY+3],[floatX+5,floatY+6],[floatX,floatY+9],[floatX-4,floatY+6]],'#b8d8d5','#527f86');
      polygon([[floatX,floatY+9],[floatX-3,floatY+12],[floatX+3,floatY+12]],'#7899a2');
    }
  }else if(farming){
    if(farmStage===0){
      // Pull a hoe through the soil, with small clods at the blade.
      const bladeX=17-farmSwing*7;
      ctx.strokeStyle='#a78454';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(farmHandX,farmHandY);ctx.lineTo(bladeX,1);ctx.stroke();
      polygon([[bladeX-2,0],[bladeX+6,0],[bladeX+5,4],[bladeX-2,3]],'#aab9b4','#6b7d79');
      ctx.fillStyle='#8b6845';for(let i=0;i<3;i++)ctx.fillRect(bladeX+3+i*3,3-farmSwing*(i+1),2,2);
    }else if(farmStage===1){
      // Scatter seeds from a pouch with an outward sweep of the hand.
      polygon([[-7,-7],[-2,-7],[-1,-1],[-8,-1]],'#9b754e','#6d513b');
      ctx.fillStyle='#efd59a';
      for(let i=0;i<5;i++){
        const flight=((s.elapsed || 0)*2+i*.2)%1;
        ctx.fillRect(farmHandX+flight*12,-8+flight*flight*12+i*.4,2,2);
      }
    }else if(farmStage===2){
      // Tilt the can and pour a moving stream of droplets.
      ctx.save();ctx.translate(farmHandX,farmHandY);ctx.rotate(.15+farmSwing*.35);
      ctx.strokeStyle='#607f83';ctx.lineWidth=2;ctx.beginPath();ctx.arc(-2,-3,4,Math.PI,Math.PI*2);ctx.stroke();
      polygon([[-6,-3],[3,-3],[3,4],[-6,4]],'#7faeb2','#4f757b');
      polygon([[3,0],[9,-4],[10,-2],[3,3]],'#7faeb2','#4f757b');ctx.restore();
      ctx.fillStyle='#9fd8e5';
      for(let i=0;i<6;i++){
        const fall=((s.elapsed || 0)*3+i/6)%1;
        ctx.fillRect(16+fall*5+i%2,-9+fall*14,1.5,3);
      }
    }else{
      // Sweep a curved sickle through a small stand of ripe grain.
      ctx.strokeStyle='#d6b765';ctx.lineWidth=1.5;ctx.beginPath();
      for(let i=0;i<4;i++){ctx.moveTo(13+i*3,4);ctx.lineTo(12+i*3,-5+farmSwing*2);}ctx.stroke();
      ctx.fillStyle='#e5c77f';for(let i=0;i<4;i++)ctx.fillRect(10+i*3,-7+farmSwing*2,3,4);
      ctx.save();ctx.translate(farmHandX,farmHandY);ctx.rotate(-.8+farmSwing*1.6);
      ctx.strokeStyle='#a78454';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(0,3);ctx.lineTo(0,-5);ctx.stroke();
      ctx.strokeStyle='#dce5df';ctx.lineWidth=2;ctx.beginPath();ctx.arc(1,-8,5,-Math.PI/2,Math.PI/2);ctx.stroke();ctx.restore();
    }
  }else if(working){
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
    ctx.fillStyle=b.villager.job==='woodcutting'?'#997044':b.villager.job==='fishing'?'#7899a2':'#d6c478';ctx.fillRect(2,-8,11,5);
    ctx.fillStyle='#e1c593';ctx.beginPath();ctx.arc(13,-5.5,2.5,0,Math.PI*2);ctx.fill();
    if(b.villager.job==='fishing')polygon([[5,-10],[9,-12],[13,-10],[9,-8]],'#b8d8d5','#527f86');
    if(b.villager.job==='farming'){
      ctx.strokeStyle='#e5c77f';ctx.lineWidth=2;ctx.beginPath();
      for(let i=0;i<4;i++){ctx.moveTo(5+i*2,-4);ctx.lineTo(4+i*2,-12);}ctx.stroke();
    }
  }
  ctx.restore();
}
// A padded viewport keeps static ground reusable during walking and panning.
// One canvas bounds memory usage even on Retina displays.
let groundCache=null,resourceCache=null;
function renderResources(resources){
  if(resourceCache?.resources!==resources || resourceCache.count!==resources.length){
    const water=new Set(),objects=[],indexes=new Map();
    resources.forEach((r,index)=>{
      indexes.set(r,index);
      if(r.kind==='water')water.add(`${r.x},${r.y}`);else objects.push(r);
    });
    resourceCache={resources,count:resources.length,water,objects,indexes,
      layout:JSON.stringify(resources.map(({kind,x,y,farm_index,construction})=>[kind,x,y,farm_index,!!construction]))};
  }
  return resourceCache;
}
function paintGround(target,landscape,water,bounds,offsetX=0,offsetY=0){
  const ctx=target,terrainSize=landscape?.map_size || 12;
  const {minX,maxX,minY,maxY}=bounds;
  const point=(x,y)=>[originX+(x-y)*tileW/2+offsetX,originY+(x+y)*tileH/2+offsetY];
  const polygon=(points,fill,stroke)=>{
    ctx.beginPath();points.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));
    ctx.closePath();ctx.fillStyle=fill;ctx.fill();
    if(stroke){ctx.strokeStyle=stroke;ctx.stroke();}
  };
  for(let x=minX;x<=maxX;x++)for(let y=minY;y<=maxY;y++){
    const [px,py]=point(x,y),wet=water.has(`${x},${y}`);
    if(px+tileW/2<0 || px-tileW/2>target.canvas?.width/(window.devicePixelRatio||1)
      || py+tileH+12<0 || py>target.canvas?.height/(window.devicePixelRatio||1))continue;
    const variation=(x*17+y*31+x*y*7)%7;
    const diamond=[[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]];
    const biome=landscape?.terrain?.[y]?.[x] || 'g';
    if(biome==='v')continue;
    // Ground thickness appears only at the actual valley boundary.
    const land=(tx,ty)=>tx>=0&&ty>=0&&tx<terrainSize&&ty<terrainSize&&landscape?.terrain?.[ty]?.[tx]!=='v';
    if(!land(x+1,y))polygon([diamond[1],diamond[2],[diamond[2][0],diamond[2][1]+12],[diamond[1][0],diamond[1][1]+12]],'#344b35');
    if(!land(x,y+1))polygon([diamond[2],diamond[3],[diamond[3][0],diamond[3][1]+12],[diamond[2][0],diamond[2][1]+12]],'#405438');
    const palette={g:['#68865a','#69875b','#668358'],f:['#506f4a','#55764c','#5a7b50'],
      m:['#83945e','#899b65','#80915b'],s:['#b4a575','#bdad7d','#b8a878'],r:['#8b8d7c','#969785','#858978'],b:['#59674b','#626f50','#536247']};
    const neighbors=[[x,y-1],[x+1,y],[x,y+1],[x-1,y]];
    const shores=neighbors.map(([tx,ty])=>!water.has(`${tx},${ty}`));
    const depth=shores.filter(Boolean).length;
    const waterColor=biome==='b'?(depth?'#718565':'#4e6857'):(depth?'#6299a1':'#447d91');
    polygon(diamond,wet?waterColor:palette[biome][variation%3],null);
    if(biome==='b'&&variation%3===0){
      ctx.strokeStyle='#9b9f69';ctx.lineWidth=tileW/46;
      for(let reed=0;reed<3;reed++){const rx=px+(reed-1)*tileW*.07,ry=py+tileH*.7;ctx.beginPath();ctx.moveTo(rx,ry);ctx.lineTo(rx+(reed-1)*tileW*.025,ry-tileH*.4);ctx.stroke();}
      if(wet){ctx.fillStyle='#a3ad74';ctx.beginPath();ctx.ellipse(px+tileW*.17,py+tileH*.55,tileW*.065,tileH*.075,0,0,Math.PI*2);ctx.fill();}
    }
    if(!wet&&variation===0){ctx.fillStyle='#8fa574';ctx.fillRect(px-5*tileW/46,py+tileH*.5,2*tileW/46,2*tileW/46);ctx.fillStyle='#d7c78a';ctx.fillRect(px+4*tileW/46,py+tileH*.65,2*tileW/46,2*tileW/46);}
    if(!wet&&(x===0||y===0||x===terrainSize-1||y===terrainSize-1)&&variation%2===0){ctx.strokeStyle='#9caf77';ctx.lineWidth=tileW/46;ctx.beginPath();ctx.moveTo(px-6*tileW/46,py+tileH*.65);ctx.lineTo(px-9*tileW/46,py+tileH*.4);ctx.moveTo(px-6*tileW/46,py+tileH*.65);ctx.lineTo(px-4*tileW/46,py+tileH*.35);ctx.stroke();}
    if(wet){
      ctx.strokeStyle='#b4d7ce99';ctx.lineWidth=tileW/46;
      for(let edge=0;edge<4;edge++)if(shores[edge]){
        ctx.beginPath();ctx.moveTo(...diamond[edge]);ctx.lineTo(...diamond[(edge+1)%4]);ctx.stroke();
      }
      const ripple=((x*73856093)^(y*19349663)^(landscape?.terrain_seed || 0))>>>0;
      if(ripple%5===0){
        const offset=(ripple%13-6)*tileW/70;
        ctx.beginPath();ctx.moveTo(px-tileW*.14+offset,py+tileH*.5);
        ctx.quadraticCurveTo(px+offset,py+tileH*.38,px+tileW*.12+offset,py+tileH*.55);ctx.stroke();
      }
    }
  }
}
function drawGround(rect,dpr,landscape,water,bounds){
  const margin=160;
  if(!groundCache || groundCache.world!==world || groundCache.landscape!==landscape){
    const signature=JSON.stringify([world.world_id,landscape?.terrain,landscape?.terrain_seed,
      landscape?.map_size,landscape?.map_resources]);
    if(groundCache?.signature!==signature)groundCache=null;
    if(groundCache)Object.assign(groundCache,{world,landscape});
    else{
      const surface=document.createElement('canvas'),target=surface.getContext?.('2d');
      if(!target){paintGround(ctx,landscape,water,bounds);return;}
      groundCache={surface,target,signature,world,landscape};
    }
  }
  const cache=groundCache,dx=originX-cache.originX,dy=originY-cache.originY;
  if(cache.tileW!==tileW || cache.width!==rect.width || cache.height!==rect.height
    || cache.dpr!==dpr || Math.abs(dx)>margin || Math.abs(dy)>margin){
    cache.surface.width=Math.ceil((rect.width+margin*2)*dpr);
    cache.surface.height=Math.ceil((rect.height+margin*2)*dpr);
    cache.target.setTransform(dpr,0,0,dpr,0,0);
    const corners=[[-margin-tileW,-margin-100],[rect.width+margin+tileW,-margin-100],
      [-margin-tileW,rect.height+margin+100],[rect.width+margin+tileW,rect.height+margin+100]]
      .map(([px,py])=>({x:(px-originX)/tileW+(py-originY)/tileH,y:(py-originY)/tileH-(px-originX)/tileW}));
    const size=landscape?.map_size || 12;
    const padded={minX:Math.max(0,Math.floor(Math.min(...corners.map(p=>p.x)))),
      maxX:Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.x)))),
      minY:Math.max(0,Math.floor(Math.min(...corners.map(p=>p.y)))),
      maxY:Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.y))))};
    paintGround(cache.target,landscape,water,padded,margin,margin);
    Object.assign(cache,{originX,originY,tileW,width:rect.width,height:rect.height,dpr});
  }
  ctx.drawImage(cache.surface,originX-cache.originX-margin,originY-cache.originY-margin,
    cache.surface.width/dpr,cache.surface.height/dpr);
}
function draw(seconds=0){
  const rect=canvas.getBoundingClientRect(),dpr=window.devicePixelRatio||1;
  const width=Math.round(rect.width*dpr),height=Math.round(rect.height*dpr);
  if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}
  ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,rect.width,rect.height);
  const townKey=`${world.world_id}/${world.shared_map?'shared':viewed}`;
  const town=world.towns[viewed], own=world.towns[identity?.town_id];
  if(cameraTown!==townKey){
    const center=world.shared_map?(own?.player || town?.player || world.town_sites[0]):{x:5.5,y:5.5};
    camera={x:center.x,y:center.y,zoom:1};cameraTown=townKey;
  }
  const hero=playerSprite(world.shared_map?(own || town):town,seconds);
  tileW=Math.min(54,(rect.width-30)/12)*camera.zoom;tileH=tileW/2;
  originX=rect.width/2-(camera.x-camera.y)*tileW/2;
  originY=rect.height/2-(camera.x+camera.y)*tileH/2-tileH/2;
  if(typeof camera==='undefined'&&hero&&playerView.following){
    originX=rect.width/2-(hero.x-hero.y)*tileW/2;
    originY=rect.height/2-(hero.x+hero.y)*tileH/2-tileH/2;
  }
  hitTargets=[];
  const resources=world.shared_map?.map_resources || town?.map_resources || [];
  const resourceData=renderResources(resources),water=resourceData.water;
  const corners=[[-tileW,-100],[rect.width+tileW,-100],[-tileW,rect.height+100],[rect.width+tileW,rect.height+100]]
    .map(([px,py])=>({x:(px-originX)/tileW+(py-originY)/tileH,y:(py-originY)/tileH-(px-originX)/tileW}));
  const size=mapSize(town),minX=Math.max(0,Math.floor(Math.min(...corners.map(p=>p.x)))),maxX=Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.x))));
  const minY=Math.max(0,Math.floor(Math.min(...corners.map(p=>p.y)))),maxY=Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.y))));
  const showBuildingRange=town && (panel==='build' || (selection?.type==='building' && town.buildings[selection.index]?.kind==='hall'));
  const preview=hoverTile || keyboardTile;
  const previewKind=costs[tool]?tool:tool==='move'&&moveIndex!==null?town?.buildings[moveIndex]?.kind:null;
  const sourceKind=previewKind==='woodcutter'?'tree':(previewKind==='farm'||previewKind==='fishing_hut')?'water':null;
  const reachable=new Set(preview&&sourceKind?resources.filter(r=>r.kind===sourceKind&&Math.abs(r.x-preview.x)+Math.abs(r.y-preview.y)<=3).map(r=>`${r.x},${r.y}`):[]);
  drawGround(rect,dpr,world.shared_map || town,water,{minX,maxX,minY,maxY});
  // Selection and construction overlays remain live above the cached ground.
  if(tool || showBuildingRange || reachable.size || selection || keyboardTile){
    for(let x=minX;x<=maxX;x++)for(let y=minY;y<=maxY;y++){
      const [px,py]=point(x,y);
      if(px+tileW/2<0 || px-tileW/2>rect.width || py+tileH+12<0 || py>rect.height)continue;
      if((world.shared_map || town)?.terrain?.[y]?.[x]==='v')continue;
      const diamond=[[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]];
      if(tool)polygon(diamond,'#00000000','#91a37455');
      if(showBuildingRange && insideBuildingRange(town,{x,y}))polygon(diamond,'#e4c98022','#e4c98088');
      if(reachable.has(`${x},${y}`))polygon(diamond,'#b8dcad44','#c6dfaa');
      if(preview?.x===x&&preview?.y===y&&previewKind&&town){const valid=placementValid(town,preview,previewKind)&&buildingUsable(town,{...preview,kind:previewKind});ctx.save();ctx.lineWidth=2;polygon(diamond,valid?'#d6d8a366':'#c76d6455',valid?'#eee5ab':'#f3a295');ctx.restore();}
      const selected=selection?.type==='building'?town?.buildings[selection.index]:selection?.type==='resource'?resources[selection.index]:null;
      if((selected?.x===x&&selected?.y===y)||(!previewKind&&keyboardTile?.x===x&&keyboardTile?.y===y)){
        ctx.save();ctx.lineWidth=3;polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],'#fff0b133','#fff0b1');ctx.restore();
      }
    }
  }
  const entries=world.shared_map?Object.entries(world.towns):[[viewed,town]];
  const buildings=world.shared_map?entries.flatMap(([id,t])=>t.buildings.map((b,index)=>({...b,townId:id,objectIndex:index}))):town?.buildings || [{kind:'hall',x:5,y:5}];
  const workerLayout='['+JSON.stringify(buildings.map(({kind,x,y,farm_index,construction})=>[kind,x,y,farm_index,!!construction]))+','+resourceData.layout+']';
  const villagers=entries.flatMap(([id,t])=>animatedVillagers(t,seconds,id,workerLayout));
  const drawnPiers=new Set();
  for(const worker of villagers){
    const pier=worker.state.pier;
    if(!pier?.length)continue;
    const id=JSON.stringify(pier);
    if(drawnPiers.has(id))continue;
    drawnPiers.add(id);
    // Decks sit on the ground plane, under buildings and the fishing worker.
    for(const tile of pier.slice(1)){
      const [px,py]=point(tile.x,tile.y);
      polygon([[px,py+tileH*.15],[px+tileW*.32,py+tileH*.5],
        [px,py+tileH*.85],[px-tileW*.32,py+tileH*.5]],'#ac895d','#624e38');
      ctx.strokeStyle='#71583d';ctx.lineWidth=tileW/46;
      for(let plank=0;plank<3;plank++){
        const offset=(plank-1)*tileH*.16;
        ctx.beginPath();ctx.moveTo(px-tileW*.2,py+tileH*.5+offset);
        ctx.lineTo(px+tileW*.2,py+tileH*.5+offset);ctx.stroke();
      }
    }
  }
  const cropObjects=[];
  for(const field of buildings){
    if(field.kind!=='acre'||field.construction)continue;
    const [px,py]=point(field.x,field.y);
    if(px < -tileW || px > rect.width+tileW || py < -tileH || py > rect.height+80*tileW/46)continue;
    const farmer=villagers.find(worker=>worker.villager.job==='farming'&&!worker.state.construction
      &&worker.state.phase==='working'&&worker.state.source?.kind==='acre'
      &&worker.state.source.x===field.x&&worker.state.source.y===field.y);
    // Soil and furrows stay below every character; stalks use their own depth.
    drawFarmCrops(field,farmer?.state,crop=>cropObjects.push(crop));
  }
  const otherPlayers=world.shared_map?entries.filter(([id,t])=>t.player&&id!==hero?.townId).map(([id,t])=>remotePlayerSprite(id,t,seconds)):[];
  const objects=[...buildings,...cropObjects,...resourceData.objects,...villagers,...npcSprites(world.shared_map || town,seconds),...otherPlayers,...(hero?[hero]:[])].filter(b=>{
    const [px,py]=point(b.x,b.y);
    return px>=-tileW&&px<=rect.width+tileW&&py>=-tileH&&py<=rect.height+80*tileW/46;
  });
  for(const b of objects.sort((a,b)=>(a.depth ?? a.x+a.y)-(b.depth ?? b.x+b.y))){
    const [px,py]=point(b.x,b.y),scale=tileW/46;
    if(px < -tileW || px > rect.width+tileW || py < -tileH || py > rect.height+80*scale)continue;
    if(b.kind==='crop'){drawFarmCrop(b);continue;}
    if(b.kind==='player'){
      const baseY=py+tileH*.65;
      ctx.strokeStyle='#ffe8a0';ctx.lineWidth=2;ctx.beginPath();ctx.ellipse(px,baseY,10*scale,5*scale,0,0,Math.PI*2);ctx.stroke();
      drawVillager(b);
      ctx.fillStyle='#fff0b1';ctx.font=`bold ${12*scale}px sans-serif`;ctx.textAlign='center';
      ctx.fillText(b.townId===identity?.town_id?'You':world.towns[b.townId]?.name || 'Founder',px,baseY-31*scale);
      hitTargets.push({type:'player',index:0,townId:b.townId,left:px-10*scale,right:px+10*scale,top:baseY-27*scale,bottom:baseY+5*scale});
      continue;
    }
    const character=b.kind==='villager'||b.kind==='npc';
    const type=b.kind==='npc'?'npc':b.kind==='villager'?'villager':resourceData.indexes.has(b)?'resource':'building';
    const index=character?b.index:type==='resource'?resourceData.indexes.get(b):b.objectIndex ?? buildings.indexOf(b);
    const centerX=px+(character?(b.index%3-1)*3:0);
    const baseY=py+(character?tileH*.65+Math.floor(b.index/3):tileH/2);
    hitTargets.push({type,index,townId:b.townId || viewed,left:centerX-(character?9*scale:tileW*.45),right:centerX+(character?9*scale:tileW*.45),top:baseY-(b.kind==='tree'?70:b.kind==='hall'?75:character?26:b.kind==='acre'?0:45)*scale,bottom:baseY+(character?5*scale:tileH/2)});
    if(character&&selection?.type===type&&selection.index===b.index&&(!world.shared_map||b.townId===viewed||type==='npc')){
      ctx.fillStyle='#fff0b144';ctx.strokeStyle='#fff0b1';ctx.lineWidth=2;ctx.beginPath();ctx.ellipse(centerX,baseY,12*scale,6*scale,0,0,Math.PI*2);ctx.fill();ctx.stroke();
    }
    if(b.kind==='tree'||b.kind==='rock'){drawResource(b);continue;}
    if(character){
      drawVillager(b);
      continue;
    }
    if(b.kind==='acre'&&!b.construction)continue;
    drawBuilding(b,world.towns[b.townId] || town);
    if(world.shared_map && b.kind==='hall'){ctx.fillStyle='#fff0b1';ctx.font=`bold ${12*scale}px sans-serif`;ctx.textAlign='center';ctx.fillText(world.towns[b.townId].name,px,py-80*scale);}
  }
  if(preview && previewKind && town && town.terrain?.[preview.y]?.[preview.x]!=='v' && viewed===identity?.town_id){
    const valid=placementValid(town,preview,previewKind);
    ctx.save();ctx.globalAlpha=valid ? .65 : .35;drawBuilding({...preview,kind:previewKind},town);ctx.restore();
  }
}
function acrePlacementValid(town,tile,index){
  const farm=town.buildings[index];
  return farm?.kind==='farm'&&!farm.construction&&Math.abs(tile.x-farm.x)+Math.abs(tile.y-farm.y)<=3;
}
function buildingRange(town){return town.level>=2?15:10;}
function insideBuildingRange(town,tile){
  const hall=town.buildings.find(b=>b.kind==='hall');
  const origin=town.town_origin;
  return !!hall && Math.abs(tile.x-hall.x)+Math.abs(tile.y-hall.y)<=buildingRange(town)
    && (!origin || Math.abs(tile.x-origin.x)+Math.abs(tile.y-origin.y)<=15);
}
function placementValid(town,tile,kind){
  return insideBuildingRange(town,tile) && town.terrain?.[tile.y]?.[tile.x]!=='v'
    && (kind!=='acre'||acrePlacementValid(town,tile,tool==='move'?town.buildings[moveIndex]?.farm_index:acreFarmIndex))
    && !(world.shared_map?Object.values(world.towns).flatMap(t=>t.buildings):town.buildings).some(b=>b.x===tile.x&&b.y===tile.y)
    && !(town.npcs || []).some(n=>n.x===tile.x&&n.y===tile.y)
    && !town.map_resources.some(r=>r.x===tile.x&&r.y===tile.y)
    && (!world.shared_map || !Object.values(world.towns).some(t=>t.player&&Math.abs(t.player.x-tile.x)<.68&&Math.abs(t.player.y-tile.y)<.68))
    && (!world.shared_map || kind!=='hall' || !Object.values(world.towns).some(t=>t!==town&&t.buildings.some(b=>b.kind==='hall'&&Math.hypot(b.x-tile.x,b.y-tile.y)<40)))
    && (tool==='move' || town.wood>=costs[kind]);
}
function buildingUsable(town,b){
  const source={woodcutter:'tree',farm:'water',fishing_hut:'water'}[b.kind];
  return !source || !!town?.map_resources.some(r=>r.kind===source&&Math.abs(r.x-b.x)+Math.abs(r.y-b.y)<=3);
}
function drawFarmCrops(b,state,collectCrop=null){
  const elapsed=state?.elapsed ?? 6;
  const stage=Math.min(3,Math.floor(elapsed/2)),progress=(elapsed%2)/2;
  const [x,y]=point(b.x,b.y),scale=tileW/46;
  polygon([[x,y+tileH*.1],[x+tileW*.4,y+tileH*.5],[x,y+tileH*.9],[x-tileW*.4,y+tileH*.5]],'#80764a','#ab9561');
  for(let row=0;row<4;row++){
    const [ax,ay]=point(b.x-.3+row*.2,b.y-.35),[bx,by]=point(b.x-.3+row*.2,b.y+.35);
    ctx.strokeStyle='#655638';ctx.lineWidth=scale;
    ctx.beginPath();ctx.moveTo(ax,ay+tileH/2);ctx.lineTo(bx,by+tileH/2);ctx.stroke();
    for(let crop=0;crop<4;crop++){
      const order=row*4+crop;
      if(stage===0 || (stage===1&&order>=progress*16) || (state&&stage===3&&order<progress*16))continue;
      const x=b.x-.3+row*.2,y=b.y-.3+crop*.2;
      // Characters' feet sit .15 tile heights ahead of a crop's ground anchor.
      const stalk={kind:'crop',x,y,depth:x+y-.3,stage,
        height:stage===2?2+progress*4:6,
        sway:state?Math.sin(elapsed*4+order):0,
        color:stage===2?'#8db568':(row+crop)%2?'#d9bd70':'#b8b458'};
      if(collectCrop)collectCrop(stalk);else drawFarmCrop(stalk);
    }
  }
}
function drawFarmCrop(crop){
  const [px,py]=point(crop.x,crop.y),scale=tileW/46;
  if(crop.stage===1){
    ctx.fillStyle='#efd59a';ctx.fillRect(px-scale,py+tileH/2-scale,2*scale,2*scale);
  }else{
    ctx.strokeStyle=crop.color;ctx.lineWidth=2*scale;
    ctx.beginPath();ctx.moveTo(px,py+tileH/2);
    ctx.lineTo(px+crop.sway*scale,py+tileH/2-crop.height*scale);ctx.stroke();
  }
}
function drawBuilding(b,town=world.towns[viewed],farmState=null){
  if(b.construction){
    const [x,top]=point(b.x,b.y),y=top+tileH/2,s=tileW/46;
    const progress=b.construction.progress/b.construction.total,w=tileW*.38,d=w*tileH/tileW;
    polygon([[x-w,y],[x,y+d],[x+w,y],[x,y-d]],'#8b795c','#d0b68a');
    ctx.strokeStyle='#d0b68a';ctx.lineWidth=3*s;
    if(progress>=1/3){
      for(const [px,py] of [[x-w,y],[x,y+d],[x+w,y]]){
        ctx.beginPath();ctx.moveTo(px,py);ctx.lineTo(px,py-25*s);ctx.stroke();
      }
      polygon([[x-w,y-25*s],[x,y+d-25*s],[x+w,y-25*s],[x,y-d-25*s]],'#b69a7044','#d0b68a');
    }
    if(progress>=2/3){
      polygon([[x-w,y],[x,y+d],[x,y+d-25*s],[x-w,y-25*s]],colors[b.kind] || '#b69a70');
      polygon([[x,y+d],[x+w,y],[x+w,y-25*s],[x,y+d-25*s]],'#857659');
      polygon([[x-w,y-25*s],[x,y-40*s],[x+w,y-25*s],[x,y+d-24*s]],'#80574b');
    }
    ctx.fillStyle='#6a5b43';ctx.fillRect(x-w,y+5*s,w*2,4*s);
    ctx.fillStyle='#e9c77e';ctx.fillRect(x-w,y+5*s,w*2*progress,4*s);
    return;
  }
  if(b.kind==='acre'){
    drawFarmCrops(b,farmState);
    return;
  }
  const [x,top]=point(b.x,b.y),y=top+tileH/2,s=tileW/46;
  const w=tileW*(b.kind==='storehouse' ? .43 : .38),depth=w*tileH/tileW,h=(b.kind==='hall'?42:b.kind==='woodcutter'?23:27)*s;
  // Wall bases share the tile's ground plane and its isometric slope.
  ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(x+8*s,y+depth*.6,w+8*s,depth+2*s,0,0,Math.PI*2);ctx.fill();
  const usable=buildingUsable(town,b);
  polygon([[x-w,y],[x,y+depth],[x,y+depth-h],[x-w,y-h]],usable?colors[b.kind]:'#c76d64');
  polygon([[x,y+depth],[x+w,y],[x+w,y-h],[x,y+depth-h]],usable?'#857659':'#a34f49');
  polygon([[x-w,y],[x,y+depth],[x,y+depth-3*s],[x-w,y-3*s]],'#797665');
  polygon([[x,y+depth],[x+w,y],[x+w,y-3*s],[x,y+depth-3*s]],'#5e6255');
  polygon([[x-w-3*s,y-h],[x,y-h-15*s],[x+w+3*s,y-h],[x,y-h+depth+1*s]],usable?(b.kind==='hall'?'#5a747b':'#80574b'):'#a34f49');
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
  if(b.kind==='farm'){
    for(let i=0;i<3;i++){
      const cx=x-16*s+i*8*s,cy=y+5*s+i*2*s;
      ctx.fillStyle='#80764a';ctx.fillRect(cx,cy,6*s,3*s);
      ctx.strokeStyle='#d9bd70';ctx.lineWidth=2*s;ctx.beginPath();ctx.moveTo(cx+3*s,cy);ctx.lineTo(cx+3*s,cy-7*s);ctx.stroke();
    }
  }
  if(b.kind==='fishing_hut'){
    ctx.strokeStyle='#d5dfd8';ctx.lineWidth=1.5*s;ctx.beginPath();ctx.moveTo(x+12*s,y-h+4*s);ctx.lineTo(x+12*s,y+4*s);ctx.stroke();
    ctx.fillStyle='#7899a2';ctx.beginPath();ctx.ellipse(x+12*s,y+5*s,6*s,3*s,0,0,Math.PI*2);ctx.fill();
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
  if(!town || x<0||y<0||x>=mapSize(town)||y>=mapSize(town))return;
  if(town.terrain?.[y]?.[x]==='v')return;
  const npc=(town.npcs || []).findIndex(n=>n.x===x&&n.y===y);
  const resource=town.map_resources.findIndex(r=>r.x===x&&r.y===y);
  const index=town.buildings.findIndex(b=>b.x===x&&b.y===y);
  if(!tool){
    if(world.shared_map){
      for(const [id,t] of Object.entries(world.towns)){const i=t.buildings.findIndex(b=>b.x===x&&b.y===y);if(i>=0){inspect('building',i,id);return;}}
    }
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
    else{
      if(!insideBuildingRange(town,{x,y})){notice(`Move within ${buildingRange(town)} tile steps of your town hall.`);return;}
      await act({action:'move',index:moveIndex,x,y});moveIndex=null;render();
    }
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
  buildTile=costs[tool]&&viewed===identity?.town_id&&town&&insideBuildingRange(town,tile)&&tile.x>=0&&tile.y>=0&&tile.x<mapSize(town)&&tile.y<mapSize(town)
    &&(tool!=='acre'||acrePlacementValid(town,tile,acreFarmIndex))&&town.terrain?.[tile.y]?.[tile.x]!=='v'&&!(town.npcs || []).some(n=>n.x===tile.x&&n.y===tile.y)&&!town.buildings.some(b=>b.x===tile.x&&b.y===tile.y)&&!town.map_resources.some(r=>r.x===tile.x&&r.y===tile.y)?tile:null;
  hoverTile=(costs[tool]||tool==='move'&&moveIndex!==null)&&viewed===identity?.town_id&&town&&tile.x>=0&&tile.y>=0&&tile.x<(town.map_size || 12)&&tile.y<(town.map_size || 12)?tile:null;
};
canvas.onmouseleave=()=>{buildTile=null;hoverTile=null;};
canvas.onclick=async event=>{
  if(event.button!==undefined&&event.button!==0)return;
  canvas.focus?.();
  const rect=canvas.getBoundingClientRect(),px=event.clientX-rect.left,py=event.clientY-rect.top;
  keyboardTile=null;
  if(!tool){
    const hit=[...hitTargets].reverse().find(h=>px>=h.left&&px<=h.right&&py>=h.top&&py<=h.bottom);
    if(hit){inspect(hit.type,hit.index,hit.townId);return;}
  }
  const tile=tileAt(event);
  await selectTile(tile.x,tile.y);
};
canvas.addEventListener('keydown',event=>{
  const directions={ArrowUp:[-1,0],ArrowDown:[1,0],ArrowLeft:[0,1],ArrowRight:[0,-1]};
  if(directions[event.key]){
    event.preventDefault();const [dx,dy]=directions[event.key];
    keyboardTile=keyboardTile || (world.shared_map?{x:Math.round(camera.x),y:Math.round(camera.y)}:{x:5,y:5});
    keyboardTile={x:Math.max(0,Math.min(mapSize(world.towns[viewed])-1,keyboardTile.x+dx)),y:Math.max(0,Math.min(mapSize(world.towns[viewed])-1,keyboardTile.y+dy))};draw();
  }else if(event.key==='Enter'||event.key===' '){event.preventDefault();const tile=keyboardTile || (world.shared_map?{x:Math.round(camera.x),y:Math.round(camera.y)}:{x:5,y:5});selectTile(tile.x,tile.y);}
});
canvas.addEventListener('wheel',event=>{
  event.preventDefault();
  const rect=canvas.getBoundingClientRect();
  const unit=event.deltaMode===1?16:event.deltaMode===2?rect.height:1;
  const dx=event.deltaX*unit,dy=event.deltaY*unit;
  if(event.altKey||event.ctrlKey){
    const px=event.clientX-rect.left,py=event.clientY-rect.top;
    const x=(px-originX)/tileW+(py-originY)/tileH,y=(py-originY)/tileH-(px-originX)/tileW;
    const previous=camera.zoom;
    const sensitivity=event.ctrlKey?.01:.002;
    camera.zoom=Math.max(.35,Math.min(2.5,previous*Math.exp(-dy*sensitivity)));
    camera.x=x-.5+(camera.x+.5-x)*previous/camera.zoom;
    camera.y=y-.5+(camera.y+.5-y)*previous/camera.zoom;
  }else{
    camera.x+=dx/tileW+dy/tileH;camera.y+=dy/tileH-dx/tileW;
  }
  const size=mapSize(world.towns[viewed]);
  camera.x=Math.max(0,Math.min(size-1,camera.x));camera.y=Math.max(0,Math.min(size-1,camera.y));
  buildTile=null;
},{passive:false});
window.addEventListener('resize',()=>draw());
function animate(now){
  draw(lastFrame===null?0:Math.min((now-lastFrame)/1000,.1));lastFrame=now;
  requestAnimationFrame(animate);
}
requestAnimationFrame(animate);
refresh().catch(()=>notice('Cannot reach the host. Check the connection.'));
setInterval(()=>{if(document.activeElement?.tagName!=='SELECT')refresh().catch(()=>notice('Connection lost. Waiting for the host…'));},2000);

setInterval(()=>refreshPlayers().catch(()=>{}),50);
