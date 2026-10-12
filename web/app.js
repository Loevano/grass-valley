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
const mapSize = town => town?.map_size || world.shared_map?.map_size || 120;
let chunkTerrainWorld=null, chunkTerrainCache=new Map(), chunkResourceCache=new Map(), requestedMapView=null, chunkHydratedMap=null;
function mapChunkKeys(landscape=world.shared_map){
  if(!landscape?.chunked)return [];
  const chunkSize=landscape.chunk_size,limit=landscape.map_size/chunkSize;
  const rect=canvas.getBoundingClientRect();
  const radius=Math.min(3,Math.max(1,Math.ceil((rect.width+rect.height)/Math.max(tileW,1)/chunkSize)));
  const cx=Math.max(0,Math.min(limit-1,Math.floor(camera.x/chunkSize)));
  const cy=Math.max(0,Math.min(limit-1,Math.floor(camera.y/chunkSize)));
  const keys=[];
  for(let x=Math.max(0,cx-radius);x<=Math.min(limit-1,cx+radius);x++)
    for(let y=Math.max(0,cy-radius);y<=Math.min(limit-1,cy+radius);y++)keys.push(`${x},${y}`);
  return keys;
}
function hydrateMapChunks(next){
  const landscape=next.shared_map;
  if(!landscape?.chunked)return;
  if(chunkTerrainWorld!==next.world_id){chunkTerrainCache.clear();chunkResourceCache.clear();chunkTerrainWorld=next.world_id;chunkHydratedMap=null;}
  for(const [key,rows] of Object.entries(landscape.terrain_chunks || {}))chunkTerrainCache.set(key,rows);
  for(const [key,update] of Object.entries(landscape.resource_chunks || {}))chunkResourceCache.set(key,update);
  // Keep explored chunks in the hydrated map until actual cache eviction.
  // A camera request changes what the host sends, not what is still loaded.
  for(const key of landscape.visible_chunks){
    for(const cache of [chunkTerrainCache,chunkResourceCache])if(cache.has(key)){
      const value=cache.get(key);cache.delete(key);cache.set(key,value);
    }
  }
  while(chunkTerrainCache.size>128)chunkTerrainCache.delete(chunkTerrainCache.keys().next().value);
  while(chunkResourceCache.size>128)chunkResourceCache.delete(chunkResourceCache.keys().next().value);
  // Stable ordering keeps selection indices and local worker layouts independent
  // of which side of a chunk boundary the camera happens to be on.
  const loadedKeys=[...new Set([...chunkTerrainCache.keys(),...chunkResourceCache.keys()])].sort();
  const keys=loadedKeys.join(';');
  const terrainChunks=loadedKeys.map(key=>chunkTerrainCache.get(key));
  const resourceChunks=loadedKeys.map(key=>chunkResourceCache.get(key));
  const sameView=chunkHydratedMap?.keys===keys;
  const sameTerrain=sameView&&terrainChunks.every((rows,i)=>rows===chunkHydratedMap.terrainChunks[i]);
  const sameResources=sameView&&resourceChunks.every((update,i)=>update===chunkHydratedMap.resourceChunks[i]);
  const terrain=sameTerrain?chunkHydratedMap.terrain:{};
  landscape.map_resources=sameResources?chunkHydratedMap.resources:[];
  landscape.tree_stumps=sameResources?chunkHydratedMap.stumps:[];
  for(const key of loadedKeys){
    const update=chunkResourceCache.get(key);
    if(update){
      if(!sameResources){
        landscape.map_resources.push(...update.map_resources);
        landscape.tree_stumps.push(...update.tree_stumps);
      }
    }
    const rows=chunkTerrainCache.get(key);
    if(!rows)continue;
    if(sameTerrain)continue;
    const [cx,cy]=key.split(',').map(Number),size=landscape.chunk_size;
    rows.forEach((row,dy)=>{
      const target=terrain[cy*size+dy] ||= {};
      for(let dx=0;dx<size;dx++)target[cx*size+dx]=row[dx];
    });
  }
  landscape.terrain=terrain;
  chunkHydratedMap={keys,terrainChunks,resourceChunks,terrain,resources:landscape.map_resources,stumps:landscape.tree_stumps};
  delete landscape.terrain_chunks;
  delete landscape.resource_chunks;
}
let frameActors=null,animationFrameRunning=false;
const workers = new Map();
let workerTown = null, lastFrame = null, animalClock = 0;
const walkKeys = new Set();
const defaultBindings={up:'w',left:'a',down:'s',right:'d',camera:'y',fish:'f',chop:'c',gather:'g',build:'b',interact:'i'};
const bindingLabels={up:'Walk up',left:'Walk left',down:'Walk down',right:'Walk right',camera:'Toggle camera lock',fish:'Fish',chop:'Cut wood',gather:'Gather berries and herbs',build:'Toggle building mode',interact:'Interact with nearby NPC'};
let keybindings={...defaultBindings}, cameraLocked=true, graphicsQuality='high';
try{
  const saved=JSON.parse(localStorage.getItem('lantern-settings') || 'null');
  if(saved?.keybindings){
    const oldActions=['up','left','down','right','camera'];
    if(oldActions.every(action=>/^[a-z0-9]$/.test(saved.keybindings[action]))
      && new Set(oldActions.map(action=>saved.keybindings[action])).size===oldActions.length){
      const used=new Set();
      for(const action of Object.keys(defaultBindings)){
        let key=saved.keybindings[action] || defaultBindings[action];
        if(!/^[a-z0]$/.test(key)||used.has(key))key=[defaultBindings[action],...'abcdefghijklmnopqrstuvwxyz0'].find(k=>!used.has(k));
        keybindings[action]=key;used.add(key);
      }
    }
  }
  if(typeof saved?.cameraLocked==='boolean')cameraLocked=saved.cameraLocked;
  if(['low','medium','high'].includes(saved?.graphicsQuality))graphicsQuality=saved.graphicsQuality;
}catch{}
function renderPixelRatio(){
  const dpr=window.devicePixelRatio || 1;
  return graphicsQuality==='low'?Math.min(dpr,.75):graphicsQuality==='medium'?Math.min(dpr,1):dpr;
}
function saveSettings(){
  try{localStorage.setItem('lantern-settings',JSON.stringify({keybindings,cameraLocked,graphicsQuality}));}
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
  $('graphics-quality').value=graphicsQuality;
  $('camera-lock-button').textContent=`Camera lock: ${cameraLocked?'On':'Off'}`;
  $('camera-lock-button').setAttribute('aria-pressed',String(cameraLocked));
  for(const action of Object.keys(defaultBindings))$('binding-'+action).value=keybindings[action].toUpperCase();
  $('world').setAttribute('aria-label',`Town grid. Use ${['up','left','down','right'].map(action=>keybindings[action].toUpperCase()).join(', ')} to walk. Press ${keybindings.camera.toUpperCase()} to toggle camera lock. Press ${keybindings.interact.toUpperCase()} to interact with a nearby NPC. Use arrow keys and Enter to inspect tiles.`);
  $('movement-hint').textContent=`Click map · ${['up','left','down','right'].map(action=>keybindings[action].toUpperCase()).join('/')} to walk · ${keybindings.camera.toUpperCase()} camera lock · ${keybindings.fish.toUpperCase()} fish · ${keybindings.chop.toUpperCase()} chop · ${keybindings.gather.toUpperCase()} gather · ${keybindings.build.toUpperCase()} building mode · ${keybindings.interact.toUpperCase()} interact`;
  $('movement-hint').textContent+=' · Space town hall';
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
      $('settings-message').textContent='Choose a letter or 0 without modifier keys.';return;
    }
    if(/^[1-9]$/.test(key)){
      $('settings-message').textContent='Keys 1–9 are reserved for windows. Choose a letter or 0.';return;
    }
    if(Object.keys(keybindings).some(other=>other!==action&&keybindings[other]===key)){
      $('settings-message').textContent='That key is already assigned. Choose another key.';return;
    }
    clearWalk();keybindings[action]=key;saveSettings();renderSettings();$('settings-message').textContent='Keybinding saved.';
  };
  $('keybindings').append(label,input);
}
$('graphics-quality').onchange=()=>{
  const quality=$('graphics-quality').value;
  if(!['low','medium','high'].includes(quality))return;
  graphicsQuality=quality;saveSettings();draw();
  $('settings-message').textContent='Graphics quality applied.';
};
$('camera-lock-button').onclick=()=>setCameraLock(!cameraLocked);
$('reset-keybindings').onclick=()=>{clearWalk();keybindings={...defaultBindings};saveSettings();renderSettings();$('settings-message').textContent='Default keybindings restored.';};
renderSettings();
let playerView = null, walkingRequest = false, walkCooldown = 0;
function clearWalk(){walkKeys.clear();walkCooldown=0;}
function canControlPlayer(allowBuilding=false){
  return (world.shared_map || viewed===identity?.town_id) && !!world.towns[identity?.town_id]?.player && (allowBuilding || !tool)
    && !world.quarry?.members?.[identity?.town_id]?.inside && !$('quarry-dialog')?.open
    && !world.lanterns?.members?.[identity?.town_id]?.inside && !$('lanterns-dialog')?.open
    && !$('world-menu').open && !$('delete-world-dialog').open && !$('upgrade-dialog').open
    && !$('settings-panel').open && !document.activeElement?.isContentEditable
    && !['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName)
    && (allowBuilding || document.activeElement?.tagName!=='BUTTON');
}
window.addEventListener('keyup',event=>walkKeys.delete(event.key.toLowerCase()));
window.addEventListener('blur',clearWalk);
document.addEventListener?.('visibilitychange',clearWalk);
// Buffer at most half a second of input while a host reply is delayed.
let movementMap=null;
const workerTools={
  fine_axe:{name:'Fine axe',job:'woodcutting',bonus:.25},
  fine_fishing_rod:{name:'Fine fishing rod',job:'fishing',bonus:.25},
  steel_fishing_rod:{name:'Steel fishing rod',job:'fishing',bonus:.5,price:30},
  masterwork_fishing_rod:{name:'Masterwork fishing rod',job:'fishing',bonus:.75,price:60},
  fine_sickle:{name:'Fine sickle',job:'farming',bonus:.25},
  steel_axe:{name:'Steel axe',job:'woodcutting',bonus:.5,price:30},
  masterwork_axe:{name:'Masterwork axe',job:'woodcutting',bonus:.75,price:60},
  steel_sickle:{name:'Steel sickle',job:'farming',bonus:.5,price:30},
  masterwork_sickle:{name:'Masterwork sickle',job:'farming',bonus:.75,price:60},
};
const localToolShops={Bram:['steel_axe','masterwork_axe'],Runa:['steel_sickle','masterwork_sickle'],Nell:['steel_fishing_rod','masterwork_fishing_rod'],Kestrel:['steel_axe','masterwork_axe']};
const localGifts={Sylva:'fine_axe',Reed:'fine_fishing_rod',Fern:'fine_sickle',Wren:'fine_fishing_rod',Alder:'fine_axe',Poppy:'fine_sickle',Dara:'desert_clothing',Petra:'swamp_boots'};
const localBarters={Tobin:[10,'vegetables',8],Clover:[10,'vegetables',8],Pip:[10,'bread',5],Hazel:[6,'berries',5],Mabel:[8,'eggs',6],Orin:[10,'vegetables',8],Willow:[8,'fish',6]};
function npcPosition(npc){return npc.routine || npc;}
function localVisitKey(npc){return `${npc.name}:${npc.x}:${npc.y}`;}
function localStoryProgress(town,name){
  const npc=(world.shared_map?.npcs || town.npcs || []).find(n=>n.name===name);
  return npc?town.local_visits?.[localVisitKey(npc)] || {}:{};
}
function storyObjectives(stage,town,progress){
  const goals=Object.entries(stage.costs || {}).map(([key,amount])=>`${amount} ${goodName(key).toLowerCase()} (${Math.floor(amountOf(town,key))} available)`);
  if(stage.roads)goals.push(`${Math.min(stage.roads,(town.roads || []).filter(r=>!r.construction).length)} / ${stage.roads} completed road tiles`);
  if(stage.rocks)goals.push(`${progress.story_rocks?.length || 0} / ${stage.rocks} outcrops examined`);
  return goals.join(' · ');
}
function renderLocalStory(town,npc,index,nearby,target){
  const story=world.local_stories?.[npc.name];
  if(!story)return;
  const progress=town.local_visits?.[localVisitKey(npc)] || {},stageIndex=progress.story_stage || 0;
  const stage=story.stages[stageIndex];
  if(!stage)return;
  const title=document.createElement('p');title.textContent=`${story.title} · ${stage.button?`Step ${stageIndex+1} / ${story.stages.length-1}`:'Completed'}`;target.append(title);
  const dialogue=document.createElement('p');dialogue.textContent='“'+stage.dialogue+'”';target.append(dialogue);
  const reward=document.createElement('p');reward.textContent=`${stage.button?'Reward':'Received'}: ${story.reward} coins · ${story.keepsake}`;target.append(reward);
  if(!stage.button)return;
  const objectives=document.createElement('p');objectives.textContent=storyObjectives(stage,town,progress) || 'Speak with this local to begin.';target.append(objectives);
  const button=document.createElement('button');button.textContent=stage.button;
  button.disabled=!nearby||!canAfford(town,stage.costs || {})
    ||(town.roads || []).filter(r=>!r.construction).length<(stage.roads || 0)
    ||(progress.story_rocks?.length || 0)<(stage.rocks || 0);
  button.onclick=()=>act({action:'advance_local_story',npc_index:index});target.append(button);
}
function renderWorkerTools(town,v,index,target){
  const owner=viewed===identity?.town_id;
  if(v.worker_tool&&workerTools[v.worker_tool]){
    const gear=workerTools[v.worker_tool],row=document.createElement('p');
    row.textContent=`${gear.name} · +${gear.bonus*100}% ${gear.job} output`;
    if(owner){const button=document.createElement('button');button.textContent='Take back tool';button.onclick=()=>act({action:'take_worker_tool',index});row.append(button);}
    target.append(row);
  }else if(owner)for(const [key,gear] of Object.entries(workerTools)){
    if(!(town.player?.worker_tools?.[key]>0))continue;
    const button=document.createElement('button');button.textContent=`Give ${gear.name.toLowerCase()} · +${gear.bonus*100}% ${gear.job} output`;
    button.onclick=()=>act({action:'give_worker_tool',item:key,index});target.append(button);
  }
}
const equipmentItems={
  swamp_boots:{name:'Swamp boots',slot:'feet',biome:'b',price:15,perk:'Walk on dark swamp ground · +20% herbs gathered'},
  desert_clothing:{name:'Desert clothing',slot:'body',price:20,perk:'+20% berries gathered'},
};
function clothingTraderItems(npc){
  if(npc.name==='Dara'||npc.village==='Sunwell')return ['desert_clothing'];
  if(npc.name==='Petra'||npc.village==='Reedhaven')return ['swamp_boots'];
  return [];
}
function groundVariant(x,y){return ((x*17+y*31+x*y*7)%7)%3;}
function biomeProtected(player,biome,x,y){return (biome==='b'&&groundVariant(x,y)!==2)||Object.entries(equipmentItems).every(([key,item])=>!item.biome||item.biome!==biome||player?.equipment?.[item.slot]===key);}
function mapRoads(town){return world.shared_map?Object.values(world.towns).flatMap(t=>t.roads || []):town?.roads || [];}
function playerBoatLevel(player){return player?.boat?player.boat_level || 1:0;}
function playerTravelSpeed(town,position){
  const level=playerBoatLevel(town.player);
  const prepared=typeof ClientScene!=='undefined'&&ClientScene.current?.world===world&&world.shared_map;
  if(prepared&&level&&ClientScene.current.water.has(`${Math.floor(position.x+.5)},${Math.floor(position.y+.5)}`))return 1+.25*(level-1);
  if(!prepared&&level&&(town.map_resources || []).some(r=>r.kind==='water'&&r.x===Math.floor(position.x+.5)&&r.y===Math.floor(position.y+.5)))return 1+.25*(level-1);
  return roadSpeed(town,position)*(town.player?.mounted?1.75:1);
}
function roadSpeed(town,position){
  if(typeof ClientScene!=='undefined'&&ClientScene.current?.world===world&&world.shared_map)return ClientScene.current.roads.has(`${Math.floor(position.x+.5)},${Math.floor(position.y+.5)}`)?1.5:1;
  return mapRoads(town).some(r=>!r.construction&&r.x===Math.floor(position.x+.5)&&r.y===Math.floor(position.y+.5))?1.5:1;}
// Measure screen distance for the 2:1 isometric projection, preserving sideways speed.
function playerMovementDistance(dx,dy){return Math.hypot(dx-dy,(dx+dy)*.5)/Math.SQRT2;}
function predictPlayerStep(town,player,dx,dy){
  if(movementMap?.world!==world || movementMap.town!==town || movementMap.boat!==!!town.player?.boat){
    const buildings=world.shared_map?Object.values(world.towns).flatMap(t=>t.buildings || []):town.buildings || [];
    const scene=typeof ClientScene!=='undefined'&&ClientScene.current?.world===world&&world.shared_map?ClientScene.current:null;
    movementMap=scene?{world,town,boat:!!town.player?.boat,water:scene.water,blocked:{has(key){
      if(scene.playerBuildings.has(key))return true;
      const [x,y]=key.split(',').map(Number),resource=ClientScene.resourceAt(x,y);
      return !!resource&&resource.kind!=='tree'&&resource.kind!=='berry_bush'&&!(resource.kind==='water'&&town.player?.boat);
    }}}:{world,town,boat:!!town.player?.boat,water:new Set((town.map_resources || []).filter(r=>r.kind==='water').map(r=>`${r.x},${r.y}`)),blocked:new Set([...buildings,...(town.map_resources || [])]
      .filter(b=>b.kind!=='tree'&&b.kind!=='berry_bush'&&!(b.kind==='water'&&town.player?.boat)).map(b=>`${b.x},${b.y}`))};
  }
  const size=town.map_size || 12,terrain=town.terrain;
  const open=(x,y)=>{
    if(x<-.32 || y<-.32 || x>size-.68 || y>size-.68)return false;
    for(let tx=Math.floor(x+.5)-1;tx<=Math.floor(x+.5)+1;tx++){
      for(let ty=Math.floor(y+.5)-1;ty<=Math.floor(y+.5)+1;ty++){
        const outside=tx>=0&&ty>=0&&tx<size&&ty<size&&(terrain?.[ty]?.[tx]==='v'||(town.chunked&&!terrain?.[ty]?.[tx]));
        const hazardous=!movementMap.water.has(`${tx},${ty}`)&&!biomeProtected(town.player,terrain?.[ty]?.[tx],tx,ty);
        if((movementMap.blocked.has(`${tx},${ty}`)||outside||hazardous)&&Math.abs(x-tx)<.68&&Math.abs(y-ty)<.68)return false;
      }
    }
    return true;
  };
  const predicted={...player},length=playerMovementDistance(dx,dy);
  if(length){
    for(let step=0;step<5;step++){
      const speed=playerTravelSpeed(town,predicted);
      const x=predicted.x+dx/length*.06*speed;if(open(x,predicted.y))predicted.x=x;
      const y=predicted.y+dy/length*.06*speed;if(open(predicted.x,y))predicted.y=y;
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
    playerView={...town.player,key,authoritative:{...town.player},target:{...town.player},steps:[],segments:[],clock:0,following:cameraLocked&&playerTown===identity?.town_id};clearWalk();
    if(playerView.following&&typeof camera!=='undefined'){camera.x=playerView.x;camera.y=playerView.y;}
  }
  const s=playerView;
  if(playerTown!==identity?.town_id)s.target={...town.player};
  if(!canControlPlayer())clearWalk();
  const horizontal=Number(walkKeys.has(keybindings.right))-Number(walkKeys.has(keybindings.left));
  const vertical=Number(walkKeys.has(keybindings.down))-Number(walkKeys.has(keybindings.up));
  const dx=Math.sign(horizontal+vertical),dy=Math.sign(vertical-horizontal);
  // Carry frame overshoot into the next step without accumulating delayed-request debt.
  walkCooldown=(dx||dy)?Math.max(-seconds,walkCooldown-seconds):0;
  if((dx||dy)&&s.steps.length<5&&walkCooldown<=1e-6){
    const step={dx,dy};walkCooldown+=.1;s.steps.push(step);
    s.target=predictPlayerStep(town,s.target,dx,dy);
    s.segments.push({step,target:s.target});
  }
  if(!walkingRequest&&s.steps.length&&playerTown===identity?.town_id){
    walkingRequest=true;
    const steps=s.steps.slice(),data={action:'walk',...steps[0],world_id:world.world_id};
    if(steps.length>1)data.steps=steps;
    const controller=new AbortController(),timeout=setTimeout(()=>controller.abort(),1000);
    request('/api/command',data,{signal:controller.signal}).then(result=>{
      if(playerView===s){
        const unrendered=new Set(s.segments.map(segment=>segment.step));s.segments=[];
        s.steps.splice(0,steps.length);s.authoritative={...result.player};s.target={...result.player};
        const current=world.towns[playerTown];if(current)current.player={...result.player};
        for(const step of s.steps){
          s.target=predictPlayerStep(current || town,s.target,step.dx,step.dy);
          if(unrendered.has(step))s.segments.push({step,target:s.target});
        }
      }
    }).catch(error=>{if(playerView===s){s.steps.length=0;s.segments=[];s.target={...s.authoritative};clearWalk();notice(error.message);}})
      .finally(()=>{clearTimeout(timeout);walkingRequest=false;});
  }
  const distance=playerMovementDistance(s.target.x-s.x,s.target.y-s.y);
  let remaining=seconds;
  // Follow each predicted step so buffering a turn cannot cut across obstacles.
  while(remaining>0){
    const target=s.segments[0]?.target || s.target;
    const length=playerMovementDistance(target.x-s.x,target.y-s.y);
    const duration=length/(3*playerTravelSpeed(town,s));
    const progress=duration?Math.min(1,remaining/duration):1;
    s.x+=(target.x-s.x)*progress;s.y+=(target.y-s.y)*progress;
    if(progress<1||!s.segments.length)break;
    remaining=Math.max(0,remaining-duration);s.segments.shift();
  }
  s.facing=s.target.facing;s.clock+=seconds;s.path=distance>.001?[s.target]:[];s.phase='idle';
  const activity=town.player.activity;
  s.source=activity;s.castTarget=activity;s.elapsed=s.clock%3;
  const working=!s.path.length&&!!activity&&!town.player.activity_status?.includes('full')&&!town.player.activity_status?.includes('Waiting');
  if(working&&!s.path.length)s.phase='working';
  if(activity)s.facing=town.player.facing;
  if(playerTown===identity?.town_id&&s.following&&(dx||dy||distance>.001)){
    if(typeof camera!=='undefined'){camera.x=s.x;camera.y=s.y;}
  }
  return {kind:'player',townId:playerTown,x:s.x,y:s.y,index:1,state:s,villager:{job:activity?.job || 'idle'},active:working,equipment:town.player.equipment,mounted:!!town.player.mounted,leadingHorse:!!town.player.leading_horse};
}
// Remote founders use small, frequent snapshots instead of the map refresh.
const remotePlayers=new Map();
let remoteWorld=null,remoteSample=-Infinity,refreshingPlayers=false;
function remotePlayerState(townId,player){
  if(remoteWorld!==world.world_id){remotePlayers.clear();remoteWorld=world.world_id;remoteSample=-Infinity;}
  let state=remotePlayers.get(townId);
  if(!state){
    state={...player,target:{...player},waypoints:[],buffer:0,lead:.2,lastMovement:null,clock:0,path:[],phase:'idle'};
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
      // Queue positions so turns survive polling jitter, and play at walking speed.
      // Keep two walking steps in reserve, extending the next lead for uneven
      // arrivals. Ignore idle gaps and cap the delay so stopping stays responsive.
      const gap=state.lastMovement===null?0:state.clock-state.lastMovement;
      if(gap>0&&gap<=.5)state.lead=Math.max(.2,Math.min(.35,gap+.05),state.lead*.95);
      state.lastMovement=state.clock;
      if(playerMovementDistance(player.x-state.x,player.y-state.y)>3){
        state.x=player.x;state.y=player.y;state.waypoints.length=0;state.buffer=0;
      }else{
        if(!state.waypoints.length)state.buffer=state.lead;
        state.waypoints.push({...player});
      }
      state.target={...player};
    }
    if(!state.waypoints.length)state.facing=player.facing;
    town.player={...player};
  }
  remoteSample=sampledAt;
}
async function refreshPlayers(){
  if(refreshingPlayers || !world.world_id || document.hidden)return;
  if(!world.shared_map || !Object.entries(world.towns).some(([id,town])=>id!==identity?.town_id&&town.player))return;
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
  state.clock+=seconds;
  const waiting=Math.min(state.buffer,seconds);state.buffer-=waiting;
  let remaining=seconds-waiting;
  while(state.waypoints.length&&remaining>0){
    const next=state.waypoints[0],distance=playerMovementDistance(next.x-state.x,next.y-state.y);
    const speed=3*playerTravelSpeed(town,state),duration=distance/speed;
    const progress=duration?Math.min(1,remaining/duration):1;
    state.x+=(next.x-state.x)*progress;state.y+=(next.y-state.y)*progress;
    state.facing=next.facing;
    if(progress<1)break;
    remaining=Math.max(0,remaining-duration);state.waypoints.shift();
  }
  state.path=state.waypoints.length?[state.waypoints[0]]:[];
  const activity=town.player.activity;state.source=activity;state.castTarget=activity;state.activityElapsed=state.clock%3;
  const active=!!activity&&!town.player.activity_status?.includes('full')&&!town.player.activity_status?.includes('Waiting');
  state.phase=active&&!state.path.length?'working':'idle';
  return {kind:'player',townId,x:state.x,y:state.y,index:1,villager:{job:activity?.job || 'idle'},active,state,equipment:town.player.equipment,mounted:!!town.player.mounted,leadingHorse:!!town.player.leading_horse};
}
const labels = {road:'Dirt road',hall:'Town hall',home:'Cottage',woodcutter:'Woodcutter',farm:'Farm',storehouse:'Storehouse',fishing_hut:'Fishing hut',acre:'Acre'};
function buildingLevel(building){return building.level || 1;}
function housingCapacity(town){
  return 5+2*town.buildings.filter(b=>b.kind==='home'&&!b.construction).reduce((sum,b)=>sum+buildingLevel(b)+(b.profession_upgrades?.includes('timber_frame')?1:0),0);
}
function fishingBoatLevel(building){
  const upgrades=building.profession_upgrades || [];
  return upgrades.includes('master_boat')?3:upgrades.includes('improved_boat')?2:upgrades.includes('boat')?1:0;
}
function fishingBoatCapacity(building){return [0,5,10,20][fishingBoatLevel(building)];}
function fishingBoatLoad(town,building){
  building=town.buildings.find(b=>b.kind==='fishing_hut'&&b.x===building.x&&b.y===building.y) || building;
  return (town.villagers || []).reduce((load,worker,index)=>load+(worker.job==='fishing'&&WorkerAnimation.fishingBoating(town,building,index)
    ?Object.entries(worker.pending_output || {}).reduce((sum,[key,amount])=>sum+(['fish','trout','salmon','sturgeon'].includes(key)?amount:0),0):0),0);
}

function resourceReach(building){
  if(building.kind==='mine')return 10;
  return building.kind==='woodcutter'?10:building.kind==='fishing_hut'?3+buildingLevel(building)-1+(fishingBoatLevel(building)?3+2*(fishingBoatLevel(building)-1):0):3;
}
function buildingVisualLevel(building,town){
  return building.kind==='hall'?Math.min(5,town?.level || 1):Math.min(3,buildingLevel(building));
}
function buildingHeight(building,town){
  return ({hall:42,woodcutter:23,mine:27,smelter:36,steelworks:38,stable:32,tannery:28,saddler:27,tradehouse:35,barn:34,mill:40,smokehouse:32,workshop:25,sawpit:16,market:17,fish_pond:18,orchard:20,bakery:30,butcher:25}[building.kind] || 27)+4*(buildingVisualLevel(building,town)-1);
}
function buildingStorageSize(building){return building.kind==='home'?0:building.kind==='hall'?200:building.kind==='storehouse'?[50,75,100,150,200][buildingLevel(building)-1]:50;}
function storageCapacity(town){return town.buildings.filter(b=>!b.construction).reduce((sum,b)=>sum+buildingStorageSize(b),0);}

const storageKeys=['wood','food','planks','wheat','fodder','flour','wool','tools','manure','herbs'];
function buildingName(building){return building.kind==='storehouse'&&buildingLevel(building)>=5?'Warehouse':labels[building.kind];}
const storageGoods={
  hall:[...storageKeys,'vegetables','carrots','fish','trout','salmon','sturgeon','berries','eggs','meat','milk','fruit','smoked_fish','bread','prepared_meat'],
  storehouse:[...storageKeys,'vegetables','carrots','fish','trout','salmon','sturgeon','berries','eggs','meat','milk','fruit','smoked_fish','bread','prepared_meat'],
  market:[...storageKeys,'vegetables','carrots','fish','trout','salmon','sturgeon','berries','eggs','meat','milk','fruit','smoked_fish','bread','prepared_meat'],
  home:[],woodcutter:['wood'],fishing_hut:['fish','trout','salmon','sturgeon'],farm:['vegetables','wheat','carrots','fodder','manure'],acre:['vegetables','wheat','carrots','fodder','manure'],
  barn:['fodder','vegetables','manure','eggs','wool','meat','milk'],orchard:['fruit'],
  bakery:['flour','wood','bread'],butcher:['meat','prepared_meat'],sawpit:['wood','planks'],mill:['wheat','flour'],
  smokehouse:['fish','trout','salmon','sturgeon','wood','smoked_fish'],workshop:['planks','wood','tools'],fish_pond:['fodder','fish']
};
function buildingStorageGoods(building){return (world.progression?.storage_goods || storageGoods)[building.kind] || [];}
function foodGoods(){return world.progression?.edible || {vegetables:1,carrots:1,fish:1,trout:1.5,salmon:2,sturgeon:3,berries:1,eggs:1,meat:1,milk:1,fruit:1,smoked_fish:1.5,bread:1.5,prepared_meat:1.5};}
function buildingStorageKeys(building){
  const edible=foodGoods(),groups=new Set(buildingStorageGoods(building).map(key=>key in edible?'food':key));
  return (world.progression?.storage_keys || storageKeys).filter(key=>groups.has(key));
}
function roomAllocations(building){
  if(building.kind==='storehouse'){
    const enabled=roomEnabled(building);
    return Object.fromEntries(buildingStorageKeys(building).map(key=>[key,enabled[key]?buildingStorageSize(building):0]));
  }
  const fixed=world.progression?.fixed_storage || {
    hall:{wood:100,food:100},home:{},woodcutter:{wood:50},fishing_hut:{food:50},
    farm:{food:20,wheat:10,fodder:10,manure:10},acre:{food:20,wheat:10,fodder:10,manure:10},
    barn:{food:20,fodder:15,wool:10,manure:5},orchard:{food:50},
    bakery:{flour:20,wood:5,food:25},butcher:{food:50},sawpit:{wood:30,planks:20},mill:{wheat:30,flour:20},
    smokehouse:{food:45,wood:5},workshop:{planks:25,wood:15,tools:10},fish_pond:{fodder:15,food:35},
    market:Object.fromEntries(storageKeys.map(key=>[key,5]))
  };
  const allocations=fixed[building.kind] || {};
  return Object.fromEntries(Object.entries(allocations).filter(([key])=>buildingStorageKeys(building).includes(key)));
}
function resourceStorage(town,key){
  const scene=typeof ClientScene!=='undefined'&&ClientScene.current?.world===world?ClientScene.current:null;
  let cached=scene?.capacities.get(town);
  if(scene&&!cached){cached=new Map();scene.capacities.set(town,cached);}
  if(cached?.has(key))return cached.get(key);
  const group=key in foodGoods()?'food':key;
  const capacity=town.buildings.filter(b=>!b.construction&&(key==='food'?buildingStorageKeys(b).includes('food'):buildingStorageGoods(b).includes(key))).reduce((sum,b)=>sum+(roomAllocations(b)[group] || 0),0);
  cached?.set(key,capacity);return capacity;
}
function localTradeFoodRoom(town,key='vegetables'){
  const value=foodGoods()[key] || 1;
  if(town.inventory_version!==1)return Math.max(0,resourceStorage(town,key)-amountOf(town,'food'))/value;
  const edible=foodGoods();
  return town.buildings.filter(b=>!b.construction&&buildingStorageGoods(b).includes(key)).reduce((sum,b)=>{
    const stock={...b.stored_goods};
    for(const [key,n] of Object.entries(b.output_buffer || {}))stock[key]=(stock[key] || 0)+n;
    const used=Object.entries(stock).reduce((n,[key,count])=>n+(key==='food'||key in edible?count*(edible[key] || 1):0),0);
    const room=Math.max(0,(roomAllocations(b).food || 0)-used);
    return sum+(b.kind==='storehouse'?room/value:Math.min(room/value,Math.max(0,buildingStorageSize(b)-Object.values(stock).reduce((n,count)=>n+count,0))));
  },0);
}
function roomEnabled(building){
  return Object.fromEntries(buildingStorageKeys(building).map(key=>[key,building.storage_enabled?!!building.storage_enabled[key]:((building.storage_allocations?.[key] || 0)>0)]));
}
function renderStorage(town,item,index,target=$('selection-details')){
  const keys=buildingStorageKeys(item),capacity=buildingStorageSize(item),allocations=roomAllocations(item);
  const owner=viewed===identity?.town_id,canConfigure=owner&&item.kind==='storehouse';
  const enabled=roomEnabled(item),edible=foodGoods(),stock={},stored={...item.stored_goods};
  for(const [key,n] of Object.entries(item.output_buffer || {}))stored[key]=(stored[key] || 0)+n;
  for(const [key,n] of Object.entries(stored)){
    const category=key in edible?'food':key;
    stock[category]=(stock[category] || 0)+n*(edible[key] || 1);
  }
  const summary=document.createElement('p');summary.className='storage-summary';
  summary.textContent=item.kind==='storehouse'?`${capacity} capacity per enabled resource`:`${Math.floor(Object.values(stock).reduce((sum,n)=>sum+n,0))} / ${capacity} stored`;
  target.append(summary);
  if(item.kind==='storehouse'){
    const note=document.createElement('p');note.className='storage-note';
    note.textContent='Enable resources for incoming deliveries. Disabled goods remain available to use. Food shares one capacity for edible goods.';target.append(note);
  }
  for(const key of keys){
    const row=document.createElement('div');row.className='storage-row';
    const caption=document.createElement('label');caption.className='storage-caption';
    const name=document.createElement('span');name.textContent=goodName(key);caption.append(name);
    if(canConfigure){
      const toggle=document.createElement('input');toggle.type='checkbox';toggle.checked=enabled[key];
      toggle.setAttribute('aria-label',`Accept ${goodName(key)}`);
      toggle.onchange=()=>{
        if(viewed!==identity?.town_id)return;
        enabled[key]=toggle.checked;
        act({action:'configure_storage',index,enabled:{...enabled}});
      };
      caption.append(toggle);
    }
    const amount=stock[key] || 0,limit=allocations[key] || 0;
    const count=document.createElement('span');count.className='storage-count';
    count.textContent=item.kind==='storehouse'&&!enabled[key]?`${Math.floor(amount)} · Disabled`:`${Math.floor(amount)} / ${limit}`;caption.append(count);row.append(caption);
    const meter=document.createElement('meter');meter.min=0;meter.max=Math.max(1,limit,amount);meter.value=amount;
    meter.setAttribute('aria-label',`${goodName(key)} stored`);meter.setAttribute('aria-valuetext',`${Math.floor(amount)} stored, ${limit} capacity`);row.append(meter);target.append(row);
  }
  for(const [key,n] of Object.entries(stored))if(n>0&&!buildingStorageGoods(item).includes(key))target.append(fact(String(Math.floor(n)),`${goodName(key)} from previous storage · available to use or haul away`));
  for(const [key,n] of Object.entries(item.input_buffer || {}))if(n>0)target.append(fact(String(Math.floor(n)),`${goodName(key)} reserved for production`));
}

function upgradeBenefit(kind,level,item){
  if(kind==='home'&&item?.profession_upgrades?.includes('timber_frame'))level++;
  if(kind==='fishing_hut')return `+${25*(level-1)}% food / catch / worker\n${resourceReach({...item,kind,level})} tile reach`;
  return kind==='barn'?`${4*level} animal spaces`:['sawpit','mill','smokehouse','workshop','fish_pond','orchard','market','bakery','butcher'].includes(kind)?'Separate worker capacity':kind==='home'?`${2*level} spaces\n-${25*(level-1)}% departure`:kind==='storehouse'?`${buildingStorageSize({kind,level})} capacity per resource${level>=5?'\nWarehouse':''}`:`+${25*(level-1)}% ${kind==='woodcutter'?'wood':kind==='fishing_hut'?'food / catch':kind==='acre'?'field food':'food'} / worker`;
}
let pendingUpgrade=null;
const professionBenefits={
  axe:'+25% wood / worker',
  nets:'+25% food / catch',
  wool_nets:'+25% food / catch\nStacks with nets',
  tools:'+25% wood / worker\nStacks with axe',
  boat:'1 fisherman aboard · Other fishers use the dock\nHolds 5 fish\n+3 tile reach\nDepth 2 water · Trout (1.5 food / 2 coins)',
  improved_boat:'2 fishermen aboard\nHolds 10 fish\n+2 additional tile reach\nDepth 3 water · Salmon (2 food / 3 coins)\n+25% fish / catch',
  master_boat:'3 fishermen aboard\nHolds 20 fish\n+2 additional tile reach\nDepth 4 water · Sturgeon (3 food / 5 coins)\n+25% fish / catch (total boat bonus: +50%)',
  saw:'+25% plank processing speed',
  steel_pickaxe:'+25% iron ore per mining batch',
  timber_frame:'+2 spaces'
};
function upgradePreview(command,town,item){
  if(command.action==='upgrade'){
    const spec=world.hall_upgrades?.find(next=>next.level===town.level+1);
    if(!spec)return null;
    const benefits=[`+${spec.coins} coins`, `Build range: ${buildingRange(town)} → ${buildingRange(town)+5} tile steps.`];
    const catalog=world.progression;
    for(const [category,entries] of Object.entries({Buildings:catalog?.buildings,Crops:catalog?.crops,Animals:catalog?.animals,Equipment:catalog?.upgrades})){
      const names=Object.entries(entries || {}).filter(([,value])=>(typeof value==='number'?value:value.level)===spec.level).map(([key,value])=>value.label || labels[key] || goodName(key));
      if(names.length)benefits.push(`${category}: ${names.join(', ')}`);
    }
    return {title:`Town hall · Level ${spec.level}`,benefits,cost:Object.fromEntries(['wood','planks','leather','iron_ore','iron','steel'].filter(k=>k in spec).map(k=>[k,spec[k]])),seconds:spec.seconds,requirements:spec.milestone,ready:!!world.hall_ready?.[viewed]&&!town.upgrade_remaining};
  }
  if(command.action==='profession_upgrade'){
    const spec=world.progression?.upgrades?.[command.upgrade];
    if(!spec)return null;
    return {title:spec.label,benefits:[professionBenefits[command.upgrade] || spec.label],cost:materialCosts(spec),seconds:command.upgrade==='boat'?0:30,requirements:`Hall level ${spec.level}${spec.xp?` · ${spec.xp} ${goodName(spec.skill).toLowerCase()} practice`:''}${spec.requires?` · ${world.progression.upgrades[spec.requires]?.label || goodName(spec.requires)}`:''}`,ready:town.level>=spec.level&&professionSkill(town,spec.skill)>=(spec.xp || 0)&&(!spec.requires||item.profession_upgrades?.includes(spec.requires))&&!item.profession_upgrades?.includes(command.upgrade)};
  }
  const capacity=command.action==='upgrade_capacity',level=capacity?item.settler_level || 1:buildingLevel(item);
  const currentBenefits=upgradeBenefit(item.kind,level,item).split('\n'),nextBenefits=upgradeBenefit(item.kind,level+1,item).split('\n');
  return {title:`${buildingName(item)} · ${capacity?'Capacity tier':'Level'} ${level+1}`,benefits:capacity?[`${settlerSpaces[item.kind]*level} → ${settlerSpaces[item.kind]*(level+1)} spaces`]:nextBenefits.map((benefit,i)=>currentBenefits[i]&&currentBenefits[i]!==benefit?`${currentBenefits[i]} → ${benefit}`:benefit),cost:{wood:capacity?20*level:costs[item.kind]*level},seconds:30*level,requirements:'',ready:level<(capacity?3:item.kind==='storehouse'?5:3)};
}
function upgradeCard(button,preview,{title=preview?.title,status='',benefits=preview?.benefits || []}={}){
  const card=document.createElement('div');card.className='upgrade-card';
  const heading=document.createElement('strong');heading.className='upgrade-card-title';heading.textContent=title;card.append(heading);
  for(const benefit of benefits){const row=document.createElement('p');row.className='upgrade-card-benefit';row.textContent=benefit;card.append(row);}
  if(status){const row=document.createElement('p');row.className='upgrade-card-status';row.textContent=status;card.append(row);}
  else if(preview){
    const facts=document.createElement('dl');facts.className='upgrade-card-facts';
    for(const [label,value] of [['Cost',costText(preview.cost)],['Build time',`${preview.seconds} seconds`],['Requires',preview.requirements]]){
      if(!value)continue;
      const term=document.createElement('dt');term.textContent=label;
      const detail=document.createElement('dd');detail.textContent=value;facts.append(term);facts.append(detail);
    }
    card.append(facts);
  }
  card.append(button);return card;
}
function renderUpgradeDialog(){
  if(!pendingUpgrade)return;
  const {command,worldId,townId,signature}=pendingUpgrade,town=world.towns[townId];
  const item=town?.buildings[command.index];
  const current=item&&JSON.stringify([item.kind,item.x,item.y,buildingLevel(item),item.settler_level || 1,town.level,item.profession_upgrades || []]);
  const valid=world.world_id===worldId&&viewed===townId&&identity?.town_id===townId&&current===signature;
  const preview=valid?upgradePreview(command,town,item):null;
  if(!preview){$('confirm-upgrade').disabled=true;$('upgrade-error').hidden=false;$('upgrade-error').textContent='This upgrade has changed. Close this popup and select it again.';return;}
  const capacity=command.action==='upgrade_capacity',equipment=command.action==='profession_upgrade';
  const level=command.action==='upgrade'?town.level:capacity?item.settler_level || 1:buildingLevel(item);
  $('upgrade-title').textContent=equipment?preview.title:`Upgrade ${buildingName(item)}`;
  $('upgrade-description').textContent=equipment?buildingName(item):`${capacity?'Capacity tier':'Level'} ${level} → ${level+1}`;
  $('upgrade-location').textContent=`(${item.x}, ${item.y})`;
  $('upgrade-benefits').replaceChildren();
  for(const benefit of preview.benefits){
    const row=document.createElement('li');
    const metric=benefit.match(/^([+-]?\d+%?) (.+) → ([+-]?\d+%?) \2$/);
    const spaces=benefit.match(/^(\d+) → (\d+) spaces$/);
    const colon=benefit.indexOf(': ');
    let label='',value=benefit,gain='';
    if(metric||spaces){
      const before=(metric||spaces)[1],after=metric?metric[3]:spaces[2],unit=metric?metric[2]:'spaces';
      label=({'capacity per resource':'Capacity per resource',spaces:'Housing capacity','animal spaces':'Animal capacity','departure':'Departure chance'})[unit] || unit;
      value=`${before} → ${after}${unit.includes('spaces')?' spaces':''}`;
      if(unit.includes('spaces')||unit==='capacity per resource')gain=`+${Number(after)-Number(before)}`;
    }else if(colon>=0){label=benefit.slice(0,colon);value=benefit.slice(colon+2);}
    if(label){const name=document.createElement('span');name.className='upgrade-benefit-label';name.textContent=label;row.append(name);}
    const detail=document.createElement('strong');detail.textContent=value;row.append(detail);
    if(gain){const change=document.createElement('span');change.className='upgrade-benefit-gain';change.textContent=gain;row.append(change);}
    $('upgrade-benefits').append(row);
  }
  $('upgrade-cost').textContent=costText(preview.cost);
  $('upgrade-time').textContent=preview.seconds?`${preview.seconds} seconds of work, plus travel and waiting`:'0 seconds';
  $('upgrade-requirements').textContent=preview.requirements;
  $('upgrade-requirements').hidden=$('upgrade-requirements-label').hidden=!preview.requirements;
  $('confirm-upgrade').textContent=`${command.upgrade==='boat'?'Buy boat':'Upgrade'} for ${costText(preview.cost)}`;
  const error=item.construction?'Finish the current construction first.':!preview.ready?'Upgrade requirements are not met.':!canAfford(town,preview.cost)?'Gather more resources to afford this upgrade.':'';
  $('upgrade-error').textContent=error;$('upgrade-error').hidden=!error;$('confirm-upgrade').disabled=!!error;
}
function showUpgrade(command){
  const town=world.towns[viewed];
  if(!town||viewed!==identity?.town_id)return;
  if(command.action==='upgrade')command={...command,index:town.buildings.findIndex(b=>b.kind==='hall')};
  const item=town.buildings[command.index];if(!item)return;
  pendingUpgrade={command,worldId:world.world_id,townId:viewed,signature:JSON.stringify([item.kind,item.x,item.y,buildingLevel(item),item.settler_level || 1,town.level,item.profession_upgrades || []])};
  clearWalk();renderUpgradeDialog();$('upgrade-dialog').showModal();
}
$('cancel-upgrade').onclick=()=>$('upgrade-dialog').close();
$('upgrade-dialog').addEventListener('close',()=>{pendingUpgrade=null;});
$('confirm-upgrade').onclick=()=>{
  renderUpgradeDialog();
  if(!pendingUpgrade||$('confirm-upgrade').disabled)return;
  const {command}=pendingUpgrade;pendingUpgrade=null;$('upgrade-dialog').close();
  const {index,...hallCommand}=command;
  act(command.action==='upgrade'?hallCommand:command);
};
const costs = {road:0,home:20,woodcutter:15,farm:15,storehouse:25,fishing_hut:15,acre:5};
const colors = {hall:'#a88861',home:'#c5ad80',woodcutter:'#947b55',farm:'#bc9b5c',storehouse:'#8c9b8b',fishing_hut:'#7899a2'};
// Small vector illustrations share the map's earth, sage, and amber palette.
Object.assign(labels,{barn:'Barn',sawpit:'Sawpit',mill:'Mill',smokehouse:'Smokehouse',workshop:'Workshop',fish_pond:'Fish pond',orchard:'Orchard',market:'Market stall',bakery:'Bakery',butcher:'Butcher shop'});
Object.assign(costs,{barn:25,sawpit:25,mill:30,smokehouse:25,workshop:30,fish_pond:25,orchard:25,market:20,bakery:30,butcher:25});
Object.assign(colors,{barn:'#b9866b',sawpit:'#aa8b60',mill:'#c1b080',smokehouse:'#8c8173',workshop:'#a28c72',fish_pond:'#7899a2',orchard:'#83a36b',market:'#c5ad80',bakery:'#c1a078',butcher:'#b9866b'});
Object.assign(labels,{tannery:'Tannery',mine:'Mine',smelter:'Iron smelter',stable:'Stable',saddler:'Saddler',steelworks:'Steelworks',tradehouse:'Trade house'});
Object.assign(costs,{tannery:30,mine:40,smelter:40,stable:40,saddler:30,steelworks:40,tradehouse:50});
Object.assign(colors,{tannery:'#ad8a67',mine:'#7f8c8c',smelter:'#a08875',stable:'#bda077',saddler:'#b89268',steelworks:'#758792',tradehouse:'#a1ab7e'});
const professionJobs={trading:'Merchant',mining:'Mining',smelting:'Smelting',tanning:'Leatherworking',horsekeeping:'Horsekeeping',saddlery:'Saddlery',steelmaking:'Steelmaking',packing:'Trade packing',husbandry:'Husbandry',woodworking:'Woodworking',milling:'Milling',preserving:'Preserving',crafting:'Crafting',breeding:'Fish breeding',orcharding:'Orcharding',baking:'Baking',butchery:'Butchery'};
const goodName=key=>key.replaceAll('_',' ').replace(/^./,c=>c.toUpperCase());
function amountOf(town,key){return ['wood','food','coins'].includes(key)?town[key] || 0:town.goods?.[key] || 0;}
function renderResourceList(town){
  $('resources-list').replaceChildren();
  if(!town)return;
  const edible=foodGoods();
  const keys=new Set([...(world.progression?.storage_keys || storageKeys),'coins',...Object.keys(edible),...Object.keys(town.goods || {})]);
  const groups=[['Overview',['food','coins']],['Materials',['wood','planks','wool','tools']],
    ['Farming supplies',['wheat','fodder','flour','manure','herbs']],['Food types',Object.keys(edible)]];
  groups.push(['Other resources',[...keys].filter(key=>!groups.some(([,members])=>members.includes(key)))]);
  for(const [title,members] of groups){
    const remaining=members.filter(key=>keys.has(key));
    if(!remaining.length)continue;
    const group=document.createElement('div');group.className='resource-group';
    const heading=document.createElement('h3');heading.textContent=title;group.append(heading);
    for(const key of remaining){
      keys.delete(key);
      const amount=String(Math.floor(amountOf(town,key)));
      group.append(fact(key==='coins'?amount:`${amount} / ${Math.floor(resourceStorage(town,key))}`,goodName(key)));
    }
    $('resources-list').append(group);
  }
}
function renderRules(town){
  const target=$('rules-list');
  // Keep unfinished edits intact while snapshots refresh.
  const signature=JSON.stringify([identity?.town_id,town?.rules?.production_caps,world.progression]);
  if(renderRules.signature===signature)return;
  if(target.contains?.(document.activeElement))return;
  renderRules.signature=signature;target.replaceChildren();
  if(!town)return;
  const catalog=world.progression || {};
  const keys=[...new Set(['wood',...Object.keys(catalog.crops || {}),...Object.keys(catalog.edible || {}),
    ...Object.values(catalog.recipes || {}).map(recipe=>recipe[1]),
    ...Object.values(catalog.profession_recipes || {}).flat().map(recipe=>recipe.output),'wool','manure','herbs','hides'])];
  for(const key of keys){
    const row=document.createElement('form');row.className='production-rule';
    const label=document.createElement('label');label.textContent=goodName(key);label.setAttribute('for','cap-'+key);
    const input=document.createElement('input');input.id='cap-'+key;input.type='number';input.min='0';input.step='any';
    input.placeholder='Unlimited';input.value=town.rules?.production_caps?.[key] ?? '';
    input.setAttribute('aria-label',`${goodName(key)} production cap`);
    const save=document.createElement('button');save.textContent='Save';
    row.onsubmit=event=>{event.preventDefault();
      if(!row.reportValidity())return;
      const cap=input.value.trim()===''?null:Number(input.value);
      act({action:'configure_rule',resource:key,cap});
    };
    row.append(label,input,save);target.append(row);
  }
}
function materialCosts(spec){return Object.fromEntries(Object.entries(spec).filter(([k])=>['wood','coins','planks','wool','tools','leather','iron_ore','iron','saddles','steel','trade_crates'].includes(k)));}
function canAfford(town,cost){return Object.entries(cost).every(([k,v])=>amountOf(town,k)>=v);}
function costText(cost){return Object.entries(cost).map(([k,v])=>`${v} ${goodName(k).toLowerCase()}`).join(' + ');}
function itemCard(key,name,stats,quantity){
  const card=document.createElement('div');card.className='item-card';
  const header=document.createElement('div');header.className='item-heading';header.append(illustration(key));
  const details=document.createElement('div');
  const title=document.createElement('strong');title.textContent=name;details.append(title);
  const effect=document.createElement('small');effect.className='item-stats';effect.textContent=stats;details.append(effect);
  const count=document.createElement('small');count.className='item-quantity';count.textContent=quantity;details.append(count);
  header.append(details);card.append(header);return card;
}
function toolRecipientPicker(town,key,carrierIndex=null){
  const gear=workerTools[key];
  const eligible=town.villagers.map((v,index)=>({v,index})).filter(({v,index})=>!v.worker_tool||index===carrierIndex)
    .sort((a,b)=>Number(b.v.job===gear.job)-Number(a.v.job===gear.job));
  const select=document.createElement('select');select.setAttribute('aria-label',`Villager to receive ${gear.name.toLowerCase()}`);
  const nobody=document.createElement('option');nobody.value='';nobody.textContent='Nobody';select.append(nobody);
  for(const {v,index} of eligible){
    const option=document.createElement('option');option.value=String(index);option.textContent=`${v.name} · ${goodName(v.job)}${v.job===gear.job?' · Matches current job':''}`;select.append(option);
  }
  select.value=carrierIndex===null?'':String(carrierIndex);
  select.disabled=!eligible.length;
  select.onchange=()=>{
    if(select.disabled||select.value===(carrierIndex===null?'':String(carrierIndex)))return;
    if(select.value==='')act({action:'take_worker_tool',index:carrierIndex});
    else if(carrierIndex===null)act({action:'give_worker_tool',item:key,index:Number(select.value)});
    else act({action:'transfer_worker_tool',item:key,index:carrierIndex,target_index:Number(select.value)});
  };
  return select;
}
function renderItems(town){
  const target=$('items-list');target.replaceChildren();
  if(!town)return;
  const tools=document.createElement('div');tools.className='resource-group';
  const heading=document.createElement('h3');heading.textContent='Villager tools';tools.append(heading);
  const hint=document.createElement('p');hint.className='muted';hint.textContent='Select a villager to assign. Nobody returns the tool. Matching workers appear first.';tools.append(hint);
  for(const [key,gear] of Object.entries(workerTools)){
    const available=town.player?.worker_tools?.[key] || 0;
    const assigned=town.villagers.map((v,index)=>({v,index})).filter(({v})=>v.worker_tool===key);
    if(available<1&&!assigned.length)continue;
    const row=itemCard(key,gear.name,`+${gear.bonus*100}% ${gear.job} output`,`${available} available · ${assigned.length} assigned`);
    for(const {v,index} of assigned){
      const recipient=toolRecipientPicker(town,key,index);recipient.setAttribute('aria-label',`Assign ${gear.name.toLowerCase()} carried by ${v.name}`);row.append(recipient);
    }
    if(available>0)row.append(toolRecipientPicker(town,key));
    tools.append(row);
  }
  if(tools.children.length>2)target.append(tools);
  const clothing=document.createElement('div');clothing.className='resource-group';
  const title=document.createElement('h3');title.textContent='Your equipment';clothing.append(title);
  for(const [key,item] of Object.entries(equipmentItems)){
    const count=town.player?.items?.[key] || 0,equipped=town.player?.equipment?.[item.slot]===key;
    if(count<1)continue;
    const stats=`${item.slot==='feet'?'Feet':'Body'} · ${item.perk}`;
    const row=itemCard(key,item.name,stats,`${count} owned${equipped?' · Equipped':''}`);
    const button=document.createElement('button');button.textContent=equipped?`Unequip ${item.name.toLowerCase()}`:`Equip ${item.name.toLowerCase()}`;button.onclick=()=>act({action:equipped?'unequip_item':'equip_item',item:key});row.append(button);clothing.append(row);
  }
  if(clothing.children.length>1)target.append(clothing);
  if(!target.children.length){const empty=document.createElement('p');empty.className='muted';empty.textContent='No items yet. Collect tools from Sylva, Reed and Fern, or buy stronger tools from Bram and Runa, or find equipment in treasure and local trades.';target.append(empty);}
}
function professionSkill(town,job){return Math.max(town.player?.[job] || 0,...town.villagers.map(v=>v[job] || 0));}
function jobRequirement(v,job){
  return Object.entries(world.progression?.job_requirements?.[job] || {})
    .filter(([skill,amount])=>(v[skill] || 0)<amount)
    .map(([skill,amount])=>`${amount} ${goodName(skill).toLowerCase()} practice (has ${Math.floor(v[skill] || 0)})`).join(', ');
}
function workerEfficiency(v,job){
  const root=world.progression?.talent_roots?.[job] || job;
  return (1+Math.floor((v[job] || 0)/20))*(1+.25*(v.talents?.[root] || 0));
}
async function playerAction(action){
  try{
    const result=await request('/api/command',{action,world_id:world.world_id});
    if(result?.player?.activity_status?.startsWith('Found '))notice(result.player.activity_status);
    await refresh();
  }catch(e){notice(e.message);}
}
const iconPaths = {
  fine_axe:'<path fill="#94704c" d="m6 28 15-24 3 2-15 24z"/><path fill="#bacac5" d="m18 5 7-3 5 9-8 4z"/>',
  steel_axe:'<path fill="#94704c" d="m6 28 15-24 3 2-15 24z"/><path fill="#dce8ed" d="m18 5 7-3 5 9-8 4z"/>',
  masterwork_axe:'<path fill="#94704c" d="m6 28 15-24 3 2-15 24z"/><path fill="#d9bd70" d="m18 5 7-3 5 9-8 4z"/>',
  fine_sickle:'<path stroke="#94704c" stroke-width="4" d="M11 29V17"/><path fill="#bacac5" d="M11 18Q2 8 12 3q13-4 17 12Q19 4 11 9q-5 4 0 9z"/>',
  steel_sickle:'<path stroke="#94704c" stroke-width="4" d="M11 29V17"/><path fill="#dce8ed" d="M11 18Q2 8 12 3q13-4 17 12Q19 4 11 9q-5 4 0 9z"/>',
  masterwork_sickle:'<path stroke="#94704c" stroke-width="4" d="M11 29V17"/><path fill="#d9bd70" d="M11 18Q2 8 12 3q13-4 17 12Q19 4 11 9q-5 4 0 9z"/>',
  steel_fishing_rod:'<path stroke="#bacac5" stroke-width="3" d="M5 29 18 4"/><path d="m18 4 9 3v15q0 6-5 3v-3"/>',
  masterwork_fishing_rod:'<path stroke="#d9bd70" stroke-width="3" d="M5 29 18 4"/><path d="m18 4 9 3v15q0 6-5 3v-3"/>',
  fine_fishing_rod:'<path stroke="#94704c" stroke-width="3" d="M5 29 18 4"/><path d="m18 4 9 3v15q0 6-5 3v-3"/>',
  swamp_boots:'<path fill="#94704c" d="M4 4h9v16l4 3v5H3v-8zm14 0h9v16l4 3v5H17v-8z"/><path d="M4 10h9m5 0h9M3 25h14m0 0h14"/>',
  desert_clothing:'<path fill="#d6c478" d="m10 4-8 7 4 7 5-3-2 14h14l-2-14 5 3 4-7-8-7-6 3z"/><path d="m10 4 6 9 6-9M10 22h12m-6-9v16"/>',
  fishing:'<path fill="#7899a2" d="M7 17q7-11 17 0-10 11-17 0z"/><path fill="#cda875" d="m7 17-5-5v10z"/><circle fill="#e8eadb" cx="20" cy="16" r="1.5"/><path d="m12 6 14-3v8q0 4-3 3"/>',
  woodcutting:'<path fill="#94704c" d="m7 27 12-22 3 2-12 22z"/><path fill="#bacac5" d="m18 6 7-3 5 9-8 3z"/>',
  gathering:'<path fill="#719451" d="M16 27V11Q4 2 4 13q0 8 12 8Q28 18 28 8q-12 0-12 9"/><circle fill="#b9866b" cx="9" cy="20" r="4"/><circle fill="#d9bd70" cx="16" cy="23" r="4"/>',
  farming:'<path d="M16 29V4m0 9-7-5m7 12 7-5"/><path fill="#d9bd70" d="m16 4-5-3-2 5 7 7 7-7-2-5zm-7 4-6 1 2 6 11 5zm14 7 6 1-2 6-11 7z"/>',
  husbandry:'<ellipse fill="#e8eadb" cx="14" cy="16" rx="10" ry="8"/><path fill="#94704c" d="M21 10h8v10h-8z"/><path d="M8 23v6m10-6v6m6-19 3-5"/>',
  woodworking:'<path fill="#cda875" d="M3 18h26v10H3z"/><path fill="#bacac5" d="m6 15 17-12 5 6-18 9z"/><path d="m7 22 18 0m-10-9 2 3m3-7 2 3"/>',
  milling:'<path fill="#cda875" d="m10 13 12 0 4 17H6z"/><path fill="#e8eadb" d="M16 12 3 3h10zm3 3L29 3v10zm-3 4 13 10H19zm-3-3L3 29V19z"/>',
  preserving:'<path fill="#b9866b" d="M5 18q8-10 18 0-10 10-18 0z"/><path d="m23 18 6-5v10zM9 11q-4-4 0-8m7 6q-4-4 0-8m7 10q-4-4 0-8"/>',
  crafting:'<path fill="#94704c" d="m7 28 15-23 3 2-15 23z"/><path fill="#bacac5" d="m14 5 5-4 12 8-5 6z"/>',
  breeding:'<path fill="#6ba2af" d="M2 9q7-4 14 0t14 0v18H2z"/><path fill="#cda875" d="M7 19q6-8 14 0-8 8-14 0z"/><path d="m21 19 6-4v8z"/>',
  orcharding:'<path fill="#719451" d="M16 10q-7-10-11-3 3 8 11 3z"/><path d="m16 11 3-8"/><path fill="#b9866b" d="M16 12C1 5 1 27 12 29h8c11-2 11-24-4-17z"/>',
  baking:'<path fill="#d9bd70" d="M3 24v-9C3 2 29 2 29 15v9q-13 9-26 0z"/><path d="m10 11-3 7m10-8-3 8m10-7-3 7"/>',
  butchery:'<path fill="#94704c" d="m6 29 8-14 3 2-8 14z"/><path fill="#bacac5" d="m12 13 8-11 10 7-9 13z"/><circle fill="#e8eadb" cx="23" cy="9" r="2"/>',
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
Object.assign(iconPaths,{
  mine:'<path fill="#89928a" d="m3 28 4-20 10-5 12 14-3 11z"/><path fill="#473f32" d="M10 15h12v13H10z"/><path d="M8 13h16M8 13v15m16-15v15"/>',
  smelter:'<path fill="#a08875" d="M6 12h18v17H6zM21 3h5v13h-5z"/><path fill="#443b32" d="M10 18h10v11H10z"/><path fill="#f2ad52" d="m12 28 3-9 3 9z"/>',
  steelworks:'<path fill="#758792" d="M3 12h26v7H3l8 5h12l6-5M12 24h10v6H12z"/>',
  tannery:'<path fill="#bc8a57" d="m6 6 6 3 4-4 4 4 6-3-3 9 3 11-8-2-2 4-2-4-8 2 3-11z"/>',
  saddler:'<path fill="#bc8a57" d="M5 12q7 5 11-5 5 10 11 5l-2 10H7z"/><path d="M11 21v6m10-6v6"/>',
  stable:'<path fill="#ac7a50" d="M4 14h17V5h6v15H9v8H5zm14 5h4v9h-4z"/><path d="M22 6V3m3 3V3M4 16H2v9"/>',
  tradehouse:'<path fill="#a1ab7e" d="M4 9h24v7H4z"/><path d="M6 16v13m20-13v13"/><path fill="#c9ac75" d="M8 19h16v10H8z"/><path d="m8 19 16 10m0-10L8 29"/>',
});
function illustration(kind){
  const cached=illustration.templates?.get(kind);if(cached)return cached.cloneNode(true);
  const span=document.createElement('span');span.className='illustration';span.setAttribute('aria-hidden','true');
  span.innerHTML=`<svg viewBox="0 0 32 32" fill="none" stroke="#594e39" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">${iconPaths[kind] || iconPaths.villagers}</svg>`;
  if(span.cloneNode){illustration.templates ||= new Map();illustration.templates.set(kind,span);return span.cloneNode(true);}
  return span;
}
function fact(value,label){
  const card=document.createElement('div');card.className='selection-fact';
  const number=document.createElement('strong');number.textContent=value;if(typeof value==='string'&&value.includes('\n'))number.className='fact-lines';card.append(number);
  const caption=document.createElement('small');caption.textContent=label;card.append(caption);return card;
}
function skillBar(label,value){
  const row=document.createElement('div');row.className='skill-row';
  const caption=document.createElement('small');caption.textContent=`${label} · ${Math.floor(value)}`;row.append(caption);
  const meter=document.createElement('meter');meter.min=0;meter.max=Math.max(10,Math.ceil((value+1)/10)*10);meter.value=value;
  meter.setAttribute('aria-label',`${label}: ${Math.floor(value)}; scale to ${meter.max}`);row.append(meter);return row;
}
function villagerSkillIcon(job,label,value){
  const tile=document.createElement('span');tile.className='villager-skill';tile.tabIndex=0;
  const scale=Math.max(10,Math.ceil((value+1)/10)*10);
  const description=`${label} · ${Math.floor(value)} practice`;
  tile.title=description;tile.setAttribute('aria-label',description);
  const icon=document.createElement('span');icon.className='villager-skill-art';
  icon.append(illustration(job));
  const fill=illustration(job);fill.className='illustration villager-skill-fill';fill.style.clipPath=`inset(${100-value/scale*100}% 0 0)`;icon.append(fill);tile.append(icon);
  const number=document.createElement('strong');number.textContent=Math.floor(value);number.setAttribute('aria-hidden','true');tile.append(number);
  return tile;
}
function notice(message) { $('notice').textContent=message; $('notice').style.display='block'; clearTimeout(notice.timer); notice.timer=setTimeout(()=>$('notice').style.display='none',4500); }
async function request(path, data, options={}) {
  const response = await fetch(path,{...options,method:data?'POST':'GET',headers:{'Content-Type':'application/json','Authorization':identity?.token || '',...options.headers},body:data?JSON.stringify(data):undefined});
  const result = await response.json(); if(!response.ok) throw Error(result.error || 'Connection failed'); return result;
}
async function act(data) { try { await request('/api/command',{...data,world_id:world.world_id}); commandEffect(data); await refresh(); } catch(e) {notice(e.message);await refresh();} }
$('join-form').onsubmit=async event=>{
  event.preventDefault();
  const worldId=world.world_id,button=$('login-submit');
  if(button.disabled)return;
  button.disabled=true;$('login-error').textContent='';
  try{
    const data={player_name:$('player-name').value,pin:$('player-pin').value,world_id:worldId};
    if(!$('town-name-field').hidden)data.town_name=$('name').value;
    const response=await request('/api/login',data,{headers:{'Content-Type':'application/json','Authorization':identities[worldId]?.token || ''}});
    if(world.world_id!==worldId)throw Error('The host changed worlds. Connect again.');
    if(response.needs_town){$('town-name-field').hidden=false;$('name').required=true;$('name').focus();return;}
    identity=response;identities[worldId]=identity;
    try{localStorage.setItem('lantern-identities',JSON.stringify(identities));}catch{}
    viewed=identity.town_id;panel=null;$('player-pin').value='';$('login-dialog').close();$('world-menu').close();await refresh();
  }catch(e){$('login-error').textContent=e.message;}finally{button.disabled=false;}
};
for(const id of ['player-name','player-pin'])$(id).addEventListener('input',()=>{$('town-name-field').hidden=true;$('name').required=false;});
$('login-dialog').addEventListener('cancel',event=>event.preventDefault());
$('login-worlds').onclick=()=>{$('world-menu').showModal();};
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
  panel=next;selection=null;tool=null;moveIndex=null;buildTile=null;hoverTile=null;
  render();
}
for(const name of ['build','villagers','valley','help','resources','items','rules']){
  $(name+'-button').onclick=()=>openPanel(panel===name?null:name);
}
function updateSettingsButton(){
  const open=$('settings-panel').open;
  $('settings-button').setAttribute('aria-expanded',String(open));
  $('settings-button').classList.toggle('selected',open);
}
$('settings-button').onclick=()=>{
  clearWalk();
  if($('settings-panel').open)$('settings-panel').close();
  else $('settings-panel').showModal();
  updateSettingsButton();
};
$('close-settings').onclick=()=>{ $('settings-panel').close();updateSettingsButton(); };
$('settings-panel').addEventListener('close',updateSettingsButton);
window.addEventListener('keydown',event=>{
  if($('quarry-dialog')?.open||$('lanterns-dialog')?.open)return;
  const key=event.key.toLowerCase();
  if(key===','&&event.metaKey&&!event.ctrlKey&&!event.altKey&&!event.shiftKey){
    event.preventDefault();
    if(!event.repeat&&!$('settings-panel').open&&!$('world-menu').open&&!$('delete-world-dialog').open && !$('upgrade-dialog').open)$('settings-button').onclick();
    return;
  }
  if(/^[1-9]$/.test(key)&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&!event.shiftKey){
    if(!event.repeat&&!$('settings-panel').open&&!$('world-menu').open&&!$('delete-world-dialog').open && !$('upgrade-dialog').open
      &&!document.activeElement?.isContentEditable&&!['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName)){
      const name=['world-menu','build','villagers','resources','items','valley','help','rules'][Number(key)-1];
      if(name&&!$(name+'-button').disabled){
        event.preventDefault();
        if(name==='world-menu')clearWalk();
        $(name+'-button').onclick();
        if(name!=='world-menu')canvas.focus();
      }
    }
    return;
  }
  if(key===' '&&!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer(true)&&document.activeElement?.tagName!=='BUTTON'){
    const hall=world.towns[viewed]?.buildings?.find(building=>building.kind==='hall');
    if(hall){
      event.preventDefault();setCameraLock(false);camera.x=hall.x;camera.y=hall.y;keyboardTile=null;draw();
    }
    return;
  }
  if(key===keybindings.build&&!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer(true)){
    event.preventDefault();openPanel(panel==='build'?null:'build');canvas.focus();return;
  }
  if(key===keybindings.interact&&!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    event.preventDefault();
    const town=world.towns[identity.town_id],landscape=world.shared_map || town;
    let nearest=-1,distance=Infinity;
    (landscape.npcs || []).forEach((npc,index)=>{
      const position=npcPosition(npc),next=Math.hypot(position.x-town.player.x,position.y-town.player.y);
      if(next<=1.8&&next<distance){nearest=index;distance=next;}
    });
    if(nearest<0)notice('Walk beside a local to interact.');
    else {inspect('npc',nearest);act({action:'interact_local',npc_index:nearest});}
    return;
  }
  if(!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    const actions=[['fish','f',['water'],'No water to fish.'],['gather','g',['chest','berry_bush','herb_patch'],'No loot, berries, or herbs nearby.'],['chop','c',['tree'],'No wood to cut.'],['mine','m',['rock'],'No stone to mine.']];
    const action=actions.find(([name,fallback])=>key===(keybindings[name] || fallback));
    if(action){
      const town=world.towns[identity.town_id],landscape=world.shared_map || town;
      const nearby=(landscape.map_resources || []).some(r=>action[2].includes(r.kind)&&Math.abs(r.x-town.player.x)+Math.abs(r.y-town.player.y)<=1.8);
      if(!nearby || action[0]==='mine'){
        event.preventDefault();notice(nearby?'Mining is not available yet.':action[3]);return;
      }
    }
  }
  if(key===keybindings.camera&&!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    event.preventDefault();setCameraLock(!cameraLocked);draw();notice(`Camera lock ${cameraLocked?'on':'off'}.`);
  }
  if(['up','left','down','right'].some(action=>keybindings[action]===key)&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    event.preventDefault();walkKeys.add(key);
  }
  for(const action of ['fish','chop','gather'])if(key===keybindings[action]&&!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    event.preventDefault();playerAction(action);
  }
  if(event.key==='Escape' && !$('settings-panel').open && !$('world-menu').open && !$('delete-world-dialog').open && !$('upgrade-dialog').open){
    if(world.towns[identity?.town_id]?.player?.activity)playerAction('stop_activity');
    openPanel(null);canvas.focus();
  }
});
function inspect(type,index,townId=viewed){
  if(world.shared_map && world.towns[townId])viewed=townId;
  panel=type==='villager'&&panel==='villagers'?'villagers':'selection';selection={type,index};tool=null;moveIndex=null;buildTile=null;hoverTile=null;render();
}
function villagerActivity(town,v,index){
  if(v.construction_target)return v.construction_arrived?'Building':'Walking to construction';
  if(v.logistics_trip){
    const trip=v.logistics_trip;
    if(trip.operation==='sell')return (trip.step || 0)<trip.pickup_step?'Collecting goods for sale':'Returning to market to sell';
    return trip.operation==='collect'?'Gathering goods':'Delivering supplies';
  }
  if(v.job==='idle')return 'Resting';
  if(v.job==='building')return 'Waiting for construction';
  const status=v.production_status || '';
  if(status.startsWith('Butchering '))return status;
  if(status.startsWith('Workplace buffer full'))return 'Waiting for collection';
  if(status.startsWith('Waiting for delivery') || status.startsWith('Waiting for reachable storage'))return 'Waiting for supplies';
  if(status.startsWith('Delivery'))return 'Waiting for a route';
  if(status.includes('already tended'))return 'Waiting for next cycle';
  if(status.startsWith('Waiting for supply'))return 'Waiting for resources';
  if(status.startsWith('Waiting for goods'))return 'Waiting for goods';
  if(status.startsWith('Requires '))return 'Waiting for training';
  if(status==='Production cap reached')return status;
  if(status==='Storage full')return 'Waiting for storage';
  if(status==='Buy animals first')return 'Waiting for animals';
  if(status.startsWith('Waiting for '))return status.split(':')[0];
  const state=workers.get(world.shared_map?`${viewed}/${index}`:index);
  if(state?.phase==='returning')return 'Delivering resource';
  if(state?.phase==='depositing')return 'Unloading resource';
  if(state?.path.length)return 'Walking to work';
  if(!v.work_target)return 'Waiting for work';
  return {woodcutting:'Gathering resource',farming:'Tending crops',fishing:'Fishing',hauling:'Hauling goods'}[v.job] || 'Working';
}
function villagerCard(town,v,index,withLink=false){
  const div=document.createElement('div');div.className='villager';
  const name=document.createElement(withLink?'button':'strong');name.textContent=v.name;
  if(withLink){name.className='villager-name';name.onclick=()=>inspect('villager',index);}div.append(name);
  const profile=document.createElement('dl');profile.className='villager-profile';
  const homes=town.buildings.filter(b=>b.kind==='home'&&!b.construction);
  const housing=homes.length?`Shared cottages · ${[...new Set(homes.map(buildingLevel))].sort((a,b)=>a-b).map(level=>`level ${level}`).join(', ')}`:'Town hall · shared shelter';
  const job=v.job==='idle'&&v.construction_target?'Builder (automatic)':{idle:'Resting',building:'Builder',woodcutting:'Woodcutting',farming:'Farming',fishing:'Fishing',hauling:'Hauling',...professionJobs}[v.job] || goodName(v.job);
  const talent=Object.entries(v.talents || {}).filter(([,value])=>value>0).map(([job])=>goodName(job)).join(', ') || 'No special talent';
  for(const [label,value] of [['Name',v.name],['Job',job],['House type',housing],['Talent',talent]]){
    const term=document.createElement('dt');term.textContent=label;
    const detail=document.createElement('dd');detail.textContent=value;profile.append(term);profile.append(detail);
  }
  div.append(profile);
  renderWorkerTools(town,v,index,div);
  div.append(skillBar('Woodcutting',v.woodcutting));div.append(skillBar('Farming',v.farming));div.append(skillBar('Fishing',v.fishing || 0));
  div.append(skillBar('Hauling',v.hauling || 0));
  const hauling=document.createElement('small');hauling.className='villager-hauling';
  hauling.textContent=`Hauling: ${(50*workerEfficiency(v,'hauling')).toFixed(0)} goods per trip · ${(1.8*(1+.25*Math.floor((v.hauling || 0)/20))).toFixed(2)} tiles/s before road bonuses. Successful deliveries earn practice; every 20 practice adds 50 carrying capacity and 25% base travel speed.`;div.append(hauling);
  if(['hauling','trading'].includes(v.job)){
    const trip=v.logistics_trip,load=document.createElement('small');load.className='villager-cargo';
    const carrying=trip&&!trip.delivered&&(trip.step || 0)>=(trip.pickup_step ?? 0);
    const goods=Object.entries(trip?.cargo || {}).filter(([,amount])=>amount>0).map(([key,amount])=>`${Number(amount.toFixed(2))} ${goodName(key)}`).join(' · ');
    load.textContent=carrying&&goods?`Carrying: ${goods}`:trip&&!trip.delivered&&goods?`Collecting: ${goods}`:'Carrying: nothing';div.append(load);
  }
  for(const [job,label] of Object.entries(professionJobs))if(v[job] || v.job===job)div.append(skillBar(label,v[job] || 0));
  const traits=document.createElement('small');traits.className='villager-talents';
  traits.textContent=Object.entries(v.talents || {}).map(([job,value])=>`${value>0?'Strength':'Weakness'}: ${goodName(job)} (${value>0?'+':'−'}25%)`).join(' · ');div.append(traits);
  if(!['idle','building','hauling','trading'].includes(v.job)){const output=document.createElement('small');output.className='villager-output';output.textContent=`Work output: ${workerEfficiency(v,v.job).toFixed(2)}× base before workplace bonuses`;div.append(output);}
  const activity=document.createElement('small');activity.className='villager-activity';activity.textContent=villagerActivity(town,v,index);div.append(activity);
  const select=document.createElement('select');select.setAttribute('aria-label',`Job for ${v.name}`);
  const unlockedJobs=Object.entries(professionJobs).filter(([job])=>Object.values(world.progression?.buildings || {}).some(spec=>spec.job===job&&town.level>=spec.level));
  for(const [job,label] of [['idle','Resting'],['building','Builder'],['hauling','Hauling'],['woodcutting','Woodcutting'],['farming','Farming'],['fishing','Fishing'],...unlockedJobs]){const option=document.createElement('option');option.value=job;const requirement=jobRequirement(v,job);option.textContent=label+(requirement?` · Requires ${requirement}`:'');option.disabled=Boolean(requirement);select.append(option);}
  select.value=v.job;select.disabled=viewed!==identity?.town_id;select.onchange=()=>act({action:'job',index,job:select.value});div.append(select);
  return div;
}
function villagerRow(town,v,index){
  const row=document.createElement('tr');
  const name=document.createElement('th');name.setAttribute('scope','row');name.textContent=v.name;
  const gear=workerTools[v.worker_tool];
  if(gear){
    const equipped=document.createElement('span');equipped.className='villager-equipment';equipped.tabIndex=0;equipped.setAttribute('role','img');
    const description=`${gear.name} · +${gear.bonus*100}% ${gear.job} output · ${v.job===gear.job?'Active for current job':'Inactive for current job'}`;
    equipped.title=description;equipped.setAttribute('aria-label',description);equipped.append(illustration(v.worker_tool));
    const tooltip=document.createElement('span');tooltip.className='skill-tooltip';tooltip.textContent=description;tooltip.setAttribute('aria-hidden','true');equipped.append(tooltip);name.append(equipped);
  }
  row.append(name);
  const job=document.createElement('td');
  row.append(job);
  const housing=document.createElement('td');
  const homes=town.buildings.filter(b=>b.kind==='home'&&!b.construction);
  housing.textContent=`Shared, lvl ${homes.length?[...new Set(homes.map(buildingLevel))].sort((a,b)=>a-b).join(', '):town.level}`;row.append(housing);
  const skills=document.createElement('td'),talents=document.createElement('td'),status=document.createElement('td');
  row.append(skills);row.append(talents);row.append(status);
  const icons=document.createElement('div');icons.className='villager-skills';
  for(const [job,label] of Object.entries({woodcutting:'Woodcutting',farming:'Farming',fishing:'Fishing',hauling:'Hauling',...professionJobs})){
    if(v[job]>0)icons.append(villagerSkillIcon(job,label,v[job]));
  }
  skills.append(icons);
  const card=villagerCard(town,v,index);
  for(const child of Array.from(card.children)){
    if(child.className==='villager-talents'){if(!child.textContent)child.textContent='No special talent';talents.append(child);}
    if(child.className==='villager-activity')status.append(child);
  }
  job.append(card.children[card.children.length-1]);
  return row;
}
function villagersTable(town){
  const table=document.createElement('table');table.className='villagers-table';table.setAttribute('aria-label','Villagers overview');
  const head=document.createElement('thead'),heading=document.createElement('tr');
  for(const label of ['Name','Job','Housing','Skills','Talents','Work status']){
    const cell=document.createElement('th');cell.setAttribute('scope','col');cell.textContent=label;heading.append(cell);
  }
  head.append(heading);table.append(head);
  const body=document.createElement('tbody');
  for(const [index,v] of town.villagers.entries())body.append(villagerRow(town,v,index));
  table.append(body);return table;
}
const settlerSpaces={hall:1,home:2,storehouse:1,woodcutter:1,fishing_hut:1,farm:4,acre:1,barn:2,sawpit:2,mill:2,smokehouse:2,workshop:2,fish_pond:2,orchard:2,market:1,bakery:2,butcher:2};
function settlerBuilding(town,v,index){
  if(Number.isInteger(v.building_index))return v.building_index;
  const kind={woodcutting:'woodcutter',farming:'farm',fishing:'fishing_hut'}[v.job];
  const places=town.buildings.map((b,i)=>({b,i})).filter(({b})=>b.kind===kind&&(!b.construction||b.construction.upgrade)&&buildingUsable(town,b));
  return places.length?places[index%places.length].i:null;
}
function renderSettlers(town,item,buildingIndex,target=$('selection-details'),upgrades=target,details=target){
  const owner=viewed===identity?.town_id;
  if(item.kind==='hall'){
    target.append(fact(owner?'You':'Town founder','Resident'));
    const resident=document.createElement('button');resident.textContent=owner?'You':'Town founder';
    resident.onclick=()=>inspect('player');target.append(resident);
    const note=document.createElement('p');note.textContent='Only the town owner’s controllable character can settle here. Assign other settlers to cottages or workplaces.';
    details.append(note);return;
  }
  const level=item.settler_level || 1,capacity=settlerSpaces[item.kind]*level;
  const assigned=town.villagers.map((v,i)=>({v,i})).filter(({v,i})=>settlerBuilding(town,v,i)===buildingIndex);
  target.append(fact(`${assigned.length} / ${capacity}`,['hall','home'].includes(item.kind)?'Settlers':'Workers'));
  const note=document.createElement('p');
  note.textContent=item.kind==='acre'?'Assigned settlers tend this acre and fetch water when watering.':
    item.kind==='storehouse'?'Assigned haulers collect workplace outputs and deliver processor inputs.':item.kind==='market'?'Assigned merchants collect goods and return here to sell automatic batches when stock reaches your trigger.':['hall','home'].includes(item.kind)?'Assigned settlers rest here and can still help with construction. Housing and storage benefits are automatic.':
    'Assign settlers here to work at this building.';
  details.append(note);
  const roster=document.createElement('div');roster.className='building-workers';
  for(const {v,i} of assigned){
    const row=document.createElement('div');row.className='building-worker';
    const summary=document.createElement('button');summary.className='worker-summary';
    const name=document.createElement('strong');name.textContent=v.name;summary.append(name);
    const skills=document.createElement('small');
    const skill={storehouse:'hauling',woodcutter:'woodcutting',farm:'farming',acre:'farming',fishing_hut:'fishing'}[item.kind] || world.progression?.buildings[item.kind]?.job;
    const skillLabels={woodcutting:'Woodcutting',farming:'Farming',fishing:'Fishing',hauling:'Hauling',...professionJobs};
    skills.textContent=(skill?[skill]:['woodcutting','farming','fishing']).map(key=>`${skillLabels[key]} · Lv ${Math.floor(v[key] || 0)}`).join(' / ');
    summary.append(skills);summary.onclick=()=>inspect('villager',i);row.append(summary);
    if(owner){
      const kick=document.createElement('button');kick.className='worker-kick danger';kick.textContent='Kick';
      kick.setAttribute('aria-label',`Kick ${v.name} from ${labels[item.kind] || 'this building'}`);
      kick.onclick=()=>{if(viewed===identity?.town_id)act({action:'remove_settler',index:i});};row.append(kick);
    }
    roster.append(row);
  }
  target.append(roster);
  if(!owner)return;
  const chooser=document.createElement('select');chooser.setAttribute('aria-label','Settler to add');
  const available=town.villagers.map((v,i)=>({v,i})).filter(({v,i})=>settlerBuilding(town,v,i)!==buildingIndex);
  const job={woodcutter:'woodcutting',farm:'farming',acre:'farming',fishing_hut:'fishing'}[item.kind] || world.progression?.buildings[item.kind]?.job;
  const talentRoot=world.progression?.talent_roots?.[job] || job;
  available.sort((a,b)=>Number(b.v.job==='idle')-Number(a.v.job==='idle')
    || Number(settlerBuilding(town,a.v,a.i)!==null)-Number(settlerBuilding(town,b.v,b.i)!==null)
    || (job?(b.v.talents?.[talentRoot] || 0)-(a.v.talents?.[talentRoot] || 0):0));
  const requirements=world.progression?.job_requirements?.[job];
  if(requirements){const note=document.createElement('small');note.textContent='Each settler needs '+Object.entries(requirements).map(([skill,amount])=>`${amount} ${goodName(skill).toLowerCase()} practice`).join(', ');target.append(note);}
  const eligible=available.filter(({v})=>!jobRequirement(v,job));
  for(const {v,i} of available){const option=document.createElement('option');option.value=i;const requirement=jobRequirement(v,job);option.textContent=v.name+(v.job==='idle'?' · Resting':'')+(requirement?` · Requires ${requirement}`:job?` · ${workerEfficiency(v,job).toFixed(2)}× output`:'')+(settlerBuilding(town,v,i)!==null?' · reassign':'');option.disabled=Boolean(requirement);chooser.append(option);}
  chooser.value=eligible.length?String(eligible[0].i):'';
  const add=document.createElement('button');add.textContent=['hall','home'].includes(item.kind)?'Add settler':'Add worker';add.disabled=assigned.length>=capacity||!eligible.length;
  chooser.disabled=add.disabled;
  add.onclick=()=>{if(!add.disabled)act({action:'assign_settler',index:Number(chooser.value),building_index:buildingIndex});};
  const assignment=document.createElement('div');assignment.className='worker-assignment';
  assignment.append(chooser);assignment.append(add);target.append(assignment);
  const upgrade=document.createElement('button');upgrade.className='upgrade-button';upgrade.textContent='Capacity';
  upgrade.title=level>=3?'Settler capacity fully upgraded':`Upgrade to ${settlerSpaces[item.kind]*(level+1)} spaces · ${20*level} wood · ${30*level} seconds`;
  upgrade.setAttribute('aria-label',`Capacity: ${upgrade.title}`);
  upgrade.disabled=level>=3;
  upgrade.onclick=()=>{if(!upgrade.disabled)showUpgrade({action:'upgrade_capacity',index:buildingIndex});};
  upgrades.append(upgradeCard(upgrade,upgradePreview({action:'upgrade_capacity',index:buildingIndex},town,item),{title:level>=3?'Capacity · Tier 3':`Capacity · Tier ${level+1}`,status:level>=3?'Fully upgraded':'',benefits:[`Settler spaces: ${capacity}${level<3?` → ${settlerSpaces[item.kind]*(level+1)}`:''}.`]}));
}
function selectedProfessionRecipe(item){
  const recipes=world.progression?.profession_recipes?.[item.kind];
  return recipes?.find(recipe=>recipe.output===item.recipe) || recipes?.[0];
}
function renderStable(town,item,index,target){
  const nearby=Math.abs(town.player.x-item.x)+Math.abs(town.player.y-item.y)<=1.8;
  const adults=animalAges(item,'horse').filter(age=>age>=(world.progression.animal_growth_seconds || 300)).length;
  const full=(item.animals?.horse || 0)>=4*buildingLevel(item);
  const note=document.createElement('p');note.textContent='Walk beside the stable to bring in a wild horse, saddle an adult, or return your riding horse. Riding adds 75% land speed; roads also apply. A saddle stays with you for future rides.';target.append(note);
  for(const [action,label,available] of [
    ['stable_horse','Settle the horse you are leading',town.player.leading_horse&&!full],
    ['ride_horse','Saddle an adult horse and ride',adults>0&&!town.player.horse&&(town.player.saddle||amountOf(town,'saddles')>=1)],
    ['return_horse','Return riding horse for breeding',town.player.horse&&!full],
  ]){
    const button=document.createElement('button');button.textContent=label;button.disabled=!nearby||!available;button.onclick=()=>act({action,index});target.append(button);
  }
}
function renderTownTrades(town,target){
  const heading=document.createElement('h3');heading.textContent='Trade with another town';target.append(heading);
  const note=document.createElement('small');note.textContent='Offers use available stock when accepted. Both owners need a finished market. Incoming goods require enabled storage. Each offer can be accepted once.';target.append(note);
  const select=(label,entries)=>{
    const input=document.createElement('select');input.setAttribute('aria-label',label);
    for(const [value,text] of entries){const option=document.createElement('option');option.value=value;option.textContent=text;input.append(option);}
    target.append(input);return input;
  };
  const towns=Object.entries(world.towns).filter(([id,t])=>id!==identity.town_id&&t.level>=5&&t.buildings.some(b=>b.kind==='market'&&!b.construction));
  if(towns.length){
    const receiver=select('Trading town',towns.map(([id,t])=>[id,t.name]));
    const goods=world.progression.tradable_goods || ['coins','wood','planks'];
    const give=select('Goods to offer',goods.map(key=>[key,`Offer ${goodName(key)}`]));
    const want=select('Goods to receive',goods.map(key=>[key,`Receive ${goodName(key)}`]));
    give.value='planks';want.value='coins';
    const quantity=label=>{const input=document.createElement('input');input.type='number';input.min='1';input.max='100';input.step='1';input.value='1';input.setAttribute('aria-label',label);target.append(input);return input;};
    const giveAmount=quantity('Quantity to offer'),wantAmount=quantity('Quantity to receive');
    const button=document.createElement('button');button.textContent='Create trade offer';button.onclick=()=>act({action:'create_trade',target_town:receiver.value,give_resource:give.value,give_amount:Number(giveAmount.value),want_resource:want.value,want_amount:Number(wantAmount.value)});target.append(button);
  }else{const message=document.createElement('p');message.textContent='Another town needs to reach hall 5 and build a market before you can trade.';target.append(message);}
  for(const [sourceId,source] of Object.entries(world.towns))for(const offer of source.trade_offers || []){
    const outgoing=sourceId===identity.town_id,incoming=offer.target_town===identity.town_id;
    if(!outgoing&&!incoming)continue;
    const row=document.createElement('p');row.textContent=`${source.name} offers ${offer.give_amount} ${goodName(offer.give_resource).toLowerCase()} for ${offer.want_amount} ${goodName(offer.want_resource).toLowerCase()} to ${world.towns[offer.target_town]?.name || 'another town'}`;
    const button=document.createElement('button');button.textContent=outgoing?'Cancel offer':'Accept trade';button.disabled=!outgoing&&(!canAfford(town,{[offer.want_resource]:offer.want_amount})||!canAfford(source,{[offer.give_resource]:offer.give_amount}));button.onclick=()=>act({action:outgoing?'cancel_trade':'accept_trade',source_town:sourceId,offer_id:offer.id});row.append(button);target.append(row);
  }
}
function marketSaleGoods(catalog){
  return catalog.sellable_goods || [...new Set(['hides','leather','iron_ore','iron','saddles','steel','trade_crates','fish','trout','salmon','sturgeon','planks','wool','flour','smoked_fish','tools','herbs','fruit','milk','eggs','meat','bread','prepared_meat',...Object.values(catalog.profession_recipes || {}).flat().map(recipe=>recipe.output)])];
}
function renderMarketSales(town,target){
  const keys=marketSaleGoods(world.progression);
  const signature=JSON.stringify([world.world_id,identity?.town_id,town.rules?.market_sales,keys]);
  // Reattach the same editor during snapshot refreshes to preserve drafts.
  if(renderMarketSales.signature!==signature){
    const editor=document.createElement('details');
    const title=document.createElement('summary');title.textContent='Automatic sales';editor.append(title);
    const note=document.createElement('p');note.textContent='Assign a merchant to this market. When town stock reaches the trigger, they collect up to the batch size from one building and sell it on returning. Sales repeat while stock reaches the trigger. Leave the trigger blank to disable a product.';editor.append(note);
    for(const key of keys){
      const rule=town.rules?.market_sales?.[key];
      const row=document.createElement('form');row.className='production-rule market-sale-rule';
      const name=document.createElement('strong');name.className='market-sale-name';name.textContent=goodName(key);row.append(name);
      const field=(labelText,id,value,max,step)=>{
        const label=document.createElement('label');label.textContent=labelText;
        const input=document.createElement('input');input.id=id;input.type='number';input.min='1';input.step=step;
        if(max)input.max=String(max);input.value=value;
        input.setAttribute('aria-label',`${goodName(key)} ${labelText.toLowerCase()}`);
        label.append(input);row.append(label);return input;
      };
      const cap=field('Trigger stock','sale-cap-'+key,rule?.cap ?? '',null,'any');cap.placeholder='Disabled';
      const amount=field('Batch size','sale-amount-'+key,rule?.amount ?? 10,10,'1');amount.required=true;
      const save=document.createElement('button');save.textContent='Save';row.append(save);
      row.onsubmit=event=>{event.preventDefault();if(!row.reportValidity())return;
        act({action:'configure_market_sale',resource:key,cap:cap.value.trim()===''?null:Number(cap.value),amount:Number(amount.value)});
      };
      editor.append(row);
    }
    editor.open=renderMarketSales.editor?.open || false;
    renderMarketSales.signature=signature;renderMarketSales.editor=editor;
  }
  target.append(renderMarketSales.editor);
}
function renderRecipes(town,item,index,target){
  const recipes=world.progression?.profession_recipes?.[item.kind];
  if(!recipes)return;
  const job=world.progression.buildings[item.kind].job;
  const workers=town.villagers.filter(v=>v.building_index===index&&v.job===job&&!jobRequirement(v,job));
  const chooser=document.createElement('select');chooser.setAttribute('aria-label','Production recipe');
  for(const spec of recipes){
    const unlocked=spec.level===1||workers.some(v=>(v[job] || 0)>=spec.level);
    const option=document.createElement('option');option.value=spec.output;
    option.textContent=`${goodName(spec.output)} · Lv ${spec.level}${unlocked?'':' · Locked'}`;
    option.disabled=!unlocked;chooser.append(option);
  }
  chooser.value=selectedProfessionRecipe(item).output;
  chooser.disabled=viewed!==identity?.town_id;
  target.append(chooser);
  const note=document.createElement('small');
  const updateNote=()=>{
    const spec=recipes.find(recipe=>recipe.output===chooser.value);
    const seconds=world.progression.production_seconds?.[spec.output] || 3;
    const food=world.progression.edible?.[spec.output];
    note.textContent=`${costText(spec.inputs)} → ${spec.amount} ${goodName(spec.output).toLowerCase()} · ${seconds}s · ${(0.04*seconds/3).toFixed(2)} practice${food?` · ${spec.amount*food} food`:''}${spec.level>1?` · Worker level ${spec.level}+`:''}`;
  };
  updateNote();
  chooser.onchange=()=>{if(!chooser.disabled){updateNote();act({action:'set_recipe',index,recipe:chooser.value});}};
  target.append(note);
}
function productionDescription(item){
  if(item.kind==='mine')return 'Output: 2 iron ore / 9s batch\nRequires an iron vein within 10 tile steps\nOre regenerates slowly; steel pickaxes add 25% output';
  const spec=selectedProfessionRecipe(item);
  const recipe=spec?[spec.inputs,spec.output,spec.amount]:world.progression.recipes[item.kind];
  if(recipe){const seconds=world.progression.production_seconds?.[recipe[1]] || 3;return `${costText(recipe[0])} → ${recipe[2]} ${goodName(recipe[1]).toLowerCase()} / ${seconds}s batch${item.kind==='smokehouse'?'\nAccepts fish, trout, salmon, and sturgeon':''}\n${(0.04*seconds/3).toFixed(2)} practice / batch (neutral talent)\nSkill: larger batches + more ingredients\nRequires qualified workers${spec?` · Recipe level ${spec.level}`:''}`;}
  return {barn:'Adult output: eggs, wool, milk\nFeed: fodder / vegetables\nGrowth: 5 minutes of full feeding\nPigs provide meat at the butcher\n4 animals / level',orchard:'Output: fruit\nRequires workers',stable:'Capture wild horses and bring them here\nFeed: fodder / vegetables\nSettling and growth: 5 minutes of full feeding\nBreed two adults; saddle one to ride\n4 horses / level',market:'Sell goods for coins\nOffer goods to another town\nBoth owners need a completed market'}[item.kind] || 'Requires workers';
}
function animalAges(barn,animal){
  const duration=world.progression?.animal_growth_seconds || 300;
  return Array.from({length:barn.animals?.[animal] || 0},(_,i)=>barn.animal_growth?.[animal]?.[i] ?? duration);
}
function renderAnimalButchery(town,item,index,target){
  const catalog=world.progression,owner=viewed===identity?.town_id;
  const note=document.createElement('p');
  const task=item.butchering;
  note.textContent=task?`Butchering ${goodName(task.animal).toLowerCase()} → ${task.amount} meat · ${Math.ceil(task.remaining)}s of work remaining${task.remaining===0?' · Waiting for buffer space or production cap':''}`:`Uses 1 adult animal permanently · ${catalog.butcher_seconds || 60}s of work.`;
  target.append(note);
  if(!owner)return;
  const chooser=document.createElement('select');chooser.setAttribute('aria-label','Animal to butcher');
  const choices=[];
  town.buildings.forEach((barn,barnIndex)=>{
    if(barn.kind!=='barn'||barn.construction&&!barn.construction.upgrade)return;
    for(const animal of Object.keys(catalog.animals)){
      const adults=animalAges(barn,animal).filter(age=>age>=(catalog.animal_growth_seconds || 300)).length;
      if(!adults)continue;
      choices.push({barn,barnIndex,animal});
    }
  });
  for(const {barn,barnIndex,animal} of choices){
    const option=document.createElement('option');option.value=`${barnIndex}:${animal}`;
    const multipleBarns=choices.some(choice=>choice.animal===animal&&choice.barnIndex!==barnIndex);
    option.textContent=`${goodName(animal)} → ${catalog.animal_meat?.[animal] || 0} meat${multipleBarns?` · Barn (${barn.x}, ${barn.y})`:''}`;
    chooser.append(option);
  }
  const qualified=town.villagers.some(v=>v.building_index===index&&v.job==='butchery'&&!jobRequirement(v,'butchery'));
  const button=document.createElement('button');button.textContent=task?'Butchering in progress':!chooser.children.length?'No fully grown animals':!qualified?'Assign a qualified butcher':'Use animal for meat';
  button.disabled=!!task||!chooser.children.length||!qualified;chooser.disabled=button.disabled;
  button.onclick=()=>{if(button.disabled||viewed!==identity?.town_id)return;const [barnIndex,animal]=chooser.value.split(':');act({action:'butcher_animal',index,barn_index:Number(barnIndex),animal});};
  target.append(chooser,button);
}
function renderProfession(town,item,index,target,upgrades,details,boats,unlockedUpgrades){
  if(item.construction)return;
  const owner=viewed===identity?.town_id;
  const catalog=world.progression;
  if(!catalog)return;
  if(boats){
    const note=document.createElement('p');
    note.textContent=fishingBoatLevel(item)?`Boat owned by this hut · Tier ${fishingBoatLevel(item)}/3 · Crew ${fishingBoatLevel(item)} · Cargo ${Number(fishingBoatLoad(town,item).toFixed(2))}/${fishingBoatCapacity(item)} fish · Other fishers use the dock · Reach ${resourceReach(item)} tiles · Maximum depth ${fishingBoatLevel(item)+1}. Prefers deeper water; ${['','trout','salmon','sturgeon'][fishingBoatLevel(item)]} require this depth. Returns when full or the fishing spot runs out; unloads without leaving the boat. Click its boat on the map to manage it.`:'No boat owned · Buy one boat for this fishing hut. It appears beside nearby water immediately.';
    boats.append(note);
  }
  renderRecipes(town,item,index,target);
  if(item.kind==='butcher')renderAnimalButchery(town,item,index,target);
  if(item.kind==='acre'){
    const chooser=document.createElement('select');chooser.setAttribute('aria-label','Crop');
    for(const [key,level] of Object.entries(catalog.crops)){
      if(town.level<level)continue;
      const option=document.createElement('option');option.value=key;option.textContent=goodName(key);chooser.append(option);
    }
    chooser.value=item.crop || 'vegetables';chooser.disabled=!owner;chooser.onchange=()=>act({action:'set_crop',index,crop:chooser.value});target.append(chooser);
  }
  if(['barn','stable'].includes(item.kind)){
    target.append(fact(`${Object.values(item.animals || {}).reduce((a,n)=>a+n,0)} / ${4*buildingLevel(item)}`,'Animal capacity'));
    for(const [animal,spec] of Object.entries(catalog.animals)){
      if(town.level<spec.level || (animal==='horse')!==(item.kind==='stable'))continue;
      const ages=animalAges(item,animal),duration=catalog.animal_growth_seconds || 300;
      const adults=ages.filter(age=>age>=duration).length;
      target.append(fact(`${adults} adult · ${ages.length-adults} growing`,`${goodName(animal)} → ${animal==='horse'?'Riding / breeding':animal==='pig'?'Meat and hides at butcher':goodName(spec.output)}`));
      ages.forEach((age,i)=>{if(age<duration){const growth=document.createElement('small');growth.textContent=`${goodName(animal)} ${i+1} · ${Math.floor(age/duration*100)}% grown · ${Math.ceil(duration-age)}s of full feeding left`;target.append(growth);}});
      if(owner){
        const buy=document.createElement('button');buy.textContent=`Buy ${animal} · ${spec.coins} coins${town.level<spec.level?` · Hall ${spec.level}`:''}`;
        buy.hidden=animal==='horse';buy.disabled=town.level<spec.level || town.coins<spec.coins || Object.values(item.animals || {}).reduce((a,n)=>a+n,0)>=4*buildingLevel(item);buy.onclick=()=>act({action:'buy_animal',index,animal});target.append(buy);
        const breed=document.createElement('button');breed.textContent=`Breed ${animal} · 3 fodder + 1 herb`;breed.disabled=adults<2 || !canAfford(town,{fodder:3,herbs:1}) || Object.values(item.animals || {}).reduce((a,n)=>a+n,0)>=4*buildingLevel(item);breed.onclick=()=>act({action:'breed_animal',index,animal});target.append(breed);
      }
    }
  }
  if(item.kind==='stable'&&owner)renderStable(town,item,index,target);
  if(item.kind==='market'&&owner){renderMarketSales(town,target);renderTownTrades(town,target);}
  for(const [key,spec] of Object.entries(catalog.upgrades))if(spec.building===item.kind){
    const unlocked=item.profession_upgrades?.includes(key);
    const fishingBoat=item.kind==='fishing_hut'&&['boat','improved_boat','master_boat'].includes(key);
    if(!fishingBoat&&!unlocked&&(town.level<spec.level || professionSkill(town,spec.skill)<(spec.xp || 0) || spec.requires&&!item.profession_upgrades?.includes(spec.requires)))continue;
    const button=document.createElement('button');button.textContent=unlocked?`${spec.label} · Unlocked`:fishingBoat?`${key==='boat'?'Buy':'Upgrade to'} ${spec.label.toLowerCase()}`:spec.label;
    button.className='upgrade-button';
    button.disabled=!owner || unlocked;button.onclick=()=>{if(!button.disabled)showUpgrade({action:'profession_upgrade',index,upgrade:key});};
    (unlocked?unlockedUpgrades:fishingBoat?boats:upgrades).append(upgradeCard(button,upgradePreview({action:'profession_upgrade',index,upgrade:key},town,item),{status:unlocked?'Unlocked':''}));
  }
  if(item.kind==='market'&&owner)for(const key of marketSaleGoods(catalog)){
    const price=catalog.fish?.[key]?.coins ?? catalog.sale_prices?.[key] ?? ({trout:2,salmon:3,sturgeon:5}[key] || (key==='tools'?3:1));
    const button=document.createElement('button');button.textContent=`Sell 1 ${goodName(key).toLowerCase()} · ${price} coin${price===1?'':'s'}`;button.disabled=amountOf(town,key)<1;button.onclick=()=>act({action:'sell_goods',resource:key,amount:1});target.append(button);
  }
  if(['woodcutter','fishing_hut'].includes(item.kind)){
    const reach=resourceReach(item);
    const sources=town.map_resources.filter(r=>r.kind===(item.kind==='woodcutter'?'tree':'water')&&Math.abs(r.x-item.x)+Math.abs(r.y-item.y)<=reach);
    const note=document.createElement('p');note.textContent=sources.length?`Shared supply: ${sources[0].supply == null ? '—' : Math.floor(sources[0].supply)} / ${sources[0].supply_max ?? '—'} · Regen: ${sources[0].regeneration ?? '—'} / 3s · Reach: ${reach} tiles`:'No source in reach.';details.append(note);
  }
}
let buildingFolds={key:null,open:{}};
function buildingSection(label,folded=false){
  const section=document.createElement(folded?'details':'div');section.className='building-section';
  const heading=document.createElement(folded?'summary':'h3');heading.textContent=label;section.append(heading);
  if(folded){
    section.open=!!buildingFolds.open[label];
    const state=buildingFolds;
    section.addEventListener('toggle',()=>{state.open[label]=section.open;});
  }
  return section;
}
function renderSelection(town){
  const marketFocus=document.activeElement?.id?.startsWith('sale-')?document.activeElement:null;
  $('selection-panel').classList.toggle('building-info',selection?.type==='building');
  $('selection-art').replaceChildren();$('selection-level').hidden=true;
  // Park hall controls in the hidden attached container before rebuilding cards.
  $('hall-details').append($('upgrade'));
  $('selection-panel').append($('hall-details'));
  $('selection-details').replaceChildren();$('hall-details').hidden=true;$('selection-move').hidden=true;$('selection-destroy').hidden=true;
  if(!selection)return;
  if(!town){selection=null;panel=null;return;}
  if(selection.type==='road'){
    const road=town.roads?.[selection.index];
    if(!road){selection=null;panel=null;return;}
    $('selection-title').textContent='Dirt road';
    const p=document.createElement('p');p.textContent=road.construction?`Under construction · ${Math.ceil(road.construction.total-road.construction.progress)}s of builder work remaining`:'Players and settlers walk 50% faster here. Roads are free and take 10 seconds of builder work per tile.';$('selection-details').append(p);
    if(road.construction){const progress=document.createElement('progress');progress.max=road.construction.total;progress.value=road.construction.progress;progress.setAttribute('aria-label','Dirt road construction');$('selection-details').append(progress);}
    if(viewed===identity?.town_id){const button=document.createElement('button');button.textContent='Remove road · Free';button.onclick=async()=>{await act({action:'remove_road',x:road.x,y:road.y});openPanel(null);};$('selection-details').append(button);}
    return;
  }
  if(selection.type==='player'){
    $('selection-title').textContent=viewed===identity?.town_id?'You':'Town founder';
    const p=document.createElement('p');
    p.textContent=viewed===identity?.town_id?'Your adventuring villager. Click the map and use your movement keys to explore. Open Settings to change keybindings. Your position is saved with this world.':'This is the town owner’s adventuring villager.';
    $('selection-details').append(p);
    const skills=document.createElement('div');skills.className='player-skills';skills.setAttribute('role','group');skills.setAttribute('aria-label','Player skills');
    for(const job of ['fishing','woodcutting','gathering']){
      const xp=town.player?.[job] || 0,level=1+Math.floor(xp/20);
      const tile=document.createElement('div');tile.className='player-skill';tile.tabIndex=0;
      const description=`${job[0].toUpperCase()+job.slice(1)} · Level ${level} · ${Math.floor(xp%20)}/20 practice`;
      tile.title=description;tile.setAttribute('aria-label',description);
      tile.append(illustration(job));
      const number=document.createElement('strong');number.textContent=level;number.setAttribute('aria-hidden','true');tile.append(number);
      const tooltip=document.createElement('span');tooltip.className='skill-tooltip';tooltip.textContent=description;tooltip.setAttribute('aria-hidden','true');tile.append(tooltip);
      skills.append(tile);
    }
    $('selection-details').append(skills);
    const gear=document.createElement('p');gear.textContent='Equipment: wear swamp boots to enter the darkest swamp ground. Swamp boots also give +20% herbs; desert clothing gives +20% berries when gathering. Equip clothing to receive its perk. Find gear in treasure or trade with Dara, Petra, or the Sunwell and Reedhaven traders.';$('selection-details').append(gear);
    for(const [key,item] of Object.entries(equipmentItems)){
      const count=town.player?.items?.[key] || 0,equipped=town.player?.equipment?.[item.slot]===key;
      const row=document.createElement('p');row.textContent=`${item.name} · ${item.perk} · ${count} owned${equipped?' · Equipped':''}`;
      if(viewed===identity?.town_id&&count){const button=document.createElement('button');button.textContent=equipped?`Unequip ${item.name.toLowerCase()}`:`Equip ${item.name.toLowerCase()}`;button.onclick=()=>act({action:equipped?'unequip_item':'equip_item',item:key});row.append(button);}
      $('selection-details').append(row);
    }
    for(const [name,story] of Object.entries(world.local_stories || {})){
      const progress=localStoryProgress(town,name),stage=story.stages[progress.story_stage || 0];
      if(!progress.story_stage||!stage?.button)continue;
      const row=document.createElement('p');row.textContent=`${story.title} · ${name}: ${storyObjectives(stage,town,progress)} · Return to ${name} to continue.`;$('selection-details').append(row);
    }
    for(const keepsake of town.player?.keepsakes || []){
      const row=document.createElement('p');row.textContent=`Keepsake: ${keepsake}`;$('selection-details').append(row);
    }
    for(const [key,gear] of Object.entries(workerTools)){
      const row=document.createElement('p');row.textContent=`${gear.name} · ${town.player?.worker_tools?.[key] || 0} available · Give to a worker from their panel (+${gear.bonus*100}% ${gear.job} output)`;$('selection-details').append(row);
    }
    if(viewed===identity?.town_id){
      const quarry=document.createElement('button');quarry.textContent='The Sealed Quarry · Puzzle prototype';
      quarry.onclick=()=>openQuarry();$('selection-details').append(quarry);
      const courier=document.createElement('button');courier.textContent='The Falling Lanterns · Movement quest';
      courier.onclick=()=>openLanterns();$('selection-details').append(courier);
      if(town.player?.leading_horse){const note=document.createElement('p');note.textContent='Leading a wild horse. Walk beside your stable and select it to settle the horse.';$('selection-details').append(note);}
      if(town.player?.horse){const button=document.createElement('button');button.textContent=town.player.mounted?'Dismount horse':'Mount horse · +75% land speed';button.onclick=()=>act({action:'toggle_mount'});$('selection-details').append(button);}
      if(town.player?.boat){
        const level=playerBoatLevel(town.player),boat=document.createElement('p');
        boat.textContent=`${world.progression?.boats?.[level]?.label || 'Exploration boat'} · Tier ${level}/3 · +${25*(level-1)}% sailing speed and fish per catch while afloat. Walk onto water to sail; step ashore to disembark.`;$('selection-details').append(boat);
      }
      const nextBoat=world.progression?.boats?.[playerBoatLevel(town.player)+1];
      if(nextBoat){
        const owned=!!town.player?.boat,button=document.createElement('button'),cost=materialCosts(nextBoat);
        button.textContent=`${owned?'Upgrade to':'Buy'} ${nextBoat.label.toLowerCase()} · ${costText(cost)} · Hall ${nextBoat.level}${nextBoat.xp?` · ${nextBoat.xp} fishing practice`:''}`;
        button.disabled=town.level<nextBoat.level||professionSkill(town,'fishing')<nextBoat.xp||!canAfford(town,cost);
        button.onclick=()=>act({action:owned?'upgrade_boat':'buy_boat'});$('selection-details').append(button);
      }
      if(!town.player?.boat){const boat=document.createElement('button');boat.textContent=town.level<3?'Exploration boat · Requires hall level 3':'Build exploration boat · 20 wood + 10 planks';boat.disabled=town.level<3||amountOf(town,'wood')<20||amountOf(town,'planks')<10;boat.onclick=()=>act({action:'build_boat'});$('selection-details').append(boat);}
    }
    if(town.player?.activity){
      const state=document.createElement('p');state.textContent=`${goodName(town.player.activity.job)} · ${town.player.activity_status || 'Working'}`;$('selection-details').append(state);
    }
    if(viewed===identity?.town_id)for(const [action,label] of [['fish','Fish'],['chop','Cut wood'],['gather','Gather'],['stop_activity','Stop working']]){
      const button=document.createElement('button');button.textContent=label;button.onclick=()=>playerAction(action);$('selection-details').append(button);
    }
    return;
  }
  const item=selection.type==='npc'?town.npcs?.[selection.index]:selection.type==='villager'?town.villagers[selection.index]:selection.type==='building'?town.buildings[selection.index]:town.map_resources[selection.index];
  if(!item){selection=null;panel=null;return;}
  if(selection.type==='npc'){
    $('selection-title').textContent=item.name;
    const own=world.towns[identity?.town_id],player=own?.player;
    const position=npcPosition(item),nearby=!!player&&Math.hypot(player.x-position.x,player.y-position.y)<=1.8;
    const progress=own?.local_visits?.[localVisitKey(item)] || {};
    if(!Object.keys(progress).length){
      $('selection-details').append(illustration('villagers'));
      const hint=document.createElement('p');hint.textContent='Talk to this local to discover what they offer.';$('selection-details').append(hint);
      const button=document.createElement('button');button.textContent=nearby?'Talk':`Walk beside this local and press ${keybindings.interact.toUpperCase()} to talk`;
      button.disabled=!nearby||!world.shared_map&&viewed!==identity?.town_id;
      button.onclick=()=>act({action:'interact_local',npc_index:selection.index});$('selection-details').append(button);
      return;
    }
    const role=document.createElement('p');role.textContent=item.role;
    const dialogue=document.createElement('p');dialogue.textContent='“'+item.dialogue+'”';
    $('selection-details').append(illustration('villagers'));$('selection-details').append(role);$('selection-details').append(dialogue);
    if(player){
      const clothingOffers=clothingTraderItems(item);
      if(clothingOffers.length){const hint=document.createElement('p');hint.textContent=nearby?'I trade clothing for coins. Wear it from your character panel to gain its gathering perk.':'Walk beside this local to trade clothing.';$('selection-details').append(hint);}
      if(world.shared_map||viewed===identity?.town_id){
        const gift=localGifts[item.name];
        renderLocalStory(own,item,selection.index,nearby,$('selection-details'));
        if(gift){
          const gear=workerTools[gift] || equipmentItems[gift],note=document.createElement('p');
          note.textContent=workerTools[gift]?`Gift for a worker · +${gear.bonus*100}% ${gear.job} output`:`Clothing gift · ${gear.perk} · Equip from your character panel`;$('selection-details').append(note);
          const button=document.createElement('button');button.textContent=progress.gift_claimed?`${gear.name} gift collected`:`Accept ${gear.name.toLowerCase()} · ${workerTools[gift]?'Gift for a worker':'Clothing gift'}`;
          button.disabled=!nearby||!!progress.gift_claimed;button.onclick=()=>act({action:'claim_local_gift',npc_index:selection.index});$('selection-details').append(button);
        }
        for(const key of localToolShops[item.name] || []){
          const gear=workerTools[key],button=document.createElement('button');
          button.textContent=`Buy ${gear.name.toLowerCase()} · ${gear.price} coins · +${gear.bonus*100}% ${gear.job} output`;
          button.disabled=!nearby||own.coins<gear.price;
          button.onclick=()=>act({action:'buy_worker_tool',item:key,npc_index:selection.index});$('selection-details').append(button);
        }
        if(localBarters[item.name]){
          const [wood,good,amount]=localBarters[item.name],label=good==='vegetables'?'food':goodName(good).toLowerCase();
          const note=document.createElement('p');note.textContent=`“Bring me ${wood} wood and I’ll share ${amount} ${label}. I restock every five minutes.”`;$('selection-details').append(note);
          const remaining=progress.trade_remaining || 0,button=document.createElement('button');
          button.textContent=remaining>0?`Restocking · ${Math.ceil(remaining/60)} min remaining`:`Trade ${wood} wood for ${amount} ${label}`;
          button.disabled=!nearby||remaining>0||own.wood<wood||localTradeFoodRoom(own,good)<amount;
          button.onclick=()=>act({action:'trade_local',npc_index:selection.index});$('selection-details').append(button);
        }
      }
      if(world.shared_map||viewed===identity?.town_id)for(const key of clothingOffers){
        const gear=equipmentItems[key],note=document.createElement('p');note.textContent=`${gear.name} · ${gear.perk} while equipped`;$('selection-details').append(note);
        const buy=document.createElement('button');buy.textContent=`Buy ${gear.name.toLowerCase()} · ${gear.price} coins`;buy.disabled=!nearby||own.coins<gear.price;buy.onclick=()=>act({action:'buy_equipment',item:key,npc_index:selection.index});$('selection-details').append(buy);
        const equipped=player.equipment?.[gear.slot]===key;
        const sell=document.createElement('button');sell.textContent=`Sell ${gear.name.toLowerCase()} · ${Math.floor(gear.price/2)} coins`;sell.disabled=!nearby||(player.items?.[key] || 0)<=Number(equipped);sell.onclick=()=>act({action:'sell_equipment',item:key,npc_index:selection.index});$('selection-details').append(sell);
      }
    }
    return;
  }
  if(selection.type==='villager'){
    $('selection-title').textContent=item.name;$('selection-details').append(villagerCard(town,item,selection.index));return;
  }
  if(selection.type==='resource')$('selection-details').append(illustration(item.kind));
  const p=document.createElement('p');p.className='info-copy';
  if(selection.type==='resource'){
    const swamp=town.terrain?.[item.y]?.[item.x]==='b';
    if(item.kind==='village_hut'||item.kind==='village_market'){
      $('selection-title').textContent=`${item.village} · ${item.label || (item.kind==='village_market'?'Market':'Cottage')}`;
      p.textContent=`A ${item.biome==='b'?'swamp':item.biome==='d'?'desert':'ice'} village. Walk beside a resident and click them or press ${keybindings.interact.toUpperCase()} to talk and trade travel equipment.`;
      $('selection-details').append(p);return;
    }
    if(item.kind==='chest'){
      $('selection-title').textContent='Island treasure';p.textContent=`Loot: 25 coins + equipment\nCollect: stand nearby · ${keybindings.gather.toUpperCase()}\nOne claim / world`;$('selection-details').append(p);
      if(viewed===identity?.town_id){const button=document.createElement('button');button.textContent='Collect loot';button.onclick=()=>playerAction('gather');$('selection-details').append(button);}return;
    }
    $('selection-title').textContent=({berry_bush:'Berry bush',herb_patch:'Herb patch'}[item.kind]) || (swamp?{tree:'Swamp willow',water:'Swamp water',rock:'Rock outcrop'}:{tree:'Forest',water:'Water',rock:'Rock outcrop',berry_bush:'Berry bush',herb_patch:'Herb patch'})[item.kind];
    p.textContent={tree:'Renewable wood source. Place a woodcutter within 10 horizontal or vertical tile steps, then assign a villager to woodcutting.',water:'Uses: farming / fishing\nFishing hut reach: 3–5 tile steps (+3 / +5 / +7 with boat tiers)\nFarmers fetch water when watering',rock:'Occupied tile\nMining: unavailable',berry_bush:`Food: berries\nGather: stand nearby · ${keybindings.gather.toUpperCase()}`,herb_patch:`Use: animal breeding\nGather: stand nearby · ${keybindings.gather.toUpperCase()}`}[item.kind];
    if(item.kind==='rock'&&(world.shared_map||viewed===identity?.town_id)){
      const own=world.towns[identity?.town_id],progress=own?localStoryProgress(own,'Flint'):{};
      if(progress.story_stage===1){
        const recorded=progress.story_rocks?.includes(`${item.x}:${item.y}`),button=document.createElement('button');
        button.textContent=recorded?'Outcrop markings recorded':`Examine markings · ${progress.story_rocks?.length || 0} / 3`;
        button.disabled=recorded||(progress.story_rocks?.length || 0)>=3||!own.player||Math.hypot(own.player.x-item.x,own.player.y-item.y)>1.8;
        button.onclick=()=>act({action:'inspect_story_rock',x:item.x,y:item.y});$('selection-details').append(button);
      }
    }
    if(item.kind==='tree'){
      $('selection-title').textContent=swamp?'Swamp willow':item.species==='ancient'?'Ancient tree':'Forest';
      p.textContent=`Chop: ${keybindings.chop.toUpperCase()} / nearby woodcutter\nWoodcutter reach: 10 tile steps\nFelling: leaves a stump, clears tile\nRegrowth: ${swamp||item.species==='ancient'?15:5} min · all ground\nRegrowth requires an empty tile`;
      if(swamp||item.species==='ancient')p.textContent+='\nChopping time: 5×';
    }
    if(item.kind==='wild_horse'){
      $('selection-title').textContent='Wild horse';p.textContent='Lead this horse to a finished stable (hall 8). Feed it for five minutes to settle it. Two adults can breed; a saddle lets you ride one.';
      if(viewed===identity?.town_id){const button=document.createElement('button');button.textContent='Lead horse to stable';button.disabled=town.level<8||!!town.player.leading_horse||Math.abs(town.player.x-item.x)+Math.abs(town.player.y-item.y)>1.8||!town.buildings.some(b=>b.kind==='stable'&&!b.construction);button.onclick=()=>act({action:'lead_horse',x:item.x,y:item.y});$('selection-details').append(button);}
    }
    if(item.kind==='rock'&&item.ore){$('selection-title').textContent='Iron vein';p.textContent='A mine within 10 tile steps extracts iron ore. Smelt ore with wood into iron, then make steel, saddles, and trade crates.';}
    if(item.supply_max){$('selection-details').append(fact(`${Math.floor(item.supply || 0)} / ${item.supply_max}`,item.kind==='tree'?'Wood remaining before felling':'Shared supply'));if(item.kind!=='tree')$('selection-details').append(fact(`${item.regeneration} per 3 seconds`,'Regeneration'));}
  }else{
    $('selection-title').textContent=buildingName(item);
    const foldKey=`${world.world_id}/${viewed}/${selection.index}/${item.kind}/${item.x},${item.y}`;
    if(buildingFolds.key!==foldKey)buildingFolds={key:foldKey,open:{}};
    $('selection-art').append(illustration(item.kind));
    $('selection-level').hidden=false;$('selection-level').textContent=`Lv ${item.kind==='hall'?town.level:buildingLevel(item)}`;
    const workers=buildingSection(['hall','home'].includes(item.kind)?'Settlers':'Workers');
    const production=buildingSection(['hall','home','storehouse'].includes(item.kind)?'Benefits':'Production');
    const upgrades=buildingSection('Upgrades'),details=buildingSection('Building details',true),storageOverview=buildingSection('Storage');
    const boats=item.kind==='fishing_hut'?buildingSection('Fishing boats'):null;
    const unlockedUpgrades=buildingSection('Unlocked upgrades',true);
    const upgradeActions=document.createElement('div');upgradeActions.className='building-upgrade-actions';
    const assigned=town.villagers.filter((v,i)=>settlerBuilding(town,v,i)===selection.index);
    const status=document.createElement('p');status.className='workplace-status';
    const sourceWork=['woodcutter','farm','fishing_hut'].includes(item.kind);
    status.textContent=item.construction?'Under construction':town.upgrade_remaining&&item.kind==='hall'?`Upgrading · ${town.upgrade_remaining}s remaining`:
      sourceWork&&!buildingUsable(town,item)?'Needs a source in reach':
      ['hall','home'].includes(item.kind)?'Housing available':item.kind==='acre'?'Field ready':
      !assigned.length?'Needs workers':[...new Set(assigned.map(v=>v.production_status).filter(Boolean))].join(' · ') || `${assigned.length} assigned worker${assigned.length===1?'':'s'}.`;
    if(!item.construction)renderSettlers(town,item,selection.index,workers,upgradeActions,details);
    workers.append(status);

    if(item.kind==='hall'){
      production.append(fact(`${buildingRange(town)} tile steps`,'Building range'));
    }
    p.textContent={hall:'Progression: housing / food / professions\nLevels: 1–10\nUpgrade reward: coins',home:'No fixed population limit\nArrivals require free housing + food',storehouse:'Storage: enable resources\nHaulers: collect outputs + supply workplaces',woodcutter:'Output: wood\nRequires nearby trees',farm:'Output: crops\nAcres: within 3 tile steps\nWater: fetched by farmers',fishing_hut:'Output: fish\nRequires nearby water',acre:'Workers: farmers\nRequires farm within 3 tile steps\nWater: fetched by farmers'}[item.kind];
    if(item.kind==='hall'){
      p.textContent+=`\nBuild range: ${buildingRange(town)} tile steps\nDiagonal: 2 steps\nHall upgrade: +5 range`;
      const upgrade=world.hall_upgrades?.find(next=>next.level===town.level+1);
      $('hall-progress').hidden=true;
      const hallCosts=upgrade?Object.fromEntries(['wood','planks','leather','iron_ore','iron','steel'].filter(k=>k in upgrade).map(k=>[k,upgrade[k]])):{};
      const ready=world.hall_ready?.[viewed] || false;
      $('progress').textContent=upgrade?`Level ${town.level} · ${upgrade.milestone} · ${ready?'Complete':'Still needed'} · Reward: ${upgrade.coins} coins`:`Level ${town.level} · Town hall fully upgraded.`;
      $('upgrade').textContent='Lvl';
      $('upgrade').className='upgrade-button';
      $('upgrade').title=town.upgrade_remaining?`Upgrading to level ${town.level+1} · ${town.upgrade_remaining}s remaining`:upgrade?`Upgrade to level ${upgrade.level} · ${costText(hallCosts)} · ${upgrade.seconds} seconds`:'Maximum hall level reached';
      $('upgrade').setAttribute('aria-label',`Lvl: ${$('upgrade').title}`);
      $('upgrade').disabled=!!town.upgrade_remaining || !!item.construction || viewed!==identity?.town_id || !upgrade;
    }
    $('selection-move').hidden=viewed!==identity?.town_id;
    $('selection-move').textContent=town.coins?'Move building · 1 coin':'Move building · Free';
    $('selection-move').onclick=()=>{const index=selection.index;openPanel('build');tool='move';moveIndex=index;render();};
    $('selection-destroy').hidden=viewed!==identity?.town_id || item.kind==='hall';
    if(item.construction){
      const c=item.construction,stage=c.progress===0?'Waiting for a builder':c.upgrade?'Upgrading':c.progress<c.total/3?'Foundations':c.progress<c.total*2/3?'Framing':'Finishing';
      status.textContent=`${stage} · ${Math.floor(c.progress/c.total*100)}% complete · ${Math.ceil(c.total-c.progress)}s remaining of work`;
      p.textContent='Builders: priority\nIdle villagers: automatic help\nAssigned workers: help upgrade their workplace, then resume their jobs\nTime: excludes travel and waiting\nBenefits: on completion';
      const progress=document.createElement('progress');progress.max=c.total;progress.value=c.progress;progress.setAttribute('aria-label',`${labels[item.kind]} construction`);workers.append(progress);
    }
    if(!item.construction&&world.progression?.buildings[item.kind])p.textContent=productionDescription(item);
    details.append(p);
    renderProfession(town,item,selection.index,production,upgrades,details,boats,unlockedUpgrades);
    if(!item.construction&&item.kind!=='home'){
      renderStorage(town,item,selection.index,storageOverview);
    }
    if(costs[item.kind]){
      const level=buildingLevel(item),cost=costs[item.kind]*level,maxLevel=item.kind==='storehouse'?5:3;
      if(!item.construction)production.append(fact(upgradeBenefit(item.kind,level,item),'Current benefit'));
      if(item.kind==='farm'||item.kind==='acre'){
        const note=document.createElement('p');note.textContent='Field bonus = farm + acre';details.append(note);
      }
      const button=document.createElement('button');
      button.className='upgrade-button';
      button.textContent='Lvl';
      button.title=level>=maxLevel?`Fully upgraded · Level ${maxLevel}${item.kind==='storehouse'?' · Warehouse':''}`:`Upgrade to level ${level+1} · ${cost} wood · ${30*level} seconds · ${upgradeBenefit(item.kind,level+1,item)}`;
      button.setAttribute('aria-label',`Lvl: ${button.title}`);
      button.disabled=!!item.construction||level>=maxLevel||viewed!==identity?.town_id;
      const index=selection.index;
      button.onclick=()=>{if(!button.disabled&&viewed===identity?.town_id)showUpgrade({action:'upgrade_building',index});};
      // Keep the level action before the capacity action.
      const capacityUpgrade=upgradeActions.children[0];
      upgradeActions.replaceChildren();upgradeActions.append(upgradeCard(button,upgradePreview({action:'upgrade_building',index},town,item),{title:level>=maxLevel?`Level ${level}`:`Upgrade to level ${level+1}`,status:level>=maxLevel?'Fully upgraded':'',benefits:level>=maxLevel?[upgradeBenefit(item.kind,level,item)]:undefined}));
      if(capacityUpgrade)upgradeActions.append(capacityUpgrade);
    }
    if(item.kind==='farm'&&!item.construction&&viewed===identity?.town_id){
      const button=document.createElement('button');button.textContent='Build acre · 5 wood';button.disabled=town.wood<costs.acre;
      button.onclick=()=>{acreFarmIndex=selection.index;openPanel('build');tool='acre';moveIndex=null;buildTile=null;hoverTile=null;render();};
      production.append(button);
    }
    if(!item.construction&&item.kind!=='home')$('selection-details').append(storageOverview);
    $('selection-details').append(workers);
    if(production.children.length>1)$('selection-details').append(production);
    if(boats?.children.length>1)$('selection-details').append(boats);
    if(item.kind==='hall'){
      const preview=upgradePreview({action:'upgrade'},town,item);
      upgradeActions.append(upgradeCard($('upgrade'),preview,{title:preview?`Upgrade to level ${town.level+1}`:`Town hall · Level ${town.level}`,status:town.upgrade_remaining?`Upgrading · ${town.upgrade_remaining}s remaining`:preview?'':'Town hall fully upgraded.'}));
    }
    if(upgradeActions.children.length)upgrades.append(upgradeActions);
    if(upgrades.children.length>1)$('selection-details').append(upgrades);
    if(unlockedUpgrades.children.length>1)$('selection-details').append(unlockedUpgrades);
  }
  if(selection.type==='resource')$('selection-details').append(p);
  if(marketFocus&&$('selection-details').contains?.(marketFocus))marketFocus.focus();
}
const buildingCategories=[
  ['Housing & storage',['home','storehouse']],
  ['Food & farming',['farm','fishing_hut','barn','fish_pond','orchard','mill','smokehouse','bakery','butcher']],
  ['Crafting',['woodcutter','sawpit','workshop','tannery','saddler']],
  ['Mining & metal',['mine','smelter','steelworks']],
  ['Horses',['stable']],
  ['Trade',['market','tradehouse']],
  ['Paths',['road']],
];
for(const [index,[label,kinds]] of buildingCategories.entries()){
  const group=document.createElement('div');group.className='building-category';group.setAttribute('role','group');group.setAttribute('aria-labelledby',`building-category-${index}`);
  const heading=document.createElement('h3');heading.id=`building-category-${index}`;heading.textContent=label;group.append(heading);
  for(const [kind,cost] of Object.entries(costs).filter(([kind])=>kinds.includes(kind))){const button=document.createElement('button');button.buildingKind=kind;button.textContent=`${labels[kind]} · ${cost} wood`;button.append(illustration(kind));button.onclick=()=>{panel='build';selection=null;tool=kind;moveIndex=null;buildTile=null;hoverTile=null;render();};group.append(button);}
  $('buildings').append(group);
}
$('move').onclick=()=>{panel='build';selection=null;tool='move';moveIndex=null;buildTile=null;hoverTile=null;render();};
$('cancel').onclick=()=>{tool=null;moveIndex=null;buildTile=null;hoverTile=null;render();};
async function buildAt(x,y){
  const town=world.towns[viewed];
  if(building || !(tool in costs) || viewed!==identity?.town_id || !town || town.wood<costs[tool])return;
  const selected={x,y},kind=tool;
  if(!insideBuildingRange(town,selected)){notice(`Build within ${buildingRange(town)} tile steps of your town hall.`);return;}
  if(kind==='acre'&&!acrePlacementValid(town,selected,acreFarmIndex)){notice('Place acres within 3 tile steps of their farm.');return;}
  building=true;render();
  try{
    await request('/api/command',{action:kind==='road'?'build_road':'build',kind,...selected,...(kind==='acre'?{farm_index:acreFarmIndex}:{}),world_id:world.world_id});
    commandEffect({action:'build',...selected});
    if(tool===kind&&buildTile?.x===x&&buildTile?.y===y){buildTile=null;hoverTile=null;}
    await refresh();
  }catch(e){notice(e.message);await refresh();}
  finally{building=false;render();}
}
$('upgrade').onclick=()=>{if(!$('upgrade').disabled)showUpgrade({action:'upgrade'});};
$('selection-destroy').onclick=async()=>{
  const town=world.towns[viewed],selected=selection,button=$('selection-destroy');
  if(button.disabled || viewed!==identity?.town_id || selected?.type!=='building'
    || !town?.buildings[selected.index] || town.buildings[selected.index].kind==='hall')return;
  button.disabled=true;
  try{
    await request('/api/command',{action:'destroy',index:selected.index,world_id:world.world_id});
    commandEffect({action:'destroy',index:selected.index});
    if(selection===selected)openPanel(null);
    await refresh();
  }catch(e){notice(e.message);await refresh();}
  finally{button.disabled=false;}
};
// Short lived feedback follows successful commands; completed upgrades also reach visitors.
const buildingEffects=[];
let buildingHover=null;
function addBuildingEffect(tile,kind,townId=identity?.town_id){
  if(tile)buildingEffects.push({x:tile.x,y:tile.y,kind,townId,age:0});
}
function commandEffect(data){
  const town=world.towns[identity?.town_id];
  const index=['assign_settler','remove_settler','job'].includes(data.action)?data.building_index ?? town?.villagers[data.index]?.building_index:data.index;
  const tile=town?.buildings[index] || town?.buildings.find(b=>b.kind===(data.action==='sell_goods'?'market':'hall'));
  const kind={build:'build',destroy:'dust',move:'move',upgrade:'build',upgrade_building:'build',upgrade_capacity:'build',profession_upgrade:'build',sell_goods:'coins',buy_animal:'heart',breed_animal:'heart',set_crop:'leaf',assign_settler:'people',remove_settler:'people',job:'people'}[data.action];
  if(!kind)return;
  if(data.action==='move')addBuildingEffect(tile,'dust');
  addBuildingEffect(data.action==='build'||data.action==='move'?data:tile,kind);
}
function updateBuildingEffects(next){
  if(next.world_id!==world.world_id){buildingEffects.length=0;buildingHover=null;return;}
  for(const [id,town] of Object.entries(next.towns)){
    const before=world.towns[id]?.buildings || [];
    for(const building of town.buildings || []){
      const old=before.find(b=>b.kind===building.kind&&b.x===building.x&&b.y===building.y);
      if(old?.construction&&!building.construction)addBuildingEffect(building,'complete',id);
    }
  }
}
function drawBuildingEffects(seconds){
  for(let i=buildingEffects.length-1;i>=0;i--){
    const effect=buildingEffects[i];if(typeof ClientScene==='undefined')effect.age+=seconds;
    if(effect.age>=1.2){buildingEffects.splice(i,1);continue;}
    if(!world.shared_map&&effect.townId!==viewed)continue;
    const [x,y]=point(effect.x,effect.y),scale=tileW/46,t=effect.age/1.2;
    ctx.save();ctx.translate(x,y+tileH/2);ctx.scale(scale,scale);ctx.globalAlpha=1-t;
    if(['build','move','complete'].includes(effect.kind)){
      ctx.strokeStyle=effect.kind==='complete'?'#fff0b1':'#9fd8e5';ctx.lineWidth=2;
      ctx.beginPath();ctx.ellipse(0,0,10+t*18,5+t*9,0,0,Math.PI*2);ctx.stroke();
    }
    ctx.fillStyle=effect.kind==='dust'?'#b69a70':effect.kind==='leaf'?'#8db568':'#efd59a';
    for(let n=0;n<6;n++){
      const dx=Math.cos(n*Math.PI/3)*(5+t*15),dy=-t*24+Math.sin(n*Math.PI/3)*5;
      if(effect.kind==='coins'){ctx.beginPath();ctx.arc(dx,dy,3,0,Math.PI*2);ctx.fill();}
      else if(effect.kind==='heart'){polygon([[dx,dy+3],[dx-3,dy],[dx-3,dy-2],[dx,dy-1],[dx+3,dy-2],[dx+3,dy]],'#df927c');}
      else if(effect.kind==='people'){ctx.beginPath();ctx.arc(dx,dy-2,2,0,Math.PI*2);ctx.fill();ctx.fillRect(dx-2,dy,4,4);}
      else ctx.fillRect(dx,dy,3,3);
    }
    const caption={build:'Building',move:'Moved',complete:'Complete',coins:'Sold',heart:'Animals',leaf:'Crop changed',people:'Workers updated'}[effect.kind];
    if(caption){ctx.font='bold 10px sans-serif';ctx.textAlign='center';ctx.fillText(caption,0,-30-t*12);}
    ctx.restore();
  }
}
function departureMessage(town){
  const departed=town.last_departure;
  if(!departed)return '';
  const role=departed.job==='idle'?'settler':departed.job==='baking'?'baker':departed.job==='butchery'?'butcher':departed.job==='building'?'builder':departed.job;
  const practice=Object.entries(departed.practice || {}).map(([job,xp])=>`${Math.floor(xp)} ${job} practice`).join(', ');
  const reason=departed.reason || 'food';
  return `${departed.name}, your ${role}, left after prolonged ${reason} shortages. ${practice?`Lost ${practice}. `:''}Their experience is gone; their work needs a new settler.`;
}
function updateDepartures(next){
  if(next.world_id!==world.world_id)return;
  for(const [id,town] of Object.entries(next.towns)){
    const previous=world.towns[id];
    if(!previous || (town.departure_count || 0)<=(previous.departure_count || 0))continue;
    if(viewed===id&&selection?.type==='villager')selection=null;
    if(id===identity?.town_id)notice(departureMessage(town));
  }
}
async function refresh(){
  if(refreshing)return refreshing;
  refreshing=(async()=>{
    const keys=mapChunkKeys(),view=keys.join(';');
    const query=world.shared_map?.chunked?'?'+new URLSearchParams({chunks:view,world_id:world.world_id,
      known:chunkTerrainWorld===world.world_id?[...chunkTerrainCache.keys()].join(';'):'',
      resources:chunkTerrainWorld===world.world_id?[...chunkResourceCache].map(([key,update])=>`${key}:${update.version}`).join(';'):''}):'';
    const next=await request('/api/world'+query);
    const selectedResource=next.world_id===world.world_id&&next.shared_map?.chunked&&selection?.type==='resource'
      ?world.shared_map?.map_resources?.[selection.index]:null;
    requestedMapView=next.world_id===world.world_id?view:null;
    if(next.world_id!==world.world_id){tool=null;moveIndex=null;buildTile=null;hoverTile=null;viewed=null;selection=null;panel=null;keyboardTile=null;}
    hydrateMapChunks(next);
    if(selectedResource){
      const index=next.shared_map.map_resources.findIndex(r=>r.kind===selectedResource.kind&&r.x===selectedResource.x&&r.y===selectedResource.y);
      selection=index<0?null:{...selection,index};
    }
    if(next.shared_map)for(const town of Object.values(next.towns))Object.assign(town,next.shared_map);
    updateBuildingEffects(next);
    updateDepartures(next);
    if(typeof ClientScene!=='undefined'){
      const playerRefs=new Map(Object.entries(world.towns).map(([id,t])=>[id,t.player]));
      const preparation=(function*(){
        const scene=yield* ClientScene.prepare(next);
        scene.resourceData=yield* prepareResourceData(next.shared_map?.map_resources || next.towns[viewed || identities[next.world_id]?.town_id || Object.keys(next.towns)[0]]?.map_resources || [],tileW>=32);
        return scene;
      })();let result;
      do{
        const started=performance.now();
        do{result=preparation.next();}while(!result.done&&performance.now()-started<2);
        if(!result.done)await new Promise(resolve=>setTimeout(resolve,0));
      }while(!result.done);
      // Remote samples arriving while preparation yields belong to the current world.
      if(next.world_id===world.world_id)for(const [id,t] of Object.entries(next.towns)){
        const live=world.towns[id]?.player;
        if(live&&live!==playerRefs.get(id))t.player={...t.player,x:live.x,y:live.y,facing:live.facing};
      }
      ClientScene.commit(result.value);resourceCache=result.value.resourceData;
    }
    world=next;frameActors=null;
    const savedIdentity=identities[world.world_id];
    identity=savedIdentity?.token && world.towns[savedIdentity.town_id]?savedIdentity:null;
    // Keep characters moving even if the lightweight endpoint is unavailable.
    updateRemotePlayers({world_id:world.world_id,sampled_at:world.players_sampled_at,
      players:Object.fromEntries(Object.entries(world.towns).filter(([,town])=>town.player).map(([id,town])=>[id,town.player]))});
    catalog=await request('/api/worlds');
    if(!world.towns[viewed]){viewed=identity?.town_id || Object.keys(world.towns)[0];selection=null;panel=null;tool=null;moveIndex=null;buildTile=null;hoverTile=null;keyboardTile=null;}
    render();renderWorldMenu();
    if(!world.world_id){if(!$('world-menu').open)$('world-menu').showModal();menuStarted=true;}
  })();
  try{await refreshing;}finally{refreshing=null;}
}
function render(){
  if(typeof renderQuarry==='function')renderQuarry();
  renderUpgradeDialog();
  const own=world.towns[identity?.town_id], town=world.towns[viewed];
  draw();
  renderSelection(town);
  $('join').hidden=true;
  if(world.world_id&&!own){if(!$('login-dialog').open)$('login-dialog').showModal();}
  else if($('login-dialog').open)$('login-dialog').close();
  $('controls').hidden=panel!=='build' || !own || viewed!==identity?.town_id;
  for(const name of ['selection','villagers','valley','help','resources','items','rules'])$(name+'-panel').hidden=panel!==name;
  $('side-panel').hidden=!panel && $('join').hidden;
  $('side-panel').classList.toggle('villagers-overview',panel==='villagers');
  $('game').classList.toggle('panel-open',!$('side-panel').hidden);
  for(const name of ['build','villagers','valley','help','resources','items','rules']){
    $(name+'-button').setAttribute('aria-expanded',String(panel===name));
    $(name+'-button').classList.toggle('selected',panel===name);
  }
  $('build-button').disabled=!own || (!world.shared_map && viewed!==identity?.town_id);
  $('villagers-button').disabled=!town;
  $('resources-button').disabled=!own;
  $('items-button').disabled=!own;
  $('rules-button').disabled=!own;
  if(panel==='rules')renderRules(own);
  if(panel==='items')renderItems(own);
  if(panel==='resources')renderResourceList(own);
  $('move').textContent=own?.coins?'Move a building · 1 coin':'Move a building · Free';
  $('mode-hint').textContent=tool==='move'?(moveIndex===null?'Moving: click a building. Escape to cancel.':'Moving: click open grass to move here. Escape to cancel.'):tool?`Building ${labels[tool].toLowerCase()}: hover over open grass and left-click to build. Escape to cancel.`:'';
  $('view-title').textContent=(world.shared_map?world.name:town?.name) || 'A place to call home';
  if(!tool&&own?.food_shortage_seconds)$('mode-hint').textContent='Food shortage! Feed your settlers soon or they may leave. Open Villagers for details.';
  if(!tool&&own?.housing_shortage_seconds)$('mode-hint').textContent='Housing shortage! Build or upgrade cottages soon or settlers may leave. Open Villagers for details.';
  $('mode-hint').hidden=!$('mode-hint').textContent;
  $('resources-empty').hidden=!!own;$('resource-totals').hidden=!own;
  if(own){
    $('resource-wood').textContent=Math.floor(own.wood);
    $('resource-food').textContent=Math.floor(own.food);
    $('resource-coins').textContent=Math.floor(own.coins);
    $('resource-villagers').textContent=`${own.villagers.length}/${housingCapacity(own)}`;
    $('resource-villagers').title='Current villagers / total housing capacity';
    $('goods-list').replaceChildren();
    for(const [key,value] of Object.entries(own.goods || {}))if(value>0)$('goods-list').append(fact(`${Math.floor(value)} / ${Math.floor(resourceStorage(own,key))}`,goodName(key)));
  }
  if(buildTile && (!town || !insideBuildingRange(town,buildTile) || town.buildings.some(b=>b.x===buildTile.x&&b.y===buildTile.y) || town.map_resources.some(r=>r.x===buildTile.x&&r.y===buildTile.y)))buildTile=null;
  if(typeof ClientScene==='undefined'||panel==='build')for(const group of $('buildings').children){
    for(const button of group.children){
      if(!button.buildingKind)continue;
      const kind=button.buildingKind,spec=world.progression?.buildings[kind] || {wood:costs[kind],level:1};
      button.hidden=!own || own.level<spec.level;
      button.classList.toggle('selected',kind===tool);button.disabled=!own || own.level<spec.level || !canAfford(own,materialCosts(spec));
      button.textContent=`${labels[kind]} · ${kind==='road'?'Free · 10s per tile · +50% walking':costText(materialCosts(spec))}${own&&own.level<spec.level?` · Hall ${spec.level}`:''}`;button.append(illustration(kind));
    }
    group.hidden=![...group.children].some(button=>button.buildingKind&&!button.hidden);
  }
  if(own){$('town-name').textContent=own.name;$('hint').textContent=tool==='move'?(moveIndex===null?'Click a building to move it.':'Click open grass.'):tool?(building?'Building…':own.wood<costs[tool]?'Not enough wood.':`Hover over open grass, then left-click to build a ${labels[tool].toLowerCase()}.`):'Choose a building, then hover over an open tile.';}
  if(typeof ClientScene==='undefined'||panel==='valley'){
  $('towns').replaceChildren();
  for(const village of world.shared_map?.villages || []){
    const button=document.createElement('button');
    button.textContent=`${village.name} · ${village.biome==='b'?'Swamp':village.biome==='d'?'Desert':'Ice'} village`;
    button.onclick=()=>{setCameraLock(false);camera.x=village.x;camera.y=village.y;tool=null;selection=null;panel=null;keyboardTile=null;render();};
    $('towns').append(button);
  }
  for(const [id,t] of Object.entries(world.towns)){const b=document.createElement('button');b.textContent=t.name+(id===identity?.town_id?' · Yours':'');b.classList.toggle('selected',id===viewed);b.onclick=()=>{viewed=id;if(world.shared_map){const hall=t.buildings.find(b=>b.kind==='hall');camera.x=hall.x;camera.y=hall.y;}tool=null;moveIndex=null;buildTile=null;hoverTile=null;selection=null;panel=null;keyboardTile=null;render();};$('towns').append(b);}
  }
  if(typeof ClientScene==='undefined'||panel==='villagers'){
  $('villagers').replaceChildren();
  if(town&&!town.villagers.length){const empty=document.createElement('p');empty.textContent='No villagers yet. Build housing and keep food available to welcome new residents.';$('villagers').append(empty);}
  if(town?.villagers.length)$('villagers').append(villagersTable(town));
  if(town){
    const spaces=housingCapacity(town);
    const housing=document.createElement('p');housing.textContent=`Housing · ${town.villagers.length} residents / ${spaces} spaces. New settlers need a free space and at least 20 food after meals; each arrival uses 10 food.`;$('villagers').append(housing);
  }
  if(town?.food_shortage_seconds){
    const warning=document.createElement('p');warning.textContent=`Food shortage · ${town.food_shortage_seconds}s without enough food. After 60 seconds, settlers may leave with all their skills. Upgrade cottages to reduce the risk.`;$('villagers').append(warning);
  }
  if(town?.housing_shortage_seconds){
    const warning=document.createElement('p');warning.textContent=`Housing shortage · ${town.housing_shortage_seconds}s without enough housing. After 60 seconds, settlers may leave with all their skills. Build or upgrade cottages.`;$('villagers').append(warning);
  }
  if(town?.last_departure){const loss=document.createElement('p');loss.textContent=departureMessage(town);$('villagers').append(loss);}
  }
  draw();
}
function point(x,y){return [originX+(x-y)*tileW/2,originY+(x+y)*tileH/2];}
function polygon(points,fill,stroke){ctx.beginPath();points.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));ctx.closePath();ctx.fillStyle=fill;ctx.fill();if(stroke){ctx.strokeStyle=stroke;ctx.stroke();}}
// Share static resource artwork across tiles and frames. Resolution buckets keep
// zooming sharp without generating a new canvas for every wheel event.
const resourceSprites=new Map();
const resourceSpriteLimit=512,resourceSpriteByteLimit=64*1024*1024;
let resourceSpriteBytes=0;
// Coordinates keep formations stable in existing saves and streamed chunks.
function rockVariant(r){
  const hash=(Math.imul(r.x,73856093)^Math.imul(r.y,19349663))>>>0;
  return (hash^(hash>>>16))>>>0;
}
const rockFormations=[
  [[-2,1,25,50],[20,13,6,9]],           // Large boulder with a small stone.
  [[-4,3,23,36],[19,12,7,11]],          // Broad, low boulder.
  [[-6,1,20,44],[19,11,8,14]],          // Medium boulder with a loose stone.
  [[-10,8,9,14],[9,11,7,10],[0,17,4,6]], // Small scattered stones.
  [[-3,5,17,26],[19,14,5,7]],           // Mid-sized rock and a pebble.
  [[-2,12,7,10],[12,16,4,6]]            // Small rock and a pebble.
];
function paintRock(ctx,r,detailed,polygon,joined=null){
  const variant=rockVariant(r)%48,formation=rockFormations[variant%rockFormations.length];
  let seed=variant+1;
  const random=()=>{
    seed=(seed+0x6d2b79f5)>>>0;
    let value=Math.imul(seed^(seed>>>15),seed|1);
    value^=value+Math.imul(value^(value>>>7),value|61);
    return ((value^(value>>>14))>>>0)/4294967296;
  };
  const colors=!joined&&Math.floor(variant/6)%2?['#999889','#d4cbb2','#b7b09b','#686f69','#83877b']
    :['#96958a','#d6ceb9','#b8b3a2','#626c69','#7f857c'];
  if(joined){
    const ground=[[0,0],[23,11.5],[0,23],[-23,11.5]].map(([x,y],i)=>
      [x+(joined.inset?.[i][0] || 0)*.6,y+(joined.inset?.[i][1] || 0)*.6]);
    const rim=ground.map(([x,y],i)=>[x+(joined.inset?.[i][0] || 0)*.4,y+(joined.inset?.[i][1] || 0)*.4]);
    const top=rim.map(([x,y],i)=>[x,y-joined.heights[i]]);
    // A single shadow path avoids dark overlaps at corners. The light comes
    // from the upper left, like the village buildings.
    if(detailed&&(joined.mask&6)!==6){
      ctx.beginPath();
      for(const [a,b,bit] of [[1,2,2],[2,3,4]]){
        if(joined.mask&bit)continue;
        const points=[ground[a],ground[b],[ground[b][0]+5,ground[b][1]+3],[ground[a][0]+5,ground[a][1]+3]];
        points.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));ctx.closePath();
      }
      ctx.fillStyle='#223b2940';ctx.fill();
    }
    if(!(joined.mask&2))polygon([top[1],ground[1],ground[2],top[2]],colors[3]);
    if(!(joined.mask&4))polygon([top[2],ground[2],ground[3],top[3]],colors[4]);
    const center=[rim.reduce((sum,p)=>sum+p[0],0)/4,rim.reduce((sum,p)=>sum+p[1],0)/4];
    const height=joined.heights.reduce((sum,h)=>sum+h,0)/4;
    const peak=[center[0],center[1]-height];
    // Cover the top first so antialiasing cannot expose grass between facets.
    polygon(top,colors[0]);
    // Shade the actual slopes instead of painting a repeated tile pattern.
    for(let i=0;i<4;i++){
      const j=(i+1)%4;
      const a=[rim[i][0]-center[0],(rim[i][1]-center[1])*2,joined.heights[i]-height];
      const b=[rim[j][0]-center[0],(rim[j][1]-center[1])*2,joined.heights[j]-height];
      let normal=[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
      if(normal[2]<0)normal=normal.map(n=>-n);
      const light=Math.max(0,(-.55*normal[0]-.65*normal[1]+normal[2])/(Math.hypot(...normal)*1.313));
      const shade=[106,114,105].map((dark,k)=>Math.round(dark+([214,206,185][k]-dark)*light));
      polygon([top[i],top[j],peak],`rgb(${shade.join(',')})`);
    }
    return [peak[0],peak[1]+3];
  }
  const stones=formation.map(([baseX,baseY,baseW,baseH])=>{
    const w=baseW*(.75+random()*.32),h=Math.min(baseH*(.7+random()*.3),w*1.65);
    const x=Math.max(-29+w,Math.min(29-w,baseX+(random()-.5)*7));
    const y=Math.min(19,baseY+(random()-.5)*4);
    return {x,y,w,h};
  });
  if(detailed){
    ctx.fillStyle='#223b2940';
    for(const {x,y,w} of stones){
      ctx.beginPath();ctx.ellipse(x+w*.12,y+3,w*.85,Math.max(1,w*.2),0,0,Math.PI*2);ctx.fill();
    }
  }
  for(const {x,y,w,h} of stones){
    // Wide shoulders and a short, flat crown replace the pointed outcrops.
    const left=[x-w,y-h*(.22+random()*.2)],shoulder=[x-w*(.45+random()*.3),y-h*(.72+random()*.2)];
    const crown=[x+w*(random()*.45-.15),y-h],right=[x+w*(.6+random()*.25),y-h*(.55+random()*.25)];
    const edge=[x+w,y-h*.12],foot=[x+w*.45,y+4],base=[x-w*.65,y+3];
    const seam=[x+w*(random()*.3-.2),y-h*(.3+random()*.2)];
    polygon([left,shoulder,crown,right,edge,foot,base],colors[0]);
    polygon([left,shoulder,crown,seam],colors[1]);
    polygon([seam,crown,right,edge,foot],colors[3]);
    const moss=random()<.35;
    if(!detailed)continue;
    polygon([crown,right,[x+w*.42,y-h*.35],seam],colors[2]);
    polygon([left,seam,foot,base],colors[4]);
    if(w>=17&&moss){
      polygon([shoulder,[x-w*.3,y-h*.9],[x-w*.12,y-h*.7],
        [x-w*.4,y-h*.63],[x-w*.72,y-h*.58]],'#819057');
      polygon([shoulder,[x-w*.3,y-h*.9],[x-w*.42,y-h*.77]],'#a0aa69');
    }
  }
  return [stones[0].x,stones[0].y-stones[0].h*.5];
}
function wildHorsePosition(r,now=Date.now()/1000){
  const progress=Math.max(0,Math.min(1,(now-(r.wander_started || 0))/3));
  if(!r.wander_from)return [r.x,r.y,1];
  return [r.wander_from[0]+(r.x-r.wander_from[0])*progress,
    r.wander_from[1]+(r.y-r.wander_from[1])*progress,progress];
}
function drawResource(r,joined=null){
  const position=r.kind==='wild_horse'?wildHorsePosition(r):[r.x,r.y];
  const [x,y]=point(position[0],position[1]),s=tileW/46;
  if(r.kind==='wild_horse'){
    ctx.save();ctx.translate(x,y);ctx.scale(s,s);
    if(r.wander_from&&r.x-r.wander_from[0]-(r.y-r.wander_from[1])<0)ctx.scale(-1,1);
    paintHorse(ctx,position[2]<1?Math.sin(position[2]*Math.PI*8):0);
    ctx.restore();return;
  }
  const detailed=tileW>=32;
  const willow=r.kind==='tree'&&(world.shared_map || world.towns[viewed])?.terrain?.[r.y]?.[r.x]==='b';
  const resolution=2**Math.ceil(Math.log2(Math.max(.5,s*renderPixelRatio())));
  const variant=r.kind==='tree'?`${r.species==='ancient'}/${r.species==='palm'}/${willow}/${(r.x+r.y)%2}`:r.kind==='rock'?(joined?`joined/${JSON.stringify([joined.mask,joined.heights,joined.inset || null])}`:`single/${rockVariant(r)%48}`):`${r.kind}/${r.biome || ''}/${r.variant || ''}`;
  const key=`${r.kind}/${r.ore || ''}/${variant}/${resolution}/${detailed}`;
  let sprite=resourceSprites.get(key);
  if(!sprite){
    const bytes=60*112*resolution*resolution*4;
    // Oversized zoom textures use direct drawing instead of exceeding the budget.
    if(bytes>resourceSpriteByteLimit){
      ctx.save();ctx.translate(x,y);ctx.scale(s,s);paintResource(ctx,r,willow,detailed,joined);ctx.restore();return;
    }
    while(resourceSprites.size>=resourceSpriteLimit || resourceSpriteBytes+bytes>resourceSpriteByteLimit){
      const oldest=resourceSprites.keys().next().value,evicted=resourceSprites.get(oldest);
      resourceSpriteBytes-=evicted.width*evicted.height*4;
      resourceSprites.delete(oldest);
    }
    const surface=document.createElement('canvas'),target=surface.getContext?.('2d');
    if(!target){
      ctx.save();ctx.translate(x,y);ctx.scale(s,s);paintResource(ctx,r,willow,detailed,joined);ctx.restore();return;
    }
    // Include ancient canopies, willow branches, and the ground shadow.
    surface.width=60*resolution;surface.height=112*resolution;
    target.setTransform(resolution,0,0,resolution,30*resolution,80*resolution);
    paintResource(target,r,willow,detailed,joined);
    sprite=surface;resourceSprites.set(key,sprite);
    resourceSpriteBytes+=bytes;
  }else{
    resourceSprites.delete(key);resourceSprites.set(key,sprite);
  }
  ctx.drawImage(sprite,x-30*s,y-80*s,60*s,112*s);
}
function paintResource(ctx,r,willow,detailed=true,joined=null){
  const polygon=(points,fill,stroke)=>{
    ctx.beginPath();points.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));
    ctx.closePath();ctx.fillStyle=fill;ctx.fill();
    if(stroke){ctx.strokeStyle=stroke;ctx.stroke();}
  };
  ctx.save();
  if(r.kind==='village_hut'||r.kind==='village_market'){
    const ice=r.biome==='i',desert=r.biome==='d',market=r.kind==='village_market';
    const wall=ice?'#b8d0d6':desert?'#e0bd80':'#9c8159',roof=ice?'#edf6f8':desert?'#c29157':'#647951';
    if(!ice&&!desert){
      ctx.fillStyle='#5e513c';ctx.fillRect(-15,1,4,13);ctx.fillRect(11,1,4,13);
      polygon([[-21,7],[0,-3],[21,7],[0,18]],'#a78e67');
    }
    polygon([[-18,4],[0,14],[0,-20],[-18,-30]],wall);
    polygon([[0,14],[18,4],[18,-30],[0,-20]],ice?'#87aab7':desert?'#b68d59':'#746044');
    polygon([[-22,-30],[0,-47],[22,-30],[0,-17]],roof,'#56625e');
    ctx.fillStyle='#4b514a';ctx.fillRect(-10,-16,6,18);
    ctx.fillStyle='#f4d58c';ctx.fillRect(5,-13,6,7);
    if(r.variant==='hall'){
      polygon([[-27,-31],[0,-56],[27,-31],[0,-14]],roof,'#56625e');
      ctx.fillStyle='#75593e';ctx.fillRect(-1,-67,2,13);
      polygon([[1,-67],[13,-63],[1,-59]],ice?'#6595ac':desert?'#c47853':'#a4b572');
      ctx.fillStyle='#f4d58c';ctx.fillRect(-15,-22,5,7);
    }else if(r.variant==='barn'){
      ctx.fillStyle='#65513b';ctx.fillRect(-12,-19,10,24);
      polygon([[-12,-19],[-2,-19],[-2,5],[-12,5]],'#806447','#e0c79d');
      ctx.strokeStyle='#e0c79d';ctx.beginPath();ctx.moveTo(-12,-19);ctx.lineTo(-2,5);ctx.moveTo(-2,-19);ctx.lineTo(-12,5);ctx.stroke();
    }else if(r.variant==='inn'){
      ctx.fillStyle=ice?'#7796a4':desert?'#a8784e':'#746044';ctx.fillRect(8,-48,5,15);
      ctx.fillStyle='#f4d58c';ctx.fillRect(-16,-25,5,5);
      ctx.fillStyle='#806240';ctx.fillRect(18,-12,2,19);
      ctx.fillStyle=ice?'#6595ac':desert?'#c47853':'#a4b572';ctx.fillRect(14,-12,10,7);
    }else if(r.variant==='workshop'){
      ctx.fillStyle='#76583d';ctx.fillRect(-23,7,12,4);
      ctx.fillStyle=ice?'#b8d0d6':desert?'#e0bd80':'#7b985e';
      ctx.fillRect(-21,2,4,5);ctx.fillRect(-16,3,4,4);
    }
    if(market){
      polygon([[-23,-10],[0,-1],[23,-10],[0,-20]],desert?'#c47853':ice?'#6595ac':'#a4b572');
      ctx.fillStyle='#654e37';ctx.fillRect(-17,3,34,5);
      ctx.fillStyle='#e2bd77';ctx.fillRect(-12,-1,7,4);ctx.fillStyle='#7d985e';ctx.fillRect(3,-1,7,4);
    }
    ctx.restore();return;
  }
  if(r.kind==='tree'&&r.species==='palm'){
    polygon([[-3,12],[3,12],[6,-30],[1,-31]],'#b28b5e','#806447');
    for(const [dx,dy] of [[-23,4],[-18,-9],[0,-18],[19,-11],[24,3],[10,12]]){
      polygon([[3,-31],[dx,-31+dy],[dx*.65,-25+dy],[3,-27]],dx<0?'#65894f':'#4f7745');
    }
    ctx.fillStyle='#997049';ctx.beginPath();ctx.arc(3,-28,3,0,Math.PI*2);ctx.fill();
    ctx.restore();return;
  }
  if(r.kind==='rock'){
    const [oreX,oreY]=paintRock(ctx,r,detailed,polygon,joined);
    if(r.ore&&detailed){
      ctx.fillStyle='#ad8769';
      for(const [x,y] of [[-3,0],[1,-2],[4,1]]){
        ctx.beginPath();ctx.arc(oreX+x,oreY+y,1.2,0,Math.PI*2);ctx.fill();
      }
    }
    ctx.restore();return;
  }
  if(r.kind==='wild_horse'){paintHorse(ctx,0);ctx.restore();return;}
  if(!detailed&&r.kind!=='chest'){
    if(r.kind==='tree'){
      if(r.species==='ancient')ctx.scale(1.35,1.35);
      ctx.fillStyle='#775b3e';ctx.fillRect(-3,-14,6,27);
      if(willow){ctx.fillStyle='#64784c';ctx.beginPath();ctx.ellipse(0,-34,20,14,0,0,Math.PI*2);ctx.fill();}
      else polygon([[-17,-7],[0,-57],[17,-7],[0,0]],(r.x+r.y)%2?'#355b3e':'#3e6642');
    }else if(r.kind==='berry_bush'||r.kind==='herb_patch'){
      ctx.fillStyle=r.kind==='berry_bush'?'#587a45':'#7b9e60';ctx.beginPath();ctx.ellipse(0,2,12,8,0,0,Math.PI*2);ctx.fill();
      ctx.fillStyle=r.kind==='berry_bush'?'#b35b69':'#e2c8ea';ctx.fillRect(-3,-3,6,3);
    }else polygon([[-16,10],[-11,-4],[2,-10],[16,0],[13,12],[0,17]],'#89928a');
    ctx.restore();return;
  }
  ctx.fillStyle='#223b2955';ctx.beginPath();ctx.ellipse(0,12,16,6,0,0,Math.PI*2);ctx.fill();
  if(r.kind==='tree'){
    if(r.species==='ancient')ctx.scale(1.35,1.35);
    ctx.fillStyle='#775b3e';ctx.fillRect(-3,-14,6,27);
    if(willow){
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
  }else if(r.kind==='chest'){
    polygon([[-12,11],[-12,-3],[8,-8],[14,-2],[14,12],[0,17]],'#88603b','#d6b45e');
    polygon([[-12,-3],[0,2],[14,-2],[8,-8]],'#b1824b','#e1c376');
    ctx.fillStyle='#f5d46b';ctx.fillRect(-1,1,4,7);
  }else if(r.kind==='berry_bush'||r.kind==='herb_patch'){
    ctx.fillStyle=r.kind==='berry_bush'?'#587a45':'#7b9e60';ctx.beginPath();ctx.ellipse(0,2,12,8,0,0,Math.PI*2);ctx.fill();
    ctx.fillStyle=r.kind==='berry_bush'?'#b35b69':'#e2c8ea';for(let i=0;i<5;i++){ctx.beginPath();ctx.arc((i%3-1)*6,-3+Math.floor(i/3)*6,2,0,Math.PI*2);ctx.fill();}
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
  if(animatedVillagers.roadWorld!==world){
    animatedVillagers.roadWorld=world;animatedVillagers.roadSets=new Map();
  }
  const roadKey=world.shared_map?'shared':town;
  if(!animatedVillagers.roadSets.has(roadKey)){
    animatedVillagers.roadSets.set(roadKey,new Set((world.shared_map?Object.values(world.towns).flatMap(t=>t.roads || []):town.roads || []).filter(r=>!r.construction).map(r=>`${r.x},${r.y}`)));
  }
  const roads=animatedVillagers.roadSets.get(roadKey);
  const region=typeof ClientScene!=='undefined'?ClientScene.region(town):null;
  const tiles=items=>items.map(({kind,x,y,farm_index,construction})=>[kind,x,y,farm_index,!!construction]);
  const layout=region?.layoutVersion ?? frameLayout ?? JSON.stringify([tiles(world.shared_map?Object.values(world.towns).flatMap(t=>t.buildings):town.buildings),tiles(town.map_resources),[...roads].sort()]);
  // Names are unique within a town and survive arrivals/departures. Remap the
  // index-based rendering slots without restarting surviving residents.
  if(animatedVillagers.rosterWorld!==townKey){animatedVillagers.rosterWorld=townKey;animatedVillagers.rosters=new Map();}
  const names=town.villagers.map((v,i)=>v.name || i),roster=JSON.stringify(names);
  const previousRoster=animatedVillagers.rosters.get(townId);
  if(previousRoster&&previousRoster.signature!==roster){
    const key=i=>world.shared_map?`${townId}/${i}`:i;
    const retained=new Map();
    previousRoster.names.forEach((name,i)=>{
      const state=workers.get(key(i));
      if(typeof ClientScene!=='undefined'){ClientScene.queue.cancel('plan/'+key(i));ClientScene.queue.cancel('advance/'+key(i));}
      if(state){
        if(state.pendingPlan||state.pendingAdvance)state.signature=null;
        state.pendingPlan=null;state.pendingAdvance=false;retained.set(name,state);
      }
      workers.delete(key(i));
    });
    names.forEach((name,i)=>{if(retained.has(name))workers.set(key(i),retained.get(name));});
  }
  animatedVillagers.rosters.set(townId,{signature:roster,names});
  return town.villagers.map((villager,index)=>{
    const prepared=typeof ClientScene!=='undefined'?ClientScene.current?.tasks.get(villager):null;
    const task=prepared?.task ?? JSON.stringify([villager.job,villager.building_index,villager.work_target?tiles([villager.work_target]):null,
      villager.construction_target?tiles([villager.construction_target]):null,
      villager.job==='fishing'?[town.villagers.map(v=>[v.job,v.building_index]),town.buildings.filter(b=>b.kind==='fishing_hut').map(b=>[b.level,b.profession_upgrades])]:null]);
    const trip=villager.logistics_trip;
    // Polling advances the host's step; keep the visible trip running continuously.
    const signature=prepared?.signature ?? JSON.stringify([task,trip?[trip.id,trip.operation,trip.workplace,trip.storage,trip.path]:villager.logistics_target]);
    const workerKey=world.shared_map?`${townId}/${index}`:index;
    let state=workers.get(workerKey);
    const finishingDelivery=state?.transport&&state.path.length&&state.task===task&&state.layout===layout;
    if(!state || state.layout!==layout || (state.signature!==signature&&!finishingDelivery)){
      if(region){
        if(state?.pendingPlan?.signature!==signature||state?.pendingPlan?.layout!==layout){
          const version={signature,layout};
          ClientScene.queue.cancel('advance/'+workerKey);
          const previous=state;
          const home=town.buildings.find(b=>b.kind==='hall') || {x:5,y:5};
          const position=villager.work_animation || villager.logistics_position || villager.construction_position || {x:home.x-2+index%5,y:home.y+3+Math.floor(index/5)};
          state={...(state || {x:position.x,y:position.y,path:[],clock:0,phase:'waiting',facing:1}),path:[],pendingPlan:version,pendingAdvance:false};
          workers.set(workerKey,state);
          ClientScene.queue.schedule('plan/'+workerKey,version,WorkerAnimation.planSteps(region.routing,villager,index,previous),planned=>{
            if(workers.get(workerKey)!==state)return;
            const visible=state.playbackQueue?{x:state.x,y:state.y,path:state.path,phase:state.phase,elapsed:state.elapsed,clock:state.clock}:{};
            Object.assign(state,planned,visible,{signature,layout,task,pendingPlan:null,transport:!!planned.transport});
          });
        }
      }else{
        const visible=state?.playbackQueue?{x:state.x,y:state.y,path:state.path,phase:state.phase,elapsed:state.elapsed,clock:state.clock,
          playbackQueue:state.playbackQueue,hostVersion:state.hostVersion,hostTask:state.hostTask}:{};
        state=Object.assign(WorkerAnimation.plan(world.shared_map?{...town,roads:mapRoads(town),obstacles:Object.values(world.towns).flatMap(t=>t.buildings),home_hall:town.buildings.find(b=>b.kind==='hall')}:town,villager,index,state),visible,{signature,layout,task});workers.set(workerKey,state);
      }
    }
    state.roads=roads;
    if(state.transport&&trip?.cargo&&state.signature===signature)state.transportCargo=trip.cargo;
    state.storagePaused=!!villager.production_status?.startsWith('Waiting for storage space');
    const hostWork=villager.work_animation;
    const confirmedPlayback=hostWork?.playback?.length&&!state.transport&&!villager.logistics_target&&!villager.construction_target;
    if(!confirmedPlayback&&state.playbackQueue){
      state.playbackQueue=[];state.hostVersion=undefined;state.hostTask=null;state.hostWork=false;
    }
    if(confirmedPlayback){
      WorkerAnimation.playback(state,hostWork,seconds);
    }else if(hostWork&&!state.pendingPlan&&!state.pendingAdvance&&!state.transport&&!villager.logistics_target&&!villager.construction_target){
      if(state.hostVersion!==hostWork.version || state.hostTask!==JSON.stringify(hostWork.task)){
        Object.assign(state,{x:hostWork.x,y:hostWork.y,phase:hostWork.phase,
          elapsed:hostWork.phase==='collecting_water'?0:hostWork.elapsed,waterElapsed:hostWork.elapsed,
          waterReady:hostWork.waterReady,waterSource:hostWork.waterSource,facing:hostWork.facing,
          fishingCaught:null,
          path:hostWork.path.map(p=>({...p})),hostWork:true,hostVersion:hostWork.version,
          hostTask:JSON.stringify(hostWork.task)});
      }
    }
    const resource=villager.job==='woodcutting'?'wood':'food';
    if(state.boating&&!state.transport&&state.home?.kind==='fishing_hut'&&!hostWork){
      state.boatLoad=fishingBoatLoad(town,state.home);
      state.boatCapacity=fishingBoatCapacity(state.home);
      state.boatReturnRequested=!!villager.boat_return_requested;
    }
    state.latestFishingCatch=villager.fishing_catch || (hostWork?{caught:Object.values(villager.pending_output || {}).some(n=>n>0)}:null);
    const butchering=villager.job==='butchery'&&town.buildings[villager.building_index]?.butchering;
    state.workDuration=butchering?(world.progression?.butcher_seconds || 60):villager.job==='farming'?8:3.2;
    const active=!!villager.construction_target || Object.values(villager.pending_output || {}).some(n=>n>0) || (villager.production_status==='Working' || villager.production_status?.startsWith('Butchering ') || villager.production_status?.startsWith('No catch') || (!villager.production_status&&town[resource]<resourceStorage(town,resource)));
    if(confirmedPlayback){
      // Confirmed playback already advanced the sprite; never predict a work cycle.
    }else if(region){
      if(!state.pendingPlan&&!state.pendingAdvance){
        const steps=WorkerAnimation.advanceSteps(state,seconds,active),first=steps.next();
        if(!first.done){state.pendingAdvance=true;ClientScene.queue.schedule('advance/'+workerKey,state,steps,()=>{state.pendingAdvance=false;});}
      }
    }else WorkerAnimation.advance(state,seconds,active);
    return {kind:'villager',townId,villager,index,x:state.x,y:state.y,state,active};
  });
}
function townWorkerLayout(town,buildings,resources){
  const prepared=typeof ClientScene!=='undefined'?ClientScene.region(town):null;
  if(prepared)return prepared.layout;
  // Camera exploration outside a town's routing area cannot change its paths.
  if(townWorkerLayout.world!==world){townWorkerLayout.world=world;townWorkerLayout.layouts=new Map();}
  if(townWorkerLayout.layouts.has(town))return townWorkerLayout.layouts.get(town);
  const center=town.town_origin || town.buildings.find(b=>b.kind==='hall');
  const near=item=>Math.abs(item.x-center.x)<=60&&Math.abs(item.y-center.y)<=60;
  const tiles=items=>items.filter(near).map(({kind,x,y,farm_index,construction})=>[kind,x,y,farm_index,!!construction]);
  const terrain=[];
  for(let y=Math.max(0,center.y-60);y<=Math.min(town.map_size-1,center.y+60);y++){
    let row='';
    for(let x=Math.max(0,center.x-60);x<=Math.min(town.map_size-1,center.x+60);x++)row+=town.terrain?.[y]?.[x] || 'v';
    terrain.push(row);
  }
  const layout=JSON.stringify([tiles(buildings),tiles(resources),terrain,tiles(mapRoads(town).filter(r=>!r.construction))]);
  townWorkerLayout.layouts.set(town,layout);return layout;
}
let npcClock=0;
const npcColors={g:['#a97b58','#c7a06d'],f:['#416951','#70906a'],m:['#b29b4a','#e2cc80'],
  s:['#be986a','#e6c79a'],r:['#717b88','#acb4bd'],b:['#586b4b','#a1a474'],d:['#c18d58','#f0dab0'],i:['#658da2','#e0edf0']};
const npcSamples=new WeakMap();
function npcSprites(town,seconds){
  npcClock+=seconds;
  return (tileW<32?[]:town?.npcs || []).map((npc,index)=>{
    const routine=npc.routine;
    if(!routine)return {...npc,kind:'npc',index,villager:{job:'idle'},active:false,
      state:{path:[],phase:'idle',clock:npcClock+index*.7,facing:index%2?1:-1}};
    if(!npcSamples.has(routine))npcSamples.set(routine,npcClock);
    // Extrapolate only the host's published route; snapshots stay untouched.
    let remaining=Math.min(2,npcClock-npcSamples.get(routine)),x=routine.x,y=routine.y;
    const path=routine.paused?[]:routine.path || [];
    let walking=false,facing=routine.facing;
    for(const [tx,ty] of path){
      const dx=tx-x,dy=ty-y,distance=Math.hypot(dx,dy),duration=distance/.65;
      const progress=duration?Math.min(1,remaining/duration):1;
      x+=dx*progress;y+=dy*progress;facing=dx-dy>=0?1:-1;
      if(progress<1){walking=true;break;}
      remaining=Math.max(0,remaining-duration);
    }
    const active=!routine.paused&&!walking&&routine.phase==='working';
    return {...npc,x,y,kind:'npc',index,active,
      villager:{job:routine.job,woodcutting:5,fishing:5,crafting:5,husbandry:5,baking:5},
      state:{x,y,path:walking?[path[0]]:[],phase:active?'working':'idle',
        elapsed:routine.elapsed+Math.min(2,npcClock-npcSamples.get(routine)),clock:npcClock,
        source:routine.source,facing}};
  });
}
function villagerAccessories(b){
  // Only the current profession's practice earns its tools and clothing.
  const v=b.kind==='villager'||b.kind==='npc'&&b.routine?b.villager:{};
  const job=v.job,level=v[job] || 0;
  const outfits={
    woodcutting:{color:'#78966b'},farming:{color:'#a0a16b'},fishing:{color:'#709ba8'},
    building:{color:'#bc9564',cap:'#bca477',tool:'hammer'},
    hauling:{color:'#94735b',cap:'#806448',tool:'pack'},
    trading:{color:'#bd8b59',cap:'#806448',tool:'pack'},
    husbandry:{color:'#998261',apron:'#c8b993',cap:'#a68e64',tool:'bucket'},
    woodworking:{color:'#8c785e',apron:'#b99b72',cap:'#876c4d',tool:'saw'},
    milling:{color:'#a19a80',apron:'#e2d7b7',cap:'#d8c9a4',tool:'sack'},
    preserving:{color:'#957367',apron:'#c5a18b',cap:'#a88875',tool:'fish'},
    crafting:{color:'#727d8c',apron:'#806448',cap:'#65717e',tool:'hammer'},
    breeding:{color:'#658f86',apron:'#9abeb2',cap:'#638f82',tool:'net'},
    orcharding:{color:'#81955f',apron:'#b8c899',cap:'#9eac71',tool:'basket'},
    baking:{color:'#c4aa86',apron:'#f2e6cd',cap:'#fff1d8',tool:'rolling_pin'},
    mining:{color:'#7f8c8c',cap:'#c7b480',tool:'hammer'},smelting:{color:'#a08875',apron:'#705345',tool:'hammer'},
    tanning:{color:'#ad8a67',apron:'#ddc7a2',tool:'cleaver'},horsekeeping:{color:'#bda077',cap:'#806448',tool:'bucket'},
    saddlery:{color:'#b89268',apron:'#ddc7a2',tool:'hammer'},steelmaking:{color:'#758792',apron:'#705345',tool:'hammer'},packing:{color:'#a1ab7e',tool:'pack'},
    butchery:{color:'#966d65',apron:'#dfc9bf',cap:'#d8b6a7',tool:'cleaver'},
  };
  return {axe:job==='woodcutting'&&(level>=5||['fine_axe','steel_axe','masterwork_axe'].includes(v.worker_tool)),
    blouse:job==='woodcutting'&&level>=10,woodcuttingMaster:job==='woodcutting'&&level>=20,
    sickle:job==='farming'&&(level>=5||['fine_sickle','steel_sickle','masterwork_sickle'].includes(v.worker_tool)),
    hat:job==='farming'&&level>=10,farmingMaster:job==='farming'&&level>=20,
    pouch:job==='fishing'&&level>=5,scarf:job==='fishing'&&level>=10,fishingMaster:job==='fishing'&&level>=20,
    outfit:outfits[job] || null,level};
}
function paintHorse(target,stride=0){
  target.fillStyle='#352e2566';target.beginPath();target.ellipse(0,5,16,5,0,0,Math.PI*2);target.fill();
  target.strokeStyle='#795439';target.lineWidth=3;target.beginPath();
  for(const x of [-10,-6,6,10]){target.moveTo(x,-2);target.lineTo(x+(x<0?stride:-stride)*3,9);}
  target.stroke();target.fillStyle='#ac7a50';target.beginPath();target.ellipse(0,-5,14,7,0,0,Math.PI*2);target.fill();
  target.fillRect(8,-20,6,15);target.beginPath();target.ellipse(15,-20,7,4,.3,0,Math.PI*2);target.fill();
  target.fillStyle='#433428';target.fillRect(8,-24,3,16);target.fillRect(12,-28,2,6);target.fillRect(-17,-9,4,12);
  target.fillStyle='#efe3cb';target.fillRect(18,-19,3,2);
}
function drawVillager(b){
  const accessories=villagerAccessories(b),outfit=accessories.outfit;
  if(tileW<32&&!(b.villager.job==='fishing'&&b.state.phase==='working'&&!b.state.path.length)){
    const [x,y]=point(b.x,b.y),scale=tileW/46;
    ctx.save();ctx.translate(x+(b.index%3-1)*3,y+tileH*.65+Math.floor(b.index/3));ctx.scale(scale,scale);
    if(b.mounted){paintHorse(ctx);ctx.translate(0,-12);}
    if(b.leadingHorse){ctx.save();ctx.translate(-25,3);ctx.scale(.65,.65);paintHorse(ctx);ctx.restore();}
    ctx.fillStyle='#283c38';ctx.fillRect(-4,-4,8,5);
    ctx.fillStyle=b.kind==='npc'?npcColors[b.biome][0]:b.kind==='player'?'#d6b85d':outfit?.color || ['#7899a2','#b9866b','#a0a16b','#947ca0','#c6aa62'][b.index%5];ctx.fillRect(-4,-13,8,10);
    ctx.fillStyle=['#e7c68c','#c99f79','#ad795c'][b.index%3];ctx.beginPath();ctx.arc(0,-17,4,0,Math.PI*2);ctx.fill();
    ctx.restore();return;
  }
  const [px,py]=point(b.x,b.y),s=b.state;
  const walking=s.path.length>0,working=s.phase==='working'&&b.active;
  const stride=walking?Math.sin(s.clock*13):0;
  const chopping=working&&b.villager.job==='woodcutting';
  const fishing=s.phase==='working'&&!walking&&b.villager.job==='fishing';
  const farming=s.phase==='working'&&!walking&&b.villager.job==='farming';
  const constructing=working&&!!b.villager.construction_target;
  const farmStage=Math.min(3,Math.floor((s.elapsed || 0)/2));
  const farmSwing=(Math.sin((s.elapsed || 0)*Math.PI*3)+1)/2;
  const fishingTime=s.activityElapsed ?? s.elapsed ?? 0;
  const fishingUpgrades=s.home?.profession_upgrades || [];
  const fishingNet=fishing&&(fishingUpgrades.includes('nets')||fishingUpgrades.includes('wool_nets'));
  const fishingCaught=s.fishingCaught ?? s.latestFishingCatch?.caught ?? true;
  const fishingMetal=b.villager.worker_tool==='masterwork_fishing_rod'?'#d9bd70':b.villager.worker_tool==='steel_fishing_rod'?'#bacac5':'#a78454';
  const cast=Math.min(fishingTime/.55,1),reel=Math.max(0,Math.min((fishingTime-2.35)/.85,1));
  const swing=working?(Math.sin((s.elapsed || 0)*(chopping?5:7))+1)/2:0;
  ctx.save();ctx.translate(px+(b.index%3-1)*3,py+tileH*.65+Math.floor(b.index/3));ctx.scale(tileW/46,tileW/46);
  if(!b.aboard){ctx.fillStyle='#223b2966';ctx.beginPath();ctx.ellipse(0,2,7,3,0,0,Math.PI*2);ctx.fill();}
  const facingTarget=s.phase==='collecting_water'?s.waterSource:fishing?(s.castTarget || s.source):s.source;
  const towardSource=facingTarget?(facingTarget.x-s.x)-(facingTarget.y-s.y):0;
  const facing=(working||fishing||farming)&&towardSource!==0?Math.sign(towardSource):s.facing;
  ctx.scale(facing,1);
  if(b.leadingHorse){ctx.save();ctx.translate(-28,5);ctx.scale(.75,.75);paintHorse(ctx,stride);ctx.restore();ctx.strokeStyle='#cbb78a';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(-12,-8);ctx.lineTo(-3,-8);ctx.stroke();}
  if(b.mounted){paintHorse(ctx,stride);ctx.translate(0,-12);}
  ctx.strokeStyle='#283c38';ctx.lineWidth=2.5;ctx.lineCap='round';
  if(!b.aboard){ctx.beginPath();ctx.moveTo(-2,-4);ctx.lineTo(-2-stride*3,1);ctx.moveTo(2,-4);ctx.lineTo(2+stride*3,1);ctx.stroke();}
  const bob=walking?Math.abs(stride)*1.2:fishing?reel*1.5:farming?farmSwing*(farmStage===0?2:1):chopping?swing*2.5:working?swing:Math.sin(s.clock*2)*.3;
  ctx.translate(0,-bob);
  ctx.fillStyle=b.kind==='npc'?npcColors[b.biome][0]:b.kind==='player'?'#d6b85d':outfit?.color || ['#7899a2','#b9866b','#a0a16b','#947ca0','#c6aa62'][b.index%5];ctx.fillRect(-4,-13,8,10);
  const gear=b.kind==='player'?b.equipment:null;
  if(gear?.feet==='swamp_boots'){ctx.fillStyle='#52715d';ctx.fillRect(-5,-5,4,6);ctx.fillRect(1,-5,4,6);}
  if(gear?.body==='desert_clothing'){polygon([[-5,-13],[5,-13],[6,-2],[-6,-2]],'#ead8ae');}
  if(accessories.blouse){
    polygon([[-4,-13],[4,-13],[7,-9],[5,-7],[3,-10],[4,-3],[-4,-3],[-3,-10],[-5,-7],[-7,-9]],'#b8c6ad');
    polygon([[-3,-13],[0,-10],[3,-13]],'#eee0bd');
    ctx.strokeStyle='#6c8068';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(0,-10);ctx.lineTo(0,-4);ctx.stroke();
    if(accessories.woodcuttingMaster){
      ctx.fillStyle='#e8bd55';ctx.fillRect(-4,-4,8,1);ctx.fillRect(-1,-9,2,2);
    }
  }
  if(outfit?.apron){
    polygon([[-3,-12],[3,-12],[4,-3],[-4,-3]],outfit.apron);
    ctx.fillStyle=outfit.apron;ctx.fillRect(-4,-10,8,1);
    if(accessories.level>=10){ctx.fillStyle=outfit.color;ctx.fillRect(-2,-7,4,2);}
  }
  if(outfit&&accessories.level>=20){ctx.fillStyle='#e8bd55';ctx.fillRect(-4,-4,8,1);ctx.fillRect(-2,-12,2,2);}
  ctx.fillStyle=['#e7c68c','#c99f79','#ad795c'][b.index%3];ctx.beginPath();ctx.arc(0,-17,4,0,Math.PI*2);ctx.fill();
  ctx.fillStyle=['#584638','#aa7846','#363c36','#d3bc8c'][b.index%4];ctx.beginPath();ctx.arc(0,-18,4,Math.PI,Math.PI*2);ctx.fill();
  if(b.index%2)ctx.fillRect(-4,-18,2,7);
  if(gear?.body==='desert_clothing'){ctx.fillStyle='#ead8ae';ctx.fillRect(-5,-21,10,3);ctx.fillRect(-5,-19,2,8);}
  if(outfit?.cap){
    ctx.fillStyle=outfit.cap;ctx.fillRect(-5,-21,10,3);
    if(b.villager.job==='baking'){
      ctx.beginPath();ctx.arc(-3,-23,3,0,Math.PI*2);ctx.arc(0,-24,3,0,Math.PI*2);ctx.arc(3,-23,3,0,Math.PI*2);ctx.fill();
    }else if(accessories.level>=10)ctx.fillRect(-6,-19,12,1);
  }
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
  if(outfit?.tool&&accessories.level>=5){
    const tool=outfit.tool;
    ctx.strokeStyle='#a78454';ctx.lineWidth=2;
    ctx.beginPath();ctx.moveTo(-6,0);ctx.lineTo(-7,-9);ctx.stroke();
    const metal=accessories.level>=20?'#e8bd55':'#c9d5d2';
    if(tool==='rolling_pin'){
      ctx.fillStyle='#c39b64';ctx.fillRect(-9,-10,4,9);
    }else if(tool==='hammer'||tool==='cleaver'||tool==='saw'){
      ctx.fillStyle=metal;ctx.fillRect(tool==='saw'?-13:-11,-11,tool==='saw'?8:7,tool==='cleaver'?6:3);
    }else if(tool==='net'){
      ctx.strokeStyle=metal;ctx.beginPath();ctx.ellipse(-7,-12,4,5,0,0,Math.PI*2);ctx.stroke();
      ctx.beginPath();ctx.moveTo(-10,-12);ctx.lineTo(-4,-12);ctx.moveTo(-7,-16);ctx.lineTo(-7,-8);ctx.stroke();
    }else if(tool==='fish'){
      polygon([[-10,-8],[-6,-10],[-3,-8],[-6,-5]],'#b8d8d5',metal);
    }else{
      const color={bucket:'#879b9c',pack:'#70543e',sack:'#cbb58a',basket:'#ac895d'}[tool];
      ctx.fillStyle=color;ctx.fillRect(-11,-8,7,8);
      ctx.strokeStyle=metal;ctx.beginPath();ctx.arc(-7.5,-8,3,Math.PI,Math.PI*2);ctx.stroke();
      if(tool==='basket'){ctx.fillStyle='#b35b69';ctx.fillRect(-9,-9,3,3);}
    }
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
    if(fishingNet){
      const netX=7+(surfaceX-7)*cast*(1-reel),netY=-10+(surfaceY+10)*cast*(1-reel)-reel*14;
      ctx.strokeStyle=fishingUpgrades.includes('wool_nets')?'#eee0bd':'#a6bbb1';ctx.lineWidth=1.2;
      ctx.beginPath();ctx.ellipse(netX,netY,10,5,0,0,Math.PI*2);ctx.stroke();
      for(let i=-1;i<=1;i++){
        ctx.beginPath();ctx.moveTo(netX+i*4,netY-4);ctx.lineTo(netX+i*4,netY+4);
        ctx.moveTo(netX-8,netY+i*2);ctx.lineTo(netX+8,netY+i*2);ctx.stroke();
      }
      ctx.beginPath();ctx.moveTo(7,-10-reel*3);ctx.lineTo(netX,netY);ctx.stroke();
      if(reel>0&&fishingCaught)polygon([[netX-4,netY],[netX,netY-3],[netX+5,netY],[netX,netY+3]],'#b8d8d5','#527f86');
    }else{
      ctx.strokeStyle=fishingMetal;ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(7,-10-reel*3);ctx.lineTo(tipX,tipY);ctx.stroke();
      ctx.strokeStyle='#d5dfd8';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(tipX,tipY);ctx.lineTo(floatX,floatY);ctx.stroke();
      if(cast===1&&reel===0){
        ctx.strokeStyle='#c3e1dc';ctx.beginPath();ctx.ellipse(surfaceX,surfaceY,4+Math.sin(fishingTime*6)*1.5,2,0,0,Math.PI*2);ctx.stroke();
      }
      ctx.fillStyle='#df785e';ctx.fillRect(floatX-2,floatY-2,4,3);
      ctx.fillStyle='#f3e5bc';ctx.fillRect(floatX-2,floatY+1,4,2);
      if(reel>0&&fishingCaught){
        polygon([[floatX,floatY+3],[floatX+5,floatY+6],[floatX,floatY+9],[floatX-4,floatY+6]],'#b8d8d5','#527f86');
        polygon([[floatX,floatY+9],[floatX-3,floatY+12],[floatX+3,floatY+12]],'#7899a2');
      }
    }
  }else if(working&&b.villager.job==='gathering'){
    ctx.translate(swing*4,swing*3);
    ctx.fillStyle='#ac895d';ctx.fillRect(5,-7,10,8);ctx.fillStyle='#b35b69';ctx.beginPath();ctx.arc(10,-8,3,0,Math.PI*2);ctx.fill();
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
  }else if(working&&['husbandry','woodworking','milling','preserving','crafting','breeding','orcharding','baking','butchery'].includes(b.villager.job)){
    drawProfessionAction(b.villager.job,s.elapsed || 0);
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
  if(['fetching_water','collecting_water','returning_water'].includes(s.phase)){
    ctx.fillStyle='#789da5';ctx.fillRect(6,-9,8,7);
    ctx.strokeStyle='#b7d1d5';ctx.lineWidth=1.5;ctx.beginPath();ctx.arc(10,-10,4,Math.PI,Math.PI*2);ctx.stroke();
    if(s.phase!=='fetching_water'){ctx.fillStyle='#9fd8e5';ctx.fillRect(7,-9,6,2);}
  }
  if(['hauling','trading'].includes(b.villager.job)&&s.transport){
    if(Object.values(s.cargo || {}).some(amount=>amount>0))drawHaulerCargo(s.cargo);
  }else if(s.transport||s.phase==='returning'||s.phase==='depositing'){
    ctx.save();
    if(s.phase==='depositing'){const progress=Math.min(1,(s.elapsed || 0)/.65);ctx.translate(progress*8,progress*3);ctx.globalAlpha=1-progress;}
    ctx.fillStyle=b.villager.job==='woodcutting'?'#997044':b.villager.job==='fishing'?'#7899a2':'#d6c478';ctx.fillRect(2,-8,11,5);
    ctx.fillStyle='#e1c593';ctx.beginPath();ctx.arc(13,-5.5,2.5,0,Math.PI*2);ctx.fill();
    if(b.villager.job==='fishing')polygon([[5,-10],[9,-12],[13,-10],[9,-8]],'#b8d8d5','#527f86');
    if(['woodworking','crafting'].includes(b.villager.job)){ctx.fillStyle='#b78b57';ctx.fillRect(2,-11,14,3);}
    if(['baking','butchery','preserving','orcharding','husbandry','breeding','milling'].includes(b.villager.job)){ctx.fillStyle='#ac895d';ctx.fillRect(3,-9,12,7);ctx.fillStyle={baking:'#d3ab69',butchery:'#b66e65',preserving:'#8ebbbd',orcharding:'#cc8962',husbandry:'#eee0bd',breeding:'#8ebbbd',milling:'#eee0bd'}[b.villager.job];ctx.beginPath();ctx.ellipse(9,-9,5,2,0,0,Math.PI*2);ctx.fill();}
    if(b.villager.job==='farming'){
      ctx.strokeStyle='#e5c77f';ctx.lineWidth=2;ctx.beginPath();
      for(let i=0;i<4;i++){ctx.moveTo(5+i*2,-4);ctx.lineTo(4+i*2,-12);}ctx.stroke();
    }
    ctx.restore();
  }
  ctx.restore();
}
function drawHaulerCargo(cargo){
  const entries=Object.entries(cargo).filter(([,amount])=>amount>0);
  // Open baskets expose their load; labels identify mixed batches precisely.
  ctx.fillStyle='#ac895d';ctx.fillRect(2,-8,14,9);
  ctx.strokeStyle='#725333';ctx.lineWidth=1.2;ctx.strokeRect(2,-8,14,9);
  for(let index=0;index<Math.min(3,entries.length);index++){
    const key=entries[index][0],x=4+index*4;
    ctx.fillStyle={wood:'#997044',planks:'#c59b65',fish:'#8ebbbd',smoked_fish:'#8ebbbd',wool:'#eee0bd',
      tools:'#cbd7d2',wheat:'#e5c77f',flour:'#eee0bd',vegetables:'#8da962',fruit:'#cc8962',bread:'#d3ab69',
      fodder:'#9caa63',manure:'#806448',herbs:'#70906a',meat:'#b66e65',eggs:'#eee0bd'}[key] || '#d6c478';
    ctx.fillRect(x,-12,4,6);
  }
  ctx.font='9px system-ui';ctx.textAlign='center';ctx.lineWidth=3;ctx.strokeStyle='#263a32';ctx.fillStyle='#fff3d6';
  const label=entries.map(([key,amount])=>`${Number(amount.toFixed(2))} ${goodName(key)}`).join(' · ');
  ctx.strokeText(label,0,-43);ctx.fillText(label,0,-43);
}
// Tools and materials are drawn in character coordinates; elapsed freezes on pauses.
function drawProfessionAction(job,time){
  const motion=(Math.sin(time*7)+1)/2;
  ctx.fillStyle='#806448';ctx.fillRect(5,-3,17,3);
  ctx.strokeStyle='#d5dfd8';ctx.lineWidth=2;
  if(job==='woodworking'){
    ctx.fillStyle='#b78b57';ctx.fillRect(6,-7,17,4);
    polygon([[5+motion*7,-11],[19+motion*7,-11],[18+motion*7,-7],[6+motion*7,-7]],'#c4d2ce');
    ctx.fillStyle='#e4c98a';for(let i=0;i<3;i++)ctx.fillRect(15+i*3,-3+((time*4+i)%1)*6,2,2);
  }else if(job==='crafting'||job==='butchery'){
    ctx.fillStyle=job==='crafting'?'#7c8985':'#b66e65';ctx.fillRect(8,-7,12,4);
    ctx.save();ctx.translate(9,-9-motion*6);ctx.rotate(-motion*.8);
    ctx.strokeStyle='#a78454';ctx.beginPath();ctx.moveTo(0,2);ctx.lineTo(0,-7);ctx.stroke();
    ctx.fillStyle='#d5dfd8';ctx.fillRect(-3,-9,job==='crafting'?9:7,job==='crafting'?4:7);ctx.restore();
    if(job==='crafting'&&motion>.8){ctx.fillStyle='#efd59a';ctx.fillRect(21,-10,2,2);ctx.fillRect(24,-7,2,2);}
  }else if(job==='milling'){
    ctx.fillStyle='#92978b';ctx.beginPath();ctx.ellipse(13,-6,9,4,0,0,Math.PI*2);ctx.fill();
    ctx.beginPath();ctx.moveTo(13,-6);ctx.lineTo(13+Math.cos(time*4)*7,-6+Math.sin(time*4)*3);ctx.stroke();
    ctx.fillStyle='#eee0bd';ctx.fillRect(20,-3,5,5);
  }else if(job==='baking'){
    ctx.fillStyle='#eee0bd';ctx.beginPath();ctx.ellipse(13,-6,7+motion*2,3-motion,0,0,Math.PI*2);ctx.fill();
    ctx.strokeStyle='#a78454';ctx.lineWidth=3;ctx.beginPath();ctx.moveTo(7+motion*5,-8);ctx.lineTo(18+motion*5,-8);ctx.stroke();
  }else if(job==='preserving'){
    polygon([[8,-9],[16,-12],[23,-9],[16,-6]],'#8ebbbd');
    ctx.strokeStyle='#a78454';ctx.beginPath();ctx.moveTo(7,-13-motion*4);ctx.lineTo(23,-13-motion*4);ctx.stroke();
    ctx.fillStyle='#c7c6af88';for(let i=0;i<3;i++){ctx.beginPath();ctx.arc(12+i*4,-17-((time+i*.3)%1)*10,2,0,Math.PI*2);ctx.fill();}
  }else if(job==='orcharding'){
    ctx.fillStyle='#719451';ctx.beginPath();ctx.arc(18,-18,9,0,Math.PI*2);ctx.fill();
    ctx.fillStyle='#cc8962';ctx.beginPath();ctx.arc(15,-17+motion*12,3,0,Math.PI*2);ctx.fill();
    ctx.fillStyle='#ac895d';ctx.fillRect(7,-4,13,6);
    ctx.strokeStyle='#e7c68c';ctx.beginPath();ctx.moveTo(3,-11);ctx.lineTo(15,-17+motion*12);ctx.stroke();
  }else{
    // Keepers scatter feed; pond breeders scatter it over a rippling basin.
    ctx.fillStyle=job==='breeding'?'#6ba2af':'#ac895d';ctx.beginPath();ctx.ellipse(16,1,10,4,0,0,Math.PI*2);ctx.fill();
    ctx.fillStyle='#efd59a';for(let i=0;i<5;i++){const fall=(time*1.5+i*.2)%1;ctx.fillRect(7+fall*14,-10+fall*fall*12,2,2);}
    if(job==='breeding'){ctx.strokeStyle='#b8d8d5';ctx.beginPath();ctx.ellipse(16,1,3+motion*5,1+motion*2,0,0,Math.PI*2);ctx.stroke();}
    else{ctx.fillStyle='#eee0bd';ctx.beginPath();ctx.ellipse(19+motion*2,-3,4,3,0,0,Math.PI*2);ctx.fill();}
  }
}
// A padded viewport keeps static ground reusable during walking and panning.
// One canvas bounds memory usage even on Retina displays.
let groundCache=null,resourceCache=null;
function renderResources(resources,detailed=true){
  if(typeof ClientScene!=='undefined'&&resourceCache?.resources===resources&&resourceCache.count===resources.length){
    if(resourceCache.detailed!==detailed){
      ClientScene.queue.schedule('resource-art',detailed,prepareResourceData(resources,detailed),data=>{
        if(resourceCache?.resources===resources)resourceCache=data;
      });
    }else ClientScene.queue.cancel('resource-art');
    return resourceCache;
  }
  const steps=prepareResourceData(resources,detailed);let result;
  do{result=steps.next();}while(!result.done);
  return resourceCache=result.value;
}
function* prepareResourceData(resources,detailed=true,previous=resourceCache){
  if(previous?.resources!==resources || previous.count!==resources.length || previous.detailed!==detailed){
    const layout=JSON.stringify(resources.map(({kind,x,y,farm_index,construction})=>[kind,x,y,farm_index,!!construction]));
    const sameLayout=previous?.layout===layout&&previous.detailed===detailed;
    const water=new Set(),objects=[],indexes=new Map(),buckets=new Map(),villageBarns=[];
    for(const [index,r] of resources.entries()){
      if(index%256===255)yield;
      indexes.set(r,index);
      if(r.village&&r.animals)villageBarns.push(r);
      if(r.kind==='water')water.add(`${r.x},${r.y}`);else{
        objects.push(r);
        const key=`${Math.floor(r.x/16)},${Math.floor(r.y/16)}`;
        if(!buckets.has(key))buckets.set(key,[]);
        buckets.get(key).push(r);
      }
    }
    const forest=sameLayout?previous.forest:detailed?null:new Set(objects.filter(r=>r.kind==='tree').map(r=>`${r.x},${r.y}`));
    if(forest&&!sameLayout){
      // Join small gaps between nearby trees into one ground-level canopy.
      const neighbors=new Map();
      let visited=0;
      for(const key of forest){
        if(++visited%256===0)yield;
        const [x,y]=key.split(',').map(Number);
        for(const [dx,dy] of [[-1,0],[1,0],[0,-1],[0,1]]){
          const adjacent=`${x+dx},${y+dy}`;
          neighbors.set(adjacent,(neighbors.get(adjacent) || 0)+1);
        }
      }
      for(const [key,count] of neighbors)if(count>=2)forest.add(key);
    }
    const rocks=new Map((detailed&&!sameLayout?objects.filter(r=>r.kind==='rock'):[]).map(r=>[`${r.x},${r.y}`,r]));
    const rockJoins=sameLayout?new Map([...previous.rockJoins].map(([rock,join])=>[resources[previous.indexes.get(rock)],join])):new Map();
    let visited=0;
    for(const r of rocks.values()){
      if(++visited%64===0)yield;
      const mask=[[0,-1],[1,0],[0,1],[-1,0]].reduce((mask,[dx,dy],i)=>
        mask|(rocks.has(`${r.x+dx},${r.y+dy}`)?1<<i:0),0);
      if(!mask)continue;
      const inset=[];
      const heights=[[0,0],[1,0],[1,1],[0,1]].map(([dx,dy])=>{
        const x=r.x+dx,y=r.y+dy;
        const neighbors=[[0,0],[-1,0],[-1,-1],[0,-1]].filter(([ox,oy])=>rocks.has(`${x+ox},${y+oy}`));
        const ox=neighbors.reduce((sum,[ox])=>sum+ox+.5,0)/neighbors.length;
        const oy=neighbors.reduce((sum,[,oy])=>sum+oy+.5,0)/neighbors.length;
        inset.push([23*(ox-oy)*.35,11.5*(ox+oy)*.35]);
        // A smooth density field creates one broad mound across the patch.
        let density=0;
        for(let tx=x-2;tx<=x+1;tx++)for(let ty=y-2;ty<=y+1;ty++){
          if(!rocks.has(`${tx},${ty}`))continue;
          const distance=(tx+.5-x)**2+(ty+.5-y)**2;
          density+=Math.max(0,1-distance/4);
        }
        const taper=neighbors.length===4?1:.2+neighbors.length*.1375;
        return Math.min(58,(12+density*5)*taper);
      });
      rockJoins.set(r,{mask,heights,inset});
    }
    previous={resources,count:resources.length,detailed,forest,water:sameLayout?previous.water:water,objects,indexes,buckets,rockJoins,layout,villageBarns};
  }
  return previous;
}
function visibleResourceObjects(data,rect){
  const previous=data.visible;
  if(previous&&previous.originX===originX&&previous.originY===originY&&previous.tileW===tileW
    &&previous.width===rect.width&&previous.height===rect.height)return previous.objects;
  const bottom=rect.height+80*tileW/46;
  const corners=[[-tileW,-tileH],[rect.width+tileW,-tileH],[-tileW,bottom],[rect.width+tileW,bottom]]
    .map(([px,py])=>({x:(px-originX)/tileW+(py-originY)/tileH,y:(py-originY)/tileH-(px-originX)/tileW}));
  const minX=Math.floor(Math.min(...corners.map(p=>p.x))/16),maxX=Math.floor(Math.max(...corners.map(p=>p.x))/16);
  const minY=Math.floor(Math.min(...corners.map(p=>p.y))/16),maxY=Math.floor(Math.max(...corners.map(p=>p.y))/16);
  let region=data.region;
  if(!region||minX<region.minX||maxX>region.maxX||minY<region.minY||maxY>region.maxY
    ||region.maxX-region.minX>maxX-minX+4||region.maxY-region.minY>maxY-minY+4){
    const objects=[];
    // One spare bucket per edge absorbs small pans and direction reversals.
    for(let x=minX-1;x<=maxX+1;x++)for(let y=minY-1;y<=maxY+1;y++)objects.push(...(data.buckets.get(`${x},${y}`) || []));
    // Static resources retain depth order while moving; equal depths keep
    // snapshot order so overlapping artwork and selection remain consistent.
    objects.sort((a,b)=>(a.depth ?? a.x+a.y)-(b.depth ?? b.x+b.y)||data.indexes.get(a)-data.indexes.get(b));
    region=data.region={minX:minX-1,maxX:maxX+1,minY:minY-1,maxY:maxY+1,objects};
  }
  const objects=[];
  for(const r of region.objects){
    const [px,py]=point(r.x,r.y);
    if(px>=-tileW&&px<=rect.width+tileW&&py>=-tileH&&py<=bottom)objects.push(r);
  }
  data.visible={originX,originY,tileW,width:rect.width,height:rect.height,objects};
  return objects;
}
function mergeDepthObjects(before,resources,after){
  const depth=object=>object.depth ?? object.x+object.y;
  // Resources are already sorted in the spatial cache. Only sort the smaller
  // live lists, then merge in linear time, preserving snapshot order on ties.
  before.sort((a,b)=>depth(a)-depth(b));
  after.sort((a,b)=>depth(a)-depth(b));
  const lists=[before,resources,after],positions=[0,0,0],objects=[];
  while(true){
    let next=-1;
    for(let i=0;i<lists.length;i++){
      if(positions[i]>=lists[i].length)continue;
      if(next<0 || depth(lists[i][positions[i]])<depth(lists[next][positions[next]]))next=i;
    }
    if(next<0)return objects;
    objects.push(lists[next][positions[next]++]);
  }
}
function paintGround(target,landscape,water,bounds,offsetX=0,offsetY=0,region=null,pixelRatio=renderPixelRatio(),forest=null){
  const ctx=target,terrainSize=landscape?.map_size || 12;
  const detailed=tileW>=32;
  const palette={g:['#68865a','#69875b','#668358'],f:['#506f4a','#55764c','#5a7b50'],
    m:['#83945e','#899b65','#80915b'],s:['#b4a575','#bdad7d','#b8a878'],r:['#8b8d7c','#969785','#858978'],b:['#59674b','#626f50','#536247'],d:['#d6b575','#dfbf80','#cfac6b'],i:['#c6e2e8','#d4e9ed','#bcd9e3']};
  const land=(tx,ty)=>tx>=0&&ty>=0&&tx<terrainSize&&ty<terrainSize&&landscape?.terrain?.[ty]?.[tx]!=='v'
    &&(!landscape?.chunked||!!landscape.terrain?.[ty]?.[tx]);
  const {minX,maxX,minY,maxY}=bounds;
  const point=(x,y)=>[originX+(x-y)*tileW/2+offsetX,originY+(x+y)*tileH/2+offsetY];
  const polygon=(points,fill,stroke)=>{
    ctx.beginPath();points.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));
    ctx.closePath();ctx.fillStyle=fill;ctx.fill();
    if(stroke){ctx.strokeStyle=stroke;ctx.stroke();}
  };
  // Restrict each column to the visible strip instead of traversing the much
  // larger axis-aligned bounds of an isometric rectangle.
  const area=region || {x:0,y:0,width:(target.canvas?.width || canvas.width)/pixelRatio,
    height:(target.canvas?.height || canvas.height)/pixelRatio};
  const left=area.x-tileW/2,right=area.x+area.width+tileW/2;
  const top=area.y-tileH-12,bottom=area.y+area.height;
  for(let x=minX;x<=maxX;x++){
    const start=Math.max(minY,Math.ceil(x-2*(right-originX-offsetX)/tileW),Math.ceil(2*(top-originY-offsetY)/tileH-x));
    const end=Math.min(maxY,Math.floor(x-2*(left-originX-offsetX)/tileW),Math.floor(2*(bottom-originY-offsetY)/tileH-x));
    for(let y=start;y<=end;y++){
      const [px,py]=point(x,y),wet=water.has(`${x},${y}`);
      if(px+tileW/2<0 || px-tileW/2>target.canvas?.width/pixelRatio
        || py+tileH+12<0 || py>target.canvas?.height/pixelRatio)continue;
      if(region&&(px+tileW/2<region.x || px-tileW/2>region.x+region.width
        || py+tileH+12<region.y || py>region.y+region.height))continue;
      const variation=(x*17+y*31+x*y*7)%7;
      const diamond=[[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]];
      const biome=landscape?.terrain?.[y]?.[x] || (landscape?.chunked?'v':'g');
      if(biome==='v')continue;
      // Ground thickness appears only at the actual valley boundary.
      if(!land(x+1,y))polygon([diamond[1],diamond[2],[diamond[2][0],diamond[2][1]+12],[diamond[1][0],diamond[1][1]+12]],biome==='i'?'#729ead':'#344b35');
      if(!land(x,y+1))polygon([diamond[2],diamond[3],[diamond[3][0],diamond[3][1]+12],[diamond[2][0],diamond[2][1]+12]],biome==='i'?'#94bac7':'#405438');
      const shores=wet?[!water.has(`${x},${y-1}`),!water.has(`${x+1},${y}`),
        !water.has(`${x},${y+1}`),!water.has(`${x-1},${y}`)]:null;
      const shallow=shores?.some(Boolean);
      const waterColor=biome==='b'?(shallow?'#718565':'#4e6857'):(shallow?'#6299a1':'#447d91');
      const canopy=!detailed&&forest?.has(`${x},${y}`);
      polygon(diamond,wet?waterColor:canopy?(biome==='b'?'#4b6549':'#3f6045'):palette[biome][groundVariant(x,y)],null);
      if(detailed&&!wet&&biome==='i'){
        // Stable, tile-local frost stays inside the cached ground footprint.
        const frost=((x*73856093)^(y*19349663)^(landscape?.terrain_seed || 0))>>>0;
        const mirror=frost&1?1:-1;
        const p=(dx,dy)=>[px+dx*tileW*mirror,py+dy*tileH];
        polygon([p(-.34,.48),p(-.06,.19),p(.28,.48),p(.03,.73)],'#edfaff38');
        if(frost%3!==0){
          ctx.beginPath();ctx.moveTo(...p(-.35,.56));
          ctx.quadraticCurveTo(...p(-.14,.31),...p(.16,.53));
          ctx.quadraticCurveTo(...p(.04,.49),...p(-.03,.72));
          ctx.quadraticCurveTo(...p(-.22,.64),...p(-.35,.56));
          ctx.closePath();ctx.fillStyle='#f4fbf0a6';ctx.fill();
        }
        if(frost%13===0){
          ctx.strokeStyle='#7fabbc88';ctx.lineWidth=tileW/60;
          ctx.beginPath();ctx.moveTo(...p(-.27,.56));ctx.lineTo(...p(-.025,.64));
          ctx.lineTo(...p(.15,.41));ctx.lineTo(...p(.33,.5));
          ctx.moveTo(...p(-.025,.64));ctx.lineTo(...p(.075,.83));ctx.stroke();
          ctx.strokeStyle='#f4ffffb3';ctx.beginPath();
          ctx.moveTo(...p(-.26,.53));ctx.lineTo(...p(-.035,.61));ctx.lineTo(...p(.13,.39));ctx.stroke();
        }
        if(frost%17===0){
          const [sx,sy]=p(.2,.59);ctx.strokeStyle='#f7ffffcc';ctx.lineWidth=tileW/60;
          ctx.beginPath();ctx.moveTo(sx-tileW*.035,sy);ctx.lineTo(sx+tileW*.035,sy);
          ctx.moveTo(sx,sy-tileH*.07);ctx.lineTo(sx,sy+tileH*.07);ctx.stroke();
        }
      }
      if(detailed&&!wet&&biome==='d'&&variation%3===0){
        ctx.strokeStyle='#b9965d';ctx.lineWidth=tileW/46;
        ctx.beginPath();ctx.moveTo(px-tileW*.18,py+tileH*.55);
        ctx.quadraticCurveTo(px,py+tileH*.35,px+tileW*.18,py+tileH*.55);
        ctx.stroke();
      }
      if(detailed&&!wet&&biome==='d'){
        // Sparse, seeded plants stay inside their tile for ground-cache scrolling.
        const plant=((x*73856093)^(y*19349663)^(landscape?.terrain_seed || 0))>>>0;
        if(plant%13===0){
          const mirror=plant&1?1:-1;
          const plantScale=plant%3===0?1.2:1;
          const p=(dx,dy)=>[px+dx*tileW*mirror*plantScale,py+(.74+(dy-.74)*plantScale)*tileH];
          polygon([p(-.12,.72),p(0,.67),p(.19,.74),p(.06,.81)],'#ad8d554d');
          if(plant%3===0){
            // A small branching cactus with a sunlit rib and sandy spines.
            polygon([p(-.04,.74),p(-.04,.31),p(-.015,.25),p(.025,.25),
              p(.05,.31),p(.05,.74)],'#577b4e');
            polygon([p(-.04,.58),p(-.13,.58),p(-.16,.53),p(-.16,.4),
              p(-.1,.4),p(-.1,.5),p(-.04,.5)],'#577b4e');
            polygon([p(.04,.63),p(.14,.63),p(.17,.58),p(.17,.46),
              p(.11,.46),p(.11,.56),p(.04,.56)],'#668b56');
            ctx.strokeStyle='#9daf71';ctx.lineWidth=tileW/65;
            ctx.beginPath();ctx.moveTo(...p(0,.7));ctx.lineTo(...p(0,.32));ctx.stroke();
            ctx.fillStyle='#ddce96';
            for(const [dx,dy] of [[-.025,.4],[.04,.51],[-.12,.46],[.14,.54]]){
              const [sx,sy]=p(dx,dy);ctx.fillRect(sx,sy,tileW/90,tileW/90);
            }
          }else if(plant%3===1){
            // Pointed agave leaves form a low, spreading rosette.
            for(const [dx,dy] of [[-.2,.48],[-.1,.36],[0,.3],[.11,.39],[.21,.52]]){
              polygon([p(-.035,.74),p(dx,dy),p(.04,.73)],dx<0?'#6e8760':'#89966b');
            }
            polygon([p(-.025,.75),p(.015,.44),p(.045,.74)],'#b0b081');
          }else{
            ctx.strokeStyle='#927a4e';ctx.lineWidth=tileW/60;
            ctx.beginPath();
            for(const [dx,dy] of [[-.17,.51],[-.08,.43],[.02,.4],[.12,.47],[.18,.58]]){
              ctx.moveTo(...p(0,.74));ctx.lineTo(...p(dx*.5,.6));ctx.lineTo(...p(dx,dy));
              ctx.moveTo(...p(dx*.5,.6));ctx.lineTo(...p(dx*.8-.035,dy+.07));
            }
            ctx.stroke();
          }
        }
      }
      if(detailed&&biome==='b'&&variation%3===0){
        ctx.strokeStyle='#9b9f69';ctx.lineWidth=tileW/46;
        for(let reed=0;reed<3;reed++){const rx=px+(reed-1)*tileW*.07,ry=py+tileH*.7;ctx.beginPath();ctx.moveTo(rx,ry);ctx.lineTo(rx+(reed-1)*tileW*.025,ry-tileH*.4);ctx.stroke();}
        if(wet){ctx.fillStyle='#a3ad74';ctx.beginPath();ctx.ellipse(px+tileW*.17,py+tileH*.55,tileW*.065,tileH*.075,0,0,Math.PI*2);ctx.fill();}
      }
      if(detailed&&!wet&&!['d','i'].includes(biome)&&variation===0){ctx.fillStyle='#8fa574';ctx.fillRect(px-5*tileW/46,py+tileH*.5,2*tileW/46,2*tileW/46);ctx.fillStyle='#d7c78a';ctx.fillRect(px+4*tileW/46,py+tileH*.65,2*tileW/46,2*tileW/46);}
      if(detailed&&!wet&&biome!=='i'&&(x===0||y===0||x===terrainSize-1||y===terrainSize-1)&&variation%2===0){ctx.strokeStyle='#9caf77';ctx.lineWidth=tileW/46;ctx.beginPath();ctx.moveTo(px-6*tileW/46,py+tileH*.65);ctx.lineTo(px-9*tileW/46,py+tileH*.4);ctx.moveTo(px-6*tileW/46,py+tileH*.65);ctx.lineTo(px-4*tileW/46,py+tileH*.35);ctx.stroke();}
      if(detailed&&wet){
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
}
function groundMatchesCache(landscape,water,bounds,forest,cache){
  const previous=cache.landscape,size=landscape?.map_size || 12;
  if(size!==(previous?.map_size || 12)||landscape?.terrain_seed!==previous?.terrain_seed)return false;
  const sameTerrain=landscape?.terrain===previous?.terrain&&!!landscape?.chunked===!!previous?.chunked;
  const sameWater=water===cache.water,sameForest=forest===cache.forest;
  // Include neighboring tiles because they determine shores and valley edges.
  for(let y=Math.max(0,bounds.minY-1);y<=Math.min(size-1,bounds.maxY+1);y++){
    const row=landscape?.terrain?.[y],oldRow=previous?.terrain?.[y];
    const sameRow=sameTerrain||(row===oldRow&&!!landscape?.chunked===!!previous?.chunked);
    if(sameRow&&sameWater&&sameForest)continue;
    for(let x=Math.max(0,bounds.minX-1);x<=Math.min(size-1,bounds.maxX+1);x++){
      if(!sameRow&&(row?.[x] || (landscape?.chunked?'v':'g'))
        !==(oldRow?.[x] || (previous?.chunked?'v':'g')))return false;
      if(sameWater&&sameForest)continue;
      const key=`${x},${y}`;
      if(!sameWater&&water.has(key)!==cache.water.has(key))return false;
      if(!sameForest&&!!forest?.has(key)!==!!cache.forest?.has(key))return false;
    }
  }
  return true;
}
function drawGround(rect,dpr,landscape,water,bounds,forest=null){
  const margin=384;
  // Distant terrain needs fewer pixels; keep live objects and the UI sharp.
  if(tileW<32)dpr=Math.min(dpr,1);
  if(!groundCache || groundCache.world!==world || groundCache.landscape!==landscape || groundCache.forest!==forest){
    // Snapshots retain their terrain and water sets. Compare only when a new
    // snapshot arrives, keeping fingerprint scans out of scrolling frames.
    const changed=groundCache&&(groundCache.world.world_id!==world.world_id
      ||(groundCache.detailed===(tileW>=32)
        &&!(groundCache.landscape.terrain===landscape?.terrain&&groundCache.water===water&&groundCache.forest===forest
          &&groundCache.landscape.map_size===landscape?.map_size&&groundCache.landscape.terrain_seed===landscape?.terrain_seed
          &&!!groundCache.landscape.chunked===!!landscape?.chunked)
        &&!groundMatchesCache(landscape,water,groundCache.bounds,forest,groundCache)));
    if(changed)groundCache=null;
    if(groundCache)Object.assign(groundCache,{world,landscape,water,forest});
    else{
      const surface=document.createElement('canvas'),target=surface.getContext?.('2d');
      if(!target){paintGround(ctx,landscape,water,bounds,0,0,null,renderPixelRatio(),forest);return;}
      groundCache={surface,target,world,landscape,water,forest};
    }
  }
  const cache=groundCache,now=performance.now();
  if(cache.requestedTileW!==tileW){cache.requestedTileW=tileW;cache.zoomUpdatedAt=now;}
  const ratio=tileW/(cache.tileW || tileW);
  const left=originX-(cache.originX+margin)*ratio,top=originY-(cache.originY+margin)*ratio;
  const dx=originX-cache.originX,dy=originY-cache.originY;
  const zooming=cache.tileW!==tileW;
  // Reproject cached ground during a gesture. Repaint when coverage/resolution
  // runs out, or once the gesture settles, instead of on every wheel event.
  const repaintZoom=zooming&&(ratio<.8||ratio>1.25||now-cache.zoomUpdatedAt>=150
    ||left>0||top>0||left+cache.surface.width/dpr*ratio<rect.width
    ||top+cache.surface.height/dpr*ratio<rect.height);
  if(!cache.tileW || cache.detailed!==(tileW>=32) || repaintZoom || cache.width!==rect.width || cache.height!==rect.height
    || cache.dpr!==dpr || (!zooming&&(Math.abs(dx)>margin/2 || Math.abs(dy)>margin/2))){
    const width=Math.ceil((rect.width+margin*2)*dpr),height=Math.ceil((rect.height+margin*2)*dpr);
    // Refill ahead of the viewport in small strips while its spare margin
    // absorbs fast scrolling. Large jumps catch up immediately for coverage.
    const step=delta=>Math.abs(delta)>margin?delta:Math.max(-96,Math.min(96,delta));
    const shiftX=Math.round(step(dx)*dpr),shiftY=Math.round(step(dy)*dpr);
    const reuse=cache.detailed===(tileW>=32)&&cache.tileW===tileW&&cache.width===rect.width&&cache.height===rect.height
      &&cache.dpr===dpr&&Math.abs(shiftX)<width&&Math.abs(shiftY)<height;
    let regions;
    if(reuse){
      // Copy overlapping pixels before clearing and repainting the exposed strips.
      // Integer device pixels avoid blurring the cache during repeated pans.
      cache.target.setTransform(1,0,0,1,0,0);
      cache.target.globalCompositeOperation='copy';
      cache.target.drawImage(cache.surface,Math.max(0,-shiftX),Math.max(0,-shiftY),
        width-Math.abs(shiftX),height-Math.abs(shiftY),Math.max(0,shiftX),Math.max(0,shiftY),
        width-Math.abs(shiftX),height-Math.abs(shiftY));
      cache.target.globalCompositeOperation='source-over';
      regions=[];
      if(shiftX)regions.push({x:shiftX>0?0:(width+shiftX)/dpr,y:0,width:Math.abs(shiftX)/dpr,height:height/dpr});
      if(shiftY)regions.push({x:Math.max(0,shiftX)/dpr,y:shiftY>0?0:(height+shiftY)/dpr,
        width:(width-Math.abs(shiftX))/dpr,height:Math.abs(shiftY)/dpr});
    }else{
      cache.surface.width=width;cache.surface.height=height;
      regions=[{x:0,y:0,width:width/dpr,height:height/dpr}];
    }
    const nextX=reuse?cache.originX+shiftX/dpr:originX,nextY=reuse?cache.originY+shiftY/dpr:originY;
    const offsetX=margin+nextX-originX,offsetY=margin+nextY-originY;
    cache.target.setTransform(dpr,0,0,dpr,0,0);
    const regionBounds=region=>{
      const corners=[[region.x-tileW,region.y-tileH-12],[region.x+region.width+tileW,region.y-tileH-12],
        [region.x-tileW,region.y+region.height],[region.x+region.width+tileW,region.y+region.height]]
        .map(([px,py])=>({x:(px-originX-offsetX)/tileW+(py-originY-offsetY)/tileH,
          y:(py-originY-offsetY)/tileH-(px-originX-offsetX)/tileW}));
      const size=landscape?.map_size || 12;
      return {minX:Math.max(0,Math.floor(Math.min(...corners.map(p=>p.x)))),
        maxX:Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.x)))),
        minY:Math.max(0,Math.floor(Math.min(...corners.map(p=>p.y)))),
        maxY:Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.y))))};
    };
    for(const region of regions){
      cache.target.clearRect(region.x,region.y,region.width,region.height);
      cache.target.save();cache.target.beginPath();
      cache.target.rect(region.x,region.y,region.width,region.height);cache.target.clip();
      paintGround(cache.target,landscape,water,regionBounds(region),offsetX,offsetY,region,dpr,forest);
      cache.target.restore();
    }
    Object.assign(cache,{originX:nextX,originY:nextY,detailed:tileW>=32,tileW,width:rect.width,height:rect.height,dpr});
    cache.bounds=regionBounds({x:0,y:0,width:width/dpr,height:height/dpr});
  }
  const scale=tileW/cache.tileW;
  ctx.drawImage(cache.surface,originX-(cache.originX+margin)*scale,originY-(cache.originY+margin)*scale,
    cache.surface.width/dpr*scale,cache.surface.height/dpr*scale);
}
function fishingBoatSprites(entries,resources,villagers=[]){
  const huts=[];
  for(const [townId,town] of entries)for(const [index,hut] of (town?.buildings || []).entries())
    if(hut.kind==='fishing_hut'&&fishingBoatLevel(hut))huts.push({townId,index,hut,town});
  const layout=JSON.stringify(huts.map(({townId,index,hut})=>[townId,index,hut.x,hut.y,resourceReach(hut),fishingBoatLevel(hut)]));
  let changed=fishingBoatSprites.layout!==layout;
  if(fishingBoatSprites.resources!==resources || fishingBoatSprites.count!==resources.length){
    const water=resources.filter(r=>r.kind==='water');
    Object.assign(fishingBoatSprites,{resources,count:resources.length,water,
      waterTiles:new Set(water.map(r=>`${r.x},${r.y}`))});
    changed=true;
  }
  const {water,waterTiles}=fishingBoatSprites;
  if(changed){
    const berths=new Map(),obstacles=entries.flatMap(([,town])=>town?.buildings || []);
    fishingBoatSprites.layout=layout;
    fishingBoatSprites.docked=huts.map(({townId,index,hut,town})=>{
      const nearby=water.filter(r=>Math.abs(r.x-hut.x)+Math.abs(r.y-hut.y)<=resourceReach(hut))
        .sort((a,b)=>(Math.abs(a.x-hut.x)+Math.abs(a.y-hut.y))-(Math.abs(b.x-hut.x)+Math.abs(b.y-hut.y))||a.y-b.y||a.x-b.x);
      const resourceBuckets=typeof ClientScene!=='undefined'&&ClientScene.current?.resources===resources?ClientScene.current.buckets:null;
      const pier=WorkerAnimation.fishingPier({...town,map_resources:resources,obstacles,resourceBuckets},hut);
      const dock=pier.at(-1) || hut,deck=new Set(pier.map(p=>`${p.x},${p.y}`));
      const adjacent=nearby.filter(r=>Math.abs(r.x-dock.x)+Math.abs(r.y-dock.y)===1&&!deck.has(`${r.x},${r.y}`));
      const berth=adjacent.find(r=>!berths.has(`${r.x},${r.y}`)) || adjacent[0] || nearby[0];
      const key=berth?`${berth.x},${berth.y}`:`shore:${townId}/${index}`,count=berths.get(key) || 0;
      berths.set(key,count+1);
      return {kind:'fishing_boat',townId,objectIndex:index,level:fishingBoatLevel(hut),
        x:berth?berth.x+(count ? .35*Math.sin(count*2.4) : 0):hut.x+.7,
        y:berth?berth.y+(count ? .35*Math.cos(count*2.4) : 0):hut.y+.4};
    });
  }
  return fishingBoatSprites.docked.map((boat,index)=>{
    const {hut}=huts[index];
    const assigned=villagers.filter(v=>v.townId===boat.townId&&v.state.boating&&v.state.home?.x===hut.x&&v.state.home?.y===hut.y);
    // Keep the same leader through boarding and unloading. Selecting only crew
    // currently on water can jump the boat to another worker mid-voyage.
    const captain=assigned[0];
    const aboard=v=>{
      const dock=v.state.sailing?.dock,berth=v.state.sailing?.berth;
      const snap=n=>Math.abs(n-Math.round(n))<1e-6?Math.round(n):n,x=snap(v.x),y=snap(v.y);
      const onDeck=(x,y)=>dock&&berth&&(dock.x!==berth.x||dock.y!==berth.y)&&dock.x===x&&dock.y===y;
      return waterTiles.has(`${Math.floor(x)},${Math.floor(y)}`)&&waterTiles.has(`${Math.ceil(x)},${Math.ceil(y)}`)
        &&!onDeck(Math.floor(x),Math.floor(y))&&!onDeck(Math.ceil(x),Math.ceil(y));
    };
    const passenger=captain&&aboard(captain)?captain:null;
    const crew=assigned.filter(aboard);
    // The route's berth is authoritative for boarding; decorative parking may
    // use a different nearby tile when several huts share a shore.
    const berth=captain?.state.sailing?.berth || boat;
    return {...boat,passenger,crew,x:passenger?passenger.x:berth.x,y:passenger?passenger.y:berth.y};
  });
}
function drawFishingBoat(boat){
  const [x,top]=point(boat.x,boat.y),y=top+tileH/2,s=tileW/46;
  ctx.save();ctx.lineWidth=1.5*s;
  ctx.strokeStyle='#b8dcda';ctx.beginPath();ctx.ellipse(x,y+6*s,23*s,8*s,0,0,Math.PI*2);ctx.stroke();
  polygon([[x-23*s,y-3*s],[x-13*s,y+12*s],[x+12*s,y+12*s],[x+25*s,y-4*s]],boat.level===3?'#a27640':'#805738','#493627');
  polygon([[x-23*s,y-3*s],[x-9*s,y-9*s],[x+25*s,y-4*s],[x+10*s,y+5*s],[x-12*s,y+5*s]],'#c29a63','#efcb8c');
  polygon([[x-16*s,y-2*s],[x-8*s,y-5*s],[x+17*s,y-2*s],[x+8*s,y+2*s],[x-9*s,y+2*s]],'#574637');
  ctx.fillStyle='#e0bb80';ctx.fillRect(x-8*s,y-4*s,3*s,8*s);ctx.fillRect(x+6*s,y-3*s,3*s,7*s);
  ctx.strokeStyle='#efd49f';ctx.lineWidth=2*s;ctx.beginPath();ctx.moveTo(x-15*s,y+12*s);ctx.lineTo(x+14*s,y-10*s);ctx.stroke();
  if(boat.level>=2){
    ctx.fillStyle='#dfd6ac';ctx.fillRect(x,y-31*s,2*s,32*s);
    polygon([[x+2*s,y-31*s],[x+2*s,y-7*s],[x+20*s,y-7*s]],boat.level===3?'#d4af6b':'#ece3c5','#805738');
  }
  ctx.restore();
}
// Reuse the combined list so animal routing caches stay warm between frames.
function villageAnimalBuildings(buildings,data){
  const barns=data.villageBarns || [];
  const cached=villageAnimalBuildings.cache;
  if(cached?.buildings===buildings&&cached.barns===barns)return cached.result;
  const result=barns.length?[...buildings,...barns]:buildings;
  villageAnimalBuildings.cache={buildings,barns,result};return result;
}
// Animals roam nearby open tiles; their motion is purely visual.
function barnAnimalSprites(buildings,time,landscape={}){
  const sprites=[],size=landscape.map_size || 120;
  const key=p=>`${p.x},${p.y}`;
  const prepared=typeof ClientScene!=='undefined';
  const blocked=prepared?null:new Set([...buildings,...(landscape.map_resources || []),...(landscape.npcs || [])].map(key));
  for(const barn of buildings){
    if(!['barn','stable','village_hut'].includes(barn.kind)||(barn.construction&&!barn.construction.upgrade))continue;
    if(!Object.values(barn.animals || {}).some(n=>n>0))continue;
    // A small breadth-first search keeps every trip connected to the barn.
    const routes=prepared?ClientScene.animalRoutes(buildings,landscape,barn):[[]],queue=[{x:barn.x,y:barn.y,path:[]}],seen=new Set([key(barn)]);
    if(!prepared)for(const tile of queue){
      if(tile.path.length>=2)continue;
      for(const [dx,dy] of [[1,0],[0,1],[-1,0],[0,-1]]){
        const next={x:tile.x+dx,y:tile.y+dy},biome=landscape.terrain?.[next.y]?.[next.x];
        if(next.x<0||next.y<0||next.x>=size||next.y>=size||blocked.has(key(next))||seen.has(key(next))
          ||biome==='v'||(landscape.chunked&&!biome))continue;
        seen.add(key(next));
        const path=[...tile.path,next];queue.push({...next,path});routes.push(path);
      }
    }
    routes.sort((a,b)=>b.length-a.length);
    const stock=[];
    for(const species of (barn.village?['chicken','sheep','pig','cow','horse']:barn.kind==='stable'?['horse']:['chicken','sheep','pig','cow']))
      for(let i=0;i<(barn.animals?.[species] || 0);i++)stock.push({species,growth:Math.min(1,(barn.animal_growth?.[species]?.[i] ?? 300)/300)});
    for(const [index,{species,growth}] of stock.entries()){
      const phase=time+(barn.x*7+barn.y*11+index*3.17)%13,cycle=phase%16;
      const path=[barn,...routes[index%routes.length]];
      const outbound=cycle<3,returning=cycle>=8&&cycle<11;
      const walking=path.length>1&&(outbound||returning);
      const progress=outbound?cycle/3:returning?1-(cycle-8)/3:cycle<8?1:0;
      const step=progress*(path.length-1),segment=Math.min(Math.floor(step),Math.max(0,path.length-2));
      const start=path[segment],end=path[Math.min(segment+1,path.length-1)],fraction=step-segment;
      const side=(index%4-1.5)*.06,front=.12+Math.floor(index/4)*.04;
      sprites.push({kind:'animal',species,x:start.x+(end.x-start.x)*fraction+front+side,
        y:start.y+(end.y-start.y)*fraction+front-side,phase,walking,
        feeding:!walking&&((cycle>=4&&cycle<7)||cycle>=12),
        facing:((end.x-start.x)-(end.y-start.y))*(returning?-1:1)>=0?1:-1,
        size:(stock.length>4?.62:.8)*(.5+.5*growth)});
    }
  }
  return sprites;
}
function drawBarnAnimal(animal){
  const [x,top]=point(animal.x,animal.y),s=tileW/46*animal.size;
  const chicken=animal.species==='chicken',cow=animal.species==='cow';
  const stride=animal.walking?Math.sin(animal.phase*12):0;
  const bob=animal.walking?Math.abs(stride)*.6:Math.sin(animal.phase*2)*.2;
  const coat={chicken:'#f4deb0',sheep:'#e7e2ce',pig:'#d9968e',cow:'#e5d9bc'}[animal.species];
  ctx.save();ctx.translate(x,top+tileH/2);ctx.scale(s*animal.facing,s);
  if(animal.species==='horse'){paintHorse(ctx,stride);ctx.restore();return;}
  ctx.fillStyle='#233b3044';ctx.beginPath();ctx.ellipse(0,1,chicken?4:8,2,0,0,Math.PI*2);ctx.fill();
  ctx.strokeStyle=chicken?'#c59b51':'#655443';ctx.lineWidth=chicken?1:1.5;
  for(const [index,leg] of (chicken?[-2,2]:[-5,-3,3,5]).entries()){
    const step=stride*(index%2?1:-1);
    ctx.beginPath();ctx.moveTo(leg,-4);ctx.lineTo(leg+step,0);ctx.stroke();
  }
  ctx.translate(0,-bob);
  ctx.fillStyle=coat;ctx.beginPath();ctx.ellipse(0,-5,chicken?4:cow?8:7,chicken?3:4,0,0,Math.PI*2);ctx.fill();
  if(animal.species==='sheep'){
    for(let tuft=0;tuft<5;tuft++){ctx.beginPath();ctx.arc(-5+tuft*2.5,-8,2.5,0,Math.PI*2);ctx.fill();}
  }
  if(cow){ctx.fillStyle='#695447';for(const patch of [-4,2]){ctx.beginPath();ctx.ellipse(patch,-6,2.5,2,.4,0,Math.PI*2);ctx.fill();}}
  ctx.strokeStyle=chicken?coat:animal.species==='pig'?'#b97472':'#655443';ctx.lineWidth=1;
  ctx.beginPath();ctx.moveTo(chicken?-3:-7,-6);ctx.quadraticCurveTo(chicken?-7:-10,-10+Math.sin(animal.phase*4),chicken?-6:-9,-5);ctx.stroke();
  const headX=chicken?4:7,headY=animal.feeding?-3+Math.sin(animal.phase*7):chicken?-9:-7;
  ctx.fillStyle=animal.species==='sheep'?'#796d59':coat;
  ctx.beginPath();ctx.ellipse(headX,headY,chicken?2:3,chicken?2:2.6,animal.feeding?.5:0,0,Math.PI*2);ctx.fill();
  if(chicken){
    ctx.fillStyle='#b65943';ctx.fillRect(headX-1,headY-3,2,2);
    ctx.fillStyle='#d8a14d';ctx.beginPath();ctx.moveTo(headX+1,headY);ctx.lineTo(headX+4,headY+1);ctx.lineTo(headX+1,headY+1.5);ctx.fill();
  }else{
    ctx.fillStyle=animal.species==='pig'?'#bc7976':coat;
    ctx.beginPath();ctx.ellipse(headX-1,headY-2.5,2,1,-.5,0,Math.PI*2);ctx.fill();
    if(animal.species==='pig'){ctx.fillStyle='#bc7976';ctx.fillRect(headX+1,headY,3,2);}
    if(cow){ctx.strokeStyle='#baa67b';ctx.beginPath();ctx.moveTo(headX,headY-2);ctx.lineTo(headX+1,headY-5);ctx.stroke();}
  }
  ctx.fillStyle='#302e27';ctx.beginPath();ctx.arc(headX+1,headY-.7,.55,0,Math.PI*2);ctx.fill();
  ctx.restore();
}
function draw(seconds=0){
  if(typeof ClientScene!=='undefined'&&!animationFrameRunning)return;
  if(typeof ClientScene==='undefined')animalClock+=seconds;
  const rect=canvas.getBoundingClientRect(),dpr=renderPixelRatio();
  const width=Math.round(rect.width*dpr),height=Math.round(rect.height*dpr);
  if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}
  ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,rect.width,rect.height);
  const townKey=`${world.world_id}/${world.shared_map?'shared':viewed}`;
  const town=world.towns[viewed], own=world.towns[identity?.town_id];
  const hero=frameActors?.world===world?frameActors.hero:playerSprite(world.shared_map?(own || town):town,seconds);
  if(cameraTown!==townKey){
    const home=world.shared_map?(own || town):town;
    const center=home?.buildings?.find(b=>b.kind==='hall')
      || (world.shared_map?(home?.player || world.town_sites?.[0]):null) || {x:5.5,y:5.5};
    camera={x:center.x,y:center.y,zoom:1};cameraTown=townKey;
  }
  tileW=Math.min(54,(rect.width-30)/12)*camera.zoom;tileH=tileW/2;
  originX=rect.width/2-(camera.x-camera.y)*tileW/2;
  originY=rect.height/2-(camera.x+camera.y)*tileH/2-tileH/2;
  if(typeof camera==='undefined'&&hero&&playerView.following){
    originX=rect.width/2-(hero.x-hero.y)*tileW/2;
    originY=rect.height/2-(hero.x+hero.y)*tileH/2-tileH/2;
  }
  hitTargets=[];
  const resources=world.shared_map?.map_resources || town?.map_resources || [];
  const resourceData=renderResources(resources,tileW>=32),water=resourceData.water;
  const corners=[[-tileW,-100],[rect.width+tileW,-100],[-tileW,rect.height+100],[rect.width+tileW,rect.height+100]]
    .map(([px,py])=>({x:(px-originX)/tileW+(py-originY)/tileH,y:(py-originY)/tileH-(px-originX)/tileW}));
  const size=mapSize(town),minX=Math.max(0,Math.floor(Math.min(...corners.map(p=>p.x)))),maxX=Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.x))));
  const minY=Math.max(0,Math.floor(Math.min(...corners.map(p=>p.y)))),maxY=Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.y))));
  const showBuildingRange=town && (panel==='build' || (selection?.type==='building' && town.buildings[selection.index]?.kind==='hall'));
  const preview=hoverTile || keyboardTile;
  const previewKind=tool in costs?tool:tool==='move'&&moveIndex!==null?town?.buildings[moveIndex]?.kind:null;
  const sourceKind=previewKind==='woodcutter'?'tree':(previewKind==='farm'||previewKind==='fishing_hut')?'water':null;
  const previewReach=resourceReach(tool==='move'&&moveIndex!==null?town.buildings[moveIndex]:{kind:previewKind});
  const reachable=new Set(preview&&sourceKind?resources.filter(r=>r.kind===sourceKind&&Math.abs(r.x-preview.x)+Math.abs(r.y-preview.y)<=previewReach).map(r=>`${r.x},${r.y}`):[]);
  drawGround(rect,dpr,world.shared_map || town,water,{minX,maxX,minY,maxY},resourceData.forest);
  for(const stump of (world.shared_map || town)?.tree_stumps || []){
    if(stump.x<minX||stump.x>maxX||stump.y<minY||stump.y>maxY)continue;
    const [px,py]=point(stump.x,stump.y),scale=tileW/46;
    ctx.fillStyle='#775b3e';ctx.fillRect(px-4*scale,py+tileH*.5,8*scale,5*scale);
    ctx.fillStyle='#ba9869';ctx.beginPath();ctx.ellipse(px,py+tileH*.5,4*scale,2*scale,0,0,Math.PI*2);ctx.fill();
  }
  for(const road of mapRoads(town)){
    if(road.x<minX||road.x>maxX||road.y<minY||road.y>maxY)continue;
    drawRoad(road);
  }
  // Selection and construction overlays remain live above the cached ground.
  if(tool || showBuildingRange || reachable.size){
    for(let x=minX;x<=maxX;x++)for(let y=minY;y<=maxY;y++){
      const [px,py]=point(x,y);
      if(px+tileW/2<0 || px-tileW/2>rect.width || py+tileH+12<0 || py>rect.height)continue;
      if((world.shared_map || town)?.terrain?.[y]?.[x]==='v'||(town?.chunked&&!town.terrain?.[y]?.[x]))continue;
      const diamond=[[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]];
      if(tool)polygon(diamond,'#00000000','#91a37455');
      if(showBuildingRange && insideBuildingRange(town,{x,y}))polygon(diamond,'#e4c98022','#e4c98088');
      if(reachable.has(`${x},${y}`))polygon(diamond,'#b8dcad44','#c6dfaa');
      if(preview?.x===x&&preview?.y===y&&previewKind&&town){const valid=placementValid(town,preview,previewKind)&&buildingUsable(town,{...preview,kind:previewKind});ctx.save();ctx.lineWidth=2;polygon(diamond,valid?'#d6d8a366':'#c76d6455',valid?'#eee5ab':'#f3a295');ctx.restore();}
    }
  }
  const selected=selection?.type==='building'?town?.buildings[selection.index]:selection?.type==='road'?town?.roads?.[selection.index]:selection?.type==='resource'?resources[selection.index]:null;
  const highlights=[selected,!previewKind?keyboardTile:null].filter(Boolean);
  for(let i=0;i<highlights.length;i++){
    const {x,y}=highlights[i];
    if(i&&x===highlights[0].x&&y===highlights[0].y)continue;
    const [px,py]=point(x,y),landscape=world.shared_map || town;
    if(x<minX||x>maxX||y<minY||y>maxY||px+tileW/2<0||px-tileW/2>rect.width||py+tileH+12<0||py>rect.height)continue;
    if(landscape?.terrain?.[y]?.[x]==='v'||(town?.chunked&&!landscape?.terrain?.[y]?.[x]))continue;
    ctx.save();ctx.lineWidth=3;
    polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],'#fff0b133','#fff0b1');ctx.restore();
  }
  const entries=world.shared_map?Object.entries(world.towns):town?[[viewed,town]]:[];
  const buildings=(typeof ClientScene!=='undefined'&&ClientScene.current?.world===world?(world.shared_map?ClientScene.current.buildings:town?.buildings):null) ?? (world.shared_map?entries.flatMap(([id,t])=>t.buildings.map((b,index)=>({...b,townId:id,objectIndex:index}))):town?.buildings || [{kind:'hall',x:5,y:5}]);
  const workerLayout=(typeof ClientScene!=='undefined'&&ClientScene.current?.world===world?ClientScene.current.geometry:null) ?? ('['+JSON.stringify(buildings.map(({kind,x,y,farm_index,construction})=>[kind,x,y,farm_index,!!construction]))+','+resourceData.layout+']');
  const villagers=frameActors?.world===world?frameActors.villagers:entries.flatMap(([id,t])=>animatedVillagers(t,seconds,id,
    t?.chunked?townWorkerLayout(t,buildings,resources):workerLayout));
  const drawnPiers=new Set();
  if(draw.pierLayout!==workerLayout){
    draw.pierLayout=workerLayout;
    draw.piers=entries.flatMap(([,t])=>t.buildings.filter(b=>b.kind==='fishing_hut'&&(!b.construction||b.construction.upgrade))
      .map(hut=>WorkerAnimation.fishingPier({...t,obstacles:buildings,
        resourceBuckets:typeof ClientScene!=='undefined'&&ClientScene.current?.resources===resources?ClientScene.current.buckets:null},hut)));
  }
  for(const pier of [...draw.piers,...villagers.map(worker=>worker.state.pier)]){
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
  const otherPlayers=frameActors?.world===world?frameActors.otherPlayers:world.shared_map?entries.filter(([id,t])=>t.player&&id!==hero?.townId).map(([id,t])=>remotePlayerSprite(id,t,seconds)):[];
  const visible=b=>{
    const [px,py]=point(b.x,b.y);
    return px>=-tileW&&px<=rect.width+tileW&&py>=-tileH&&py<=rect.height+80*tileW/46;
  };
  const boats=fishingBoatSprites(entries,resources,villagers);
  const passengers=new Set(boats.flatMap(b=>b.crew));
  const objects=mergeDepthObjects([...buildings,...cropObjects,...boats].filter(visible),visibleResourceObjects(resourceData,rect),
    [...villagers.filter(v=>!passengers.has(v)),...npcSprites(world.shared_map || town,typeof ClientScene==='undefined'?seconds:0),...barnAnimalSprites(villageAnimalBuildings(buildings,resourceData),animalClock,world.shared_map || town),...otherPlayers,...(hero?[hero]:[])].filter(visible));
  for(const b of objects){
    const position=b.kind==='wild_horse'?wildHorsePosition(b):[b.x,b.y];
    const [px,py]=point(position[0],position[1]),scale=tileW/46;
    if(px < -tileW || px > rect.width+tileW || py < -tileH || py > rect.height+80*scale)continue;
    if(b.kind==='animal'){drawBarnAnimal(b);continue;}
    if(b.kind==='crop'){drawFarmCrop(b);continue;}
    if(b.kind==='fishing_boat'){
      drawFishingBoat(b);
      for(const [seat,passenger] of b.crew.entries()){
        const offset=(seat-(b.crew.length-1)/2)*.28;
        drawVillager({...passenger,aboard:true,x:b.x+offset,y:b.y-offset,state:{...passenger.state,path:[]}});
      }
      // The near hull covers seated passengers' feet inside the boat.
      const boatY=py+tileH/2;
      polygon([[px-23*scale,boatY-3*scale],[px-12*scale,boatY+5*scale],[px+10*scale,boatY+5*scale],
        [px+25*scale,boatY-4*scale],[px+12*scale,boatY+12*scale],[px-13*scale,boatY+12*scale]],b.level===3?'#a27640':'#805738','#493627');
      hitTargets.push({type:'building',index:b.objectIndex,townId:b.townId,left:px-25*scale,right:px+25*scale,top:py+(tileH/2)-(b.level>=2?32:10)*scale,bottom:py+tileH/2+13*scale});
      continue;
    }
    if(b.kind==='player'){
      const baseY=py+tileH*.65;
      const sailing=world.towns[b.townId]?.player?.boat&&water.has(`${Math.floor(b.x+.5)},${Math.floor(b.y+.5)}`);
      if(sailing){
        polygon([[px-20*scale,baseY],[px-10*scale,baseY+10*scale],[px+12*scale,baseY+10*scale],[px+21*scale,baseY-2*scale]],playerBoatLevel(world.towns[b.townId].player)===3?'#a57641':playerBoatLevel(world.towns[b.townId].player)===2?'#986b47':'#805838','#d4af6b');
      }
      ctx.strokeStyle='#ffe8a0';ctx.lineWidth=2;ctx.beginPath();ctx.ellipse(px,baseY,10*scale,5*scale,0,0,Math.PI*2);ctx.stroke();
      drawVillager({...b,mounted:b.mounted&&!sailing,leadingHorse:b.leadingHorse&&!sailing});
      if(tileW>=32 || b.townId===identity?.town_id || selection?.type==='player'&&selection.townId===b.townId){
        ctx.fillStyle='#fff0b1';ctx.font=`bold ${12*scale}px sans-serif`;ctx.textAlign='center';
        ctx.fillText(b.townId===identity?.town_id?'You':world.towns[b.townId]?.name || 'Founder',px,baseY-(b.mounted&&!sailing?43:31)*scale);
      }
      hitTargets.push({type:'player',index:0,townId:b.townId,left:px-(b.mounted?20:10)*scale,right:px+(b.mounted?24:10)*scale,top:baseY-(b.mounted?39:27)*scale,bottom:baseY+5*scale});
      continue;
    }
    const character=b.kind==='villager'||b.kind==='npc';
    const type=b.kind==='npc'?'npc':b.kind==='villager'?'villager':resourceData.indexes.has(b)?'resource':'building';
    const index=character?b.index:type==='resource'?resourceData.indexes.get(b):b.objectIndex ?? buildings.indexOf(b);
    const centerX=px+(character?(b.index%3-1)*3:0);
    const baseY=py+(character?tileH*.65+Math.floor(b.index/3):tileH/2);
    const owner=world.towns[b.townId] || town;
    const simplifiedResource=tileW<32&&(b.kind==='tree'||b.kind==='rock');
    const height=simplifiedResource?tileH/(2*scale):b.kind==='tree'?(b.species==='ancient'?94:70):character?26:b.kind==='acre'?0:b.kind==='rock'?80:type==='building'?buildingHeight(b,owner)+(b.kind==='hall'?(owner?.level>=4?35:33):18):45;
    const halfWidth=simplifiedResource?tileW*.45:character?9*scale:b.kind==='rock'?28*scale:tileW*.45;
    hitTargets.push({type,index,townId:b.townId || viewed,left:centerX-halfWidth,right:centerX+halfWidth,top:baseY-height*scale,bottom:baseY+(character?5*scale:tileH/2)});
    if(character&&selection?.type===type&&selection.index===b.index&&(!world.shared_map||b.townId===viewed||type==='npc')){
      ctx.fillStyle='#fff0b144';ctx.strokeStyle='#fff0b1';ctx.lineWidth=2;ctx.beginPath();ctx.ellipse(centerX,baseY,12*scale,6*scale,0,0,Math.PI*2);ctx.fill();ctx.stroke();
    }
    if(simplifiedResource)continue;
    if(['tree','rock','wild_horse','berry_bush','herb_patch','chest','village_hut','village_market'].includes(b.kind)){drawResource(b,resourceData.rockJoins.get(b));continue;}
    if(character){
      drawVillager(b);
      continue;
    }
    if(b.kind==='acre'&&!b.construction)continue;
    const worker=villagers.find(v=>v.active&&v.state.phase==='working'&&v.state.source?.x===b.x&&v.state.source?.y===b.y);
    drawBuilding(b,world.towns[b.townId] || town,worker?.state);
    if(world.shared_map && b.kind==='hall' && tileW>=32){ctx.fillStyle='#fff0b1';ctx.font=`bold ${12*scale}px sans-serif`;ctx.textAlign='center';ctx.fillText(world.towns[b.townId].name,px,py-(buildingHeight(b,owner)+38)*scale);}
  }
  drawBuildingEffects(seconds);
  const hover=buildingHover || (selection?.type==='building'?{index:selection.index,townId:viewed}:null);
  const hoverTown=world.towns[hover?.townId],hoverBuilding=hoverTown?.buildings[hover?.index];
  if(!tool&&hoverBuilding){
    const [px,py]=point(hoverBuilding.x,hoverBuilding.y),scale=tileW/46;
    const level=hoverBuilding.kind==='hall'?hoverTown.level:buildingLevel(hoverBuilding);
    const upgrades=(hoverBuilding.profession_upgrades || []).map(key=>world.progression?.upgrades?.[key]?.label || goodName(key));
    const lines=[`${labels[hoverBuilding.kind]} · Level ${level} · Capacity tier ${hoverBuilding.settler_level || 1}`,upgrades.join(' · ')].filter(Boolean);
    if(hoverBuilding.construction)lines.push('Under construction');
    ctx.save();ctx.font=`bold ${11*scale}px sans-serif`;ctx.textAlign='center';ctx.fillStyle='#fff0b1';
    lines.forEach((line,i)=>{ctx.strokeStyle='#233b30';ctx.lineWidth=3*scale;const y=py-(buildingHeight(hoverBuilding,hoverTown)+45+i*14)*scale;ctx.strokeText(line,px,y);ctx.fillText(line,px,y);});ctx.restore();
  }
  if(preview && previewKind && town && town.terrain?.[preview.y]?.[preview.x]!=='v' && viewed===identity?.town_id){
    const valid=placementValid(town,preview,previewKind);
    ctx.save();ctx.globalAlpha=valid ? .65 : .35;
    if(previewKind==='road')drawRoad(preview);
    else drawBuilding({...preview,kind:previewKind},town);
    ctx.restore();
  }
}
function acrePlacementValid(town,tile,index){
  const farm=town.buildings[index];
  return farm?.kind==='farm'&&!farm.construction&&Math.abs(tile.x-farm.x)+Math.abs(tile.y-farm.y)<=3;
}
function buildingRange(town){return 30+5*(town.level-1);}
function insideBuildingRange(town,tile){
  const hall=town.buildings.find(b=>b.kind==='hall');
  const origin=town.town_origin;
  return !!hall && Math.abs(tile.x-hall.x)+Math.abs(tile.y-hall.y)<=buildingRange(town)
    && (!origin || Math.abs(tile.x-origin.x)+Math.abs(tile.y-origin.y)<=Math.max(15,buildingRange(town)));
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
  const source={woodcutter:'tree',fishing_hut:'water',mine:'rock'}[b.kind];
  if(!source)return true;
  const resources=town?.map_resources;
  if(!resources)return false;
  if(buildingUsable.resources!==resources || buildingUsable.count!==resources.length){
    buildingUsable.resources=resources;buildingUsable.count=resources.length;buildingUsable.results=new Map();
  }
  const reach=resourceReach(b);
  const key=`${source}/${b.x}/${b.y}/${reach}`;
  if(!buildingUsable.results.has(key))buildingUsable.results.set(key,
    resources.some(r=>r.kind===source&&(b.kind!=='mine'||r.ore==='iron')&&Math.abs(r.x-b.x)+Math.abs(r.y-b.y)<=reach));
  return buildingUsable.results.get(key);
}
function drawFarmCrops(b,state,collectCrop=null){
  const elapsed=state?.elapsed ?? 6;
  const stage=Math.min(3,Math.floor(elapsed/2)),progress=(elapsed%2)/2;
  const cropType=['vegetables','wheat','carrots','fodder'].includes(b.crop)?b.crop:'vegetables';
  const colors={vegetables:['#68934e','#8eae60'],wheat:['#d9bd70','#b8b458'],
    carrots:['#dd9048','#efa754'],fodder:['#72a65c','#9bbf70']}[cropType];
  const [x,y]=point(b.x,b.y),scale=tileW/46;
  polygon([[x,y+tileH*.1],[x+tileW*.4,y+tileH*.5],[x,y+tileH*.9],[x-tileW*.4,y+tileH*.5]],'#80764a','#ab9561');
  if(tileW<32){
    const color=stage===0?'#655638':stage===1?'#80764a':stage===2?'#8db568':colors[0];
    polygon([[x,y+tileH*.25],[x+tileW*.28,y+tileH*.5],[x,y+tileH*.75],[x-tileW*.28,y+tileH*.5]],color);
    return;
  }
  const level=buildingLevel(b);
  if(level>=2){
    // Raised edging and irrigation stay on the ground, beneath crops and settlers.
    ctx.strokeStyle=level>=3?'#b5b9a0':'#c6a573';ctx.lineWidth=(level>=3?3:2)*scale;
    ctx.beginPath();ctx.moveTo(x-tileW*.4,y+tileH*.5);ctx.lineTo(x,y+tileH*.9);ctx.lineTo(x+tileW*.4,y+tileH*.5);ctx.stroke();
    if(level>=3){ctx.strokeStyle='#79a6ad';ctx.lineWidth=2*scale;ctx.beginPath();ctx.moveTo(x,y+tileH*.15);ctx.lineTo(x+tileW*.34,y+tileH*.5);ctx.stroke();}
  }
  for(let row=0;row<4;row++){
    const [ax,ay]=point(b.x-.3+row*.2,b.y-.35),[bx,by]=point(b.x-.3+row*.2,b.y+.35);
    ctx.strokeStyle='#655638';ctx.lineWidth=scale;
    ctx.beginPath();ctx.moveTo(ax,ay+tileH/2);ctx.lineTo(bx,by+tileH/2);ctx.stroke();
    for(let crop=0;crop<4;crop++){
      const order=row*4+crop;
      if(stage===0 || (stage===1&&order>=progress*16) || (state&&stage===3&&order<progress*16))continue;
      const x=b.x-.3+row*.2,y=b.y-.3+crop*.2;
      // Characters' feet sit .15 tile heights ahead of a crop's ground anchor.
      const stalk={kind:'crop',cropType,x,y,depth:x+y-.3,stage,
        height:stage===2?2+progress*4:6,
        sway:state?Math.sin(elapsed*4+order):0,
        color:stage===2?'#8db568':colors[(row+crop)%2]};
      if(collectCrop)collectCrop(stalk);else drawFarmCrop(stalk);
    }
  }
}
function drawFarmCrop(crop){
  const [px,py]=point(crop.x,crop.y),scale=tileW/46;
  if(crop.stage===1){
    ctx.fillStyle='#efd59a';ctx.fillRect(px-scale,py+tileH/2-scale,2*scale,2*scale);
  }else{
    const base=py+tileH/2,tip=px+crop.sway*scale;
    const height=crop.height*scale;
    if(crop.cropType==='vegetables'){
      polygon([[px,base],[tip-3*scale,base-height*.5],[tip-scale,base-height],
        [tip+scale,base-height*.65],[tip+3*scale,base-height*.8],[px+2*scale,base]],crop.color);
      ctx.strokeStyle='#b5cb83';ctx.lineWidth=scale;
      ctx.beginPath();ctx.moveTo(px,base);ctx.lineTo(tip,base-height*.8);ctx.stroke();
      return;
    }
    if(crop.cropType==='carrots'){
      if(crop.stage===3)polygon([[px,base+scale],[px-2*scale,base-2*scale],
        [px+2*scale,base-2*scale]],crop.color);
      ctx.strokeStyle=crop.stage===2?crop.color:'#63964c';ctx.lineWidth=scale;
      ctx.beginPath();
      for(const spread of [-2,0,2]){
        ctx.moveTo(px,base-2*scale);ctx.lineTo(tip+spread*scale,base-height+(spread===0?-scale:scale));
      }
      ctx.stroke();return;
    }
    if(crop.cropType==='fodder'){
      ctx.strokeStyle=crop.color;ctx.lineWidth=scale;ctx.beginPath();
      for(const spread of [-3,-1,1,3]){
        ctx.moveTo(px,base);ctx.lineTo(tip+spread*scale,base-height*(Math.abs(spread)===3?.65:1));
      }
      ctx.stroke();return;
    }
    ctx.strokeStyle=crop.color;ctx.lineWidth=2*scale;
    ctx.beginPath();ctx.moveTo(px,py+tileH/2);
    ctx.lineTo(px+crop.sway*scale,py+tileH/2-crop.height*scale);ctx.stroke();
    if(crop.stage===3){
      polygon([[tip,base-height-3*scale],[tip+2*scale,base-height-scale],
        [tip,base-height+scale],[tip-2*scale,base-height-scale]],'#efd59a');
    }
  }
}
function drawRoad(road){
  const [px,py]=point(road.x,road.y);
  ctx.save();if(road.construction)ctx.globalAlpha=.4;
  polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],'#b99c70','#a58b63');
  polygon([[px,py+tileH*.2],[px+tileW*.3,py+tileH*.5],[px,py+tileH*.8],[px-tileW*.3,py+tileH*.5]],'#c6aa7e');
  ctx.restore();
}
// Cache completed structures; moving details are drawn over the shared image.
const buildingMainContext=ctx;
const buildingSprites=new Map();
const buildingSpriteByteLimit=32*1024*1024;
let buildingSpriteBytes=0;
function drawBuilding(b,town=world.towns[viewed],farmState=null){
  if(b.construction || b.kind==='acre')return paintBuilding(b,town,farmState);
  const usable=buildingUsable(town,b),s=tileW/46;
  const resolution=2**Math.ceil(Math.log2(Math.max(.5,s*renderPixelRatio())));
  const bytes=128*144*resolution*resolution*4;
  const key=JSON.stringify([b.kind,buildingVisualLevel(b,town),b.kind==='hall'?town?.level || 1:buildingLevel(b),b.settler_level || 1,
    [...(b.profession_upgrades || [])].sort(),
    usable,resolution,tileW>=32]);
  let sprite=buildingSprites.get(key);
  const [x,top]=point(b.x,b.y);
  if(!sprite && bytes<=buildingSpriteByteLimit && typeof document!=='undefined'){
    const surface=document.createElement('canvas'),target=surface.getContext?.('2d');
    if(target){
      while(buildingSprites.size>=128 || buildingSpriteBytes+bytes>buildingSpriteByteLimit){
        const oldest=buildingSprites.keys().next().value,evicted=buildingSprites.get(oldest);
        buildingSpriteBytes-=evicted.width*evicted.height*4;buildingSprites.delete(oldest);
      }
      surface.width=128*resolution;surface.height=144*resolution;
      target.setTransform(resolution/s,0,0,resolution/s,(64-x/s)*resolution,(112-top/s)*resolution);
      paintBuilding(b,town,null,target,usable);
      sprite=surface;buildingSprites.set(key,sprite);buildingSpriteBytes+=bytes;
    }
  }
  if(sprite){
    buildingSprites.delete(key);buildingSprites.set(key,sprite);
    ctx.drawImage(sprite,x-64*s,top-112*s,128*s,144*s);
  }else paintBuilding(b,town,null,ctx,usable);
  drawBuildingAnimation(b,town,farmState?.elapsed || 0);
}
function paintBuilding(b,town,farmState=null,target=ctx,usable=buildingUsable(town,b)){
  const ctx=target;
  const paintPolygon=target===buildingMainContext?polygon:(points,fill,stroke)=>{
    ctx.beginPath();points.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));
    ctx.closePath();ctx.fillStyle=fill;ctx.fill();
    if(stroke){ctx.strokeStyle=stroke;ctx.stroke();}
  };

  if(b.construction){
    const [x,top]=point(b.x,b.y),y=top+tileH/2,s=tileW/46;
    const progress=b.construction.progress/b.construction.total,w=tileW*.38,d=w*tileH/tileW;
    paintPolygon([[x-w,y],[x,y+d],[x+w,y],[x,y-d]],'#8b795c','#d0b68a');
    ctx.strokeStyle='#d0b68a';ctx.lineWidth=3*s;
    if(progress>=1/3){
      for(const [px,py] of [[x-w,y],[x,y+d],[x+w,y]]){
        ctx.beginPath();ctx.moveTo(px,py);ctx.lineTo(px,py-25*s);ctx.stroke();
      }
      paintPolygon([[x-w,y-25*s],[x,y+d-25*s],[x+w,y-25*s],[x,y-d-25*s]],'#b69a7044','#d0b68a');
    }
    if(progress>=2/3){
      paintPolygon([[x-w,y],[x,y+d],[x,y+d-25*s],[x-w,y-25*s]],colors[b.kind] || '#b69a70');
      paintPolygon([[x,y+d],[x+w,y],[x+w,y-25*s],[x,y+d-25*s]],'#857659');
      paintPolygon([[x-w,y-25*s],[x,y-40*s],[x+w,y-25*s],[x,y+d-24*s]],'#80574b');
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
  const level=buildingVisualLevel(b,town),upgrades=b.profession_upgrades || [];
  const w=tileW*(b.kind==='storehouse' ? .43 : .38),depth=w*tileH/tileW,h=buildingHeight(b,town)*s;
  // Wall bases share the tile's ground plane and its isometric slope.
  ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(x+8*s,y+depth*.6,w+8*s,depth+2*s,0,0,Math.PI*2);ctx.fill();
  paintPolygon([[x-w,y],[x,y+depth],[x,y+depth-h],[x-w,y-h]],usable?(colors[b.kind] || '#a28c72'):'#c76d64');
  paintPolygon([[x,y+depth],[x+w,y],[x+w,y-h],[x,y+depth-h]],usable?'#857659':'#a34f49');
  if(tileW>=32&&(level>=2 || upgrades.includes('timber_frame'))){
    ctx.strokeStyle='#574b38';ctx.lineWidth=2*s;
    for(const [px,py] of [[x-w,y],[x,y+depth],[x+w,y]]){ctx.beginPath();ctx.moveTo(px,py);ctx.lineTo(px,py-h);ctx.stroke();}
    ctx.beginPath();ctx.moveTo(x-w,y-h*.55);ctx.lineTo(x,y+depth-h*.55);ctx.lineTo(x+w,y-h*.55);ctx.stroke();
  }
  if(upgrades.includes('timber_frame')){
    ctx.strokeStyle='#574b38';ctx.lineWidth=2*s;
    ctx.beginPath();ctx.moveTo(x-w,y-h);ctx.lineTo(x,y+depth-h*.55);ctx.moveTo(x,y+depth-h);ctx.lineTo(x-w,y-h*.55);ctx.stroke();
  }
  if(level>=3){
    paintPolygon([[x-w,y],[x,y+depth],[x,y+depth-9*s],[x-w,y-9*s]],usable?'#b1b3a2':'#c76d64');
    paintPolygon([[x,y+depth],[x+w,y],[x+w,y-9*s],[x,y+depth-9*s]],usable?'#858e86':'#a34f49');
    ctx.strokeStyle='#666f64';ctx.lineWidth=s;
    ctx.beginPath();ctx.moveTo(x-w,y-4*s);ctx.lineTo(x,y+depth-4*s);ctx.lineTo(x+w,y-4*s);ctx.stroke();
  }
  paintPolygon([[x-w,y],[x,y+depth],[x,y+depth-3*s],[x-w,y-3*s]],'#797665');
  paintPolygon([[x,y+depth],[x+w,y],[x+w,y-3*s],[x,y+depth-3*s]],'#5e6255');
  if(b.kind==='storehouse'&&buildingLevel(b)>=5){
    paintPolygon([[x-22*s,y-17*s],[x+4*s,y-30*s],[x+26*s,y-17*s],[x,y-4*s]],'#596e68');
    ctx.fillStyle='#d3bc83';ctx.fillRect(x-7*s,y-17*s,14*s,5*s);
  }
  const roofColors={home:'#98634f',woodcutter:'#7b714d',farm:'#af8e4f',storehouse:'#596e68',fishing_hut:'#537f91',barn:'#a4594d',sawpit:'#8e7752',mill:'#c2ac76',smokehouse:'#666c72',workshop:'#638078',fish_pond:'#497f85',orchard:'#65854e',market:'#bd8b59',bakery:'#aa744d',butcher:'#995d67'};
  const roof=b.kind==='hall'?['#5a747b','#52767b','#476975','#405c70','#374e65'][level-1]:roofColors[b.kind] || '#80574b';
  paintPolygon([[x-w-3*s,y-h],[x,y-h-15*s],[x+w+3*s,y-h],[x,y-h+depth+1*s]],usable?roof:'#a34f49');
  if(tileW<32&&b.kind!=='hall'){
    ctx.fillStyle='#3d4438';ctx.fillRect(x-6*s,y-h+13*s,5*s,9*s);return;
  }
  if(level>=2){
    ctx.strokeStyle=level>=3?'#c0bd9d':'#c9a579';ctx.lineWidth=2*s;
    ctx.beginPath();ctx.moveTo(x-w-3*s,y-h);ctx.lineTo(x,y-h+depth+s);ctx.lineTo(x+w+3*s,y-h);ctx.stroke();
  }
  if(level>=3){
    paintPolygon([[x-10*s,y-h],[x-3*s,y-h-8*s],[x+4*s,y-h],[x+4*s,y-h+5*s],[x-10*s,y-h+5*s]],'#c8b68c');
    paintPolygon([[x-12*s,y-h],[x-3*s,y-h-10*s],[x+6*s,y-h]],usable?roof:'#a34f49');
    ctx.fillStyle='#edc77d';ctx.fillRect(x-5*s,y-h,4*s,4*s);
  }
  ctx.fillStyle='#3d4438';ctx.fillRect(x-6*s,y-h+13*s,5*s,9*s);
  // Warm windows and a hanging lantern.
  ctx.save();ctx.shadowColor='#f1c779';ctx.shadowBlur=7*s;ctx.fillStyle='#edc77d';
  ctx.fillRect(x+5*s,y-h+12*s,5*s,6*s);ctx.fillRect(x-12*s,y-h+9*s,3*s,4*s);ctx.restore();
  ctx.fillStyle='#594d38';ctx.fillRect(x-12*s,y-h+10*s,3*s,s);
  ctx.fillRect(x-22*s,y-15*s,2*s,18*s);ctx.fillRect(x-22*s,y-15*s,7*s,2*s);
  ctx.save();ctx.shadowColor='#f1c779';ctx.shadowBlur=9*s;ctx.fillStyle='#edc77d';ctx.fillRect(x-18*s,y-12*s,4*s,5*s);ctx.restore();
  if(['barn','sawpit','mill','smokehouse','workshop','fish_pond','orchard','market','bakery','butcher'].includes(b.kind)){
    ctx.fillStyle='#fff0b1';ctx.font=`${10*s}px sans-serif`;ctx.textAlign='center';ctx.fillText(labels[b.kind],x,y-h-20*s);
    if(b.kind==='bakery'){ctx.fillStyle='#d3ab69';ctx.beginPath();ctx.ellipse(x,y-8*s,9*s,5*s,0,0,Math.PI*2);ctx.fill();}
    if(b.kind==='butcher'){ctx.fillStyle='#b66e65';ctx.fillRect(x-7*s,y-14*s,14*s,10*s);ctx.fillStyle='#eadac1';ctx.fillRect(x-2*s,y-12*s,4*s,6*s);}
    if(b.kind==='fish_pond'){ctx.fillStyle='#6ba2af';ctx.beginPath();ctx.ellipse(x,y+5*s,16*s,6*s,0,0,Math.PI*2);ctx.fill();}
    if(b.kind==='orchard'){ctx.fillStyle='#719451';ctx.beginPath();ctx.arc(x+10*s,y-8*s,9*s,0,Math.PI*2);ctx.fill();ctx.fillStyle='#cc8962';ctx.fillRect(x+7*s,y-12*s,3*s,3*s);}
  }
  if(b.kind==='home'){
    ctx.fillStyle='#85745f';ctx.fillRect(x+7*s,y-h-15*s,5*s,13*s);
    ctx.fillStyle='#b9a18a';ctx.fillRect(x+6*s,y-h-16*s,7*s,3*s);
  }
  if(b.kind==='woodcutter'){
    for(let i=0;i<3;i++){ctx.fillStyle='#705337';ctx.fillRect(x-18*s+i*5*s,y+3*s,5*s,6*s);ctx.fillStyle='#cda875';ctx.beginPath();ctx.arc(x-16*s+i*5*s,y+6*s,2*s,0,Math.PI*2);ctx.fill();}
    paintPolygon([[x-20*s,y-h],[x-7*s,y-h-7*s],[x+9*s,y-h+1*s],[x-4*s,y-h+9*s]],'#9b825a');
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
    for(let i=0;i<2;i++){const cx=x+9*s+i*6*s,cy=y+1*s-i*3*s;ctx.fillStyle='#ac895d';ctx.fillRect(cx,cy,6*s,7*s);paintPolygon([[cx,cy],[cx+6*s,cy+7*s],[cx+6*s,cy],[cx,cy+7*s]],'#c4a477');}
    ctx.fillStyle='#526c69';ctx.fillRect(x-14*s,y-h-3*s,28*s,2*s);
  }
  if(b.kind==='hall'){paintPolygon([[x+11*s,y-h+6*s],[x+17*s,y-h+3*s],[x+17*s,y-h+22*s],[x+14*s,y-h+19*s],[x+11*s,y-h+25*s]],'#d6c478');}
  if(b.kind==='hall'){ctx.fillStyle='#d6c478';ctx.fillRect(x,y-h-30*s,2*s,17*s);paintPolygon([[x+2*s,y-h-30*s],[x+16*s,y-h-26*s],[x+2*s,y-h-21*s]],'#d6c478');}
  if(b.kind==='hall'&&(b.settler_level || 1)>=2){
    ctx.fillStyle='#edc77d';
    for(let i=0;i<b.settler_level-1;i++)ctx.fillRect(x-15*s+i*6*s,y-h+17*s,3*s,5*s);
  }
  if(b.kind==='hall'&&level>=4){
    paintPolygon([[x-17*s,y-h-4*s],[x-10*s,y-h],[x-10*s,y-h-22*s],[x-17*s,y-h-26*s]],'#b1b3a2');
    paintPolygon([[x-10*s,y-h],[x-3*s,y-h-4*s],[x-3*s,y-h-26*s],[x-10*s,y-h-22*s]],'#858e86');
    paintPolygon([[x-19*s,y-h-26*s],[x-10*s,y-h-34*s],[x-s,y-h-26*s],[x-10*s,y-h-22*s]],roof);
    ctx.fillStyle='#edc77d';ctx.fillRect(x-8*s,y-h-19*s,3*s,5*s);
    if(level>=5){ctx.fillStyle='#d6c478';ctx.fillRect(x-17*s,y-h-19*s,3*s,14*s);ctx.fillRect(x+11*s,y-h-7*s,4*s,19*s);}
  }
  if(upgrades.includes('axe')||upgrades.includes('tools')||upgrades.includes('saw')){
    ctx.strokeStyle='#c6a573';ctx.lineWidth=2*s;ctx.beginPath();ctx.moveTo(x+12*s,y-3*s);ctx.lineTo(x+12*s,y-18*s);ctx.stroke();
    paintPolygon([[x+12*s,y-18*s],[x+19*s,y-20*s],[x+19*s,y-(upgrades.includes('saw')?8:14)*s],[x+12*s,y-14*s]],upgrades.includes('tools')?'#c5d4d0':'#98aaa7');
  }
  if(upgrades.includes('nets')||upgrades.includes('wool_nets')){
    ctx.strokeStyle=upgrades.includes('wool_nets')?'#eee1bc':'#bec8b7';ctx.lineWidth=s;
    for(let i=0;i<4;i++){
      ctx.beginPath();ctx.moveTo(x-16*s+i*3*s,y-16*s);ctx.lineTo(x-13*s+i*3*s,y-4*s);ctx.stroke();
      ctx.beginPath();ctx.moveTo(x-16*s,y-16*s+i*4*s);ctx.lineTo(x-4*s,y-13*s+i*4*s);ctx.stroke();
    }
  }
  if(fishingBoatLevel(b)){
    ctx.fillStyle=['','#c29a63','#ece3c5','#d4af6b'][fishingBoatLevel(b)];
    ctx.fillRect(x+10*s,y-9*s,8*s,8*s);
    ctx.fillStyle='#493627';ctx.font=`bold ${6*s}px sans-serif`;ctx.textAlign='center';
    ctx.fillText(String(fishingBoatLevel(b)),x+14*s,y-3*s);
  }

  ctx.save();ctx.translate(x,y);ctx.scale(s,s);
  drawBuildingFeatures(b,h/s,ctx,paintPolygon);
  // Wall plaques separate structural level from the independently purchased slots.
  ctx.fillStyle='#344b42';ctx.fillRect(-15,-h/s+17,10,6);
  ctx.fillStyle='#fff0b1';ctx.font='bold 5px sans-serif';ctx.textAlign='center';
  ctx.fillText(String(b.kind==='hall'?town?.level || 1:buildingLevel(b)),-10,-h/s+22);
  if((b.settler_level || 1)>1){
    ctx.fillStyle='#6baeb5';ctx.fillRect(5,-h/s+22,10,5);
    ctx.fillStyle='#eee0bd';for(let i=0;i<b.settler_level;i++){ctx.beginPath();ctx.arc(7+i*3,-h/s+24,1,0,Math.PI*2);ctx.fill();}
  }
  ctx.restore();
}
function drawBuildingAnimation(b,town,time){
  if(tileW<32 || !['mill','smokehouse','bakery','fish_pond'].includes(b.kind))return;
  const [x,top]=point(b.x,b.y),s=tileW/46,height=buildingHeight(b,town);
  ctx.save();ctx.translate(x,top+tileH/2);ctx.scale(s,s);
  if(b.kind==='mill'){
    ctx.strokeStyle='#e0ce97';ctx.lineWidth=3;
    for(let i=0;i<4;i++){
      const a=i*Math.PI/2+Math.PI/4+time*2;
      ctx.beginPath();ctx.moveTo(0,-height);ctx.lineTo(Math.cos(a)*17,-height+Math.sin(a)*17);ctx.stroke();
    }
  }else if(b.kind==='fish_pond'){
    ctx.strokeStyle='#b8d8d5';ctx.lineWidth=1;
    ctx.beginPath();ctx.ellipse(0,5,4+Math.sin(time*4),2,0,0,Math.PI*2);ctx.stroke();
  }else if(time){
    ctx.fillStyle='#c7c6af66';
    for(let i=0;i<3;i++){
      const rise=(time*.4+i/3)%1;
      ctx.beginPath();ctx.arc(12+Math.sin(rise*5)*3,-height-20-rise*12,2+rise*3,0,Math.PI*2);ctx.fill();
    }
  }
  ctx.restore();
}
function drawBuildingFeatures(b,height,ctx,polygon){
  if(b.kind==='stable'){
    ctx.strokeStyle='#6a513a';ctx.lineWidth=2;for(const x of [-18,0,18]){ctx.beginPath();ctx.moveTo(x,8);ctx.lineTo(x,-7);ctx.stroke();}ctx.beginPath();ctx.moveTo(-18,1);ctx.lineTo(18,1);ctx.stroke();
  }else if(['mine','smelter','steelworks'].includes(b.kind)){
    ctx.fillStyle='#4c493f';ctx.fillRect(-13,-17,16,17);ctx.strokeStyle='#b6a17f';ctx.lineWidth=3;ctx.strokeRect(-13,-17,16,17);
    ctx.fillStyle=b.kind==='mine'?'#c48658':'#f2ad52';ctx.fillRect(-10,-5,10,3);
    if(b.kind!=='mine'){ctx.fillStyle='#777e79';ctx.fillRect(9,-height-14,7,18);}
  }else if(['tannery','saddler','tradehouse'].includes(b.kind)){
    ctx.fillStyle='#674c33';ctx.fillRect(-18,-3,34,4);ctx.fillStyle=b.kind==='tradehouse'?'#c9ac75':'#bc8a57';ctx.fillRect(-11,-13,18,10);ctx.strokeStyle='#e4cea4';ctx.lineWidth=1;ctx.strokeRect(-11,-13,18,10);
  }else if(b.kind==='barn'){
    ctx.fillStyle='#633f34';ctx.fillRect(-12,-23,12,23);
    ctx.strokeStyle='#e3cba1';ctx.lineWidth=2;ctx.strokeRect(-12,-23,12,23);
    ctx.beginPath();ctx.moveTo(-12,-23);ctx.lineTo(0,0);ctx.moveTo(0,-23);ctx.lineTo(-12,0);ctx.stroke();
    for(let i=0;i<3;i++){ctx.fillStyle='#d6b765';ctx.fillRect(5+i*4,3,3,7);}
  }else if(b.kind==='sawpit'){
    polygon([[-19,4],[0,13],[19,4],[0,-5]],'#4e4134','#b28c5d');
    ctx.fillStyle='#b78b57';ctx.fillRect(-17,-1,33,5);
    ctx.fillStyle='#cda875';ctx.beginPath();ctx.arc(16,1,3,0,Math.PI*2);ctx.fill();
    polygon([[-14,-9],[15,-9],[12,-5],[-11,-5]],'#c4d2ce');
  }else if(b.kind==='smokehouse'||b.kind==='bakery'){
    ctx.fillStyle='#7b8075';ctx.fillRect(9,-height-16,7,18);
    ctx.fillStyle='#b1b3a2';ctx.fillRect(8,-height-18,9,3);
    if(b.kind==='bakery'){polygon([[-15,0],[-15,-9],[-9,-14],[-3,-9],[-3,0]],'#b59878');ctx.fillStyle='#463b32';ctx.fillRect(-12,-8,6,8);ctx.fillStyle='#e7af5d';ctx.fillRect(-11,-3,4,2);}
    else{ctx.strokeStyle='#a78454';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(-17,-17);ctx.lineTo(-2,-17);ctx.stroke();for(let i=0;i<3;i++)polygon([[-15+i*5,-16],[-12+i*5,-12],[-15+i*5,-7],[-18+i*5,-12]],'#b28c75');}
  }else if(b.kind==='workshop'){
    ctx.fillStyle='#ac895d';ctx.fillRect(-15,-5,30,5);ctx.fillRect(-12,0,3,7);ctx.fillRect(10,0,3,7);
    polygon([[-5,-10],[8,-10],[4,-6],[-2,-6]],'#84958e');
    ctx.strokeStyle='#d5dfd8';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(9,-height+8);ctx.lineTo(9,-height+18);ctx.moveTo(5,-height+8);ctx.lineTo(13,-height+8);ctx.stroke();
  }else if(b.kind==='market'){
    for(let i=0;i<5;i++)polygon([[-22+i*9,-height],[-13+i*9,-height],[-9+i*9,-height+13],[-18+i*9,-height+13]],i%2?'#eee0bd':'#698a72');
    ctx.fillStyle='#ac895d';ctx.fillRect(-18,-2,36,5);
    for(let i=0;i<5;i++){ctx.fillStyle=i%2?'#cc8962':'#d9bd70';ctx.beginPath();ctx.arc(-13+i*6,-4,3,0,Math.PI*2);ctx.fill();}
  }else if(b.kind==='fish_pond'){
    ctx.strokeStyle='#b1b3a2';ctx.lineWidth=3;ctx.beginPath();ctx.ellipse(0,5,18,7,0,0,Math.PI*2);ctx.stroke();
    polygon([[-8,5],[-3,3],[2,5],[-3,7]],'#d5dfd8');
  }else if(b.kind==='orchard'){
    for(const cx of [-13,12]){ctx.fillStyle='#806448';ctx.fillRect(cx-1,-12,3,17);ctx.fillStyle='#719451';ctx.beginPath();ctx.arc(cx,-17,9,0,Math.PI*2);ctx.fill();ctx.fillStyle='#cc8962';for(let i=0;i<3;i++){ctx.beginPath();ctx.arc(cx-5+i*4,-18+i%2*5,2,0,Math.PI*2);ctx.fill();}}
  }else if(b.kind==='butcher'){
    polygon([[-22,-height+2],[17,-height+2],[22,-height+12],[-17,-height+12]],'#b66e65');
    ctx.fillStyle='#eadac1';ctx.fillRect(-5,-height+3,3,8);ctx.fillRect(-9,-height+6,11,2);
  }
}
async function selectTile(x,y){
  const town=world.towns[viewed];
  if(!town || x<0||y<0||x>=mapSize(town)||y>=mapSize(town))return;
  if(town.terrain?.[y]?.[x]==='v')return;
  const npc=(town.npcs || []).findIndex(n=>{const p=npcPosition(n);return Math.round(p.x)===x&&Math.round(p.y)===y;});
  const resource=town.map_resources.findIndex(r=>r.x===x&&r.y===y);
  const index=town.buildings.findIndex(b=>b.x===x&&b.y===y);
  if(!tool){
    if(world.shared_map){
      for(const [id,t] of Object.entries(world.towns)){const i=t.buildings.findIndex(b=>b.x===x&&b.y===y);if(i>=0){inspect('building',i,id);return;}}
    }
    if(npc>=0&&tileW>=32)inspect('npc',npc);
    else if(index>=0)inspect('building',index);
    else if(resource>=0)inspect('resource',resource);
    else {
      const towns=world.shared_map?Object.entries(world.towns):[[viewed,town]];
      for(const [id,t] of towns){const i=(t.roads || []).findIndex(r=>r.x===x&&r.y===y);if(i>=0){inspect('road',i,id);return;}}
      openPanel(null);
    }
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
function hitTargetAt(px,py){
  const hits=hitTargets.filter(h=>px>=h.left&&px<=h.right&&py>=h.top&&py<=h.bottom).reverse();
  const front=hits[0];
  // Nearby tree canopies must not block a building's clickable area.
  return front?.type==='resource'?(hits.find(h=>h.type==='building') || front):front;
}
canvas.onmousemove=event=>{
  const rect=canvas.getBoundingClientRect(),px=event.clientX-rect.left,py=event.clientY-rect.top;
  const hit=hitTargetAt(px,py);
  buildingHover=!tool&&hit?.type==='building'?{index:hit.index,townId:hit.townId}:null;
  const town=world.towns[viewed],tile=tileAt(event);
  buildTile=(tool in costs)&&viewed===identity?.town_id&&town&&insideBuildingRange(town,tile)&&tile.x>=0&&tile.y>=0&&tile.x<mapSize(town)&&tile.y<mapSize(town)
    &&(tool!=='acre'||acrePlacementValid(town,tile,acreFarmIndex))&&town.terrain?.[tile.y]?.[tile.x]!=='v'&&!(town.npcs || []).some(n=>n.x===tile.x&&n.y===tile.y)&&!town.buildings.some(b=>b.x===tile.x&&b.y===tile.y)&&!town.map_resources.some(r=>r.x===tile.x&&r.y===tile.y)?tile:null;
  hoverTile=((tool in costs)||tool==='move'&&moveIndex!==null)&&viewed===identity?.town_id&&town&&tile.x>=0&&tile.y>=0&&tile.x<(town.map_size || 12)&&tile.y<(town.map_size || 12)?tile:null;
};
canvas.onmouseleave=()=>{buildTile=null;hoverTile=null;buildingHover=null;};
canvas.onclick=async event=>{
  if(event.button!==undefined&&event.button!==0)return;
  canvas.focus?.();
  const rect=canvas.getBoundingClientRect(),px=event.clientX-rect.left,py=event.clientY-rect.top;
  keyboardTile=null;
  if(!tool){
    const hit=hitTargetAt(px,py);
    if(hit){
      if(hit.type==='villager'&&selection?.type==='villager'&&selection.index===hit.index&&(!hit.townId||hit.townId===viewed))openPanel(null);
      else inspect(hit.type,hit.index,hit.townId);
      return;
    }
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
  }else if(event.key==='Enter'){event.preventDefault();const tile=keyboardTile || (world.shared_map?{x:Math.round(camera.x),y:Math.round(camera.y)}:{x:5,y:5});selectTile(tile.x,tile.y);}
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
if(window.ResizeObserver){
  const toolbarObserver=new window.ResizeObserver(entries=>{
    $('game').style.setProperty('--toolbar-height',`${entries[0].target.getBoundingClientRect().height}px`);
  });
  toolbarObserver.observe($('toolbar'));
}
window.addEventListener('resize',()=>draw());
function updateActors(seconds){
  const town=world.towns[viewed],own=world.towns[identity?.town_id];
  const hero=playerSprite(world.shared_map?(own || town):town,seconds);
  const entries=world.shared_map?Object.entries(world.towns):town?[[viewed,town]]:[];
  const buildings=ClientScene.current?.buildings || [];
  const villagers=entries.flatMap(([id,t])=>animatedVillagers(t,seconds,id,
    ClientScene.region(t)?.layout ?? townWorkerLayout(t,buildings,t.map_resources || [])));
  animalClock+=seconds;npcClock+=seconds;
  for(let i=buildingEffects.length-1;i>=0;i--){buildingEffects[i].age+=seconds;if(buildingEffects[i].age>=1.2)buildingEffects.splice(i,1);}
  const otherPlayers=world.shared_map?entries.filter(([id,t])=>t.player&&id!==hero?.townId).map(([id,t])=>remotePlayerSprite(id,t,seconds)):[];
  frameActors={world,hero,villagers,otherPlayers};
}
function animate(now){
  const seconds=lastFrame===null?0:Math.min((now-lastFrame)/1000,.1);lastFrame=now;
  if(typeof ClientScene!=='undefined'){
    updateActors(seconds);ClientScene.queue.run(2);
    for(const sprite of frameActors.villagers){sprite.x=sprite.state.x;sprite.y=sprite.state.y;}
  }
  animationFrameRunning=true;
  try{draw(typeof ClientScene==='undefined'?seconds:0);}finally{animationFrameRunning=false;}
  if(world.shared_map?.chunked&&!refreshing&&requestedMapView!==mapChunkKeys().join(';')){
    refresh().catch(()=>notice('Cannot load this part of the map. Waiting for the host…'));
  }
  requestAnimationFrame(animate);
}
document.addEventListener?.('visibilitychange',()=>{lastFrame=null;clearWalk();});
requestAnimationFrame(animate);
refresh().catch(()=>notice('Cannot reach the host. Check the connection.'));
setInterval(()=>{if(document.activeElement?.tagName!=='SELECT')refresh().catch(()=>notice('Connection lost. Waiting for the host…'));},2000);

setInterval(()=>refreshPlayers().catch(()=>{}),50);
