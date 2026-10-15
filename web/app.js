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
let orchardIndex = null;
let panel = null, selection = null, hitTargets = [], keyboardTile = null;
let catalog = {worlds:[], can_manage:false}, menuStarted = false, refreshing = null, menuBusy = false, deletingWorld = null;
let regeneratingWorld = null;
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
let workerTown = null, lastFrame = null, animalClock = 0, worldReceivedAt = null;
const walkKeys = new Set();
const defaultBindings={up:'w',left:'a',down:'s',right:'d',camera:'y',fish:'f',chop:'c',gather:'g',build:'b',interact:'i',mine:'m'};
const bindingLabels={up:'Walk up',left:'Walk left',down:'Walk down',right:'Walk right',camera:'Toggle camera lock',fish:'Fish',chop:'Cut wood',gather:'Gather berries and herbs',build:'Toggle building mode',interact:'Interact with nearby NPC',mine:'Clear grassland stone'};
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
  $('movement-hint').textContent=`${['up','left','down','right'].map(action=>keybindings[action].toUpperCase()).join('/')} to walk · 7 Help`;
  $('help-walk-keys').textContent=['up','left','down','right'].map(action=>keybindings[action].toUpperCase()).join('/');
  for(const action of ['camera','fish','chop','gather','mine','build','interact'])$('help-key-'+action).textContent=keybindings[action].toUpperCase();
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
    && !$('world-menu').open && !$('delete-world-dialog').open && !$('regenerate-world-dialog').open && !$('upgrade-dialog').open
    && !$('settings-panel').open && !$('resources-panel').open && !document.activeElement?.isContentEditable
    && !['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName)
    && (allowBuilding || document.activeElement?.tagName!=='BUTTON');
}
window.addEventListener('keyup',event=>walkKeys.delete(event.key.toLowerCase()));
window.addEventListener('blur',clearWalk);
document.addEventListener?.('visibilitychange',clearWalk);
// Buffer at most half a second of input while a host reply is delayed.
let movementMap=null;
const workerTools={
  crafted_axe:{name:'Crafted axe',job:'woodcutting',bonus:.25},
  crafted_saw:{name:'Crafted saw',job:'woodworking',bonus:.25},
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
    row.textContent=`${gear.name} · +${gear.bonus*100}% ${goodName(gear.job).toLowerCase()} output${v.worker_tool==='crafted_saw'?' and ingredients per batch':''}`;
    if(owner){const button=document.createElement('button');button.textContent='Take back tool';button.onclick=()=>act({action:'take_worker_tool',index});row.append(button);}
    target.append(row);
  }else if(owner)for(const [key,gear] of Object.entries(workerTools)){
    if(availableWorkerTools(town,key)<1)continue;
    const button=document.createElement('button');button.textContent=`Give ${gear.name.toLowerCase()} · +${gear.bonus*100}% ${goodName(gear.job).toLowerCase()} output${key==='crafted_saw'?' and ingredients per batch':''}`;
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
      .filter(b=>b.kind!=='tree'&&b.kind!=='berry_bush'&&!(b.kind==='water'&&town.player?.boat)).flatMap(b=>footprintTiles(b).map(p=>`${p.x},${p.y}`)))};
  }
  const size=town.map_size || 12,terrain=town.terrain;
  const fences=barnFenceSegments(world.shared_map?Object.values(world.towns).flatMap(t=>t.buildings || []):town.buildings || [],size);
  const open=(x,y)=>{
    if(x<-.32 || y<-.32 || x>size-.68 || y>size-.68 || barnFenceBlocked(x,y,fences))return false;
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
    const steps=s.steps.slice(),predicted=s.target,data={action:'walk',...steps[0],world_id:world.world_id};
    if(steps.length>1)data.steps=steps;
    const controller=new AbortController(),timeout=setTimeout(()=>controller.abort(),1000);
    request('/api/command',data,{signal:controller.signal}).then(result=>{
      if(playerView===s){
        const unrendered=new Set(s.segments.map(segment=>segment.step));
        s.steps.splice(0,steps.length);
        // Confirmation changes input ownership, not the unfinished playback path.
        // Only a host correction may replace already acknowledged waypoints.
        const confirmed=playerMovementDistance(result.player.x-predicted.x,result.player.y-predicted.y)<1e-6;
        s.segments=confirmed?s.segments.filter(segment=>!s.steps.includes(segment.step)):[];
        s.authoritative={...result.player};s.target={...result.player};
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
  const moving=distance>.001||s.segments.some(segment=>playerMovementDistance(segment.target.x-s.x,segment.target.y-s.y)>.001);
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
  s.facing=(s.segments[0]?.target || s.target).facing;s.clock+=seconds;s.path=moving?[s.target]:[];s.phase='idle';
  const activity=town.player.activity;
  s.source=activity;s.castTarget=activity;s.elapsed=s.clock%3;
  const working=!s.path.length&&!!activity&&!town.player.activity_status?.includes('full')&&!town.player.activity_status?.includes('Waiting');
  if(working&&!s.path.length)s.phase='working';
  if(activity)s.facing=town.player.facing;
  if(playerTown===identity?.town_id&&s.following&&(dx||dy||moving)){
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
const labels = {road:'Dirt road',hall:'Town hall',home:'Cottage',woodcutter:'Woodcutter',farm:'Farm',storehouse:'Storehouse',fishing_hut:'Fishing hut',acre:'Acre',well:'Well'};
function buildingLevel(building){return ['acre','orchard_tree'].includes(building.kind)?1:building.level || 1;}
const woodWorkerCapacities={woodcutter:[1,2,3],sawpit:[1,2],workshop:[1]};
const workerCapacities={...woodWorkerCapacities,farm:[1,2,4],acre:[1],mill:[1],bakery:[1],mine:[1,2,4,8],smelter:[1,2,3],steelworks:[1]};
const decorationKinds=['wooden_bench','wooden_statue','lantern','flower_box','wooden_table','signpost','garden_arch'];
function decorationOrientation(building){
  return Number.isInteger(building.orientation)&&building.orientation>=0&&building.orientation<4?building.orientation:0;
}
function woodCapacities(){return world.progression?.wood_worker_capacities || woodWorkerCapacities;}
function storageUpgradeKinds(){return world.progression?.storage_upgrade_kinds || [...Object.keys(woodCapacities()),'mill','bakery'];}
function workplaceStorageSize(building){return (world.progression?.workplace_storage_sizes || {mill:160,bakery:100})[building.kind] || 50;}
function capacityTiers(){return world.progression?.worker_capacities || workerCapacities;}
function settlerCapacity(building){
  const level=building.settler_level || 1,tiers=capacityTiers()[building.kind];
  return tiers?tiers[Math.min(level,tiers.length)-1]:(world.progression?.buildings?.[building.kind]?.slots ?? settlerSpaces[building.kind])*level;
}
function capacityMaxLevel(building){return capacityTiers()[building.kind]?.length || 3;}
function grainEquipmentMultiplier(building){
  return Math.max(1,...(building.profession_upgrades || []).map(key=>{
    const spec=world.progression?.upgrades?.[key];
    return spec?.building===building.kind?spec.throughput || 1:1;
  }));
}
function storageMultiplier(building){return storageUpgradeKinds().includes(building.kind)?(1+.25*(buildingLevel(building)-1))*grainEquipmentMultiplier(building):1;}
function foodRates(){return world.progression?.food_per_minute || {idle:.6,working:1.5};}
function grainProduction(building,worker=null){
  const catalog=world.progression,recipe=selectedProfessionRecipe(building);
  const base=recipe?[recipe.inputs,recipe.output,recipe.amount]:catalog?.recipes?.[building.kind];
  if(!base)return null;
  const factor=60/(catalog.production_seconds?.[base[1]] || 3)*grainEquipmentMultiplier(building)*(worker?workerEfficiency(worker,catalog.buildings[building.kind].job):1);
  const amount=base[2]*factor,food=amount*(foodGoods()[base[1]] || (base[1]==='flour'?foodGoods().bread || 1.5:0));
  return {inputs:Object.fromEntries(Object.entries(base[0]).map(([key,n])=>[key,n*factor])),output:base[1],amount,food,feeds:food/foodRates().working};
}

function housingCapacity(town){
  return 5+town.buildings.filter(b=>b.kind==='home'&&(!b.construction||'upgrade' in b.construction)).reduce((sum,b)=>sum+Math.max(2*(buildingLevel(b)+(b.profession_upgrades?.includes('timber_frame')?1:0)),town.sandbox?settlerSpaces.home*(b.settler_level || 1):0),0);
}
function fishingBoatLevel(building){
  const upgrades=building.profession_upgrades || [];
  return upgrades.includes('master_boat')?3:upgrades.includes('improved_boat')?2:upgrades.includes('boat')?1:0;
}
function fishingBoatCapacity(building){return [0,5,10,20][fishingBoatLevel(building)];}
function fishingBoatCargo(town,building){
  building=town.buildings.find(b=>b.kind==='fishing_hut'&&b.x===building.x&&b.y===building.y) || building;
  const cargo={};
  for(const [index,worker] of (town.villagers || []).entries()){
    if(worker.job!=='fishing'||!WorkerAnimation.fishingBoating(town,building,index))continue;
    for(const [key,amount] of Object.entries(worker.pending_output || {}))
      if(['fish','trout','salmon','sturgeon'].includes(key)&&amount>0)cargo[key]=(cargo[key] || 0)+amount;
  }
  return cargo;
}
function fishingBoatLoad(town,building){return Object.values(fishingBoatCargo(town,building)).reduce((sum,amount)=>sum+amount,0);}

function resourceReach(building){
  if(building.kind==='mine')return 10;
  return building.kind==='woodcutter'?10:building.kind==='fishing_hut'?3+buildingLevel(building)-1+(fishingBoatLevel(building)?3+2*(fishingBoatLevel(building)-1):0):3;
}
function buildingVisualLevel(building,town){
  return building.kind==='hall'?Math.min(5,town?.level || 1):Math.min(building.kind==='home'?4:3,buildingLevel(building));
}
const detailedBuildingKinds=['butcher','bakery','smokehouse','tannery','woodcutter','workshop','farm','home','hall','fishing_hut','fish_pond','mine','smelter','steelworks','storehouse','mill','market','orchard','stable','saddler','tradehouse','well'];
function buildingHeight(building,town){
  if(decorationKinds.includes(building.kind))return building.kind==='wooden_bench'?15:32;
  return ({hall:62,storehouse:48,well:32,orchard_tree:36,woodcutter:44,fishing_hut:44,mine:27,smelter:36,steelworks:38,stable:32,tannery:38,saddler:40,tradehouse:44,barn:53,mill:52,smokehouse:62,workshop:44,sawpit:28,market:36,fish_pond:32,orchard:40,bakery:58,butcher:44,home:46,farm:44}[building.kind] || 27)+4*(buildingVisualLevel(building,town)-1)+(building.kind==='mill'&&footprintTiles(building).length>1?10:0);
}
const storehouseCapacities=[50,75,100,150,200,250,300,350];
function farmStorageCapacity(building){
  if(building.kind==='acre')return world.progression?.farming_harvest_yield || 10;
  const level=Math.min(3,Math.max(1,buildingLevel(building)));
  return (building.kind==='farm'?48:1)*(world.progression?.farming_harvest_yield || 10)*(1+.5*(level-1))**2;
}
function buildingStorageSize(building){
  if(['farm','acre'].includes(building.kind))return Object.values(roomAllocations(building)).reduce((sum,n)=>sum+n,0);
  return ['home','market','well','orchard_tree',...decorationKinds].includes(building.kind)?0:building.kind==='hall'?200:building.kind==='storehouse'?storehouseCapacities[buildingLevel(building)-1]:workplaceStorageSize(building)*storageMultiplier(building);}
function storageCapacity(town){return town.buildings.filter(b=>!b.construction).reduce((sum,b)=>sum+buildingStorageSize(b),0);}

const storageKeys=['wood','stone','food','planks','wheat','fodder','flour','wool','tools','manure','herbs'];
function buildingName(building){return building.kind==='storehouse'&&buildingLevel(building)>=5?'Warehouse':labels[building.kind];}
const storageGoods={
  hall:[...storageKeys,'vegetables','carrots','fish','trout','salmon','sturgeon','berries','eggs','meat','milk','fruit','smoked_fish','smoked_trout','smoked_salmon','smoked_sturgeon','bread','prepared_meat'],
  storehouse:[...storageKeys,'vegetables','carrots','fish','trout','salmon','sturgeon','berries','eggs','meat','milk','fruit','smoked_fish','smoked_trout','smoked_salmon','smoked_sturgeon','bread','prepared_meat'],
  market:[],
  home:[],woodcutter:['wood'],fishing_hut:['fish','trout','salmon','sturgeon'],farm:['vegetables','wheat','carrots','fodder','manure'],acre:['vegetables','wheat','carrots','fodder','manure'],
  barn:['fodder','vegetables','manure','eggs','wool','meat','milk','hides'],orchard:['fruit'],
  bakery:['flour','wood','bread'],butcher:['meat','prepared_meat'],sawpit:['wood','planks'],mill:['wheat','flour'],
  smokehouse:['fish','trout','salmon','sturgeon','wood','smoked_fish','smoked_trout','smoked_salmon','smoked_sturgeon'],workshop:['planks','wood','tools'],fish_pond:['fodder','fish']
};
function buildingStorageGoods(building){return (world.progression?.storage_goods || storageGoods)[building.kind] || [];}
function foodGoods(){return world.progression?.edible || {vegetables:1,carrots:1,fish:1,trout:1.5,salmon:2,sturgeon:3,berries:1,eggs:1,meat:1,milk:1,fruit:1,smoked_fish:1.5,smoked_trout:2,smoked_salmon:2.5,smoked_sturgeon:3.5,bread:1.5,prepared_meat:4};}
function buildingStorageKeys(building){
  const goods=new Set(buildingStorageGoods(building));
  return [...new Set([...(world.progression?.storage_keys || storageKeys),...Object.keys(foodGoods())])].filter(key=>goods.has(key));
}
function roomAllocations(building){
  if(building.kind==='storehouse'){
    const enabled=roomEnabled(building);
    return Object.fromEntries(buildingStorageKeys(building).map(key=>[key,enabled[key]?buildingStorageSize(building):0]));
  }
  const fixed=world.progression?.fixed_storage || {
    hall:{wood:100,stone:100,food:100},home:{},woodcutter:{wood:50},fishing_hut:{food:50},
    farm:{food:20,wheat:10,fodder:10,manure:10},acre:{food:20,wheat:10,fodder:10,manure:10},
    barn:{food:20,fodder:15,wool:10,manure:5,hides:10},orchard:{food:50},
    bakery:{flour:40,wood:10,herbs:20,food:40},butcher:{food:50},sawpit:{wood:30,planks:20},mill:{wheat:80,flour:80},
    smokehouse:{food:45,wood:5},workshop:{planks:25,wood:15,tools:10},fish_pond:{fodder:15,food:35},
    market:{}
  };
  const allocations={...(fixed[building.kind] || {})};
  if(!world.progression?.fixed_storage&&'food' in allocations){
    const capacity=allocations.food;delete allocations.food;
    for(const key of buildingStorageGoods(building))if(key==='food'||key in foodGoods())allocations[key]=capacity;
  }
  if(['farm','acre'].includes(building.kind)){
    for(const key of Object.keys(allocations))allocations[key]=['food','vegetables','wheat','carrots','fodder'].includes(key)?farmStorageCapacity(building):allocations[key]*buildingLevel(building);
  }else for(const key of Object.keys(allocations))allocations[key]*=storageMultiplier(building);
  return Object.fromEntries(Object.entries(allocations).filter(([key])=>buildingStorageKeys(building).includes(key)));
}
function resourceStorage(town,key){
  const scene=typeof ClientScene!=='undefined'&&ClientScene.current?.world===world?ClientScene.current:null;
  let cached=scene?.capacities.get(town);
  if(scene&&!cached){cached=new Map();scene.capacities.set(town,cached);}
  if(cached?.has(key))return cached.get(key);
  const edible=foodGoods();
  const capacity=town.buildings.filter(b=>!b.construction).reduce((sum,b)=>sum+(key==='food'
    ?Object.entries(roomAllocations(b)).reduce((total,[resource,n])=>total+(resource==='food'||resource in edible?n*(edible[resource] || 1):0),0)
    :buildingStorageGoods(b).includes(key)?roomAllocations(b)[key] || 0:0),0);
  cached?.set(key,capacity);return capacity;
}
function localTradeFoodRoom(town,key='vegetables'){
  if(town.inventory_version!==1)return Math.max(0,resourceStorage(town,key)-amountOf(town,key));
  return town.buildings.filter(b=>!b.construction&&buildingStorageGoods(b).includes(key)).reduce((sum,b)=>{
    const stock={...b.stored_goods};
    for(const [key,n] of Object.entries(b.output_buffer || {}))stock[key]=(stock[key] || 0)+n;
    return sum+Math.max(0,(roomAllocations(b)[key] || 0)-(stock[key] || 0));
  },0);
}
function roomEnabled(building){
  const enabled=building.storage_enabled || Object.fromEntries(Object.entries(building.storage_allocations || {}).map(([key,n])=>[key,n>0]));
  return Object.fromEntries(buildingStorageKeys(building).map(key=>[key,!!(enabled[key] ?? (key in foodGoods()?enabled.food:false))]));
}
function storageGoodUnlocked(town,key){
  const catalog=world.progression;
  if(town.sandbox||!catalog?.buildings)return true;
  const hallUnlocked=kind=>town.level>=(catalog.buildings[kind]?.level || 1);
  if(key in (catalog.crops || {}))return town.level>=catalog.crops[key];
  const animals=Object.values(catalog.animals || {}).filter(spec=>spec.output===key);
  if(animals.length)return animals.some(spec=>town.level>=spec.level);
  if(key==='manure')return hallUnlocked('barn');
  if(key==='hides')return hallUnlocked('butcher');
  if(key==='fruit')return hallUnlocked('orchard');
  const fish=catalog.fish?.[key] || catalog.fish?.[key.replace(/^smoked_/, '')];
  if(fish){
    const tier=Math.max(0,...town.buildings.filter(b=>b.kind==='fishing_hut'&&!b.construction).map(fishingBoatLevel));
    return tier>=fish.boat&&(!key.startsWith('smoked_')||hallUnlocked('smokehouse'));
  }
  for(const [kind,recipes] of Object.entries(catalog.profession_recipes || {})){
    const spec=recipes.find(recipe=>recipe.output===key);
    if(!spec)continue;
    const job=catalog.buildings[kind]?.job;
    return hallUnlocked(kind)&&(spec.level===1||town.villagers.some(v=>
      v.job===job&&town.buildings[v.building_index]?.kind===kind&&!jobRequirement(v,job)&&(v[job] || 0)>=spec.level));
  }
  const producers=Object.entries(catalog.recipes || {}).filter(([,recipe])=>recipe[1]===key);
  return !producers.length||producers.some(([kind])=>hallUnlocked(kind));
}
function renderStorage(town,item,index,target=$('selection-details')){
  const capacity=buildingStorageSize(item),allocations=roomAllocations(item);
  const owner=viewed===identity?.town_id,canConfigure=owner&&item.kind==='storehouse';
  const enabled=roomEnabled(item),edible=foodGoods(),stored={...item.stored_goods},toggles=new Map(),categoryToggles=[];
  for(const [key,n] of Object.entries(item.output_buffer || {}))stored[key]=(stored[key] || 0)+n;
  const keys=buildingStorageKeys(item).filter(key=>storageGoodUnlocked(town,key)||stored[key]>0||item.input_buffer?.[key]>0||amountOf(town,key)>0);
  const syncToggles=()=>{
    for(const [key,toggle] of toggles)toggle.checked=enabled[key];
    for(const {toggle,members} of categoryToggles){
      toggle.checked=members.every(key=>enabled[key]);
      toggle.indeterminate=!toggle.checked&&members.some(key=>enabled[key]);
    }
  };
  const stock=stored;
  const displayCapacity=Object.entries(allocations).reduce((sum,[key,n])=>sum+(key==='food'||key in edible?n:0),0)
    +Math.min(capacity,Object.entries(allocations).reduce((sum,[key,n])=>sum+(key!=='food'&&!(key in edible)?n:0),0));
  const summary=document.createElement(item.kind==='storehouse'?'p':'span');summary.className='storage-summary';
  summary.textContent=item.kind==='storehouse'?`${capacity} capacity per enabled resource`:`${Math.floor(Object.values(stock).reduce((sum,n)=>sum+n,0))} / ${displayCapacity}`;
  if(item.kind==='storehouse')target.append(summary);
  else{
    const heading=target.children[0];heading.className='storage-heading';heading.append(summary);
  }
  if(storageUpgradeKinds().includes(item.kind)){
    const note=document.createElement('p');note.className='storage-note';
    note.textContent=`Production buffer: ${capacity} spaces${['sawpit','workshop','mill','bakery'].includes(item.kind)?` · Separate input buffer: ${Number(Object.values(item.input_buffer || {}).reduce((sum,n)=>sum+n,0).toFixed(2))} / ${capacity}`:''}.`;
    target.append(note);
  }
  if(item.kind==='storehouse'){
    const note=document.createElement('p');note.className='storage-note';
    note.textContent='Storage controls: Help (7) → Resources & storage.';target.append(note);
  }
  for(const [title,members] of resourceCategories(keys)){
    const group=resourceGroup(title);target.append(group);
    if(canConfigure){
      const heading=group.children[0];heading.className='storage-category-heading';
      const toggle=document.createElement('input');toggle.type='checkbox';
      toggle.setAttribute('aria-label',`Accept all ${title} resources`);
      toggle.onchange=()=>{
        if(viewed!==identity?.town_id)return;
        for(const key of members)enabled[key]=toggle.checked;
        syncToggles();
        act({action:'configure_storage',index,enabled:{...enabled}});
      };
      categoryToggles.push({toggle,members});heading.append(toggle);
    }
    for(const key of members){
      const label=key==='food'?'Provisions':goodName(key);
      const row=document.createElement('div');row.className='storage-row';
      const caption=document.createElement('label');caption.className='storage-caption';
      const name=document.createElement('span');name.textContent=label;caption.append(name);
      if(canConfigure){
        const toggle=document.createElement('input');toggle.type='checkbox';toggle.checked=enabled[key];
        toggle.setAttribute('aria-label',`Accept ${label}`);
        toggle.title='Option/Alt-click to toggle all shown resources on or off';
        let toggleAll=false;
        toggle.onclick=event=>{toggleAll=!!event.altKey;};
        toggle.onchange=()=>{
          if(viewed!==identity?.town_id)return;
          if(toggleAll){
            const acceptAll=!keys.every(resource=>enabled[resource]);
            for(const resource of keys)enabled[resource]=acceptAll;
          }else enabled[key]=toggle.checked;
          toggleAll=false;
          syncToggles();
          act({action:'configure_storage',index,enabled:{...enabled}});
        };
        toggles.set(key,toggle);
        caption.append(toggle);
      }
      const amount=stock[key] || 0,limit=allocations[key] || 0,displayLimit=Number(limit.toFixed(2));
      const count=document.createElement('span');count.className='storage-count';
      count.textContent=item.kind==='storehouse'&&!enabled[key]?`${Math.floor(amount)} · Disabled`:`${Math.floor(amount)} / ${displayLimit}`;caption.append(count);row.append(caption);
      const meter=document.createElement('meter');meter.min=0;meter.max=Math.max(1,limit,amount);meter.value=amount;
      meter.setAttribute('aria-label',`${label} stored`);meter.setAttribute('aria-valuetext',`${Math.floor(amount)} stored, ${displayLimit} capacity`);row.append(meter);group.append(row);
    }
  }
  syncToggles();
  for(const [key,n] of Object.entries(stored))if(n>0&&!buildingStorageGoods(item).includes(key))target.append(fact(String(Math.floor(n)),`${goodName(key)} from previous storage · available to use or haul away`));
  for(const [key,n] of Object.entries(item.input_buffer || {}))if(n>0)target.append(fact(String(Math.floor(n)),`${goodName(key)} reserved for production`));
}

function upgradeBenefit(kind,level,item){
  if(storageUpgradeKinds().includes(kind)){
    const building={...item,kind,level};
    return `${buildingStorageSize(building)} storage spaces\n${Object.entries(roomAllocations(building)).map(([key,amount])=>`${goodName(key)}: ${amount}`).join(', ')}\n${['mill','bakery'].includes(kind)?'One worker · Equipment increases production':'Worker capacity upgraded separately'}`;
  }
  if(kind==='orchard')return `+${25*(level-1)}% picking speed · Tree growth and harvest size stay fixed`;
  if(kind==='farm')return `+${25*(level-1)}% crops / worker\n${farmStorageCapacity({kind,level})} capacity per crop`;
  if(kind==='home'&&item?.profession_upgrades?.includes('timber_frame'))level++;
  if(kind==='fishing_hut')return `+${25*(level-1)}% food / catch / worker\n${resourceReach({...item,kind,level})} tile reach`;
  return kind==='barn'?`${4*level} animal spaces`:['sawpit','mill','smokehouse','workshop','fish_pond','orchard','market','bakery','butcher'].includes(kind)?'Separate worker capacity':kind==='home'?`${2*level} spaces\n-${Math.min(75,25*(level-1))}% departure`:kind==='storehouse'?`${buildingStorageSize({kind,level})} capacity per resource${level>=5?'\nWarehouse':''}`:`+${25*(level-1)}% ${kind==='woodcutter'?'wood':kind==='fishing_hut'?'food / catch':kind==='acre'?'field food':'food'} / worker`;
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
  if(['acre','orchard_tree'].includes(item?.kind)&&['upgrade_building','upgrade_capacity'].includes(command.action))return null;
  const preview=buildingUpgradePreview(command,town,item);
  return preview&&town.sandbox?{...preview,cost:{},seconds:0}:preview;
}
function buildingUpgradePreview(command,town,item){
  if(command.action==='upgrade'){
    const spec=world.hall_upgrades?.find(next=>next.level===town.level+1);
    if(!spec)return null;
    const benefits=[`+${spec.coins} coins`, `Build range: ${buildingRange(town)} → ${buildingRange(town)+5} tile steps.`];
    const catalog=world.progression;
    for(const [category,entries] of Object.entries({Buildings:catalog?.buildings,Crops:catalog?.crops,Animals:catalog?.animals,Equipment:catalog?.upgrades})){
      const names=Object.entries(entries || {}).filter(([,value])=>(typeof value==='number'?value:value.level)===spec.level).map(([key,value])=>value.label || labels[key] || goodName(key));
      if(names.length)benefits.push(`${category}: ${names.join(', ')}`);
    }
    return {title:`Town hall · Level ${spec.level}`,benefits,cost:Object.fromEntries(['wood','planks','tools','leather','iron_ore','iron','steel'].filter(k=>k in spec).map(k=>[k,spec[k]])),seconds:spec.seconds,requirements:spec.milestone,ready:!!world.hall_ready?.[viewed]&&!town.upgrade_remaining};
  }
  if(command.action==='profession_upgrade'){
    const spec=world.progression?.upgrades?.[command.upgrade];
    if(!spec)return null;
    const benefits=[professionBenefits[command.upgrade] || spec.label];
    if(spec.throughput){
      const next={...item,profession_upgrades:[...(item.profession_upgrades || []),command.upgrade]};
      const before=grainProduction(item),after=grainProduction(next);
      benefits.splice(0,1,`${before.amount} → ${after.amount} ${goodName(after.output).toLowerCase()}/min`,
        `Feeds ${before.feeds} → ${after.feeds} working villagers${item.kind==='mill'?' as basic bread':''}`,
        `Ingredients: ${costText(before.inputs)}/min → ${costText(after.inputs)}/min`,
        `Storage and producer carrying capacity: ${grainEquipmentMultiplier(item)}× → ${grainEquipmentMultiplier(next)}×`,
        'One worker · Neutral talent and low practice · Continuous inputs');
    }
    return {title:spec.label,benefits,cost:materialCosts(spec),seconds:command.upgrade==='boat'?0:30,requirements:`Hall level ${spec.level}${spec.xp&&!town.sandbox?` · ${spec.xp} ${goodName(spec.skill).toLowerCase()} practice`:''}${spec.requires?` · ${world.progression.upgrades[spec.requires]?.label || goodName(spec.requires)}`:''}`,ready:town.level>=spec.level&&(town.sandbox||professionSkill(town,spec.skill)>=(spec.xp || 0))&&(!spec.requires||item.profession_upgrades?.includes(spec.requires))&&!item.profession_upgrades?.includes(command.upgrade)};
  }
  const capacity=command.action==='upgrade_capacity',level=capacity?item.settler_level || 1:buildingLevel(item);
  if(decorationKinds.includes(item.kind))return null;
  const currentBenefits=upgradeBenefit(item.kind,level,item).split('\n'),nextBenefits=upgradeBenefit(item.kind,level+1,item).split('\n');
  const maxLevel=capacity?capacityMaxLevel(item):item.kind==='storehouse'?storehouseCapacities.length:item.kind==='home'?4:3;
  const cost={wood:capacity?20*level:costs[item.kind]*level};
  if(!capacity&&item.kind==='home'&&level===3)cost.stone=world.progression?.home_stone_cost ?? 30;
  return {title:`${buildingName(item)} · ${capacity?'Capacity tier':'Level'} ${level+1}`,benefits:capacity?[`${settlerCapacity(item)} → ${settlerCapacity({...item,settler_level:level+1})} worker spaces`]:nextBenefits.map((benefit,i)=>currentBenefits[i]&&currentBenefits[i]!==benefit?`${currentBenefits[i]} → ${benefit}`:benefit),cost,seconds:30*level,requirements:'',ready:level<maxLevel};
}
function upgradeCard(button,preview,{title=preview?.title,status='',benefits=preview?.benefits || []}={}){
  const card=document.createElement('div');card.className='upgrade-card';
  const heading=document.createElement('strong');heading.className='upgrade-card-title';heading.textContent=title;card.append(heading);
  if(benefits.length){
    const benefit=status==='Unlocked'?benefits.join('\n'):benefits[0].split('\n')[0];
    const metric=benefit.match(/^([+-]?\d+%?) (.+) → ([+-]?\d+%?) \2$/);
    const row=document.createElement('p');row.className='upgrade-card-benefit';
    row.textContent=metric?`${metric[1]} → ${metric[3]} ${metric[2]}`:benefit;card.append(row);
  }
  if(status){const row=document.createElement('span');row.className='upgrade-card-status';row.textContent=status;card.append(row);}
  else if(preview){
    const meta=document.createElement('small');meta.className='upgrade-card-meta';
    meta.textContent=`${costText(preview.cost) || 'Free'} · ${preview.seconds?`${preview.seconds}s`:'Instant'}`;card.append(meta);
  }
  if(!status)card.append(button);return card;
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
  $('upgrade-cost').textContent=costText(preview.cost) || 'Free';
  $('upgrade-time').textContent=preview.seconds?`${preview.seconds} seconds of work, plus travel and waiting`:'0 seconds';
  $('upgrade-requirements').textContent=preview.requirements;
  $('upgrade-requirements').hidden=$('upgrade-requirements-label').hidden=!preview.requirements;
  $('confirm-upgrade').textContent=`${command.upgrade==='boat'?'Buy boat':'Upgrade'} for ${costText(preview.cost) || 'Free'}`;
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
const costs = {road:0,home:20,woodcutter:15,farm:15,storehouse:25,fishing_hut:15,acre:5,well:15};
const colors = {hall:'#a88861',home:'#c5ad80',woodcutter:'#947b55',farm:'#bc9b5c',storehouse:'#8c9b8b',fishing_hut:'#7899a2'};
// Small vector illustrations share the map's earth, sage, and amber palette.
Object.assign(labels,{orchard_tree:'Fruit tree',barn:'Barn',sawpit:'Sawpit',mill:'Mill',smokehouse:'Smokehouse',workshop:'Workshop',fish_pond:'Fish pond',orchard:'Orchard',market:'Market stall',bakery:'Bakery',butcher:'Butcher shop'});
Object.assign(labels,{workshop:'Workshop',wooden_bench:'Wooden bench',wooden_statue:'Carved wooden statue',lantern:'Lantern'});
Object.assign(labels,{flower_box:'Flower box',wooden_table:'Wooden table',signpost:'Signpost',garden_arch:'Garden arch'});
Object.assign(costs,Object.fromEntries(decorationKinds.map(kind=>[kind,0])));
Object.assign(costs,{orchard_tree:5,barn:25,sawpit:25,mill:30,smokehouse:25,workshop:30,fish_pond:25,orchard:25,market:20,bakery:30,butcher:25});
Object.assign(colors,{barn:'#b9866b',sawpit:'#aa8b60',mill:'#c1b080',smokehouse:'#8c8173',workshop:'#a28c72',fish_pond:'#7899a2',orchard:'#83a36b',market:'#c5ad80',bakery:'#c1a078',butcher:'#b9866b'});
Object.assign(labels,{tannery:'Tannery',mine:'Mine',smelter:'Forgery',stable:'Stable',saddler:'Saddler',steelworks:'Steelworks',tradehouse:'Trade house'});
Object.assign(costs,{tannery:30,mine:40,smelter:40,stable:40,saddler:30,steelworks:40,tradehouse:50});
Object.assign(colors,{tannery:'#ad8a67',mine:'#7f8c8c',smelter:'#a08875',stable:'#bda077',saddler:'#b89268',steelworks:'#758792',tradehouse:'#a1ab7e'});
const professionJobs={trading:'Merchant',mining:'Mining',smelting:'Smelting',tanning:'Leatherworking',horsekeeping:'Horsekeeping',saddlery:'Saddlery',steelmaking:'Steelmaking',packing:'Trade packing',husbandry:'Husbandry',woodworking:'Sawing',milling:'Milling',preserving:'Preserving',crafting:'Crafting',breeding:'Fish breeding',orcharding:'Orcharding',baking:'Baking',butchery:'Butchery'};
const goodName=key=>key==='woodworking'?'Sawing':key.replaceAll('_',' ').replace(/^./,c=>c.toUpperCase());
function resourceCategories(keys){
  const remaining=new Set(keys),catalog=world.progression || {};
  const products=kind=>(catalog.profession_recipes?.[kind] || []).map(recipe=>recipe.output);
  const categories=[
    ['Overview',['food','coins']],
    ['Baking',['wheat','flour','bread','bread_rolls','herb_bread','fruit_pies','cakes',...products('bakery')]],
    ['Fishing',['fish','trout','salmon','sturgeon',...Object.keys(catalog.fish || {}),'smoked_fish','smoked_trout','smoked_salmon','smoked_sturgeon',...Object.keys(catalog.smoked_fish || {})]],
    ['Masonry',['stone']],
    ['Timber',['wood','planks','tools','wooden_bowls','crates','furniture','carved_furniture','crafted_axe','crafted_saw',...products('workshop')]],
    ['Farming',['vegetables','carrots','fodder','manure','herbs']],
    ['Orcharding and gathering',['fruit','berries']],
    ['Livestock',['eggs','milk','wool']],
    ['Butchery',['meat','prepared_meat','herbed_meat','smoked_meat','sausages','meat_pies',...products('butcher')]],
    ['Leatherworking',['hides','leather','saddles']],
    ['Metalworking',['iron_ore','iron','steel']],
    ['Trading',['trade_crates']],
  ];
  const groups=[];
  for(const [title,members] of categories){
    const included=[...new Set(members)].filter(key=>remaining.has(key));
    if(!included.length)continue;
    included.forEach(key=>remaining.delete(key));groups.push([title,included]);
  }
  if(remaining.size)groups.push(['Other resources',[...remaining]]);
  return groups;
}
function resourceGroup(title){
  const group=document.createElement('div');group.className='resource-group';
  const heading=document.createElement('h3');heading.textContent=title;group.append(heading);
  return group;
}
function amountOf(town,key){return ['wood','food','coins'].includes(key)?town[key] || 0:town.goods?.[key] || 0;}
function productionGoods(){
  const catalog=world.progression || {};
  return [...new Set(['wood',...Object.keys(catalog.crops || {}),...Object.keys(catalog.edible || {}),
    ...Object.values(catalog.recipes || {}).map(recipe=>recipe[1]),
    ...Object.values(catalog.profession_recipes || {}).flat().map(recipe=>recipe.output),'wool','manure','herbs','hides'])];
}
function resourceStocks(town){
  const edible=foodGoods(),stocks={};
  const add=(key,field,amount)=>{
    if(!amount)return;
    const row=stocks[key] ||= {free:amountOf(town,key),reserved:0,pending:0};row[field]+=amount;
    if(key in edible)add('food',field,amount*edible[key]);
  };
  for(const key of new Set([...(world.progression?.storage_keys || storageKeys),'coins',...Object.keys(edible),...Object.keys(town.goods || {})]))
    stocks[key]={free:amountOf(town,key),reserved:0,pending:0};
  for(const building of town.buildings || []){
    for(const [key,amount] of Object.entries(building.input_buffer || {}))add(key,'reserved',amount);
    // Carried and orphaned outputs are already included in the building total.
    for(const [key,amount] of Object.entries(building.pending_output || {}))add(key,'pending',amount);
  }
  for(const villager of town.villagers || []){
    const trip=villager.logistics_trip;
    if(trip?.picked_up&&!trip.delivered)
      for(const [key,amount] of Object.entries(trip.cargo || {}))add(key,trip.operation==='collect'?'pending':'reserved',amount);
  }
  for(const row of Object.values(stocks))row.total=row.free+row.reserved+row.pending;
  return stocks;
}
function resourceMinuteActivity(town){
  const result={},history=town.resource_activity;
  for(const bucket of history?.buckets || []){
    if(bucket.at<=history.seconds-60)continue;
    for(const direction of ['in','out'])for(const [key,amount] of Object.entries(bucket[direction] || {})){
      const row=result[key] ||= {in:0,out:0};row[direction]+=amount;
    }
  }
  return result;
}
function resourceNumber(value){return String(Number(Math.max(0,value || 0).toFixed(2)));}
function resourceArt(key){
  if(iconPaths[key])return illustration(key);
  // Unknown goods from newer hosts retain a neutral, readable fallback.
  return illustration('resource_crate');
}
function renderResourceList(town){
  const target=$('resources-list');
  if(!town){target.replaceChildren();renderResourceList.signature=null;return;}
  const stocks=resourceStocks(town),activity=resourceMinuteActivity(town),catalog=world.progression || {};
  const buyable=catalog.buyable_goods || [],sellable=marketSaleGoods(catalog),producible=productionGoods();
  const signature=JSON.stringify([world.world_id,identity?.town_id,Object.keys(stocks),buyable,sellable,producible]);
  if(renderResourceList.signature!==signature){
    const table=document.createElement('table');table.className='resources-table';
    const head=document.createElement('thead'),heading=document.createElement('tr');
    for(const label of ['Resource','Icon','Stock (all)','Free','Reserved','Production cap','Buy at','Sell at','In/min','Out/min']){
      const cell=document.createElement('th');cell.scope='col';cell.textContent=label;heading.append(cell);
    }
    head.append(heading);table.append(head);
    const body=document.createElement('tbody');table.append(body);
    const rows=new Map(),worldId=world.world_id,townId=identity?.town_id;
    for(const [title,members] of resourceCategories(Object.keys(stocks))){
      const category=document.createElement('tr');category.className='resource-category';
      const caption=document.createElement('th');caption.colSpan=10;caption.scope='rowgroup';caption.textContent=title;category.append(caption);body.append(category);
      for(const key of members){
        const row=document.createElement('tr');row.dataset && (row.dataset.resource=key);
        const name=document.createElement('th');name.scope='row';name.textContent=goodName(key);row.append(name);
        const art=document.createElement('td');art.className='resource-art';art.append(resourceArt(key));row.append(art);
        const cells={};
        for(const field of ['total','free','reserved','cap','buy','sell','in','out']){
          cells[field]=document.createElement('td');row.append(cells[field]);
        }
        cells.total.className='resource-stock';
        const inputs={};
        if(producible.includes(key)){
          const input=document.createElement('input');input.type='number';input.min='0';input.step='any';input.placeholder='Unlimited';
          input.setAttribute('aria-label',`${goodName(key)} production cap`);
          input.setAttribute('aria-describedby','resources-cap-help');cells.cap.append(input);inputs.cap=input;
          input.onchange=()=>{
            if(world.world_id!==worldId||identity?.town_id!==townId)return;
            if(input.reportValidity&&!input.reportValidity())return;
            act({action:'configure_rule',resource:key,cap:input.value.trim()===''?null:Number(input.value)});
          };
        }else cells.cap.textContent='—';
        for(const [field,allowed] of [['buy',buyable.includes(key)],['sell',sellable.includes(key)]]){
          if(!allowed){cells[field].textContent='—';continue;}
          const input=document.createElement('input');input.type='number';input.min='0';input.step='any';input.placeholder='Off';
          input.setAttribute('aria-label',`${goodName(key)} ${field==='buy'?'buy at':'sell at'}`);
          input.setAttribute('aria-describedby','resources-trade-help');cells[field].append(input);inputs[field]=input;
          input.onchange=()=>{
            if(world.world_id!==worldId||identity?.town_id!==townId)return;
            if([inputs.buy,inputs.sell].some(node=>node?.reportValidity&&!node.reportValidity()))return;
            const value=field=>inputs[field]?.value?.trim()?Number(inputs[field].value):null;
            const buy=value('buy'),sell=value('sell');
            if(buy!==null&&sell!==null&&buy>=sell){notice('Sell at must be higher than Buy at.');return;}
            act({action:'configure_resource_trade',resource:key,buy_below:buy,sell_above:sell});
          };
        }
        rows.set(key,{cells,inputs});body.append(row);
      }
    }
    renderResourceList.signature=signature;renderResourceList.table=table;renderResourceList.rows=rows;
    target.replaceChildren(table);
  }
  const hasMarket=(town.buildings || []).some(b=>b.kind==='market'&&!b.construction);
  for(const [key,{cells,inputs}] of renderResourceList.rows){
    const stock=stocks[key],flow=activity[key] || {in:0,out:0};
    for(const field of ['total','free','reserved'])cells[field].textContent=resourceNumber(stock[field]);
    cells.total.title=`${resourceNumber(stock.pending)} awaiting workplace delivery; counted once in total stock`;
    for(const field of ['in','out'])cells[field].textContent=resourceNumber(flow[field]);
    const sale=town.rules?.market_sales?.[key],buy=town.rules?.market_purchases?.[key];
    if(inputs.cap){
      inputs.cap.title='Production target: blank is unlimited; 0 pauses production';
      if(document.activeElement!==inputs.cap)inputs.cap.value=String(town.rules?.production_caps?.[key] ?? '');
    }
    for(const [field,input] of Object.entries(inputs).filter(([field])=>field!=='cap')){
      input.disabled=!hasMarket;
      input.title=!hasMarket?'Finish a market stall first':field==='buy'?`Buy below this free stock at ${2*(catalog.fish?.[key]?.coins ?? catalog.sale_prices?.[key] ?? 1)} coins per item; blank disables buying`:sale&&!sale.strict?`Existing market rule: starts at ${sale.cap} or more and sells batches. Editing here retains the threshold amount instead.`:'Sell surplus above this free stock; blank disables selling';
      if(document.activeElement!==input)input.value=String((field==='buy'?buy:sale&&(sale.enabled ?? true)?sale.cap:null) ?? '');
    }
  }
}
function materialCosts(spec){return Object.fromEntries(Object.entries(spec).filter(([k,v])=>v&&['wood','stone','coins','planks','wool','tools','leather','iron_ore','iron','saddles','steel','trade_crates','furniture','carved_furniture','lanterns','flower_boxes','wooden_tables','signposts','garden_arches'].includes(k)));}
function buildingCosts(town,spec){return town?.sandbox?{}:materialCosts(spec);}
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
function availableWorkerTools(town,key){
  return (town.player?.worker_tools?.[key] || 0)+(['crafted_axe','crafted_saw'].includes(key)?amountOf(town,key):0);
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
  const hint=document.createElement('p');hint.className='muted';hint.textContent='Select a villager · Nobody returns the tool.';tools.append(hint);
  for(const [key,gear] of Object.entries(workerTools)){
    const available=availableWorkerTools(town,key);
    const assigned=town.villagers.map((v,index)=>({v,index})).filter(({v})=>v.worker_tool===key);
    if(available<1&&!assigned.length)continue;
    const row=itemCard(key,gear.name,`+${gear.bonus*100}% ${goodName(gear.job).toLowerCase()} output${key==='crafted_saw'?' and ingredients per batch':''}`,`${Number(available.toFixed(2))} available · ${assigned.length} assigned`);
    for(const {v,index} of assigned){
      const recipient=toolRecipientPicker(town,key,index);recipient.setAttribute('aria-label',`Assign ${gear.name.toLowerCase()} carried by ${v.name}`);row.append(recipient);
    }
    if(available>=1)row.append(toolRecipientPicker(town,key));
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
  if(!target.children.length){const empty=document.createElement('p');empty.className='muted';empty.textContent='No items yet. Find tools and equipment through local gifts, trades, and treasure.';target.append(empty);}
}
function professionSkill(town,job){return Math.max(town.player?.[job] || 0,...town.villagers.map(v=>v[job] || 0));}
function jobRequirement(v,job){
  if(v.sandbox)return '';
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
  crafted_axe:'<path fill="#94704c" d="m6 28 15-24 3 2-15 24z"/><path fill="#bacac5" d="m18 5 7-3 5 9-8 4z"/>',
  crafted_saw:'<path fill="#bacac5" d="M4 13h22v8l-3-2-3 2-3-2-3 2-3-2-3 2-4-2z"/><path fill="none" stroke="#94704c" stroke-width="3" d="M26 11h4v12h-4z"/>',
  wooden_bench:'<path fill="#a78454" d="M3 9h26v5H3zm0 9h26v5H3zM5 23h3v7H5zm19 0h3v7h-3z"/>',
  lantern:'<path d="M16 3v5M9 11h14v16H9zM7 10h18M7 28h18"/><path fill="#e8c16d" d="M12 14h8v10h-8z"/>',
  wooden_statue:'<path fill="#b18a58" d="M6 26h20v5H6zm6-14h8l4 14H8z"/><circle fill="#b18a58" cx="16" cy="7" r="6"/>',
  flower_box:'<path fill="#a78454" d="M3 17h26l-3 12H6z"/><path fill="#719451" d="M8 19V9h3v10zm7 0V6h3v13zm7 0V9h3v10z"/><circle fill="#c47d70" cx="9" cy="9" r="4"/><circle fill="#e8c16d" cx="16" cy="6" r="4"/><circle fill="#c47d70" cx="24" cy="10" r="4"/>',
  wooden_table:'<path fill="#94704c" d="M7 17h4v13H7zm14 0h4v13h-4z"/><path fill="#cda875" d="m2 12 14-7 14 7-14 7z"/><path fill="#a78454" d="m2 12 14 7 14-7v4l-14 7-14-7z"/>',
  signpost:'<path fill="#94704c" d="M14 3h4v28h-4z"/><path fill="#cda875" d="M3 7h20l6 6-6 6H3z"/><path stroke="#e8dcc0" stroke-width="3" d="M8 13h12"/>',
  garden_arch:'<path fill="#a78454" d="M5 29V8h4v21zm18 0V8h4v21zM2 5h28v5H2z"/><path fill="#719451" d="M3 11h8v6H3zm17-9h8v6h-8zM3 21h8v5H3z"/><circle fill="#c47d70" cx="7" cy="13" r="3"/><circle fill="#e8c16d" cx="23" cy="5" r="3"/>',
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
  well:'<path fill="#a4aaa1" d="M5 19h22v8c0 5-22 5-22 0z"/><ellipse fill="#447d91" cx="16" cy="19" rx="11" ry="4"/><path d="M7 20V8m18 12V8M7 10h18m-9 0v10"/><path fill="#98634f" d="m3 8 13-6 13 6z"/><path fill="#b79865" d="M13 19h6v5h-6z"/>',
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
Object.assign(iconPaths,{
  resource_sack:'<path fill="#cda875" d="m10 3 12 0-3 7q16 19-3 19T13 10z"/><path d="M12 10h8m-4 6v7m-3-3h6"/>',
  resource_planks:'<path fill="#cda875" d="m3 10 22-6 4 6-22 6zm0 9 22-6 4 6-22 6z"/><path d="m9 11 12-3M9 20l12-3"/>',
  resource_stone:'<path fill="#7f8c8c" d="m3 22 3-13 13-5 10 12-4 11H9z"/><path fill="#a6b3af" d="m6 9 13-5 3 14-12 2z"/><path d="m10 20-1 7m13-9 7-2"/>',
  resource_carrot:'<path fill="#cc9956" d="m6 28 8-20 11 7z"/><path fill="#719451" d="M17 10Q7 0 16 2l5 6q-2-13 4-6l-1 8q12-7 5 1l-5 4z"/><path d="m12 17 4 2m0-7 4 2"/>',
  resource_berries:'<path fill="#719451" d="M15 14Q2 2 10 2q10 0 9 12 8-15 12-7 0 7-12 9z"/><g fill="#a76770"><circle cx="10" cy="19" r="6"/><circle cx="22" cy="19" r="6"/><circle cx="16" cy="26" r="5"/></g>',
  resource_egg:'<path fill="#e8dcc0" d="M26 21c0 13-20 13-20 0S16-4 20 4q6 8 6 17z"/><path stroke="#fff0d8" d="M11 14q-3 6-1 10"/>',
  resource_milk:'<path fill="#dce2d4" d="M10 3h12v8l5 6v12H5V17l5-6z"/><path fill="#7899a2" d="M5 20h22v7H5z"/><path d="M10 7h12"/>',
  resource_ingot:'<path fill="#bacac5" d="m7 8 17 0 5 13-22 6-5-9z"/><path fill="#758792" d="m7 18 22-6v9L7 27z"/><path d="m2 18 5 0 17-5"/>',
  resource_crate:'<path fill="#a78454" d="M4 6h24v23H4z"/><path d="M4 6l24 23M28 6 4 29M4 12h24M4 23h24"/>',
  resource_bowl:'<ellipse fill="#cda875" cx="16" cy="12" rx="13" ry="6"/><path fill="#94704c" d="M3 12q1 17 13 17t13-17q-13 9-26 0z"/>',
  resource_chair:'<path fill="#a78454" d="M8 3h16v15H8zM5 18h22v5H5zM6 23h4v8H6zm16 0h4v8h-4z"/><path d="M12 6v10m8-10v10"/>',
  resource_meat:'<path fill="#ba7e74" d="M4 9Q9 1 19 6l9 12q3 10-9 12L5 21Q0 17 4 9z"/><path fill="#dfb49b" d="M8 10q6-6 13 4t-2 11L8 19q-5-5 0-9z"/><circle fill="#e8dcc0" cx="16" cy="17" r="4"/>',
});
// Dedicated interface artwork: every buildable structure and stored good has
// its own silhouette. Keep these in the existing 32px vector icon family.
Object.assign(iconPaths,{
  road:'<path fill="#a99573" d="m3 28 9-25h9L29 28z"/><path stroke="#d8c6a1" d="m12 25 2-5m2-5 1-5m7 16-1-4"/><path d="M5 28h24"/>',
  barn:'<path fill="#bda780" d="M5 14h22v15H5z"/><path fill="#718579" d="m2 15 6-9 8-4 8 4 6 9z"/><path fill="#748078" d="M5 26h22v3H5z"/><path fill="#94704c" d="M11 18h10v11H11z"/><path stroke="#d8c6a1" d="m11 18 10 11m0-11L11 29"/><path d="M12 10h8"/>',
  sawpit:'<path fill="#94704c" d="M4 12h24v10H4z"/><path fill="#718579" d="m2 12 14-9 14 9z"/><path fill="#cda875" d="M3 21h26v4H3z"/><path d="m6 25-2 5m22-5 2 5M9 25l14 5"/><path fill="#bacac5" d="m8 18 14-6 2 4-3 2-2-1-2 3-2-1-2 3z"/><path stroke="#94704c" stroke-width="3" d="m22 11 4 4"/>',
  mill:'<path fill="#c5ad80" d="m11 12 10 0 5 17H6z"/><path fill="#80574b" d="m9 12 7-9 7 9z"/><g fill="#e8dcc0"><path d="M16 14 5 3h7l6 11zm2 2L29 5v7l-11 6zm-2 2 11 11h-7l-6-11zm-2-2L3 27v-7l11-6z"/></g><circle fill="#94704c" cx="16" cy="16" r="2"/>',
  smokehouse:'<path d="M21 6q-4-2-1-5m6 7q-3-3 0-6"/><path fill="#8c8173" d="M5 14h22v15H5zM21 8V4h4v9"/><path fill="#647876" d="m2 15 14-9 14 9z"/><path fill="#473f32" d="M10 18h12v11H10z"/><path fill="#cda875" d="m12 22 3-3 4 3-4 3z"/><path d="M15 18v2"/>',
  workshop:'<path fill="#a28c72" d="M5 12h22v17H5z"/><path fill="#718579" d="m2 13 14-10 14 10z"/><path fill="#d9bd70" d="M8 16h6v6H8z"/><path fill="#cda875" d="M3 23h26v3H3z"/><path d="M6 26v4m20-4v4m-7-12 5-2m-3-1 3 6"/>',
  fish_pond:'<ellipse fill="#8c9b8b" cx="16" cy="21" rx="14" ry="9"/><ellipse fill="#6ba2af" cx="16" cy="20" rx="11" ry="6"/><path fill="#cda875" d="m9 20 4-3 6 3-6 3zm10 0 4-3v6z"/><path stroke="#719451" d="M5 17V6m0 7L2 9m3 2 3-5"/>',
  orchard_tree:'<path stroke="#94704c" stroke-width="3" d="M16 29V14"/><path fill="#719451" d="M8 22C-1 17 4 6 10 7c0-7 13-7 13 0 10 1 10 15 1 15z"/><g fill="#cc8962"><circle cx="10" cy="13" r="2"/><circle cx="22" cy="13" r="2"/><circle cx="16" cy="20" r="2"/></g>',
  orchard:'<path fill="#80764a" d="m2 26 14-6 14 6-14 4z"/><path stroke="#94704c" stroke-width="3" d="M16 26V12"/><path fill="#719451" d="M8 19C-1 15 4 5 10 6c0-7 13-7 13 0 10 1 10 14 1 14z"/><g fill="#ba7e74"><circle cx="10" cy="12" r="2.5"/><circle cx="22" cy="11" r="2.5"/><circle cx="17" cy="18" r="2.5"/></g>',
  market:'<path fill="#cda875" d="M5 21h22v8H5z"/><path d="M6 21V10m20 11V10"/><path fill="#d9bd70" d="m6 4 20 0 4 9H2z"/><path fill="#80574b" d="M10 4h5l-1 9H7zm10 0h5l4 9h-7z"/><path fill="#719451" d="M8 21v-5h6v5z"/><path fill="#b9866b" d="M18 21v-5h6v5z"/>',
  bakery:'<path fill="#c1a078" d="M4 14h24v15H4zM22 4h4v11h-4z"/><path fill="#80574b" d="m2 15 14-11 14 11z"/><path fill="#473f32" d="M17 29V20a4 4 0 0 1 8 0v9z"/><path fill="#f2ad52" d="m19 28 2-6 2 6z"/><path fill="#d9bd70" d="M6 21c0-5 8-5 8 0v3H6z"/><path d="m9 19-1 2m4-2-1 2"/>',
  butcher:'<path fill="#b9866b" d="M5 14h22v15H5z"/><path fill="#80574b" d="m2 15 14-11 14 11z"/><path fill="#e8dcc0" d="M5 14h22v4H5z"/><path d="M9 25h14m-12 0v4m10-4v4"/><path fill="#bacac5" d="m15 20 2-7 7 3-3 6z"/><path stroke="#94704c" stroke-width="2.5" d="m15 21-3 4"/>',
  tannery:'<path fill="#a28c72" d="M4 15h12v14H4z"/><path fill="#80574b" d="m2 16 8-9 9 9z"/><path stroke="#94704c" stroke-width="2" d="M19 29V6h10v23"/><path fill="#bc8a57" d="m20 10 4 2 4-2-1 6 1 8-4-2-4 2 1-8z"/><path d="M7 21h5v8H7"/>',
  stable:'<path fill="#bda077" d="M4 14h24v15H4z"/><path fill="#718579" d="m2 15 14-11 14 11z"/><path fill="#473f32" d="M9 19h14v10H9z"/><path fill="#ac7a50" d="m14 25-1-5 3-4 5 2v5h-4v4z"/><path d="m17 17-1-3m3 3 1-3"/><path fill="#cda875" d="M7 25h18v4H7z"/>',
  saddler:'<path fill="#b89268" d="M5 14h22v15H5z"/><path fill="#647876" d="m2 15 14-11 14 11z"/><path fill="#94704c" d="M9 24h14v3H9z"/><path fill="#cda875" d="M9 19q4 3 7-4 3 7 7 4l-2 5H11z"/><path d="M12 27v3m8-3v3"/>',
  steelworks:'<path fill="#758792" d="M3 14h26v15H3zM22 3h5v13h-5z"/><path fill="#647876" d="m2 14 9-7 9 7z"/><path fill="#473f32" d="M6 20h9v9H6z"/><path fill="#f2ad52" d="m8 28 2-6 3 6z"/><path fill="#bacac5" d="M18 20h11l-3 4h-5zm3 4h5v5h-5z"/>',
  tradehouse:'<path fill="#a1ab7e" d="M4 13h24v16H4z"/><path fill="#647876" d="m2 14 14-10 14 10z"/><path fill="#d9bd70" d="M16 4V1h10l-2 4h-8"/><path fill="#cda875" d="M7 20h8v9H7zm10-4h8v13h-8z"/><path d="m7 20 8 9m2-13 8 13m-8-9h8"/>',
  wheat:'<path d="m12 29 7-25m-5 16-7-7m9 1 8-3"/><path fill="#d9bd70" d="M19 4q-7-5-7 1l5 5q10-3 7-7zM16 14q-9-9-10-2 0 5 8 8 12-1 12-8-4-4-10 2z"/><path stroke="#cda875" stroke-width="3" d="m7 24 15 3"/>',
  flour:'<path fill="#e8dcc0" d="m9 4 14 0-3 6q11 10 7 18H5Q1 20 12 10z"/><path stroke="#94704c" d="M10 10h12"/><path fill="#cda875" d="M11 16h10v9H11z"/><path d="M16 23v-5m0 3-2-2m2 0 2-2"/>',
  fodder:'<path fill="#b6ad6b" d="M4 10h24v17H4z"/><ellipse fill="#d9bd70" cx="9" cy="18" rx="6" ry="9"/><path d="M9 13q-7 5 0 10 6-5 0-7-3 1 0 4m11-10v17m4-17v17"/><path stroke="#719451" d="m16 9 2-6m4 6 5-5"/>',
  manure:'<path fill="#94704c" d="M3 27q-1-7 6-8-2-6 5-7-1-6 4-8 0 6 5 8 7 2 4 8 6 2 3 7z"/><path stroke="#cda875" d="M9 20h13m-9-7h6"/>',
  herbs:'<path stroke="#94704c" stroke-width="2" d="m10 29 8-20m-3 13 10-6m-13 6L6 12"/><g fill="#719451"><path d="M17 12q-9-5-4-10 9 0 4 10zm3 9q-2-10 8-10 4 8-8 10zM11 22Q0 24 3 14q8-3 8 8z"/></g><path stroke="#d9bd70" stroke-width="3" d="m8 26 9 2"/>',
  vegetables:'<path fill="#719451" d="M5 12q-6-8 4-9l6 7q7-12 13-4l-8 9z"/><path fill="#b7c48b" d="M5 12q11-8 21 1l-5 13-6 4-7-6z"/><path d="m8 13 7 17 5-18M6 17l10 5 9-5"/>',
  carrots:iconPaths.resource_carrot,
  berries:iconPaths.resource_berries,
  eggs:iconPaths.resource_egg,
  milk:iconPaths.resource_milk,
  planks:iconPaths.resource_planks,
  stone:iconPaths.resource_stone,
  wooden_bowls:iconPaths.resource_bowl,
  crates:iconPaths.resource_crate,
  furniture:iconPaths.resource_chair,
  meat:iconPaths.resource_meat,
  wool:'<path fill="#e8dcc0" d="M7 10C1 3 15 0 17 6c7-7 15 2 10 7 7 6-1 14-7 11-2 9-15 6-14-1-8-1-6-12 1-13z"/><path stroke="#bda780" d="M9 10q14-6 13 7-1 10-10 7-8-3-3-11 5-6 10 0 5 8-2 8-7 1-5-5 2-3 5 0"/>',
  tools:'<path stroke="#94704c" stroke-width="3.5" d="m6 28 18-22m2 22L9 7"/><path fill="#bacac5" d="m2 8 5-6 11 8-5 6zm17-1 6-4 5 6-6 5z"/>',
  fruit:'<path d="m16 10 2-7"/><path fill="#719451" d="M18 7q2-9 11-3-3 6-11 3z"/><path fill="#b9866b" d="M16 11C0 2 0 27 12 29h8c12-2 12-27-4-18z"/><path stroke="#dfb49b" d="M9 14q-4 4-1 10"/>',
  hides:'<path fill="#bda780" d="m5 4 7 4 4-3 4 3 7-4-3 11 4 12-8-2-4 5-4-5-8 2 4-12z"/><path fill="#94704c" d="m12 11 6 2-2 8-5-4z"/>',
  leather:'<path fill="#b9866b" d="M5 10h22v17H5z"/><path fill="#bc8a57" d="M5 5h22v17H5z"/><path stroke="#e8dcc0" stroke-dasharray="2 3" d="M8 8h16v11H8z"/><path d="M5 22h22"/>',
  iron_ore:'<path fill="#7f8c8c" d="m3 23 5-16 12-3 9 13-5 11H10z"/><path fill="#b9866b" d="m8 7 8 4-5 9-8 3zm12 9 7-4 2 5-5 11-5-4z"/><path d="m16 11 4-7m-9 16 8 4"/>',
  iron:iconPaths.resource_ingot,
  steel:'<path fill="#dce8ed" d="m7 5 17 0 5 11-22 6-5-8z"/><path fill="#758792" d="m7 14 22-6v8L7 22z"/><path fill="#bacac5" d="m7 22 22-6v8L7 30l-5-8v-8z"/><path d="m7 22 0 8m5-19 10-3m-10 19 10-3"/>',
  saddles:'<path fill="#bc8a57" d="M4 12q8 5 12-7 4 12 12 7l-3 11H7z"/><path fill="#94704c" d="M12 17h8v8h-8z"/><path d="M8 22v6h5v-5m11-1v6h-5v-3"/><path stroke="#e8dcc0" d="m9 15 3 6m11-6-3 6"/>',
  trade_crates:'<path fill="#a78454" d="M4 7h24v22H4z"/><path fill="#758792" d="M8 7h3v22H8zm13 0h3v22h-3z"/><path fill="#e8dcc0" d="M13 11h9v10h-9z"/><path fill="#b9866b" d="m14 16 4-3 3 4-4 3z"/>',
  carved_furniture:'<path fill="#94704c" d="M6 5q10-7 20 0l-2 13H8z"/><path fill="#cda875" d="M4 18h24v6H4z"/><path fill="#a78454" d="M7 24h4l-1 6H6zm14 0h4l1 6h-4z"/><path stroke="#d9bd70" d="m16 6 4 5-4 5-4-5zM8 21h16"/>',
  bread:'<path fill="#cda875" d="M3 23v-8C3 3 29 3 29 15v8q-13 9-26 0z"/><path stroke="#e8dcc0" stroke-width="2" d="m10 11-3 7m10-8-3 8m10-7-3 7"/>',
  bread_rolls:'<g fill="#d9bd70"><ellipse cx="11" cy="12" rx="8" ry="7"/><ellipse cx="22" cy="20" rx="8" ry="7"/><ellipse cx="9" cy="25" rx="7" ry="5"/></g><path stroke="#e8dcc0" d="m9 8 3 7m8 2 3 6m-16 0 3 4"/>',
  herb_bread:'<path fill="#cda875" d="M4 26V15C4 4 28 4 28 15v11z"/><path d="m10 14-2 6m10-7-3 7m10-5-3 6"/><path fill="#719451" d="M15 10q-9-1-7-7 7-1 7 7 3-10 10-7 0 7-10 7z"/>',
  fruit_pies:'<path fill="#a78454" d="m3 17 3 11h20l3-11z"/><ellipse fill="#b9866b" cx="16" cy="16" rx="13" ry="7"/><path stroke="#d9bd70" stroke-width="3" d="m7 12 15 9M6 18l13-9m-7 14 14-9m-13-4 14 8"/><ellipse cx="16" cy="16" rx="13" ry="7"/>',
  cakes:'<path fill="#cda875" d="M4 14h24v14H4z"/><path fill="#e8dcc0" d="M4 12q12-5 24 0v8l-4-3-4 3-4-3-4 3-4-3-4 3z"/><path stroke="#b9866b" stroke-width="3" d="M5 24h22"/><circle fill="#b9866b" cx="16" cy="8" r="4"/><path stroke="#719451" d="m16 4 3-3"/>',
  prepared_meat:'<ellipse fill="#bacac5" cx="16" cy="23" rx="14" ry="7"/><path fill="#94704c" d="M6 21q-3-12 8-12 8 0 10 12-7 7-18 0z"/><path stroke="#cda875" d="m12 12-2 7m8-6-2 7"/><path fill="#e8dcc0" d="m22 16 4-4q-1-4 2-3 5 2 2 5l-2 1-4 4z"/>',
  herbed_meat:'<ellipse fill="#bacac5" cx="16" cy="24" rx="14" ry="6"/><path fill="#b9866b" d="M5 21Q2 8 15 9q12 0 12 12-12 9-22 0z"/><path stroke="#e8dcc0" d="m10 14-2 6m9-6-2 7"/><path fill="#719451" d="M18 14q-7-9 0-11l3 7q2-10 8-6-1 8-11 10z"/>',
  smoked_meat:'<path d="M9 9q-4-3 0-7m8 6q-4-3 0-7m8 8q-4-3 0-7"/><path fill="#94704c" d="M4 16q8-8 16-3l8 9q1 7-9 7L5 24z"/><path fill="#cda875" d="M4 16q8-5 12 6l3 7L5 24z"/><path d="m9 17 3 6m7-8 3 4"/>',
  sausages:'<path stroke="#cda875" stroke-width="2" d="m5 8 3 3m16 0 3-3M5 21l-2 3m24 0 2 3"/><path fill="#b9866b" d="M8 10q8 9 16 0l4 5q-12 13-24 0zm-3 9q10 10 20 2l3 5Q14 35 3 25z"/><path d="m11 15-2 3m13-3 2 3m-9 8-1 4"/>',
  meat_pies:'<path fill="#a78454" d="m3 18 4 11h18l4-11z"/><path fill="#cda875" d="M3 18C1 2 31 2 29 18q-13 9-26 0z"/><path d="m12 10-2 6m7-6-2 6m7-5-2 6"/><path fill="#b9866b" d="m9 23 6-3 8 4-6 3z"/>',
  berry_bush:'<path fill="#719451" d="M5 27C-1 23 2 15 7 15c-5-10 10-14 12-6 10-3 14 9 7 12 6 9-5 11-10 6z"/><g fill="#a76770"><circle cx="10" cy="18" r="3"/><circle cx="22" cy="17" r="3"/><circle cx="16" cy="24" r="3"/></g>',
  herb_patch:'<path fill="#80764a" d="m2 26 14-6 14 6-14 4z"/><path stroke="#719451" stroke-width="2" d="M9 25V10m7 17V6m7 19V12M9 18l-5-5m12 7 5-6m2 7 5-5"/><g fill="#cfb7cd"><circle cx="9" cy="8" r="3"/><circle cx="16" cy="5" r="3"/><circle cx="24" cy="11" r="3"/></g>',
  wild_horse:'<path fill="#ac7a50" d="M5 13h13l3-9 8 5-2 7h-6v7l-3 6h-3l1-10H9L7 29H4l1-10z"/><path fill="#705337" d="m18 13 1-10 4 1-2 12z"/><path d="m23 5 1-3M5 14l-3 9"/><circle fill="#e8dcc0" cx="26" cy="9" r="1"/>',
  chest:'<path fill="#a78454" d="M3 15h26v14H3z"/><path fill="#cda875" d="M3 15V9C3 0 29 0 29 9v6z"/><path fill="#d9bd70" d="M7 5h3v24H7zm15 0h3v24h-3zM14 13h5v7h-5z"/><path d="M3 15h26m-13 0v3"/>',
  village_hut:'<path fill="#bda780" d="M6 14h20v15H6z"/><path fill="#719451" d="m2 16 6-10 8-3 8 3 6 10z"/><path fill="#94704c" d="M13 20h6v9h-6z"/><path fill="#e4c980" d="M8 17h4v5H8zm12 0h4v5h-4z"/>',
  village_market:'<path fill="#cda875" d="M5 21h22v8H5z"/><path d="M6 21V12m20 9V12"/><path fill="#719451" d="M3 5h26v9q-4 5-8 0-5 5-10 0-4 5-8 0z"/><path fill="#b9866b" d="M9 21v-4h6v4z"/><path fill="#d9bd70" d="M19 21v-4h5v4z"/>',
  boat:'<path fill="#7899a2" d="M2 28q7 4 14 0 7 4 14 0"/><path fill="#94704c" d="m2 19 9 2h19l-7 7H9z"/><path stroke="#94704c" stroke-width="2" d="M16 21V2"/><path fill="#e8dcc0" d="m18 3 10 14H18z"/><path fill="#d9bd70" d="M13 5 5 17h8z"/>',
});
for(const [product,kind] of Object.entries({flower_boxes:'flower_box',wooden_tables:'wooden_table',signposts:'signpost',garden_arches:'garden_arch'}))iconPaths[product]=iconPaths[kind];
// Species keep different body shapes and markings after smoking, so the
// silhouette identifies the fish even when the warm smoked palette is used.
for(const [kind,body,color,marks] of [
  ['fish','M8 17Q17 5 27 17 17 29 8 17z','#7899a2','M17 10l-4-5 8 3'],
  ['trout','M7 18Q16 7 28 17 17 28 7 18z','#8c9b8b','M14 15h.1m4-2h.1m-2 7h.1m5-2h.1'],
  ['salmon','M6 18Q18 7 29 16 21 26 6 18z','#b9866b','m12 17 11-1m-8-4 2-5 5 5'],
  ['sturgeon','M6 19Q15 11 30 17L21 23 6 19z','#758792','m10 16 3-6 3 4 3-6 3 6m-10 3 11 1'],
]){
  for(const smoked of [false,true]){
    iconPaths[(smoked?'smoked_':'')+kind]=`${smoked?'<path d="M11 7q-3-3 0-6m8 5q-3-3 0-5m7 6q-3-3 0-6"/>':''}<path fill="${smoked?'#cda875':color}" d="m8 17-6-5v12l6-5z"/><path fill="${smoked?'#a87850':color}" d="${body}"/><path d="${marks}"/><path d="M23 15q-3 3 0 5"/><circle fill="#e8dcc0" cx="25" cy="16" r="1.3"/>`;
  }
}
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
$('cancel-regenerate-world').onclick=()=>{$('regenerate-world-dialog').close();regeneratingWorld=null;};
$('regenerate-world-dialog').addEventListener('close',()=>{regeneratingWorld=null;});
async function manageWorld(path,data,closeMenu=false){
  if(menuBusy)return;
  menuBusy=true;
  $('world-menu-error').textContent='';$('delete-world-error').textContent='';$('regenerate-world-error').textContent='';
  for(const button of document.querySelectorAll('dialog button'))button.disabled=true;
  try{
    await request(path,data);
    if(path==='/api/worlds/delete'||path==='/api/worlds/regenerate'){
      delete identities[data.world_id];localStorage.setItem('lantern-identities',JSON.stringify(identities));
      if(data.world_id==='legacy')localStorage.removeItem('lantern-identity');
      $('delete-world-dialog').close();deletingWorld=null;
      $('regenerate-world-dialog').close();regeneratingWorld=null;
    }
    if(refreshing)await refreshing;
    await refresh();
    if(closeMenu)$('world-menu').close();
  }catch(e){$(deletingWorld?'delete-world-error':regeneratingWorld?'regenerate-world-error':'world-menu-error').textContent=e.message;}
  finally{menuBusy=false;for(const button of document.querySelectorAll('dialog button'))button.disabled=false;$('close-world-menu').disabled=!world.world_id;}
}
$('new-world-form').onsubmit=event=>{event.preventDefault();manageWorld('/api/worlds/create',{name:$('world-name').value,sandbox:$('world-mode').value==='sandbox'},true);};
$('delete-world-form').onsubmit=event=>{event.preventDefault();if(deletingWorld)manageWorld('/api/worlds/delete',{world_id:deletingWorld.id,confirmation:$('delete-world-name').value});};
$('regenerate-world-form').onsubmit=event=>{event.preventDefault();if(regeneratingWorld)manageWorld('/api/worlds/regenerate',{world_id:regeneratingWorld.id,confirmation:$('regenerate-world-name').value},true);};
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
    const details=document.createElement('small');details.textContent=`${entry.sandbox?'Sandbox · ':''}${entry.town_count}/4 towns · ${entry.id===catalog.active_world?'Playing now':'Saved'}`;row.append(details);
    if(catalog.can_manage || entry.id===catalog.active_world){const open=document.createElement('button');open.textContent=entry.id===catalog.active_world?'Continue playing':'Open world';open.onclick=()=>catalog.can_manage?manageWorld('/api/worlds/open',{world_id:entry.id},true):$('world-menu').close();row.append(open);}
    if(catalog.can_manage){const remove=document.createElement('button');remove.className='danger quiet';remove.textContent='Destroy world';remove.onclick=()=>{deletingWorld=entry;$('delete-world-description').textContent=`Destroy “${entry.name}”?`;$('delete-world-name').value='';$('delete-world-error').textContent='';$('delete-world-dialog').showModal();};row.append(remove);}
    if(catalog.can_manage&&entry.sandbox){const regenerate=document.createElement('button');regenerate.className='quiet';regenerate.textContent='Regenerate world';regenerate.onclick=()=>{clearWalk();regeneratingWorld=entry;$('regenerate-world-description').textContent=`Generate fresh terrain for “${entry.name}” and reset its progress?`;$('regenerate-world-name').value='';$('regenerate-world-error').textContent='';$('regenerate-world-dialog').showModal();};row.append(regenerate);}
    $('world-list').append(row);
  }
}
function openPanel(next){
  if(next==='resources')clearWalk();
  if(world.shared_map && next==='build' && world.towns[identity?.town_id])viewed=identity.town_id;
  panel=next;selection=null;tool=null;moveIndex=null;buildTile=null;hoverTile=null;
  render();
}
for(const name of ['build','villagers','valley','help','resources','items']){
  $(name+'-button').onclick=()=>openPanel(panel===name?null:name);
}
$('close-resources').onclick=()=>openPanel(null);
$('resources-panel').addEventListener('cancel',event=>{event.preventDefault();openPanel(null);});
$('resources-panel').addEventListener('close',()=>{if(panel==='resources')openPanel(null);});
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
  if($('resources-panel').open){
    if(event.key==='Escape'||(event.key==='4'&&!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&!event.shiftKey
      &&!document.activeElement?.isContentEditable&&!['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName))){
      event.preventDefault();openPanel(null);canvas.focus();
    }
    return;
  }
  const key=event.key.toLowerCase();
  if(key===','&&event.metaKey&&!event.ctrlKey&&!event.altKey&&!event.shiftKey){
    event.preventDefault();
    if(!event.repeat&&!$('settings-panel').open&&!$('world-menu').open&&!$('delete-world-dialog').open && !$('regenerate-world-dialog').open && !$('upgrade-dialog').open)$('settings-button').onclick();
    return;
  }
  if(/^[1-9]$/.test(key)&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&!event.shiftKey){
    if(!event.repeat&&!$('settings-panel').open&&!$('world-menu').open&&!$('delete-world-dialog').open && !$('regenerate-world-dialog').open && !$('upgrade-dialog').open
      &&!document.activeElement?.isContentEditable&&!['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName)){
      const name=['world-menu','build','villagers','resources','items','valley','help'][Number(key)-1];
      if(name&&!$(name+'-button').disabled){
        event.preventDefault();
        if(name==='world-menu')clearWalk();
        $(name+'-button').onclick();
        if(name!=='world-menu'&&name!=='resources')canvas.focus();
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
      const nearby=(landscape.map_resources || []).some(r=>action[2].includes(r.kind)&&Math.abs(r.x-town.player.x)+Math.abs(r.y-town.player.y)<=1.8&&(action[0]!=='mine'||['g','m'].includes(landscape.terrain?.[r.y]?.[r.x])));
      if(!nearby){
        event.preventDefault();notice(action[0]==='mine'?'Walk beside a grassland stone to mine it.':action[3]);return;
      }
    }
  }
  if(key===keybindings.camera&&!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    event.preventDefault();setCameraLock(!cameraLocked);draw();notice(`Camera lock ${cameraLocked?'on':'off'}.`);
  }
  if(['up','left','down','right'].some(action=>keybindings[action]===key)&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    event.preventDefault();walkKeys.add(key);
  }
  for(const action of ['fish','chop','gather','mine'])if(key===keybindings[action]&&!event.repeat&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&canControlPlayer()){
    event.preventDefault();playerAction(action);
  }
  if(event.key==='Escape' && !$('settings-panel').open && !$('world-menu').open && !$('delete-world-dialog').open && !$('regenerate-world-dialog').open && !$('upgrade-dialog').open){
    if(world.towns[identity?.town_id]?.player?.activity)playerAction('stop_activity');
    openPanel(null);canvas.focus();
  }
});
function inspect(type,index,townId=viewed){
  if(world.shared_map && world.towns[townId])viewed=townId;
  panel=type==='villager'&&panel==='villagers'?'villagers':'selection';selection={type,index};tool=null;moveIndex=null;buildTile=null;hoverTile=null;render();
}
function productionStatus(v,building){
  const status=v.production_status || '';
  if(!status.startsWith('Workplace buffer full'))return status;
  const resources=[...new Set([v.pending_output,building?.stored_goods,building?.output_buffer,building?.pending_output,building?.orphaned_output]
    .flatMap(stock=>Object.entries(stock || {}).filter(([,amount])=>amount>0).map(([key])=>key)))];
  return `${resources.length?resources.map(goodName).join(', '):'Resource'} storage full`;
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
  const status=productionStatus(v,town.buildings[settlerBuilding(town,v,index)]);
  if(status.startsWith('Butchering '))return status;
  if(status.endsWith(' storage full'))return status;
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
  if(withLink){const name=document.createElement('button');name.textContent=v.name;name.className='villager-name';name.onclick=()=>inspect('villager',index);div.append(name);}
  const profile=document.createElement('dl');profile.className='villager-profile';
  const homes=town.buildings.filter(b=>b.kind==='home'&&!b.construction);
  const housing=homes.length?`Shared cottages · ${[...new Set(homes.map(buildingLevel))].sort((a,b)=>a-b).map(level=>`level ${level}`).join(', ')}`:'Town hall · shared shelter';
  const job=v.job==='idle'&&v.construction_target?'Builder (automatic)':{idle:'Resting',building:'Builder',woodcutting:'Woodcutting',farming:'Farming',fishing:'Fishing',hauling:'Hauling',...professionJobs}[v.job] || goodName(v.job);
  for(const [label,value] of [['House type',housing]]){
    const term=document.createElement('dt');term.textContent=label;
    const detail=document.createElement('dd');detail.textContent=value;profile.append(term);profile.append(detail);
  }
  div.append(profile);
  renderWorkerTools(town,v,index,div);
  div.append(skillBar('Woodcutting',v.woodcutting));div.append(skillBar('Farming',v.farming));div.append(skillBar('Fishing',v.fishing || 0));
  div.append(skillBar('Hauling',v.hauling || 0));
  const hauling=document.createElement('small');hauling.className='villager-hauling';
  hauling.textContent=`Hauling: ${(50*workerEfficiency(v,'hauling')).toFixed(0)} goods per trip · ${(1.8*(1+.25*Math.floor((v.hauling || 0)/20))).toFixed(2)} tiles/s before road bonuses. Successful deliveries earn practice; every 20 practice adds 50 carrying capacity and 25% base travel speed.`;div.append(hauling);
  {
    const trip=v.logistics_trip,load=document.createElement('small');load.className='villager-cargo';
    const carrying=trip?.picked_up&&!trip.delivered;
    const goods=Object.entries(trip?(trip.delivered?{}:trip.cargo || {}):v.pending_output || {}).filter(([,amount])=>amount>0).map(([key,amount])=>`${Number(amount.toFixed(2))} ${goodName(key)}`).join(' · ');
    load.textContent=goods?(carrying||!trip?`Carrying: ${goods}`:`Collecting: ${goods}`):'Carrying: nothing';div.append(load);
  }
  for(const [job,label] of Object.entries(professionJobs))if(v[job] || v.job===job)div.append(skillBar(label,v[job] || 0));
  const traits=document.createElement('small');traits.className='villager-talents';
  traits.textContent=Object.entries(v.talents || {}).map(([job,value])=>`${value>0?'Strength':'Weakness'}: ${goodName(job)} (${value>0?'+':'−'}25%)`).join(' · ') || 'No special talent';div.append(traits);
  if(!['idle','building','hauling','trading'].includes(v.job)){const output=document.createElement('small');output.className='villager-output';output.textContent=`Work output: ${workerEfficiency(v,v.job).toFixed(2)}× base before workplace bonuses`;div.append(output);}
  const activityLabel=villagerActivity(town,v,index);
  if(activityLabel!==job){const activity=document.createElement('small');activity.className='villager-activity';activity.textContent=activityLabel;div.append(activity);}
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
    const description=`${gear.name} · +${gear.bonus*100}% ${goodName(gear.job).toLowerCase()} output · ${v.job===gear.job?'Active for current job':'Inactive for current job'}`;
    equipped.title=description;equipped.setAttribute('aria-label',description);equipped.append(illustration(v.worker_tool));
    const tooltip=document.createElement('span');tooltip.className='skill-tooltip';tooltip.textContent=description;tooltip.setAttribute('aria-hidden','true');equipped.append(tooltip);name.append(equipped);
  }
  row.append(name);
  const job=document.createElement('td');
  row.append(job);
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
  for(const label of ['Name','Job','Skills','Talents','Work status']){
    const cell=document.createElement('th');cell.setAttribute('scope','col');cell.textContent=label;heading.append(cell);
  }
  head.append(heading);table.append(head);
  const body=document.createElement('tbody');
  for(const [index,v] of town.villagers.entries())body.append(villagerRow(town,v,index));
  table.append(body);return table;
}
const settlerSpaces={orchard_tree:0,hall:1,home:2,storehouse:1,woodcutter:1,fishing_hut:1,farm:1,acre:1,barn:2,sawpit:1,mill:1,smokehouse:2,workshop:1,fish_pond:2,orchard:2,market:1,bakery:1,butcher:2,...Object.fromEntries(decorationKinds.map(kind=>[kind,0]))};
function settlerBuilding(town,v,index){
  if(Number.isInteger(v.building_index))return v.building_index;
  const kind={woodcutting:'woodcutter',farming:'farm',fishing:'fishing_hut'}[v.job];
  const places=town.buildings.map((b,i)=>({b,i})).filter(({b})=>b.kind===kind&&(!b.construction||b.construction.upgrade)&&buildingUsable(town,b));
  return places.length?places[index%places.length].i:null;
}
function renderSettlers(town,item,buildingIndex,target=$('selection-details'),upgrades=target,details=target){
  const owner=viewed===identity?.town_id;
  if(item.kind==='hall'){
    const resident=document.createElement('button');resident.textContent=`Resident · ${owner?'You':'Town founder'}`;
    resident.onclick=()=>inspect('player');target.append(resident);
    const note=document.createElement('p');note.textContent='Only the town owner’s controllable character can settle here. Assign other settlers to cottages or workplaces.';
    details.append(note);return;
  }
  const level=item.settler_level || 1,capacity=settlerCapacity(item),maxLevel=capacityMaxLevel(item);
  const assigned=town.villagers.map((v,i)=>({v,i})).filter(({v,i})=>settlerBuilding(town,v,i)===buildingIndex);
  const heading=target.children[0],label=item.kind==='home'?'Settlers':'Workers';
  if(heading)heading.textContent=`${label} · ${assigned.length}/${capacity}`;
  else{const count=document.createElement('h3');count.textContent=`${label} · ${assigned.length}/${capacity}`;target.append(count);}
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
  if(requirements&&!town.sandbox){const note=document.createElement('small');note.textContent='Each settler needs '+Object.entries(requirements).map(([skill,amount])=>`${amount} ${goodName(skill).toLowerCase()} practice`).join(', ');target.append(note);}
  const eligible=available.filter(({v})=>!jobRequirement(v,job));
  for(const {v,i} of available){const option=document.createElement('option');option.value=i;const requirement=jobRequirement(v,job);option.textContent=v.name+(v.job==='idle'?' · Resting':'')+(requirement?` · Requires ${requirement}`:job?` · ${workerEfficiency(v,job).toFixed(2)}×`:'')+(v.job!=='idle'&&settlerBuilding(town,v,i)!==null?' · reassign':'');option.disabled=Boolean(requirement);chooser.append(option);}
  chooser.value=eligible.length?String(eligible[0].i):'';
  const add=document.createElement('button');add.textContent=['hall','home'].includes(item.kind)?'Add settler':'Add worker';add.disabled=assigned.length>=capacity||!eligible.length;
  chooser.disabled=add.disabled;
  add.onclick=()=>{if(!add.disabled)act({action:'assign_settler',index:Number(chooser.value),building_index:buildingIndex});};
  const assignment=document.createElement('div');assignment.className='worker-assignment';
  assignment.append(chooser);assignment.append(add);target.append(assignment);
  if(['mill','bakery','acre'].includes(item.kind))return;
  const upgrade=document.createElement('button');upgrade.className='upgrade-button';upgrade.textContent='Upgrade';
  upgrade.title=level>=maxLevel?'Settler capacity fully upgraded':`Upgrade to ${settlerCapacity({...item,settler_level:level+1})} spaces · ${town.sandbox?'Free · Instant':`${20*level} wood · ${30*level} seconds`}`;
  upgrade.setAttribute('aria-label',`Upgrade ${item.kind==='home'?'settler':'worker'} capacity: ${upgrade.title}`);
  upgrade.disabled=level>=maxLevel;
  upgrade.onclick=()=>{if(!upgrade.disabled)showUpgrade({action:'upgrade_capacity',index:buildingIndex});};
  upgrades.append(upgradeCard(upgrade,upgradePreview({action:'upgrade_capacity',index:buildingIndex},town,item),{title:item.kind==='home'?'Settler capacity':'Worker capacity',status:level>=maxLevel?'Maxed':'',benefits:[`${capacity}${level<maxLevel?` → ${settlerCapacity({...item,settler_level:level+1})}`:''} space${capacity===1&&level>=maxLevel?'':'s'}`]}));
}
function selectedProfessionRecipe(item){
  const recipes=world.progression?.profession_recipes?.[item.kind];
  const output=item.kind==='workshop'?item.craft_queue?.[0]?.recipe:item.recipe;
  return recipes?.find(recipe=>recipe.output===output) || recipes?.[0];
}
function renderStable(town,item,index,target){
  const nearby=Math.abs(town.player.x-item.x)+Math.abs(town.player.y-item.y)<=1.8;
  const adults=animalAges(item,'horse').filter(age=>age>=(world.progression.animal_growth_seconds || 300)).length;
  const full=(item.animals?.horse || 0)>=4*buildingLevel(item);
  const note=document.createElement('p');note.textContent='Stand beside the stable to settle, saddle, or return a horse. Guide: Help (7) → Animals & horses.';target.append(note);
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
  const note=document.createElement('small');note.textContent='Both towns need a finished market and room for incoming goods.';target.append(note);
  const select=(label,entries,categorized=false)=>{
    const input=document.createElement('select');input.setAttribute('aria-label',label);
    for(const [title,keys] of categorized?resourceCategories(entries.map(([key])=>key)):[['',entries.map(([key])=>key)]]){
      const group=categorized?document.createElement('optgroup'):input;
      if(categorized){group.label=title;input.append(group);}
      for(const value of keys){const option=document.createElement('option');option.value=value;option.textContent=entries.find(([key])=>key===value)[1];group.append(option);}
    }
    target.append(input);return input;
  };
  const towns=Object.entries(world.towns).filter(([id,t])=>id!==identity.town_id&&t.level>=5&&t.buildings.some(b=>b.kind==='market'&&!b.construction));
  if(towns.length){
    const receiver=select('Trading town',towns.map(([id,t])=>[id,t.name]));
    const goods=world.progression.tradable_goods || ['coins','wood','planks'];
    const give=select('Goods to offer',goods.map(key=>[key,`Offer ${goodName(key)}`]),true);
    const want=select('Goods to receive',goods.map(key=>[key,`Receive ${goodName(key)}`]),true);
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
  return catalog.sellable_goods || [...new Set(['hides','leather','iron_ore','iron','saddles','steel','trade_crates','fish','trout','salmon','sturgeon','planks','wool','flour','smoked_fish','smoked_trout','smoked_salmon','smoked_sturgeon','tools','herbs','fruit','milk','eggs','meat','bread','prepared_meat','vegetables','berries','carrots','fodder','manure','stone','wheat','wood',...Object.values(catalog.profession_recipes || {}).flat().map(recipe=>recipe.output)])];
}
function manualMarketSaleGoods(catalog){
  return catalog.manual_sellable_goods || marketSaleGoods(catalog);
}
function renderQuickMarketSale(town,target){
  const keys=manualMarketSaleGoods(world.progression),quantities=[1,5,10,50,100,200];
  const signature=JSON.stringify([world.world_id,identity?.town_id,keys]);
  if(renderQuickMarketSale.signature!==signature){
    const worldId=world.world_id,townId=identity?.town_id;
    const form=document.createElement('form');form.className='market-quick-sale';
    const heading=document.createElement('h3');heading.textContent='Quick sale';form.append(heading);
    const amountLabel=document.createElement('label');amountLabel.textContent='Amount';
    const amount=document.createElement('select');amount.id='sale-quantity';amount.setAttribute('aria-label','Amount to sell');
    for(const quantity of quantities){const option=document.createElement('option');option.value=String(quantity);option.textContent=String(quantity);amount.append(option);}
    amount.value='1';amountLabel.append(amount);form.append(amountLabel);
    const resourceLabel=document.createElement('label');resourceLabel.textContent='Resource';
    const resource=document.createElement('input');resource.id='sale-resource';resource.type='text';resource.placeholder='Type or choose a resource';resource.autocomplete='off';
    resource.setAttribute('list','sale-resource-options');resource.setAttribute('aria-label','Resource to sell');
    const options=document.createElement('datalist');options.id='sale-resource-options';
    for(const [,members] of resourceCategories(keys))for(const key of members){const option=document.createElement('option');option.value=goodName(key);options.append(option);}
    resourceLabel.append(resource);form.append(resourceLabel,options);
    const stock=document.createElement('small');stock.setAttribute('aria-live','polite');
    const sell=document.createElement('button');sell.type='submit';sell.id='sale-submit';
    form.append(stock,sell);
    let currentTown=town;
    const selectedResource=()=>keys.find(key=>goodName(key).toLowerCase()===resource.value.trim().toLowerCase());
    const update=(latest=currentTown)=>{
      currentTown=latest;
      const key=selectedResource(),quantity=Number(amount.value),available=key?amountOf(currentTown,key):0;
      const price=key?(world.progression.fish?.[key]?.coins ?? world.progression.sale_prices?.[key] ?? (key==='tools'?3:1)):0;
      const total=quantity*price;
      stock.textContent=key?`${Math.floor(available)} available${available<quantity?` · Need ${quantity} to sell this amount.`:''}`:resource.value.trim()?'This resource cannot be sold. Choose a resource from the list.':'Choose a resource to sell.';
      sell.textContent=key?`Sell ${quantity} · ${total} coin${total===1?'':'s'}`:'Sell';
      sell.disabled=viewed!==townId||identity?.town_id!==townId||world.world_id!==worldId||!key||!quantities.includes(quantity)||available<quantity;
    };
    amount.onchange=()=>update();resource.oninput=()=>update();resource.onchange=()=>update();
    form.onsubmit=event=>{
      event.preventDefault();
      update(world.towns[identity?.town_id] || currentTown);
      if(!sell.disabled)act({action:'sell_goods',resource:selectedResource(),amount:Number(amount.value)});
    };
    renderQuickMarketSale.signature=signature;renderQuickMarketSale.editor=form;renderQuickMarketSale.update=update;
  }
  renderQuickMarketSale.update(town);target.append(renderQuickMarketSale.editor);
}
function renderMarketSales(town,target){
  const keys=marketSaleGoods(world.progression);
  const signature=JSON.stringify([world.world_id,identity?.town_id,town.rules?.market_sales,keys]);
  // Reattach the same editor during snapshot refreshes to preserve drafts.
  if(renderMarketSales.signature!==signature){
    const editor=document.createElement('div');
    const title=document.createElement('h3');title.textContent='Automatic sales';editor.append(title);
    const note=document.createElement('p');note.className='storage-note';note.textContent='Enable products and set sale caps. Saves automatically. Guide: Help (7) → Markets & trade.';editor.append(note);
    for(const [title,members] of resourceCategories(keys)){
      const group=resourceGroup(title);editor.append(group);
      for(const key of members){
        const rule=town.rules?.market_sales?.[key];
        const row=document.createElement('form');row.className='storage-row market-sale-rule';
        const caption=document.createElement('label');caption.className='storage-caption';
        const name=document.createElement('span');name.textContent=goodName(key);caption.append(name);
        const toggle=document.createElement('input');toggle.id='sale-enabled-'+key;toggle.type='checkbox';toggle.checked=!!rule&&(rule.enabled ?? true);
        toggle.setAttribute('aria-label',`Sell ${goodName(key)} automatically`);caption.append(toggle);
        row.append(caption);
        const label=document.createElement('label');label.className='market-sale-cap';label.textContent='Sale cap';
        const cap=document.createElement('input');cap.id='sale-cap-'+key;cap.type='range';cap.min='1';cap.step='any';
        cap.max=String(Math.max(200,Math.ceil(resourceStorage(town,key)),Math.ceil(rule?.cap ?? 10)));
        cap.value=rule?.cap ?? 10;
        cap.setAttribute('aria-label',`${goodName(key)} sale cap`);
        const capValue=document.createElement('output');capValue.setAttribute('for',cap.id);
        const showCap=()=>{capValue.textContent=String(cap.value);cap.setAttribute('aria-valuetext',`${cap.value} goods`);};
        showCap();label.append(capValue,cap);row.append(label);
        const save=()=>{
          if(viewed!==identity?.town_id||!row.reportValidity())return;
          const value=Number(cap.value),savedCap=Number.isFinite(value)&&value>=1?value:(rule?.cap ?? 10);
          act({action:'configure_market_sale',resource:key,enabled:toggle.checked,cap:savedCap});
        };
        toggle.onchange=save;
        cap.oninput=()=>{cap.value=Math.round(Number(cap.value));showCap();};
        cap.onchange=save;
        row.onsubmit=event=>{event.preventDefault();save();};
        group.append(row);
      }
    }
    renderMarketSales.signature=signature;renderMarketSales.editor=editor;
  }
  target.append(renderMarketSales.editor);
}
function productionFacts(rows){
  const list=document.createElement('dl');list.className='production-facts';
  for(const [label,value] of rows){
    const term=document.createElement('dt');term.textContent=label;
    const detail=document.createElement('dd');detail.textContent=value;
    list.append(term,detail);
  }
  return list;
}
function craftedDecoration(output){
  return world.progression?.decorations?.find(kind=>world.progression.buildings[kind]?.[output]);
}
function renderRecipes(town,item,index,target){
  const recipes=world.progression?.profession_recipes?.[item.kind];
  if(!recipes)return;
  let draft=null;
  if(item.kind==='workshop'){
    const key=`${world.world_id}/${viewed}/${index}/${item.x},${item.y}`;
    if(renderRecipes.workshopDraft?.key!==key)renderRecipes.workshopDraft={key,recipe:selectedProfessionRecipe(item).output,amount:'1'};
    draft=renderRecipes.workshopDraft;
  }
  const job=world.progression.buildings[item.kind].job;
  const workers=town.villagers.filter(v=>v.building_index===index&&v.job===job&&!jobRequirement(v,job));
  const chooser=document.createElement('select');chooser.setAttribute('aria-label','Production recipe');
  for(const spec of recipes){
    const unlocked=town.sandbox||spec.level===1||workers.some(v=>(v[job] || 0)>=spec.level);
    const option=document.createElement('option');option.value=spec.output;
    option.textContent=`${item.kind==='workshop'&&spec.output==='furniture'?'Wooden bench (furniture)':goodName(spec.output)}${craftedDecoration(spec.output)?' · Decoration':''} · Lv ${spec.level}${unlocked?'':' · Locked'}`;
    option.disabled=!unlocked;chooser.append(option);
  }
  chooser.value=draft?.recipe || selectedProfessionRecipe(item).output;
  chooser.disabled=viewed!==identity?.town_id;
  target.append(chooser);
  const note=document.createElement('div');note.className='recipe-details';
  const updateNote=()=>{
    const spec=recipes.find(recipe=>recipe.output===chooser.value);
    const seconds=world.progression.production_seconds?.[spec.output] || 3;
    const food=world.progression.edible?.[spec.output];
    const rows=[['Ingredients',costText(spec.inputs)],['Produces',`${spec.amount} ${goodName(spec.output).toLowerCase()}`],
      ['Batch time',`${seconds}s`],['Practice',`+${((item.kind==='butcher'?0.1:0.08)*seconds/3).toFixed(2)} per batch`]];
    if(food)rows.push(['Food value',`${Number((spec.amount*food).toFixed(2))} per batch`]);
    if(spec.level>1&&!town.sandbox)rows.push(['Requires',`Worker level ${spec.level}+`]);
    const decoration=craftedDecoration(spec.output);
    if(decoration)rows.push(['Place',`${world.progression.buildings[decoration].label} · Build → Decorations after delivery`]);
    note.replaceChildren(productionFacts(rows));
  };
  updateNote();
  chooser.onchange=()=>{if(!chooser.disabled){updateNote();if(draft)draft.recipe=chooser.value;else act({action:'set_recipe',index,recipe:chooser.value});}};
  target.append(note);
  if(item.kind==='workshop'){
    const quantity=document.createElement('input');quantity.id='workshop-quantity';quantity.type='number';quantity.min=1;quantity.max=999;quantity.step=1;quantity.value=draft.amount;
    quantity.setAttribute('aria-label','Requested quantity');quantity.disabled=chooser.disabled;
    const add=document.createElement('button');add.textContent='Add to queue';add.disabled=chooser.disabled;
    const valid=()=>Number.isInteger(Number(quantity.value))&&Number(quantity.value)>=1&&Number(quantity.value)<=999&&recipes.some(spec=>spec.output===chooser.value)&&!chooser.children[recipes.findIndex(spec=>spec.output===chooser.value)].disabled;
    quantity.oninput=()=>{draft.amount=quantity.value;add.disabled=chooser.disabled||!valid();};
    const preview=chooser.onchange;chooser.onchange=()=>{preview();add.disabled=chooser.disabled||!valid();};
    add.disabled=chooser.disabled||!valid();
    add.onclick=()=>{if(!chooser.disabled&&valid())act({action:'queue_workshop',index,recipe:chooser.value,amount:Number(quantity.value)});};
    target.append(quantity,add);
    const heading=document.createElement('h3');heading.textContent='Request queue';target.append(heading);
    const hint=document.createElement('p');hint.textContent=item.craft_queue?.length?'Workers craft requests in order. Finished goods become available after delivery.':'No requests. Add an item and quantity to start crafting.';target.append(hint);
    for(const [position,order] of (item.craft_queue || []).entries()){
      const row=document.createElement('div');row.className='storage-row';
      const label=document.createElement('span');label.textContent=`${position+1}. ${order.recipe==='furniture'?'Wooden benches':goodName(order.recipe)} · ${Number(order.remaining.toFixed(2))} / ${order.amount} remaining`;row.append(label);
      const cancel=document.createElement('button');cancel.textContent='Cancel';cancel.disabled=chooser.disabled;
      cancel.onclick=()=>{if(!cancel.disabled)act({action:'cancel_workshop',index,request_id:order.id});};row.append(cancel);target.append(row);
    }
  }
}
function productionDescription(item){
  if(item.kind==='mine')return 'Output: 2 iron ore / 9s batch\nRequires an iron vein within 10 tile steps\nOre regenerates slowly; steel pickaxes add 25% output';
  const spec=selectedProfessionRecipe(item);
  const recipe=spec?[spec.inputs,spec.output,spec.amount]:world.progression.recipes[item.kind];
  if(recipe){const seconds=world.progression.production_seconds?.[recipe[1]] || 3;return `${costText(recipe[0])} → ${recipe[2]} ${goodName(recipe[1]).toLowerCase()} / ${seconds}s batch${item.kind==='smokehouse'?'\nEach raw fish becomes its own smoked species\n+0.5 food and +1 coin per fish\nAccepts fish, trout, salmon, and sturgeon':''}\n${((item.kind==='butcher'?0.1:0.08)*seconds/3).toFixed(2)} practice / batch (neutral talent)\nSkill: larger batches + more ingredients\nRequires qualified workers${spec?` · Recipe level ${spec.level}`:''}`;}
  return {barn:'Set animal counts\nGrow animals, then butcher adults\nChicken: 60s · Sheep: 120s · Pig: 240s · Cow: 360s\nNo feed or workers required\nEach keeper adds 1× growth speed\n4 animals / level',orchard:'Plant fruit trees within 3 tiles\nTrees regrow fruit after every harvest\nWorkers pick fruit and carry it to this shed',stable:'Capture wild horses and bring them here\nFeed: fodder / vegetables\nSettling and growth: 5 minutes of full feeding\nBreed two adults; saddle one to ride\n4 horses / level',market:'Sell goods for coins\nOffer goods to another town\nBoth owners need a completed market'}[item.kind] || 'Requires workers';
}
function animalAges(barn,animal){
  const duration=animalGrowthDuration(animal);
  return Array.from({length:barn.animals?.[animal] || 0},(_,i)=>barn.animal_growth?.[animal]?.[i] ?? duration);
}
function animalName(barn,animal,number){
  return barn.animal_names?.[animal]?.[number] || `${goodName(animal)} ${number+1}`;
}
function animalGrowthDuration(animal){
  return world.progression?.animals?.[animal]?.growth_seconds || world.progression?.animal_growth_seconds || 300;
}
function barnGrowthSpeed(town,item,index){
  const assigned=town.villagers.filter((v,i)=>settlerBuilding(town,v,i)===index).slice(0,settlerCapacity(item));
  return 1+assigned.filter(v=>v.job==='husbandry'&&!v.construction_target).length;
}
function barnGrowthSeconds(age,duration,speed,now=performance.now()){
  if(age>=duration)return 0;
  const clock=world.simulation_clock;
  if(!clock||worldReceivedAt===null)return Math.ceil((duration-Math.min(duration,age))/speed);
  const ticks=Math.max(1,Math.ceil((duration-Math.min(duration,age))/(speed*clock.tick_seconds)));
  const elapsed=Math.max(0,(now-worldReceivedAt)/1000);
  return Math.max(0,Math.ceil(clock.next_tick_in+(ticks-1)*clock.tick_seconds-elapsed));
}
function updateBarnTimers(now=performance.now()){
  const timers=renderBarnAnimals.timers;
  if(!timers||panel!=='selection'||selection?.type!=='building'||selection.index!==timers.index
      ||world.world_id!==timers.worldId||viewed!==timers.townId)return;
  const town=world.towns[viewed],item=town?.buildings[timers.index];
  if(!item||item.kind!=='barn'||item.x!==timers.x||item.y!==timers.y)return;
  const speed=barnGrowthSpeed(town,item,timers.index);
  const count=Object.values(item.animals || {}).reduce((sum,n)=>sum+n,0);
  const growing=Object.keys(item.animals || {}).reduce((sum,animal)=>sum+animalAges(item,animal).filter(age=>age<animalGrowthDuration(animal)).length,0);
  const state=`${growing} growing · ${count-growing} fully grown · ${speed}× speed`;
  if(timers.state.textContent!==state)timers.state.textContent=state;
  for(const {animal,timing,stock} of timers.cycles){
    const text=`${animalGrowthDuration(animal)}s base cycle · ${Number((animalGrowthDuration(animal)/speed).toFixed(1))}s average with current keepers`;
    if(timing.textContent!==text)timing.textContent=text;
    const ages=animalAges(item,animal),growing=ages.filter(age=>age<animalGrowthDuration(animal)).length;
    const stockText=ages.length?`${growing} growing · ${ages.length-growing} fully grown`:'Not stocked';
    if(stock.textContent!==stockText)stock.textContent=stockText;
  }
  for(const {animal,number,duration,time,percent,progress,label} of timers.rows){
    if(number>=(item.animals?.[animal] || 0))continue;
    const age=Math.min(duration,item.animal_growth?.[animal]?.[number] ?? duration);
    const seconds=barnGrowthSeconds(age,duration,speed,now);
    const text=age>=duration?'Ready for butchery':seconds?`${Math.floor(seconds/60)}m ${seconds%60}s until fully grown`:'Awaiting host update…';
    const labelText=animalName(item,animal,number)+(age>=duration?'':' · Growing');
    if(label.textContent!==labelText)label.textContent=labelText;
    if(time.textContent!==text)time.textContent=text;
    const value=`${Math.floor(age/duration*100)}%`;
    if(percent.textContent!==value)percent.textContent=value;
    if(progress.value!==age)progress.value=age;
    percent.hidden=progress.hidden=age>=duration;
  }
}
function renderBarnAnimals(town,item,index,target){
  renderBarnAnimals.dragging=false;
  const catalog=world.progression,owner=viewed===identity?.town_id,capacity=4*buildingLevel(item);
  const count=Object.values(item.animals || {}).reduce((sum,n)=>sum+n,0);
  const growing=Object.keys(item.animals || {}).reduce((sum,animal)=>sum+animalAges(item,animal).filter(age=>age<animalGrowthDuration(animal)).length,0);
  const controls=[];
  const validCount=input=>input.value!==''&&Number.isInteger(Number(input.value))&&Number(input.value)>=0&&Number(input.value)<=Number(input.max);
  const updateSliders=()=>{
    const hiddenCount=count-controls.reduce((sum,{animal})=>sum+(item.animals?.[animal] || 0),0);
    const total=hiddenCount+controls.reduce((sum,{input})=>sum+Number(input.value),0);
    for(const {input,amount} of controls){
      input.max=capacity-total+Number(input.value);
      amount.textContent=`${input.value} / ${input.max}`;
      input.setAttribute('aria-valuetext',`${input.value} animals, maximum ${input.max}`);
    }
    spaces.textContent=`${total} / ${capacity} animal spaces`;
  };
  const speed=barnGrowthSpeed(town,item,index);
  const timers={worldId:world.world_id,townId:viewed,index,x:item.x,y:item.y,rows:[],cycles:[]};
  renderBarnAnimals.timers=timers;
  const overview=document.createElement('div');overview.className='livestock-overview';
  const spaces=document.createElement('strong');spaces.textContent=`${count} / ${capacity} animal spaces`;
  const state=document.createElement('span');state.textContent=`${growing} growing · ${count-growing} fully grown · ${speed}× speed`;timers.state=state;
  overview.append(spaces,state);target.append(overview);
  const cycle=document.createElement('p');cycle.className='livestock-cycle';cycle.textContent='Grow → Fully grown → Butcher';target.append(cycle);
  const note=document.createElement('p');note.className='livestock-note';
  note.textContent='No feed required. Growth and replacement rules: Help (7) → Animals & horses.';target.append(note);
  for(const [animal,spec] of Object.entries(catalog.animals)){
    if(animal==='horse'||town.level<spec.level)continue;
    const duration=animalGrowthDuration(animal),ages=animalAges(item,animal);
    const products=catalog.animal_products?.[animal] ?? (animal==='pig'?0:25);
    const card=document.createElement('div');card.className='livestock-card';
    const heading=document.createElement('div');heading.className='livestock-heading';
    const name=document.createElement('strong');name.textContent=goodName(animal);
    const growing=ages.filter(age=>age<duration).length;
    const stock=document.createElement('span');stock.className='livestock-state';stock.textContent=ages.length?`${growing} growing · ${ages.length-growing} fully grown`:'Not stocked';
    heading.append(name,stock);card.append(heading);
    if(owner){
      const control=document.createElement('label');control.className='livestock-count';
      const caption=document.createElement('span');caption.textContent='Animals to keep';
      const amount=document.createElement('output');
      const input=document.createElement('input');input.id=`barn-count-${viewed}-${index}-${animal}`;input.type='range';input.min=0;input.max=capacity-count+ages.length;input.step=1;input.value=ages.length;
      input.setAttribute('aria-label',`${goodName(animal)} to keep`);amount.setAttribute('for',input.id);
      controls.push({animal,input,amount});
      input.onpointerdown=()=>{renderBarnAnimals.dragging=true;};
      input.onpointerup=input.onpointercancel=input.onblur=()=>{renderBarnAnimals.dragging=false;};
      input.oninput=()=>{if(validCount(input))updateSliders();};
      input.onchange=()=>{
        renderBarnAnimals.dragging=false;
        if(viewed!==identity?.town_id||!validCount(input))return;
        updateSliders();
        act({action:'set_animal_count',index,animal,count:Number(input.value)});
      };
      control.append(caption,amount,input);card.append(control);
    }
    const timing=document.createElement('p');timing.className='livestock-note';
    timing.textContent=`${duration}s base cycle · ${Number((duration/speed).toFixed(1))}s average with current keepers`;timers.cycles.push({animal,timing,stock});
    const yieldNote=document.createElement('p');yieldNote.className='livestock-yield';
    yieldNote.textContent=animal==='pig'?'Raised for meat and hides':`${products} ${goodName(spec.output).toLowerCase()} per growth cycle`;
    const meat=document.createElement('small');meat.className='livestock-note';meat.textContent=`${catalog.animal_meat?.[animal] || 0} meat when butchered; free young ${animal} only when none remain in this barn`;
    ages.forEach((age,number)=>{
      const confirmed=Math.min(duration,age);
      const row=document.createElement('div');row.className='livestock-growth';
      const caption=document.createElement('div');caption.className='livestock-growth-caption';
      const label=document.createElement('span');label.textContent=animalName(item,animal,number)+(confirmed>=duration?'':' · Growing');
      const percent=document.createElement('span');percent.textContent=`${Math.floor(confirmed/duration*100)}%`;percent.hidden=confirmed>=duration;
      caption.append(label,percent);row.append(caption);
      const progress=document.createElement('progress');progress.max=duration;progress.value=confirmed;progress.hidden=confirmed>=duration;
      progress.setAttribute('aria-label',`${goodName(animal)} ${number+1} growth`);row.append(progress);
      const seconds=barnGrowthSeconds(confirmed,duration,speed),time=document.createElement('small');
      time.textContent=confirmed>=duration?'Ready for butchery':seconds?`${Math.floor(seconds/60)}m ${seconds%60}s until fully grown`:'Awaiting host update…';
      timers.rows.push({animal,number,duration,time,percent,progress,label});row.append(time);card.append(row);
    });
    card.append(timing,yieldNote,meat);
    target.append(card);
  }
  updateSliders();
}
function renderAnimalButchery(town,item,index,target){
  const catalog=world.progression,owner=viewed===identity?.town_id;
  const card=document.createElement('div');card.className='animal-butchery';target.append(card);
  const heading=document.createElement('h3');heading.textContent='Butcher animals';card.append(heading);
  const note=document.createElement('p');note.className='livestock-note';note.textContent='Adults wait for butchery; a free young animal appears only when no others of that species remain in this barn.';card.append(note);
  const task=item.butchering,duration=catalog.butcher_seconds || 60;
  if(task){
    const status=document.createElement('p');status.className='butchery-status';
    status.textContent=`${task.name || goodName(task.animal)} → ${task.amount} meat · ${Math.ceil(task.remaining)}s of work remaining${task.remaining===0?' · Waiting for buffer space or production cap':''}`;card.append(status);
    if(task.stage&&task.stage!=='working')status.textContent=`${task.stage==='dragging'?'Dragging animal to shop':'Collecting animal from barn'} · ${status.textContent}`;
    const progress=document.createElement('progress');progress.max=duration;progress.value=duration-task.remaining;progress.setAttribute('aria-label','Animal butchery progress');card.append(progress);
  }
  if(!owner)return;
  const chooser=document.createElement('select');chooser.setAttribute('aria-label','Animal to butcher');
  const choices=[];
  town.buildings.forEach((barn,barnIndex)=>{
    if(barn.kind!=='barn'||barn.construction&&!barn.construction.upgrade)return;
    for(const animal of Object.keys(catalog.animal_meat || {}).filter(animal=>animal!=='horse')){
      const ages=animalAges(barn,animal),adult=ages.findIndex(age=>age>=animalGrowthDuration(animal));
      const adults=ages.filter(age=>age>=animalGrowthDuration(animal)).length;
      if(adults)choices.push({barn,barnIndex,animal,adults,name:animalName(barn,animal,adult)});
    }
  });
  for(const {barn,barnIndex,animal,adults,name} of choices){
    const option=document.createElement('option');option.value=`${barnIndex}:${animal}`;
    const multipleBarns=choices.some(choice=>choice.animal===animal&&choice.barnIndex!==barnIndex);
    option.textContent=`${name} · ${goodName(animal)} · ${adults} ready${multipleBarns?` · Barn (${barn.x}, ${barn.y})`:''}`;chooser.append(option);
  }
  if(choices.length)chooser.value=chooser.children[0].value;
  const label=document.createElement('label');label.hidden=!!task;label.textContent='Fully grown animal';label.append(chooser);card.append(label);
  const preview=document.createElement('p');preview.className='livestock-yield';preview.hidden=!!task;
  const updatePreview=()=>{
    const [barnIndex,animal]=(chooser.value || '').split(':');
    const replacement=animal&&town.buildings[Number(barnIndex)].animals[animal]===1;
    preview.textContent=animal?`${catalog.animal_meat[animal]} meat${replacement?` + 1 free young ${animal}`:''} · ${duration}s butcher work`:'Select a fully grown barn animal for butchery.';
  };
  chooser.onchange=updatePreview;updatePreview();card.append(preview);
  const qualified=town.villagers.some(v=>v.building_index===index&&v.job==='butchery'&&!jobRequirement(v,'butchery'));
  const button=document.createElement('button');button.className='livestock-butcher';button.textContent=task?'Butchering in progress':!choices.length?'No fully grown animals':!qualified?'Assign a qualified butcher':'Butcher';
  button.disabled=!!task||!choices.length||!qualified;chooser.disabled=button.disabled;
  button.onclick=()=>{if(button.disabled||viewed!==identity?.town_id)return;const [barnIndex,animal]=chooser.value.split(':');act({action:'butcher_animal',index,barn_index:Number(barnIndex),animal});};card.append(button);
}
function renderProfession(town,item,index,target,upgrades,details,boats,unlockedUpgrades){
  if(item.construction)return;
  const owner=viewed===identity?.town_id;
  const catalog=world.progression;
  if(!catalog)return;
  if(boats){
    const note=document.createElement('p');
    note.textContent=fishingBoatLevel(item)?`Boat owned · Tier ${fishingBoatLevel(item)}/3. Click its boat on the map for cargo, crew and boat upgrades.`:'No boat owned · Buy one boat for this fishing hut. It appears beside nearby water immediately.';
    boats.append(note);
  }
  if(item.kind==='butcher'){
    renderAnimalButchery(town,item,index,target);
    const heading=document.createElement('h3');heading.textContent='Prepare meat';target.append(heading);
  }
  renderRecipes(town,item,index,target);
  if(item.kind==='farm'){
    const acres=town.buildings.filter(a=>a.kind==='acre'&&a.farm_index===index);
    const crops=new Set(acres.map(a=>a.crop || 'vegetables'));
    const chooser=document.createElement('select');chooser.setAttribute('aria-label','Crop for all acres');
    if(crops.size!==1){
      const option=document.createElement('option');option.value='';option.textContent=acres.length?'Mixed crops':'No acres yet';option.disabled=true;chooser.append(option);
    }
    for(const [key,level] of Object.entries(catalog.crops)){
      if(town.level<level)continue;
      const option=document.createElement('option');option.value=key;option.textContent=goodName(key);chooser.append(option);
    }
    chooser.value=crops.size===1?[...crops][0]:'';chooser.disabled=!owner||!acres.length;
    chooser.onchange=()=>{if(!chooser.disabled&&chooser.value)act({action:'set_crop',index,crop:chooser.value});};
    const label=document.createElement('label');label.textContent='Crop for all acres';label.append(chooser);target.append(label);
    target.append(fact(`${catalog.farming_harvest_yields?.vegetables ?? catalog.farming_harvest_yield ?? 10} vegetables · ${catalog.farming_harvest_yield ?? 10} other crops`,'Base harvest per acre · Skills, tools and upgrades apply'));
  }
  if(item.kind==='barn')renderBarnAnimals(town,item,index,target);
  if(item.kind==='stable'){
    target.append(fact(`${Object.values(item.animals || {}).reduce((a,n)=>a+n,0)} / ${4*buildingLevel(item)}`,'Animal capacity'));
    for(const [animal,spec] of Object.entries(catalog.animals)){
      if(town.level<spec.level || (animal==='horse')!==(item.kind==='stable'))continue;
      const ages=animalAges(item,animal),duration=catalog.animal_growth_seconds || 300;
      const adults=ages.filter(age=>age>=duration).length;
      target.append(fact(`${adults} adult · ${ages.length-adults} growing`,`${goodName(animal)} → Riding / breeding`));
      ages.forEach(age=>{if(age<duration){const growth=document.createElement('small');growth.style.display='block';growth.textContent=`${Math.floor(age/duration*100)}%`;target.append(growth);}});
      if(owner){
        const full=Object.values(item.animals || {}).reduce((a,n)=>a+n,0)>=4*buildingLevel(item);
        const breed=document.createElement('button');breed.textContent=`Breed ${animal} · 3 fodder + 1 herb`;breed.disabled=adults<2 || !canAfford(town,{fodder:3,herbs:1}) || full;breed.onclick=()=>{if(!breed.disabled)act({action:'breed_animal',index,animal});};target.append(breed);
      }
    }
  }
  if(item.kind==='stable'&&owner)renderStable(town,item,index,target);
  if(item.kind==='market'&&owner){renderMarketSales(town,target);renderTownTrades(town,target);}
  for(const [key,spec] of Object.entries(catalog.upgrades))if(spec.building===item.kind){
    const unlocked=item.profession_upgrades?.includes(key);
    const fishingBoat=item.kind==='fishing_hut'&&['boat','improved_boat','master_boat'].includes(key);
    if(fishingBoat&&(key!=='boat'||unlocked))continue;
    if(!fishingBoat&&!unlocked&&(town.level<spec.level || !town.sandbox&&professionSkill(town,spec.skill)<(spec.xp || 0) || spec.requires&&!item.profession_upgrades?.includes(spec.requires)))continue;
    const button=document.createElement('button');button.textContent=fishingBoat&&key==='boat'?'Buy':'Upgrade';
    button.setAttribute('aria-label',fishingBoat?`${key==='boat'?'Buy':'Upgrade to'} ${spec.label.toLowerCase()}`:`Upgrade ${spec.label}`);
    button.className='upgrade-button';
    button.disabled=!owner || unlocked;button.onclick=()=>{if(!button.disabled)showUpgrade({action:'profession_upgrade',index,upgrade:key});};
    (unlocked?unlockedUpgrades:fishingBoat?boats:upgrades).append(upgradeCard(button,upgradePreview({action:'profession_upgrade',index,upgrade:key},town,item),{status:unlocked?'Unlocked':''}));
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
function renderBuildingStatus(town,item,index,target){
  if(item.kind==='butcher'&&item.butchering&&!item.construction)return;
  const section=buildingSection('Status'),assigned=town.villagers.map((v,i)=>({v,i})).filter(({v,i})=>settlerBuilding(town,v,i)===index);
  section.className='building-section building-status';
  const problems=[];
  const line=(label,value,max,caption)=>{
    const text=document.createElement('p');text.className='workplace-status';text.textContent=label;section.append(text);
    const progress=document.createElement('progress');progress.max=max;progress.value=Math.max(0,Math.min(max,value));progress.setAttribute('aria-label',caption);section.append(progress);
  };
  if(item.construction){
    const c=item.construction;
    line(`${c.upgrade?'Upgrading':'Under construction'} · ${Math.floor(c.progress/c.total*100)}% complete · ${Math.ceil(c.total-c.progress)}s remaining of work`,c.progress,c.total,`${buildingName(item)} construction`);
    if(!c.progress)problems.push('Waiting for a builder. Assign a builder or free an idle settler to start work.');
  }else if(item.kind==='hall'&&town.upgrade_remaining){
    const total=world.hall_upgrades?.find(next=>next.level===town.level+1)?.seconds || town.upgrade_remaining;
    line(`Upgrading · ${town.upgrade_remaining}s remaining`,total-town.upgrade_remaining,total,'Town hall upgrade');
  }else if(item.kind==='orchard_tree'){
    const state=item.tree || {stage:'sapling',elapsed:0},duration=(world.progression?.orchard_stage_seconds || {sapling:60,blossom:30,fruit:60,harvested:30})[state.stage];
    line(orchardTreeStatus(item),duration?state.elapsed || 0:1,duration || 1,'Fruit tree growth');
    if(!town.buildings[item.orchard_index])problems.push('No linked orchard.');
  }else if(item.kind==='acre'){
    const field=item.field || {stage:'unplowed',elapsed:0},growing=['sprouting','growing'].includes(field.stage),ripe=field.stage==='ripe';
    line(({unplowed:'Unplowed soil',plowed:'Waiting for seeding',sprouting:'Sprouting',thirsty:'Needs water',growing:'Growing',ripe:'Ready to harvest'})[field.stage] || field.stage,growing?(field.elapsed || 0):ripe?1:0,growing?(world.progression?.farming_growth_seconds || 15):1,'Field growth');
    if(!town.buildings[item.farm_index])problems.push('No linked farm. This acre needs a farm to be tended.');
    else if(field.stage==='thirsty')problems.push('Waiting for a farmer to bring water. Keep a route to water or a well clear.');
  }else if(['hall','home','well',...decorationKinds].includes(item.kind)){
    line(item.kind==='well'?'Water available':decorationKinds.includes(item.kind)?'Decoration placed':'Ready',1,1,'Building ready');
  }else{
    const sourceWork=['woodcutter','farm','fishing_hut'].includes(item.kind);
    if(sourceWork&&!buildingUsable(town,item))problems.push(`No ${item.kind==='woodcutter'?'trees':'water'} within ${resourceReach(item)} tile steps. Move this building closer to a source.`);
    if(!assigned.length)problems.push('No workers assigned. Add a worker below to start work.');
    const paused=new Set();
    for(const {v} of assigned){
      const status=productionStatus(v,item);
      if(/waiting|requires|full|blocked|no |buy |cap reached|already tended|unavailable|cancelled/i.test(status)){
        const help=/cap reached/i.test(status)?'Use stored goods or raise the production cap in Resources.':
          /full/i.test(status)?'Haul, use or sell goods to free storage space.':
          /requires/i.test(status)?'Assign a qualified worker or train this worker.':
          /route|blocked|unavailable|cancelled/i.test(status)?'Keep paths and adjacent access tiles clear.':
          /suppl|delivery|resources/i.test(status)?'Check the required inputs and access to enabled storage.':'';
        paused.add(v);
        problems.push(`${status}${help?`. ${help}`:''}`);
      }
    }
    problems.splice(0,problems.length,...new Set(problems));
    const active=assigned.filter(({v})=>v.production_work||v.work_animation||v.logistics_trip);
    if(!active.length)line(problems.length?problems.shift():'Waiting for next cycle',0,1,'Work cycle');
    const activities=active.map(({v,i})=>{
      const state=v.work_animation,recipe=selectedProfessionRecipe(item),output=recipe?.output || world.progression?.recipes?.[item.kind]?.[1];
      const trip=v.logistics_trip,task=item.butchering;
      let value=0,total=1;
      if(trip){value=trip.step || 0;total=Math.max(1,(trip.path?.length || 1)-1);}
      else if(task&&state?.phase==='working'){total=world.progression?.butcher_seconds || 60;value=total-task.remaining;}
      else if(v.production_work){total=world.progression?.production_seconds?.[output] || 3;value=v.production_work.elapsed || 0;}
      else if(state?.phase==='working'){total=v.job==='farming'?2:v.job==='orcharding'?3.2/(workerEfficiency(v,'orcharding')*(1+.25*(buildingLevel(item)-1))):3.2;value=state.elapsed || 0;}
      else if(state?.phase==='depositing'||state?.phase==='collecting_water'){total=.65;value=state.elapsed || 0;}
      let label=productionStatus(v,item) || villagerActivity(town,v,i);
      if(item.kind==='farm'&&state?.phase==='working'&&state.farmAction)label=goodName(state.farmAction);
      else if(label==='Working')label=output?`Producing ${goodName(output).toLowerCase()}`:({woodcutter:'Cutting wood',fishing_hut:'Fishing'})[item.kind] || label;
      return {label:label.split(' · ')[0],progress:Math.max(0,Math.min(1,value/total)),paused:paused.has(v)};
    });
    if(activities.length){
      const working=activities.filter(activity=>!activity.paused),summary=working.length?working:activities;
      const label=working.length?[...new Set(working.map(activity=>activity.label))].sort().join(' · '):problems.shift() || 'Paused';
      line(label,summary.reduce((sum,activity)=>sum+activity.progress,0)/summary.length,1,`${buildingName(item)} average work progress`);
    }
  }
  target.append(section);
  if(problems.length){
    const explanation=document.createElement('div');explanation.className='building-problems';
    for(const problem of new Set(problems)){const text=document.createElement('p');text.textContent=problem;explanation.append(text);}
    target.append(explanation);
  }
}
function renderFishingBoat(town,item,index){
  const level=fishingBoatLevel(item);
  if(item.kind!=='fishing_hut'||!level){selection=null;panel=null;return;}
  const key=`${world.world_id}/${viewed}/boat/${index}`;
  if(buildingFolds.key!==key)buildingFolds={key,open:{}};
  $('selection-title').textContent=world.progression?.upgrades?.[['','boat','improved_boat','master_boat'][level]]?.label || 'Fishing boat';
  $('selection-art').append(illustration('fishing'));
  $('selection-level').hidden=false;$('selection-level').textContent=`Tier ${level}`;
  if(item.construction)renderBuildingStatus(town,item,index,$('selection-details'));
  const cargo=fishingBoatCargo(town,item),load=Object.values(cargo).reduce((sum,amount)=>sum+amount,0),capacity=fishingBoatCapacity(item);
  const status=buildingSection(item.construction?'Cargo':'Status');
  status.className='building-section building-status';
  const text=document.createElement('p');text.className='workplace-status';text.textContent=`Cargo · ${Number(load.toFixed(2))} / ${capacity} fish`;status.append(text);
  const progress=document.createElement('progress');progress.max=capacity;progress.value=load;progress.setAttribute('aria-label','Boat cargo');status.append(progress);
  if(load)status.append(productionFacts(Object.entries(cargo).map(([key,amount])=>[goodName(key),String(Number(amount.toFixed(2)))])));
  else{const empty=document.createElement('p');empty.textContent='No fish aboard.';status.append(empty);}
  $('selection-details').append(status);
  const details=buildingSection('Boat');details.append(productionFacts([['Crew',`${level} fisher${level===1?'':'s'}`],['Reach',`${resourceReach(item)} tiles`],['Maximum depth',String(level+1)]]));
  const back=document.createElement('button');back.textContent='Open fishing hut';back.onclick=()=>inspect('building',index);details.append(back);$('selection-details').append(details);
  const upgrades=buildingSection('Upgrades');
  for(const upgrade of ['improved_boat','master_boat']){
    const spec=world.progression?.upgrades?.[upgrade];if(!spec)continue;
    const unlocked=item.profession_upgrades?.includes(upgrade),button=document.createElement('button');button.className='upgrade-button';button.textContent='Upgrade';
    button.setAttribute('aria-label',`Upgrade to ${spec.label.toLowerCase()}`);button.disabled=unlocked||!!item.construction||viewed!==identity?.town_id;
    button.onclick=()=>{if(!button.disabled&&viewed===identity?.town_id)showUpgrade({action:'profession_upgrade',index,upgrade});};
    upgrades.append(upgradeCard(button,upgradePreview({action:'profession_upgrade',index,upgrade},town,item),{status:unlocked?'Unlocked':''}));
  }
  if(upgrades.children.length>1)$('selection-details').append(upgrades);
}
function renderSelection(town){
  const marketFocus=document.activeElement?.id?.startsWith('sale-')?document.activeElement:null;
  const barnFocus=document.activeElement?.id?.startsWith('barn-count-')?{id:document.activeElement.id,value:document.activeElement.value}:null;
  const workshopFocus=document.activeElement?.id==='workshop-quantity';
  $('selection-panel').classList.toggle('building-info',['building','boat'].includes(selection?.type));
  $('selection-art').replaceChildren();$('selection-level').hidden=true;
  // Park hall controls in the hidden attached container before rebuilding cards.
  $('hall-details').append($('upgrade'));
  $('selection-panel').append($('hall-details'));
  $('selection-details').replaceChildren();$('selection-fence').hidden=true;$('selection-decoration').hidden=true;$('hall-details').hidden=true;$('selection-move').hidden=true;$('selection-destroy').hidden=true;
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
    p.textContent=viewed===identity?.town_id?'Your adventuring villager · Position saved with this world.':'This is the town owner’s adventuring villager.';
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
      const row=document.createElement('p');row.textContent=`${gear.name} · ${Number(availableWorkerTools(town,key).toFixed(2))} available · Give to a worker from their panel (+${gear.bonus*100}% ${goodName(gear.job).toLowerCase()} output${key==='crafted_saw'?' and ingredients per batch':''})`;$('selection-details').append(row);
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
      if(!town.player?.boat){const boat=document.createElement('button');boat.textContent=town.level<3?'Exploration boat · Requires hall level 3':`Build exploration boat · ${town.sandbox?'Free':'20 wood + 10 planks'}`;boat.disabled=town.level<3||!canAfford(town,buildingCosts(town,{wood:20,planks:10}));boat.onclick=()=>act({action:'build_boat'});$('selection-details').append(boat);}
    }
    if(town.player?.activity){
      const state=document.createElement('p');state.textContent=`${goodName(town.player.activity.job)} · ${town.player.activity_status || 'Working'}`;$('selection-details').append(state);
    }
    if(viewed===identity?.town_id)for(const [action,label] of [['fish','Fish'],['chop','Cut wood'],['gather','Gather'],['mine','Clear stone'],['stop_activity','Stop working']]){
      const button=document.createElement('button');button.textContent=label;button.onclick=()=>playerAction(action);$('selection-details').append(button);
    }
    return;
  }
  const item=selection.type==='npc'?town.npcs?.[selection.index]:selection.type==='villager'?town.villagers[selection.index]:['building','boat'].includes(selection.type)?town.buildings[selection.index]:town.map_resources[selection.index];
  if(!item){selection=null;panel=null;return;}
  if(selection.type==='boat'){renderFishingBoat(town,item,selection.index);return;}
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
          note.textContent=workerTools[gift]?`Gift for a worker · +${gear.bonus*100}% ${goodName(gear.job).toLowerCase()} output`:`Clothing gift · ${gear.perk} · Equip from your character panel`;$('selection-details').append(note);
          const button=document.createElement('button');button.textContent=progress.gift_claimed?`${gear.name} gift collected`:`Accept ${gear.name.toLowerCase()} · ${workerTools[gift]?'Gift for a worker':'Clothing gift'}`;
          button.disabled=!nearby||!!progress.gift_claimed;button.onclick=()=>act({action:'claim_local_gift',npc_index:selection.index});$('selection-details').append(button);
        }
        for(const key of localToolShops[item.name] || []){
          const gear=workerTools[key],button=document.createElement('button');
          button.textContent=`Buy ${gear.name.toLowerCase()} · ${gear.price} coins · +${gear.bonus*100}% ${goodName(gear.job).toLowerCase()} output`;
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
  if(selection.type==='resource'){
    $('selection-details').append(illustration(item.kind));
    const p=document.createElement('p');p.className='info-copy';
    const swamp=town.terrain?.[item.y]?.[item.x]==='b';
    const grasslandRock=item.kind==='rock'&&['g','m'].includes((world.shared_map || town).terrain?.[item.y]?.[item.x]);
    if(item.kind==='village_hut'||item.kind==='village_market'||item.kind==='village_decor'){
      $('selection-title').textContent=`${item.village} · ${item.label || (item.kind==='village_market'?'Market':'Cottage')}`;
      p.textContent=`A ${item.biome==='b'?'swamp':item.biome==='d'?'desert':'ice'} village. Walk beside a resident and click them or press ${keybindings.interact.toUpperCase()} to talk and trade travel equipment.`;
      $('selection-details').append(p);return;
    }
    if(item.kind==='chest'){
      $('selection-title').textContent='Island treasure';p.textContent=`Loot: 25 coins + equipment\nCollect: stand nearby · ${keybindings.gather.toUpperCase()}\nOne claim / world`;$('selection-details').append(p);
      if(viewed===identity?.town_id){const button=document.createElement('button');button.textContent='Collect loot';button.onclick=()=>playerAction('gather');$('selection-details').append(button);}return;
    }
    $('selection-title').textContent=({berry_bush:'Berry bush',herb_patch:'Herb patch'}[item.kind]) || (swamp?{tree:'Swamp willow',water:'Swamp water',rock:'Rock outcrop'}:{tree:'Forest',water:'Water',rock:'Rock outcrop',berry_bush:'Berry bush',herb_patch:'Herb patch'})[item.kind];
    p.textContent={tree:'Renewable wood source. Place a woodcutter within 10 horizontal or vertical tile steps, then assign a villager to woodcutting.',water:'Uses: farming / fishing\nFishing hut reach: 3–5 tile steps (+3 / +5 / +7 with boat tiers)\nFarmers fetch water when watering',rock:grasslandRock?'Occupied tile':'Occupied tile\nMining: unavailable',berry_bush:`Food: berries\nGather: stand nearby · ${keybindings.gather.toUpperCase()}`,herb_patch:`Use: animal breeding\nGather: stand nearby · ${keybindings.gather.toUpperCase()}`}[item.kind];
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
      const landscape=world.shared_map || town,biome=landscape.terrain?.[item.y]?.[item.x];
      const regrows=(landscape.terrain_version || 1)<6 || biome==='f' || biome==='b';
      $('selection-title').textContent=swamp?'Swamp willow':item.species==='ancient'?'Ancient tree':regrows?'Forest':'Grassland tree';
      p.textContent=`Chop: ${keybindings.chop.toUpperCase()} / nearby woodcutter\nWoodcutter reach: 10 tile steps\nFelling: ${regrows?'leaves a stump, clears tile':'permanently clears this tree'}\nRegrowth: ${regrows?`${swamp||item.species==='ancient'?15:5} min · requires an empty tile`:'None on grassland'}`;
      if(swamp||item.species==='ancient')p.textContent+='\nChopping time: 5×';
    }
    if(item.kind==='wild_horse'){
      $('selection-title').textContent='Wild horse';p.textContent='Lead this horse to a finished stable (hall 8). Feed it for five minutes to settle it. Two adults can breed; a saddle lets you ride one.';
      if(viewed===identity?.town_id){const button=document.createElement('button');button.textContent='Lead horse to stable';button.disabled=town.level<8||!!town.player.leading_horse||Math.abs(town.player.x-item.x)+Math.abs(town.player.y-item.y)>1.8||!town.buildings.some(b=>b.kind==='stable'&&!b.construction);button.onclick=()=>act({action:'lead_horse',x:item.x,y:item.y});$('selection-details').append(button);}
    }
    if(item.kind==='rock'&&item.ore){$('selection-title').textContent='Iron vein';p.textContent='A mine within 10 tile steps extracts iron ore. Smelt ore with wood into iron, then make steel, saddles, and trade crates.';}
    if(grasslandRock){
      p.textContent+=`\nClear: stand nearby · ${keybindings.mine.toUpperCase()}\nPermanently removes this rock and collects ${world.progression?.stone_per_rock ?? 10} stone.\nRequires storage room for the full yield.`;
    }
    if(item.supply_max){$('selection-details').append(fact(`${Math.floor(item.supply || 0)} / ${item.supply_max}`,item.kind==='tree'?'Wood remaining before felling':'Shared supply'));if(item.kind!=='tree')$('selection-details').append(fact(`${item.regeneration} per 3 seconds`,'Regeneration'));}
    $('selection-details').append(p);
  }else{
    $('selection-title').textContent=buildingName(item);
    if(item.kind==='orchard_tree'){
      const orchard=town.buildings[item.orchard_index];
      $('selection-art').append(illustration('orchard_tree'));
      renderBuildingStatus(town,item,selection.index,$('selection-details'));
      $('selection-details').append(fact(orchard?`${buildingName(orchard)} · ${orchard.x}, ${orchard.y}`:'No linked orchard','Owning orchard'),
        fact(`${item.tree?.fruit ?? world.progression?.orchard_harvest_yield ?? 8} fruit`,'Harvest · Workers pick automatically when ripe'));
      $('selection-level').hidden=true;
      $('selection-move').hidden=viewed!==identity?.town_id;
      $('selection-move').textContent=`Move · ${!town.sandbox&&town.coins?'1 coin':'Free'}`;
      $('selection-move').onclick=()=>{const index=selection.index;openPanel('build');tool='move';moveIndex=index;render();};
      $('selection-destroy').hidden=viewed!==identity?.town_id;
      $('selection-destroy').textContent='Remove tree · No refund';
      return;
    }
    if(item.kind==='acre'){
      const field=item.field || {stage:'unplowed',elapsed:0},farm=town.buildings[item.farm_index];
      $('selection-art').append(illustration('acre'));
      renderBuildingStatus(town,item,selection.index,$('selection-details'));
      $('selection-details').append(fact(goodName(field.crop || item.crop || 'vegetables'),'Current crop'),
        fact(farm?`${buildingName(farm)} · ${farm.x}, ${farm.y}`:'No linked farm','Owning farm'));
      $('selection-level').hidden=true;$('selection-move').hidden=true;
      $('selection-destroy').hidden=viewed!==identity?.town_id;
      $('selection-destroy').textContent='Destroy · No refund';
      return;
    }
    const foldKey=`${world.world_id}/${viewed}/${selection.index}/${item.kind}/${item.x},${item.y}`;
    if(buildingFolds.key!==foldKey)buildingFolds={key:foldKey,open:{}};
    $('selection-art').append(illustration(item.kind));
    if(!['barn','workshop'].includes(item.kind))renderBuildingStatus(town,item,selection.index,$('selection-details'));
    $('selection-level').hidden=false;$('selection-level').textContent=`Lv ${item.kind==='hall'?town.level:buildingLevel(item)}`;
    const decoration=decorationKinds.includes(item.kind);
    const workers=buildingSection(decoration?'Decoration':item.kind==='well'?'Water':['hall','home'].includes(item.kind)?'Settlers':'Workers');
    const production=decoration?document.createElement('div'):buildingSection(item.kind==='barn'?'Animals':item.kind==='butcher'?'Butchery':item.kind==='workshop'?'Requests':item.kind==='market'?'Trade':['hall','home','storehouse'].includes(item.kind)?'Benefits':'Production',!['barn','butcher','workshop'].includes(item.kind));
    const upgrades=decoration?document.createElement('div'):buildingSection('Upgrades'),details=buildingSection('Building details',true),storageOverview=decoration?document.createElement('div'):buildingSection('Storage');
    const boats=item.kind==='fishing_hut'?buildingSection('Fishing boats'):null;
    const unlockedUpgrades=decoration?document.createElement('div'):buildingSection('Unlocked upgrades',true);
    const upgradeActions=document.createElement('div');upgradeActions.className='building-upgrade-actions';
    const assigned=town.villagers.filter((v,i)=>settlerBuilding(town,v,i)===selection.index);
    const status=document.createElement('p');status.className='workplace-status';
    const sourceWork=['woodcutter','farm','fishing_hut'].includes(item.kind);
    status.textContent=decoration?'Village decoration · No production bonus':item.construction?'Under construction':town.upgrade_remaining&&item.kind==='hall'?`Upgrading · ${town.upgrade_remaining}s remaining`:
      sourceWork&&!buildingUsable(town,item)?'Needs a source in reach':
      ['hall','home'].includes(item.kind)?'':item.kind==='well'?'Water available':item.kind==='acre'?'Field ready':
      !assigned.length?'Needs workers':[...new Set(assigned.map(v=>productionStatus(v,item)).filter(Boolean))].join(' · ') || `${assigned.length} assigned worker${assigned.length===1?'':'s'}.`;
    if(!item.construction&&!decoration&&item.kind!=='well')renderSettlers(town,item,selection.index,workers,upgradeActions,details);
    if(item.kind==='well'){
      const note=document.createElement('p');note.textContent='Automatic crop water · No workers needed. Keep an adjacent tile accessible.';details.append(note);
    }
    if(status.textContent&&item.kind==='barn')workers.append(status);
    if(item.kind==='market'&&!item.construction&&viewed===identity?.town_id)renderQuickMarketSale(town,production);

    if(item.kind==='hall'){
      production.append(fact(`${buildingRange(town)} tile steps`,'Building range'));
    }
    if(item.kind==='hall'){
      const upgrade=world.hall_upgrades?.find(next=>next.level===town.level+1);
      $('hall-progress').hidden=true;
      const hallCosts=upgrade?Object.fromEntries(['wood','planks','leather','iron_ore','iron','steel'].filter(k=>k in upgrade).map(k=>[k,upgrade[k]])):{};
      const ready=world.hall_ready?.[viewed] || false;
      $('progress').textContent=upgrade?`Level ${town.level} · ${upgrade.milestone} · ${ready?'Complete':'Still needed'} · Reward: ${upgrade.coins} coins`:`Level ${town.level} · Town hall fully upgraded.`;
      $('upgrade').textContent='Upgrade';
      $('upgrade').className='upgrade-button';
      $('upgrade').title=town.upgrade_remaining?`Upgrading to level ${town.level+1} · ${town.upgrade_remaining}s remaining`:upgrade?`Upgrade to level ${upgrade.level} · ${costText(hallCosts)} · ${upgrade.seconds} seconds`:'Maximum hall level reached';
      $('upgrade').setAttribute('aria-label',$('upgrade').title);
      $('upgrade').disabled=!!town.upgrade_remaining || !!item.construction || viewed!==identity?.town_id || !upgrade;
    }
    $('selection-move').hidden=viewed!==identity?.town_id;
    $('selection-move').textContent=`${item.kind==='farm'?'Move farm & acres':item.kind==='orchard'?'Move orchard & trees':'Move'} · ${!town.sandbox&&town.coins?'1 coin':'Free'}`;
    $('selection-move').onclick=()=>{const index=selection.index;openPanel('build');tool='move';moveIndex=index;render();};
    if(decoration&&!item.construction&&viewed===identity?.town_id){
      const control=$('selection-decoration-orientation'),index=selection.index;
      $('selection-decoration').hidden=false;control.replaceChildren();
      for(const [orientation,label] of ['Lower left ↙','Upper left ↖','Upper right ↗','Lower right ↘'].entries()){
        const option=document.createElement('option');option.value=String(orientation);option.textContent=label;control.append(option);
      }
      control.value=String(decorationOrientation(item));
      control.onchange=()=>act({action:'set_decoration_orientation',index,orientation:Number(control.value)});
    }
    if(barnActive(item)&&viewed===identity?.town_id){
      const pen=barnPen(item,mapSize(town)),control=$('selection-fence-orientation');
      $('selection-fence').hidden=false;control.replaceChildren();
      for(const [orientation,label] of ['Lower right ↘','Lower left ↙','Upper left ↖','Upper right ↗'].entries()){
        const option=document.createElement('option');option.value=String(orientation);option.textContent=label;option.disabled=!pen.available[orientation];control.append(option);
      }
      control.value=String(pen.orientation);
      const index=selection.index;
      control.onchange=()=>act({action:'set_fence_orientation',index,orientation:Number(control.value)});
    }
    $('selection-destroy').hidden=viewed!==identity?.town_id || item.kind==='hall';
    $('selection-destroy').textContent=item.kind==='farm'?'Destroy farm & acres · No refund':item.kind==='orchard'?'Destroy orchard & trees · No refund':'Destroy · No refund';
    if(item.kind==='orchard'){
      const note=document.createElement('p');note.textContent='Moving this orchard moves all linked trees with it. Destroying it also removes those trees.';details.append(note);
    }
    if(item.kind==='farm'){
      const note=document.createElement('p');note.textContent='Moving this farm moves all linked acres with it. Destroying it also removes those acres.';details.append(note);
    }
    if(item.construction&&item.kind==='barn'){
      const c=item.construction,stage=c.progress===0?'Waiting for a builder':c.upgrade?'Upgrading':c.progress<c.total/3?'Foundations':c.progress<c.total*2/3?'Framing':'Finishing';
      status.textContent=`${stage} · ${Math.floor(c.progress/c.total*100)}% complete · ${Math.ceil(c.total-c.progress)}s remaining of work`;
      const progress=document.createElement('progress');progress.max=c.total;progress.value=c.progress;progress.setAttribute('aria-label',`${labels[item.kind]} construction`);workers.append(progress);
    }
    renderProfession(town,item,selection.index,production,upgrades,details,boats,unlockedUpgrades);
    if(!item.construction&&!['home','market','well',...decorationKinds].includes(item.kind)){
      renderStorage(town,item,selection.index,storageOverview);
    }
    if(costs[item.kind]&&item.kind!=='well'){
      const level=buildingLevel(item),preview=upgradePreview({action:'upgrade_building',index:selection.index},town,item),maxLevel=item.kind==='storehouse'?storehouseCapacities.length:item.kind==='home'?4:3;
      if(!item.construction&&!['barn','butcher'].includes(item.kind)&&!storageUpgradeKinds().includes(item.kind)){
        const benefits=document.createElement('p');benefits.className='info-copy';
        benefits.textContent=upgradeBenefit(item.kind,level,item);production.append(benefits);
      }
      if(item.kind==='farm'){
        const note=document.createElement('p');note.textContent='Farm upgrades apply to every linked acre.';details.append(note);
      }
      const button=document.createElement('button');
      button.className='upgrade-button';
      button.textContent='Upgrade';
      button.title=level>=maxLevel?`Fully upgraded · Level ${maxLevel}${item.kind==='storehouse'?' · Warehouse':''}`:`Upgrade to level ${level+1} · ${town.sandbox?'Free · Instant':`${costText(preview.cost)} · ${30*level} seconds`} · ${upgradeBenefit(item.kind,level+1,item)}`;
      button.setAttribute('aria-label',button.title);
      button.disabled=!!item.construction||level>=maxLevel||viewed!==identity?.town_id;
      const index=selection.index;
      button.onclick=()=>{if(!button.disabled&&viewed===identity?.town_id)showUpgrade({action:'upgrade_building',index});};
      // Keep the level action before the capacity action.
      const capacityUpgrade=upgradeActions.children[0];
      upgradeActions.replaceChildren();upgradeActions.append(upgradeCard(button,upgradePreview({action:'upgrade_building',index},town,item),{title:level>=maxLevel?`Level ${level}`:`Level ${level} → ${level+1}`,status:level>=maxLevel?'Maxed':'',benefits:level>=maxLevel?[]:undefined}));
      if(capacityUpgrade)upgradeActions.append(capacityUpgrade);
    }
    if(item.kind==='farm'&&!item.construction&&viewed===identity?.town_id){
      const button=document.createElement('button');button.textContent=`Build acre · ${town.sandbox?'Free':'5 wood'}`;button.disabled=!town.sandbox&&town.wood<costs.acre;
      button.onclick=()=>{acreFarmIndex=selection.index;openPanel('build');tool='acre';moveIndex=null;buildTile=null;hoverTile=null;render();};
      production.append(button);
    }
    if(item.kind==='orchard'&&!item.construction){
      const trees=town.buildings.map((b,i)=>({b,i})).filter(({b})=>b.kind==='orchard_tree'&&b.orchard_index===selection.index);
      production.append(fact(`${trees.length} trees · ${trees.filter(({b})=>b.tree?.stage==='ripe').length} ripe`,'Orchard'),fact(`${world.progression?.orchard_harvest_yield ?? 8} fruit per tree`,'Recurring harvest · No watering or replanting'));
      for(const {b,i} of trees){
        const button=document.createElement('button');button.textContent=`Tree · ${b.x}, ${b.y} · ${orchardTreeStatus(b)}`;button.onclick=()=>inspect('building',i);production.append(button);
      }
      if(viewed===identity?.town_id){
        const button=document.createElement('button');button.textContent=`Plant fruit tree · ${town.sandbox?'Free':'5 wood'}`;button.disabled=!town.sandbox&&town.wood<costs.orchard_tree;
        button.onclick=()=>{orchardIndex=selection.index;openPanel('build');tool='orchard_tree';moveIndex=null;buildTile=null;hoverTile=null;render();};production.append(button);
      }
    }
    if(['barn','butcher','workshop'].includes(item.kind)&&production.children.length>1)$('selection-details').append(production);
    if(item.kind==='workshop')renderBuildingStatus(town,item,selection.index,$('selection-details'));
    if(!item.construction&&!['home','market','well',...decorationKinds].includes(item.kind))$('selection-details').append(storageOverview);
    $('selection-details').append(workers);
    if(!['barn','butcher','workshop'].includes(item.kind)&&production.children.length>1)workers.append(production);
    if(item.kind!=='barn'&&boats?.children.length>1)upgrades.append(boats);
    if(item.kind==='hall'){
      const preview=upgradePreview({action:'upgrade'},town,item);
      upgradeActions.append(upgradeCard($('upgrade'),preview,{title:preview?`Level ${town.level} → ${town.level+1}`:`Level ${town.level}`,status:town.upgrade_remaining?`Upgrading · ${town.upgrade_remaining}s`:preview?'':'Maxed',benefits:preview?undefined:[]}));
    }
    if(upgradeActions.children.length)upgrades.append(upgradeActions);
    if(upgrades.children.length>1)$('selection-details').append(upgrades);
    if(unlockedUpgrades.children.length>1)$('selection-details').append(unlockedUpgrades);
  }
  if(marketFocus&&$('selection-details').contains?.(marketFocus))marketFocus.focus();
  if(barnFocus){const input=$(barnFocus.id);if(input&&$('selection-details').contains?.(input)){input.value=barnFocus.value;input.focus();}}
  if(workshopFocus&&$('selection-details').contains?.($('workshop-quantity')))$('workshop-quantity').focus();
}
const buildingCategories=[
  ['Housing & storage',['home','storehouse']],
  ['Food & farming',['farm','well','fishing_hut','barn','fish_pond','orchard','mill','smokehouse','bakery','butcher']],
  ['Crafting',['woodcutter','sawpit','workshop','tannery','saddler']],
  ['Decorations',decorationKinds],
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
  if(building || !(tool in costs) || viewed!==identity?.town_id || !town || !town.sandbox&&town.wood<costs[tool])return;
  const selected={x,y},kind=tool;
  if(!insideBuildingRange(town,selected)){notice(`Build within ${buildingRange(town)} tile steps of your town hall.`);return;}
  if(kind==='orchard_tree'&&!orchardPlacementValid(town,selected,orchardIndex)){notice('Plant fruit trees within 3 tiles of their orchard in each direction.');return;}
  if(kind==='acre'&&!acrePlacementValid(town,selected,acreFarmIndex)){notice('Place acres within 3 tiles of their farm in each direction.');return;}
  building=true;render();
  try{
    await request('/api/command',{action:kind==='road'?'build_road':'build',kind,...selected,...(kind==='acre'?{farm_index:acreFarmIndex}:{}),...(kind==='orchard_tree'?{orchard_index:orchardIndex}:{}),world_id:world.world_id});
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
  const role=departed.job==='idle'?'settler':departed.job==='baking'?'baker':departed.job==='butchery'?'butcher':departed.job==='building'?'builder':departed.job==='woodworking'?'sawyer':departed.job;
  const practice=Object.entries(departed.practice || {}).map(([job,xp])=>`${Math.floor(xp)} ${job==='woodworking'?'sawing':job} practice`).join(', ');
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
    const previousWorldId=world.world_id;
    const next=await request('/api/world'+query),receivedAt=performance.now();
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
        if(!mapOverview())scene.resourceData=yield* prepareResourceData(next.shared_map?.map_resources || next.towns[viewed || identities[next.world_id]?.town_id || Object.keys(next.towns)[0]]?.map_resources || [],tileW>=32);
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
    world=next;worldReceivedAt=receivedAt;frameActors=null;
    const savedIdentity=identities[world.world_id];
    identity=savedIdentity?.token && world.towns[savedIdentity.town_id]?savedIdentity:null;
    // Keep characters moving even if the lightweight endpoint is unavailable.
    updateRemotePlayers({world_id:world.world_id,sampled_at:world.players_sampled_at,
      players:Object.fromEntries(Object.entries(world.towns).filter(([,town])=>town.player).map(([id,town])=>[id,town.player]))});
    catalog=await request('/api/worlds');
    if(!world.towns[viewed]){viewed=identity?.town_id || Object.keys(world.towns)[0];selection=null;panel=null;tool=null;moveIndex=null;buildTile=null;hoverTile=null;keyboardTile=null;}
    // Keep dropdowns and a slider being dragged intact while snapshots advance.
    const barnSliderDragging=renderBarnAnimals.dragging&&document.activeElement?.id?.startsWith('barn-count-');
    if((document.activeElement?.tagName!=='SELECT'&&!barnSliderDragging)||previousWorldId!==world.world_id){render();renderWorldMenu();}
    else updateBarnTimers();
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
  for(const name of ['selection','villagers','valley','help','items'])$(name+'-panel').hidden=panel!==name;
  if(panel==='resources'&&own){
    renderResourceList(own);
    if(!$('resources-panel').open)$('resources-panel').showModal();
  }else if($('resources-panel').open)$('resources-panel').close();
  $('side-panel').hidden=(!panel || panel==='resources') && $('join').hidden;
  $('side-panel').classList.toggle('villagers-overview',panel==='villagers');
  $('side-panel').classList.toggle('help-guide',panel==='help');
  $('game').classList.toggle('panel-open',!$('side-panel').hidden);
  for(const name of ['build','villagers','valley','help','resources','items']){
    $(name+'-button').setAttribute('aria-expanded',String(panel===name));
    $(name+'-button').classList.toggle('selected',panel===name);
  }
  $('build-button').disabled=!own || (!world.shared_map && viewed!==identity?.town_id);
  $('villagers-button').disabled=!town;
  $('resources-button').disabled=!own;
  $('items-button').disabled=!own;
  if(panel==='items')renderItems(own);
  $('move').textContent=!own?.sandbox&&own?.coins?'Move a building · 1 coin':'Move a building · Free';
  $('mode-hint').textContent=tool==='move'?(moveIndex===null?'Moving: click a building. Escape to cancel.':'Moving: click open grass to move here. Escape to cancel.'):tool?`Building ${labels[tool].toLowerCase()}: hover over open grass and left-click to build. Escape to cancel.`:'';
  if(tool&&tool!=='move'){
    if(building)$('mode-hint').textContent='Building…';
    else if(!own?.sandbox&&own?.wood<costs[tool])$('mode-hint').textContent='Not enough wood.';
  }
  $('view-title').textContent=((world.shared_map?world.name:town?.name) || 'A place to call home')+(world.sandbox?' · Sandbox':'');
  if(!tool&&own?.food_shortage_seconds)$('mode-hint').textContent=own.sandbox?'Food shortage · Sandbox villagers stay while you test your supply chain.':'Food shortage! Feed your settlers soon or they may leave. Open Villagers for details.';
  if(!tool&&own?.housing_shortage_seconds)$('mode-hint').textContent=own.sandbox?'Housing shortage · Build or upgrade cottages. Sandbox villagers stay.':'Housing shortage! Build or upgrade cottages soon or settlers may leave. Open Villagers for details.';
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
  if(buildTile && (!town || !insideBuildingRange(town,buildTile) || town.buildings.some(b=>b.x===buildTile.x&&b.y===buildTile.y) || town.map_resources.some(r=>!['berry_bush','herb_patch'].includes(r.kind)&&r.x===buildTile.x&&r.y===buildTile.y)))buildTile=null;
  if(typeof ClientScene==='undefined'||panel==='build')for(const group of $('buildings').children){
    for(const button of group.children){
      if(!button.buildingKind)continue;
      const kind=button.buildingKind,spec=world.progression?.buildings[kind] || {wood:costs[kind],level:1};
      button.hidden=!own || own.level<spec.level;
      button.classList.toggle('selected',kind===tool);button.disabled=!own || own.level<spec.level || !canAfford(own,buildingCosts(own,spec));
      button.textContent=`${labels[kind]} · ${own?.sandbox?'Free · Instant':kind==='road'?'Free · 10s per tile · +50% walking':costText(buildingCosts(own,spec))}${own&&own.level<spec.level?` · Hall ${spec.level}`:''}`;button.append(illustration(kind));
      const recipe=decorationKinds.includes(kind)&&world.progression?.profession_recipes?.workshop?.find(recipe=>spec[recipe.output]);
      button.title=recipe?`Craft ${goodName(recipe.output).toLowerCase()} in the Workshop, wait for delivery, then place here.`:'';
    }
    group.hidden=![...group.children].some(button=>button.buildingKind&&!button.hidden);
  }
  if(own){$('town-name').textContent=own.name;$('hint').hidden=!!tool;$('hint').textContent='Choose a building, then hover over an open tile.';}
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
    const homes=town.buildings.filter(b=>b.kind==='home'&&!b.construction);
    const shelter=homes.length?`Shared cottages · ${[...new Set(homes.map(buildingLevel))].sort((a,b)=>a-b).map(level=>`level ${level}`).join(', ')}`:'Town hall · shared shelter';
    const housing=document.createElement('p');housing.textContent=`Housing · ${town.villagers.length} residents / ${spaces} spaces · ${shelter}. ${town.sandbox?'New housing spaces populate instantly without food. Villagers can work in every profession and never leave.':'New settlers need a free space and at least 20 food after meals; each arrival uses 10 food.'}`;$('villagers').append(housing);
  }
  if(town?.food_shortage_seconds){
    const warning=document.createElement('p');warning.textContent=`Food shortage · ${town.food_shortage_seconds}s without enough food. ${town.sandbox?'Sandbox villagers stay while you test food production.':'After 60 seconds, settlers may leave with all their skills. Upgrade cottages to reduce the risk.'}`;$('villagers').append(warning);
  }
  if(town?.housing_shortage_seconds){
    const warning=document.createElement('p');warning.textContent=`Housing shortage · ${town.housing_shortage_seconds}s without enough housing. ${town.sandbox?'Sandbox villagers stay. Build or upgrade cottages.':'After 60 seconds, settlers may leave with all their skills. Build or upgrade cottages.'}`;$('villagers').append(warning);
  }
  if(town?.last_departure){const loss=document.createElement('p');loss.textContent=departureMessage(town);$('villagers').append(loss);}
  }
  draw();
}
function point(x,y){return [originX+(x-y)*tileW/2,originY+(x+y)*tileH/2];}
function polygon(points,fill,stroke,target=ctx){target.beginPath();points.forEach(([x,y],i)=>i?target.lineTo(x,y):target.moveTo(x,y));target.closePath();target.fillStyle=fill;target.fill();if(stroke){target.strokeStyle=stroke;target.stroke();}}
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
  if(r.kind==='village_decor'){
    const line=(x1,y1,x2,y2,color,width=1)=>{ctx.strokeStyle=color;ctx.lineWidth=width;ctx.beginPath();ctx.moveTo(x1,y1);ctx.lineTo(x2,y2);ctx.stroke();};
    if(r.variant==='bench'){
      for(const x of [-12,9]){line(x,2,x,13,'#514c38',3);line(x,-8,x,4,'#65543b',2);}
      polygon([[-15,1],[8,-7],[17,-2],[-6,7]],'#b79b69','#69583e');
      polygon([[-15,-7],[8,-15],[8,-9],[-15,-1]],'#8c7954','#69583e');
      line(-12,1,10,-7,'#d0b782');
    }else if(r.variant==='candle_lantern'){
      line(-3,12,-3,-34,'#5e543e',3);line(-3,-34,10,-30,'#5e543e',2);line(10,-30,10,-24,'#373f36',1.5);
      ctx.fillStyle='#f8cb6333';ctx.beginPath();ctx.ellipse(10,-16,12,15,0,0,Math.PI*2);ctx.fill();
      polygon([[5,-23],[15,-23],[16,-10],[4,-10]],'#dcb369','#514b37');
      ctx.fillStyle='#fff0aa';ctx.fillRect(9,-19,3,8);ctx.fillStyle='#fff8d4';ctx.fillRect(10,-22,1,4);
      polygon([[3,-24],[10,-28],[17,-24]],'#465548');line(5,-10,15,-10,'#465548',2);
    }else if(r.variant==='barrels'){
      for(const [x,y] of [[-7,5],[6,0]]){
        polygon([[x-6,y-12],[x+5,y-12],[x+7,y-2],[x+5,y+6],[x-5,y+6],[x-7,y-2]],'#94714d','#584c38');
        ctx.fillStyle='#b99b68';ctx.beginPath();ctx.ellipse(x,y-12,6,3,0,0,Math.PI*2);ctx.fill();
        for(const dy of [-8,2])line(x-6,y+dy,x+6,y+dy,'#505d55',2);
      }
    }else if(r.variant==='mushrooms'){
      for(const [x,y] of [[-9,4],[0,-1],[9,7]]){
        line(x,y,x,y-7,'#d5c79b',2);ctx.fillStyle=x===0?'#b28465':'#c3a975';ctx.beginPath();ctx.ellipse(x,y-7,5,3,0,Math.PI,Math.PI*2);ctx.fill();
        ctx.fillStyle='#f4d9ac';ctx.fillRect(x-2,y-9,1.5,1.5);
      }
    }else{
      const fern=r.variant==='ferns';
      for(let i=0;i<7;i++){
        const x=(i-3)*3,y=5+Math.abs(i-3),h=fern?10+((i*7)%9):19+((i*7)%15);
        line(x,y,x+(i-3)*1.5,y-h,fern?'#577957':'#7b925c',1.5);
        if(fern)for(let j=1;j<4;j++){
          line(x,y-j*3,x-5,y-j*3-3,'#82a36a',1.5);line(x,y-j*3,x+5,y-j*3-2,'#6c905a',1.5);
        }else{
          line(x,y-2,x-5,y-13,'#91a36b');
          if(r.variant==='cattails')line(x+(i-3)*1.5,y-h,x+(i-3)*1.5,y-h-5,'#89654a',3);
        }
      }
    }
    ctx.restore();return;
  }
  if(r.biome==='b'&&(r.kind==='village_hut'||r.kind==='village_market')){
    const tavern=r.variant==='tavern',market=r.kind==='village_market',hall=r.variant==='hall';
    const reed=r.variant==='reed_house',moss=r.variant==='moss_house';
    const h=tavern?43:hall?39:30,peak=tavern?65:hall?61:47;
    // Raised board decks, weathered plank walls and thick, uneven reed roofs.
    for(const x of [-18,-5,13]){ctx.fillStyle='#514e3b';ctx.fillRect(x,3,3,14);}
    polygon([[-26,4],[0,-9],[26,4],[0,18]],'#9b885e','#554e3b');
    for(let i=-18;i<20;i+=6)polygon([[i,8-Math.abs(i)*.5],[i+2,9-Math.abs(i+2)*.5],[i+14,2-Math.abs(i+2)*.5],[i+12,1-Math.abs(i)*.5]],'#b3a075');
    if(!market){
      polygon([[-19,1],[0,11],[0,11-h],[-19,1-h]],reed?'#b19a69':'#a58e69','#635a42');
      polygon([[0,11],[19,1],[19,1-h],[0,11-h]],'#706b50','#515240');
      if(detailed){
        for(let y=7-h;y<8;y+=5){
          polygon([[-18,y-9],[-1,y],[-1,y+1],[-18,y-8]],'#857450');
          polygon([[1,y],[18,y-9],[18,y-8],[1,y+1]],'#596047');
        }
        for(const x of [-18,-2]){ctx.fillStyle='#5c543d';ctx.fillRect(x,2-h,2,h-1);}
      }
      polygon([[-13,5],[-6,8],[-6,-14],[-13,-17]],'#414939','#bea477');
      polygon([[6,-7],[13,-11],[13,-19],[6,-15]],'#e9bc69','#4f513b');
      ctx.fillStyle='#fff0ae';ctx.fillRect(9,-16,1,7);
    }else{
      ctx.fillStyle='#685c42';ctx.fillRect(-19,-23,3,29);ctx.fillRect(16,-23,3,29);
    }
    const roof=moss?'#71825d':reed?'#ad9c65':tavern?'#667c70':'#8c9160';
    polygon([[-25,1-h],[0,-peak],[3,14-h],[-22,4-h]],roof,'#4f5a42');
    polygon([[0,-peak],[25,1-h],[22,6-h],[3,14-h]],moss?'#52684c':tavern?'#495f58':'#68744e','#4f5a42');
    if(detailed)for(let i=0;i<7;i++){
      ctx.strokeStyle='#c2b47b';ctx.lineWidth=.7;ctx.beginPath();ctx.moveTo(-20+i*3,1-h-i*.55);ctx.lineTo(-2+i*.7,-peak+6+i*3);ctx.stroke();
      ctx.fillStyle='#839461';ctx.fillRect(7+i*2,4-h-i,3,3+(i%3));
    }
    if(tavern||r.variant==='inn'){
      ctx.fillStyle='#838878';ctx.fillRect(10,-peak+7,5,17);ctx.fillStyle='#c6c2a0';ctx.fillRect(9,-peak+6,7,3);
      polygon([[-22,-18],[-3,-9],[4,-16],[-16,-25]],'#a9905f','#514e3b');
      ctx.fillStyle='#f3ce78';ctx.fillRect(-21,-30,4,6);ctx.fillRect(-9,-24,4,6);
      ctx.fillStyle='#65573d';ctx.fillRect(20,-28,2,31);ctx.fillRect(17,-28,11,2);
      polygon([[17,-25],[28,-25],[28,-11],[17,-11]],'#42584d','#c3a774');
      ctx.fillStyle='#e6c581';ctx.fillRect(20,-22,5,7);ctx.strokeStyle='#e6c581';ctx.strokeRect(25,-21,2,4);
      ctx.fillStyle='#d8b77d';ctx.fillRect(-22,9,11,3);
    }else if(hall){
      ctx.fillStyle='#b59d6c';ctx.fillRect(-1,-73,2,15);polygon([[1,-73],[13,-68],[1,-63]],'#d3b96f');
    }else if(r.variant==='workshop'||r.variant==='net_house'){
      ctx.strokeStyle='#b4b28a';ctx.lineWidth=.7;
      for(let i=0;i<5;i++){ctx.beginPath();ctx.moveTo(18+i,-26+i);ctx.lineTo(11+i,0+i);ctx.moveTo(14,-18+i*4);ctx.lineTo(24,-14+i*4);ctx.stroke();}
      ctx.fillStyle='#806345';ctx.fillRect(-24,5,12,5);
    }else if(r.variant==='barn'){
      polygon([[-14,-19],[-3,-14],[-3,7],[-14,2]],'#60543d','#bca276');
      polygon([[-14,-19],[-3,7],[-3,4],[-14,-16]],'#bca276');
    }
    if(market){
      polygon([[-23,-10],[0,-1],[23,-10],[0,-21]],'#b4a374','#59694d');
      polygon([[-18,4],[0,12],[18,4],[0,-4]],'#b99c69','#5b543b');
      for(const [x,color] of [[-12,'#cd9966'],[-4,'#899757'],[6,'#cfba73']]){ctx.fillStyle=color;ctx.fillRect(x,-1,5,4);}
    }
    // A warm candle in an iron cage hangs from every inhabited deck.
    ctx.fillStyle='#435548';ctx.fillRect(-23,-19,6,10);ctx.fillStyle='#f6d18b';ctx.fillRect(-22,-17,4,6);ctx.fillStyle='#fff0b4';ctx.fillRect(-20,-16,1,4);
    ctx.restore();return;
  }
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
  const tiles=items=>items.map(({kind,x,y,footprint,farm_index,construction,fence_orientation})=>[kind,x,y,footprint,farm_index,!!construction,fence_orientation]);
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
    const signature=prepared?.signature ?? JSON.stringify([task,trip?[trip.id,trip.operation,trip.workplace,trip.storage,trip.path]:villager.logistics_target,!!villager.route_waiting]);
    const workerKey=world.shared_map?`${townId}/${index}`:index;
    const hostWork=villager.work_animation;
    const workPlayback=hostWork?.playback?.length&&!villager.logistics_target&&!villager.construction_target
      &&(!villager.route_waiting||hostWork.route_pending);
    let state=workers.get(workerKey);
    let finishingDelivery=state?.transport&&state.path.length&&state.task===task;
    if(state&&!state.pendingPlan&&!state.pendingAdvance&&state.layout!==layout
      &&(state.signature===signature||finishingDelivery)){
      const routing=region?.routing || (world.shared_map?{...town,roads:mapRoads(town),
        obstacles:Object.values(world.towns).flatMap(t=>t.buildings)}:town);
      if(WorkerAnimation.refreshNavigation(state,routing,villager))state.layout=layout;
      else state.needsPlan=true;
    }
    finishingDelivery=finishingDelivery&&!state?.needsPlan;
    if(!state || state.layout!==layout || state.needsPlan || (state.signature!==signature&&!finishingDelivery)){
      if(region){
        if(state?.pendingPlan?.signature!==signature||state?.pendingPlan?.layout!==layout){
          const version={signature,layout};
          ClientScene.queue.cancel('advance/'+workerKey);
          const previous=state;
          const home=town.buildings.find(b=>b.kind==='hall') || {x:5,y:5};
          const position=(villager.construction_target?villager.construction_position:
            villager.logistics_target?villager.logistics_position:villager.work_animation)
            || villager.logistics_position || villager.construction_position || {x:home.x-2+index%5,y:home.y+3+Math.floor(index/5)};
          state={...(state || {x:position.x,y:position.y,path:[],clock:0,phase:'waiting',facing:1}),path:[],pendingPlan:version,pendingAdvance:false};
          workers.set(workerKey,state);
          ClientScene.queue.schedule('plan/'+workerKey,version,WorkerAnimation.planSteps(region.routing,villager,index,previous),planned=>{
            if(workers.get(workerKey)!==state)return;
            const visible=workPlayback&&state.playbackQueue?{x:state.x,y:state.y,path:state.path,phase:state.phase,elapsed:state.elapsed,clock:state.clock}:{};
            Object.assign(state,planned,visible,{signature,layout,task,pendingPlan:null,needsPlan:false,transport:!!planned.transport});
          });
        }
      }else{
        const visible=workPlayback&&state?.playbackQueue?{x:state.x,y:state.y,path:state.path,phase:state.phase,elapsed:state.elapsed,clock:state.clock,
          playbackQueue:state.playbackQueue,hostVersion:state.hostVersion,hostTask:state.hostTask,
          farmAction:state.farmAction,farmField:state.farmField}:{};
        state=Object.assign(WorkerAnimation.plan(world.shared_map?{...town,roads:mapRoads(town),obstacles:Object.values(world.towns).flatMap(t=>t.buildings),home_hall:town.buildings.find(b=>b.kind==='hall')}:town,villager,index,state),visible,{signature,layout,task,needsPlan:false});workers.set(workerKey,state);
      }
    }
    state.roads=roads;
    if(state.transport&&trip?.cargo&&state.signature===signature){
      state.transportCargo=trip.cargo;
      state.transportHolding=trip.picked_up&&!trip.delivered&&(trip.step || 0)>=trip.delivery_step;
    }
    state.routeWaiting=!!villager.route_waiting;
    state.storagePaused=!!villager.production_status?.startsWith('Waiting for storage space');
    const confirmedPlayback=workPlayback&&!state.transport;
    if(!confirmedPlayback&&state.playbackQueue){
      state.playbackQueue=[];state.hostVersion=undefined;state.hostTask=null;state.hostWork=false;
    }
    if(confirmedPlayback){
      WorkerAnimation.playback(state,hostWork,seconds);
    }else if(hostWork&&!state.pendingPlan&&!state.pendingAdvance&&!state.transport&&!villager.logistics_target&&!villager.construction_target){
      if(state.hostVersion!==hostWork.version || state.hostTask!==JSON.stringify(hostWork.task)){
        Object.assign(state,{x:hostWork.x,y:hostWork.y,phase:hostWork.phase,
          elapsed:hostWork.phase==='collecting_water'?0:hostWork.elapsed,waterElapsed:hostWork.elapsed,
          waterReady:hostWork.waterReady,waterSource:hostWork.waterSource,farmAction:hostWork.farmAction,facing:hostWork.facing,
          butcherAnimal:hostWork.butcherAnimal,butcherPickup:hostWork.butcherPickup,
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
  const tiles=items=>items.filter(near).map(({kind,x,y,footprint,farm_index,construction,fence_orientation})=>[kind,x,y,footprint,farm_index,!!construction,fence_orientation]);
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
  return {axe:job==='woodcutting'&&(level>=5||['crafted_axe','fine_axe','steel_axe','masterwork_axe'].includes(v.worker_tool)),
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
const villagerMainContext=ctx;
let workerOutlineCanvas=null;
function drawWorkerOutline(b){
  const surface=workerOutlineCanvas ||= document.createElement('canvas'),target=surface.getContext?.('2d');
  if(!target)return;
  const scale=tileW/46,ratio=renderPixelRatio(),[x,y]=point(b.x,b.y);
  const width=128*scale+4,height=128*scale+4;
  const left=x+(b.index%3-1)*3-64*scale-2,top=y+tileH*.65+(b.kind==='npc'?0:Math.floor(b.index/3))-96*scale-2;
  surface.width=Math.ceil(width*ratio);surface.height=Math.ceil(height*ratio);
  target.setTransform(ratio,0,0,ratio,-left*ratio,-top*ratio);
  // Render the complete pose once so the outline follows limbs, clothing and tools.
  drawVillager(b,false,target);
  target.setTransform(1,0,0,1,0,0);target.globalCompositeOperation='source-in';
  target.fillStyle='#ffe45c';target.fillRect(0,0,surface.width,surface.height);
  for(let step=0;step<8;step++){
    const angle=step*Math.PI/4;
    ctx.drawImage(surface,left+Math.cos(angle)*1.5,top+Math.sin(angle)*1.5,surface.width/ratio,surface.height/ratio);
  }
}
function drawVillager(b,highlighted=false,target=ctx){
  const ctx=target,paintPolygon=(points,fill,stroke)=>polygon(points,fill,stroke,target);
  if(target===villagerMainContext&&b.state.butcherAnimal&&!b.state.construction&&!b.state.transport)drawButcherAnimal(b);
  if(highlighted)drawWorkerOutline(b);
  if(b.pose==='passed_out'){
    const [x,y]=point(b.x,b.y),s=tileW/46;
    ctx.save();ctx.translate(x,y+tileH*.65);ctx.scale(s,s);
    ctx.fillStyle='#223b2955';ctx.beginPath();ctx.ellipse(0,2,16,5,0,0,Math.PI*2);ctx.fill();
    ctx.strokeStyle='#3b4238';ctx.lineWidth=4;ctx.beginPath();ctx.moveTo(-4,0);ctx.lineTo(-13,3);ctx.moveTo(-4,2);ctx.lineTo(-12,7);ctx.stroke();
    ctx.fillStyle=['#8e735c','#788663','#85758a','#a18c62'][b.index%4];ctx.fillRect(-5,-3,13,7);
    ctx.fillStyle='#cfab83';ctx.beginPath();ctx.arc(11,0,4,0,Math.PI*2);ctx.fill();
    ctx.strokeStyle='#69533e';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(11,-1);ctx.lineTo(14,0);ctx.stroke();
    ctx.fillStyle='#bc9d68';ctx.fillRect(1,8,4,4);ctx.strokeStyle='#dbc99d';ctx.strokeRect(5,8,2,3);
    ctx.fillStyle='#ded5aa';ctx.font='8px serif';ctx.fillText('z',14,-9);ctx.fillText('Z',20,-15);
    ctx.restore();return;
  }
  const accessories=villagerAccessories(b),outfit=accessories.outfit;
  if(tileW<32&&!(b.villager.job==='fishing'&&b.state.phase==='working'&&!b.state.path.length)){
    const [x,y]=point(b.x,b.y),scale=tileW/46;
    ctx.save();ctx.translate(x+(b.index%3-1)*3,y+tileH*.65+(b.kind==='npc'?0:Math.floor(b.index/3)));ctx.scale(scale,scale);
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
  const farming=s.phase==='working'&&!walking&&b.villager.job==='farming'&&(!s.fieldCycle||!!s.farmAction);
  const constructing=working&&!!b.villager.construction_target;
  const handlingAnimal=!!s.butcherAnimal&&['dragging','finding_animal'].includes(s.phase);
  const sawing=working&&!walking&&!constructing&&b.villager.job==='woodworking';
  const sawHandX=4+Math.sin((s.elapsed || 0)*7)*4;
  const farmStage=s.farmAction?['plowing','sowing','watering','harvesting'].indexOf(s.farmAction):Math.min(3,Math.floor((s.elapsed || 0)/2));
  const farmSwing=(Math.sin((s.elapsed || 0)*Math.PI*3)+1)/2;
  const fishingTime=s.activityElapsed ?? s.elapsed ?? 0;
  const fishingUpgrades=s.home?.profession_upgrades || [];
  const fishingNet=fishing&&(fishingUpgrades.includes('nets')||fishingUpgrades.includes('wool_nets'));
  const fishingCaught=s.fishingCaught ?? s.latestFishingCatch?.caught ?? true;
  const fishingMetal=b.villager.worker_tool==='masterwork_fishing_rod'?'#d9bd70':b.villager.worker_tool==='steel_fishing_rod'?'#bacac5':'#a78454';
  const cast=Math.min(fishingTime/.55,1),reel=Math.max(0,Math.min((fishingTime-2.35)/.85,1));
  const swing=working?(Math.sin((s.elapsed || 0)*(chopping?5:7))+1)/2:0;
  ctx.save();ctx.translate(px+(b.index%3-1)*3,py+tileH*.65+(b.kind==='npc'?0:Math.floor(b.index/3)));ctx.scale(tileW/46,tileW/46);
  if(target===villagerMainContext&&!b.aboard){ctx.fillStyle='#223b2966';ctx.beginPath();ctx.ellipse(0,2,7,3,0,0,Math.PI*2);ctx.fill();}
  const facingTarget=s.phase==='collecting_water'?s.waterSource:farming?(s.farmField || s.workSpot || s.source):fishing?(s.castTarget || s.source):s.source;
  const towardSource=facingTarget?(facingTarget.x-s.x)-(facingTarget.y-s.y):0;
  const facing=(working||fishing||farming)&&towardSource!==0?Math.sign(towardSource):s.facing;
  ctx.scale(facing,1);
  if(b.leadingHorse){ctx.save();ctx.translate(-28,5);ctx.scale(.75,.75);paintHorse(ctx,stride);ctx.restore();ctx.strokeStyle='#cbb78a';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(-12,-8);ctx.lineTo(-3,-8);ctx.stroke();}
  if(b.mounted){paintHorse(ctx,stride);ctx.translate(0,-12);}
  ctx.strokeStyle='#283c38';ctx.lineWidth=2.5;ctx.lineCap='round';
  if(!b.aboard){ctx.beginPath();ctx.moveTo(-2,-4);ctx.lineTo(-2-stride*3,1);ctx.moveTo(2,-4);ctx.lineTo(2+stride*3,1);ctx.stroke();}
  const bob=walking?Math.abs(stride)*1.2:sawing?0:fishing?reel*1.5:farming?farmSwing*(farmStage===0?2:1):chopping?swing*2.5:working?swing:Math.sin(s.clock*2)*.3;
  ctx.translate(0,-bob);
  ctx.fillStyle=b.kind==='npc'?npcColors[b.biome][0]:b.kind==='player'?'#d6b85d':outfit?.color || ['#7899a2','#b9866b','#a0a16b','#947ca0','#c6aa62'][b.index%5];ctx.fillRect(-4,-13,8,10);
  const gear=b.kind==='player'?b.equipment:null;
  if(gear?.feet==='swamp_boots'){ctx.fillStyle='#52715d';ctx.fillRect(-5,-5,4,6);ctx.fillRect(1,-5,4,6);}
  if(gear?.body==='desert_clothing'){paintPolygon([[-5,-13],[5,-13],[6,-2],[-6,-2]],'#ead8ae');}
  if(accessories.blouse){
    paintPolygon([[-4,-13],[4,-13],[7,-9],[5,-7],[3,-10],[4,-3],[-4,-3],[-3,-10],[-5,-7],[-7,-9]],'#b8c6ad');
    paintPolygon([[-3,-13],[0,-10],[3,-13]],'#eee0bd');
    ctx.strokeStyle='#6c8068';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(0,-10);ctx.lineTo(0,-4);ctx.stroke();
    if(accessories.woodcuttingMaster){
      ctx.fillStyle='#e8bd55';ctx.fillRect(-4,-4,8,1);ctx.fillRect(-1,-9,2,2);
    }
  }
  if(outfit?.apron){
    paintPolygon([[-3,-12],[3,-12],[4,-3],[-4,-3]],outfit.apron);
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
  if(accessories.hat){paintPolygon([[-6,-19],[0,-23],[6,-19]],'#ccb779');ctx.fillStyle='#ccb779';ctx.fillRect(-7,-19,14,2);}
  if(accessories.farmingMaster){ctx.fillStyle='#e8bd55';ctx.fillRect(-5,-20,10,1);}
  if(b.kind==='npc'){
    const accent=npcColors[b.biome][1];
    if(b.biome==='b'||b.biome==='f')paintPolygon([[-5,-18],[0,-25],[5,-18]],accent);
    else{ctx.fillStyle=accent;ctx.fillRect(-7,-21,14,2);ctx.fillRect(-4,-25,8,4);}
    ctx.fillStyle=accent;ctx.fillRect(-4,-12,8,2);
    ctx.fillStyle='#8b704b';ctx.fillRect(5,-7,5,6);
  }
  ctx.fillStyle='#ded2a6';ctx.fillRect(-4,-6,8,1);
  if(accessories.axe){
    ctx.strokeStyle='#a78454';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(-6,0);ctx.lineTo(-8,-10);ctx.stroke();
    paintPolygon([[-8,-11],[-13,-12],[-14,-7],[-7,-7]],'#e4ece7','#6b7d79');
  }
  if(accessories.sickle&&!farming){
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
  if(outfit?.tool&&accessories.level>=5&&!sawing){
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
      paintPolygon([[-10,-8],[-6,-10],[-3,-8],[-6,-5]],'#b8d8d5',metal);
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
  ctx.beginPath();
  if(handlingAnimal){
    ctx.moveTo(-3,-11);ctx.lineTo(-7,-8);ctx.moveTo(3,-11);ctx.lineTo(-5,-8);
  }else if(sawing){
    // One hand steadies the timber; the other follows the saw handle.
    ctx.moveTo(-3,-11);ctx.lineTo(9,-10);ctx.lineTo(17,-6);
    ctx.moveTo(3,-11);ctx.lineTo(sawHandX,-12);
  }else{
    ctx.moveTo(-3,-11);ctx.lineTo(chopping?6:-5-stride*2,chopping?-14+swing*5:-6);ctx.moveTo(3,-11);ctx.lineTo(farming?farmHandX:7,chopping?-14+swing*5:fishing?-10-reel*3:farming?farmHandY:-9-swing*4);
  }
  ctx.stroke();
  if(constructing){
    const handY=-9-swing*4;
    ctx.strokeStyle='#a78454';ctx.lineWidth=2;
    ctx.beginPath();ctx.moveTo(7,handY);ctx.lineTo(10,handY-8);ctx.stroke();
    ctx.fillStyle='#b9c3bd';ctx.fillRect(6,handY-11,9,4);
  }else if(fishing){
    // Cast into the selected water tile, then lift the catch before delivery.
    const [waterX,waterY]=s.castTarget?point(s.castTarget.x+.5,s.castTarget.y+.5):s.workSpot?point(s.source.x+.35,s.source.y+.35):s.source?point(s.source.x,s.source.y):[px+tileW*.6,py+tileH*.65];
    const surfaceX=(waterX-px-(b.index%3-1)*3)/(tileW/46)*facing;
    const surfaceY=(waterY-py-tileH*.65-(b.kind==='npc'?0:Math.floor(b.index/3)))/(tileW/46)+bob;
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
      if(reel>0&&fishingCaught)paintPolygon([[netX-4,netY],[netX,netY-3],[netX+5,netY],[netX,netY+3]],'#b8d8d5','#527f86');
    }else{
      ctx.strokeStyle=fishingMetal;ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(7,-10-reel*3);ctx.lineTo(tipX,tipY);ctx.stroke();
      ctx.strokeStyle='#d5dfd8';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(tipX,tipY);ctx.lineTo(floatX,floatY);ctx.stroke();
      if(cast===1&&reel===0){
        ctx.strokeStyle='#c3e1dc';ctx.beginPath();ctx.ellipse(surfaceX,surfaceY,4+Math.sin(fishingTime*6)*1.5,2,0,0,Math.PI*2);ctx.stroke();
      }
      ctx.fillStyle='#df785e';ctx.fillRect(floatX-2,floatY-2,4,3);
      ctx.fillStyle='#f3e5bc';ctx.fillRect(floatX-2,floatY+1,4,2);
      if(reel>0&&fishingCaught){
        paintPolygon([[floatX,floatY+3],[floatX+5,floatY+6],[floatX,floatY+9],[floatX-4,floatY+6]],'#b8d8d5','#527f86');
        paintPolygon([[floatX,floatY+9],[floatX-3,floatY+12],[floatX+3,floatY+12]],'#7899a2');
      }
    }
  }else if(working&&b.villager.job==='gathering'){
    ctx.translate(swing*4,swing*3);
    ctx.fillStyle='#ac895d';ctx.fillRect(5,-7,10,8);ctx.fillStyle='#b35b69';ctx.beginPath();ctx.arc(10,-8,3,0,Math.PI*2);ctx.fill();
  }else if(farming){
    const field=s.farmField || s.workSpot || s.source || b;
    const cropPoint=order=>{
      const [x,y]=point(field.x-.3+Math.floor(order/4)*.2,field.y-.3+order%4*.2);
      // Undo the worker's offset, facing and bob to land on the crop's ground anchor.
      return [(x-px-(b.index%3-1)*3)/(tileW/46)*facing,
        (y+tileH/2-py-tileH*.65-(b.kind==='npc'?0:Math.floor(b.index/3)))/(tileW/46)+bob];
    };
    if(farmStage===0){
      // Pull a hoe through the soil, with small clods at the blade.
      const bladeX=17-farmSwing*7;
      ctx.strokeStyle='#a78454';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(farmHandX,farmHandY);ctx.lineTo(bladeX,1);ctx.stroke();
      paintPolygon([[bladeX-2,0],[bladeX+6,0],[bladeX+5,4],[bladeX-2,3]],'#aab9b4','#6b7d79');
      ctx.fillStyle='#8b6845';for(let i=0;i<3;i++)ctx.fillRect(bladeX+3+i*3,3-farmSwing*(i+1),2,2);
    }else if(farmStage===1){
      // Scatter seeds from a pouch with an outward sweep of the hand.
      paintPolygon([[-7,-7],[-2,-7],[-1,-1],[-8,-1]],'#9b754e','#6d513b');
      ctx.fillStyle='#efd59a';
      for(let i=0;i<5;i++){
        const flight=((s.elapsed || 0)*2+i*.2)%1;
        const [x,y]=cropPoint(i*3);
        ctx.fillRect(farmHandX+(x-farmHandX)*flight-1,farmHandY+(y-farmHandY)*flight*flight-1,2,2);
      }
    }else if(farmStage===2){
      // Tilt the can and pour a moving stream of droplets.
      ctx.save();ctx.translate(farmHandX,farmHandY);ctx.rotate(.15+farmSwing*.35);
      ctx.strokeStyle='#607f83';ctx.lineWidth=2;ctx.beginPath();ctx.arc(-2,-3,4,Math.PI,Math.PI*2);ctx.stroke();
      paintPolygon([[-6,-3],[3,-3],[3,4],[-6,4]],'#7faeb2','#4f757b');
      paintPolygon([[3,0],[9,-4],[10,-2],[3,3]],'#7faeb2','#4f757b');ctx.restore();
      const tilt=.15+farmSwing*.35;
      const spoutX=farmHandX+10*Math.cos(tilt)+2*Math.sin(tilt);
      const spoutY=farmHandY+10*Math.sin(tilt)-2*Math.cos(tilt);
      ctx.fillStyle='#9fd8e5';
      for(let i=0;i<3;i++){
        const fall=((s.elapsed || 0)*3+i/3)%1;
        const [x,y]=cropPoint(i*5);
        ctx.fillRect(spoutX+(x-spoutX)*fall-.75,spoutY+(y-spoutY)*fall*fall-1.5,1.5,3);
      }
    }else{
      // The acre supplies the crop artwork; this action draws only the held sickle.
      ctx.save();ctx.translate(farmHandX,farmHandY);ctx.rotate(-.8+farmSwing*1.6);
      ctx.strokeStyle='#a78454';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(0,3);ctx.lineTo(0,-5);ctx.stroke();
      ctx.strokeStyle='#dce5df';ctx.lineWidth=2;ctx.beginPath();ctx.arc(1,-8,5,-Math.PI/2,Math.PI/2);ctx.stroke();ctx.restore();
    }
  }else if(sawing||working&&['husbandry','milling','preserving','crafting','breeding','orcharding','baking','butchery'].includes(b.villager.job)){
    drawProfessionAction(b.villager.job,s.elapsed || 0,target);
  }else if(working){
    ctx.save();ctx.translate(chopping?6:5,chopping?-13+swing*4:-11);ctx.rotate(chopping?-1+swing*2.5:-1.4+swing*2);
    ctx.strokeStyle='#a78454';ctx.lineWidth=chopping?2.5:2;ctx.beginPath();ctx.moveTo(0,chopping?4:3);ctx.lineTo(0,chopping?-16:-10);ctx.stroke();
    ctx.fillStyle='#cbd7d2';
    if(chopping)paintPolygon([[-1,-17],[10,-18],[11,-10],[-1,-11]],'#e4ece7','#6b7d79');
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
    if(Object.values(s.cargo || {}).some(amount=>amount>0))drawHaulerCargo(s.cargo,target,target===villagerMainContext);
  }else if(s.transport||s.phase==='returning'||s.phase==='depositing'){
    ctx.save();
    if(s.phase==='depositing'){const progress=Math.min(1,(s.elapsed || 0)/.65);ctx.translate(progress*8,progress*3);ctx.globalAlpha=1-progress;}
    ctx.fillStyle=b.villager.job==='woodcutting'?'#997044':b.villager.job==='fishing'?'#7899a2':'#d6c478';ctx.fillRect(2,-8,11,5);
    ctx.fillStyle='#e1c593';ctx.beginPath();ctx.arc(13,-5.5,2.5,0,Math.PI*2);ctx.fill();
    if(b.villager.job==='fishing')paintPolygon([[5,-10],[9,-12],[13,-10],[9,-8]],'#b8d8d5','#527f86');
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
function drawButcherAnimal(b){
  const s=b.state,processing=s.phase==='working',dragging=s.phase==='dragging';
  const finding=s.phase==='finding_animal';
  const pickup=s.butcherPickup;
  if(!processing&&!dragging&&!finding&&!pickup)return;
  const position=processing||dragging||finding?b:pickup;
  const [x,y]=point(position.x,position.y),scale=tileW/46;
  const towardShop=s.source?(s.source.x-s.x)-(s.source.y-s.y):0;
  const facing=processing&&towardShop?Math.sign(towardShop):s.facing || 1;
  // The animal is reserved by the host; this extra sprite never changes barn stock.
  const offset=(processing?19:-19)*facing;
  ctx.save();
  if(processing){
    const growth=Math.min(1,(s.elapsed || 0)/7);
    ctx.save();ctx.translate(x+offset*scale,y+tileH/2+2*scale);ctx.scale(scale,scale);
    ctx.fillStyle='#702d32';ctx.beginPath();ctx.ellipse(0,0,5+growth*11,2+growth*4,.12,0,Math.PI*2);ctx.fill();
    ctx.fillStyle='#9b3b40';
    for(const [dx,dy,r] of [[-7,2,3],[9,-1,4],[3,4,2]]){
      ctx.beginPath();ctx.ellipse(dx*growth,dy*growth,r*growth,Math.max(.3,r*growth*.5),0,0,Math.PI*2);ctx.fill();
    }
    ctx.restore();
  }
  if(dragging||finding){
    ctx.strokeStyle='#bca17b';ctx.lineWidth=1.5*scale;
    ctx.beginPath();ctx.moveTo(x+3*facing*scale,y+tileH*.65-10*scale);
    ctx.quadraticCurveTo(x+offset*scale*.5,y+tileH/2-2*scale,x+(offset+5*facing)*scale,y+tileH/2-3*scale);ctx.stroke();
  }
  ctx.translate(offset*scale,processing?3*scale:0);
  drawBarnAnimal({kind:'animal',species:s.butcherAnimal,x:position.x,y:position.y,
    size:.8,facing:-facing,phase:s.clock || 0,walking:false,feeding:false,dragged:dragging||processing});
  ctx.restore();
}
function drawHaulerCargo(cargo,target=ctx,showLabel=true){
  const ctx=target;
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
  if(!showLabel)return;
  ctx.font='9px system-ui';ctx.textAlign='center';ctx.lineWidth=3;ctx.strokeStyle='#263a32';ctx.fillStyle='#fff3d6';
  const label=entries.map(([key,amount])=>`${Number(amount.toFixed(2))} ${goodName(key)}`).join(' · ');
  ctx.strokeText(label,0,-43);ctx.fillText(label,0,-43);
}
// Tools and materials are drawn in character coordinates; elapsed freezes on pauses.
function drawProfessionAction(job,time,target=ctx){
  const ctx=target,paintPolygon=(points,fill,stroke)=>polygon(points,fill,stroke,target);
  const motion=(Math.sin(time*7)+1)/2;
  ctx.fillStyle='#806448';ctx.fillRect(5,-3,17,3);
  ctx.strokeStyle='#d5dfd8';ctx.lineWidth=2;
  if(job==='woodworking'){
    // Fixed trestles and timber make the push/pull stroke readable.
    ctx.fillStyle='#806448';ctx.fillRect(7,-3,3,7);ctx.fillRect(20,-3,3,7);
    ctx.fillStyle='#b78b57';ctx.fillRect(6,-7,20,4);
    ctx.strokeStyle='#725333';ctx.lineWidth=1;ctx.beginPath();ctx.moveTo(16,-7);ctx.lineTo(16,-3);ctx.stroke();
    const handleX=4+Math.sin(time*7)*4;
    paintPolygon([[handleX+2,-12],[handleX+23,-12],[handleX+22,-8],
      ...Array.from({length:10},(_,i)=>[handleX+21-i*2,i%2?-8:-6]),[handleX+2,-8]],'#c4d2ce','#6b7d79');
    ctx.strokeStyle='#8b5e3b';ctx.lineWidth=2;ctx.strokeRect(handleX-1,-14,4,6);
    ctx.fillStyle='#e7c68c';ctx.fillRect(handleX-1,-13,3,3);
    ctx.fillStyle='#e4c98a';for(let i=0;i<3;i++){const fall=(time*4+i/3)%1;ctx.fillRect(15+fall*(i-1)*5,-3+fall*8,1.5,1.5);}
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
    paintPolygon([[8,-9],[16,-12],[23,-9],[16,-6]],'#8ebbbd');
    ctx.strokeStyle='#a78454';ctx.beginPath();ctx.moveTo(7,-13-motion*4);ctx.lineTo(23,-13-motion*4);ctx.stroke();
    ctx.fillStyle='#c7c6af88';for(let i=0;i<3;i++){ctx.beginPath();ctx.arc(12+i*4,-17-((time+i*.3)%1)*10,2,0,Math.PI*2);ctx.fill();}
  }else if(job==='orcharding'){
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
let overviewCache=null,overviewFrame=null,overviewActive=false;
function mapOverview(rect=canvas.getBoundingClientRect()){
  return Math.min(54,(rect.width-30)/12)*camera.zoom<32&&!tool&&panel!=='build';
}
function townOutline(buildings){
  // Include the whole footprint, including one-building and collinear towns.
  const tiles=new Map();
  for(const building of buildings)for(const {x,y} of footprintTiles(building))for(const [dx,dy] of [[0,0],[1,0],[1,1],[0,1]])tiles.set(`${x+dx},${y+dy}`,[x+dx,y+dy]);
  const points=[...tiles.values()].sort((a,b)=>a[0]-b[0]||a[1]-b[1]);
  const cross=(a,b,c)=>(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);
  const half=items=>{
    const result=[];
    for(const p of items){while(result.length>1&&cross(result.at(-2),result.at(-1),p)<=0)result.pop();result.push(p);}
    return result.slice(0,-1);
  };
  return points.length<3?points:[...half(points),...half(points.slice().reverse())];
}
function prepareOverview(resources){
  if(overviewCache?.worldId!==world.world_id)overviewCache={worldId:world.world_id,towns:new Map()};
  const cache=overviewCache;
  if(cache.world!==world||cache.viewed!==viewed){
    const entries=world.shared_map?Object.entries(world.towns):world.towns[viewed]?[[viewed,world.towns[viewed]]]:[];
    const towns=new Map();
    for(const [id,town] of entries){
      const hallIndex=town.buildings.findIndex(b=>b.kind==='hall'),hall=town.buildings[hallIndex];
      const footprint=JSON.stringify(town.buildings.map(b=>[b.x,b.y,b.footprint]));
      const signature=JSON.stringify([town.name,town.level,hallIndex,footprint]);
      const old=cache.towns.get(id);
      towns.set(id,old?.signature===signature?old:{signature,footprint,name:town.name,hallIndex,hall:hall?{x:hall.x,y:hall.y}:null,
        outline:old?.footprint===footprint?old.outline:townOutline(town.buildings)});
    }
    cache.towns=towns;cache.world=world;cache.viewed=viewed;
  }
  if(cache.resources!==resources){
    const water=new Set(resources.filter(r=>r.kind==='water').map(r=>`${r.x},${r.y}`));
    if(!cache.water||cache.water.size!==water.size||[...water].some(key=>!cache.water.has(key)))cache.water=water;
    cache.resources=resources;
  }
  return cache;
}
function drawOverview(rect,dpr,landscape,resources,bounds,players){
  const cache=prepareOverview(resources);
  const towns=[...cache.towns.values()];
  const key=JSON.stringify([camera.x,camera.y,tileW,rect.width,rect.height,dpr,viewed,identity?.town_id,selection,buildingHover,
    players.map(p=>[p.townId,p.x,p.y])]);
  const terrainUnchanged=groundCache&&!groundCache.detailed&&groundCache.world.world_id===world.world_id
    &&((groundCache.landscape===landscape&&groundCache.water===cache.water&&groundCache.forest===null)
      ||groundMatchesCache(landscape,cache.water,groundCache.bounds,null,groundCache));
  if(terrainUnchanged)Object.assign(groundCache,{world,landscape,water:cache.water,forest:null});
  const zoomSettling=groundCache&&groundCache.tileW!==tileW&&performance.now()-groundCache.zoomUpdatedAt>=150;
  if(overviewFrame?.cache===cache&&overviewFrame.key===key&&terrainUnchanged&&!zoomSettling
    &&towns.length===overviewFrame.towns.length&&towns.every((town,i)=>town===overviewFrame.towns[i]))return;
  ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,rect.width,rect.height);hitTargets=[];
  drawGround(rect,dpr,landscape,cache.water,bounds,null);
  for(const [id,town] of cache.towns){
    const selected=id===viewed&&selection?.type==='building'&&selection.index===town.hallIndex;
    const hovered=buildingHover?.townId===id&&buildingHover.index===town.hallIndex;
    const color=selected||hovered?'#fff0b1':id===identity?.town_id?'#e4c980':'#b4d0c3';
    ctx.save();ctx.lineWidth=selected||hovered?2:1.25;
    if(town.outline.length)polygon(town.outline.map(([x,y])=>point(x,y)),selected?'#e4c98022':'#00000000',color);
    if(town.hall){
      const [px,top]=point(town.hall.x,town.hall.y),py=top+tileH/2;
      if(px>=-100&&px<=rect.width+100&&py>=-30&&py<=rect.height+30){
        // Fixed-size hall glyph and text stay readable across overview scales.
        polygon([[px-7,py-2],[px,py-9],[px+7,py-2]],color,'#233b30');
        ctx.fillStyle=color;ctx.fillRect(px-5,py-2,10,8);
        ctx.fillStyle='#233b30';ctx.fillRect(px-1.5,py+1,3,5);
        ctx.font='bold 12px sans-serif';ctx.textAlign='center';ctx.strokeStyle='#233b30';ctx.lineWidth=3;
        ctx.strokeText(town.name || 'Town',px,py-14);ctx.fillStyle=color;ctx.fillText(town.name || 'Town',px,py-14);
        const labelWidth=ctx.measureText(town.name || 'Town')?.width || 40;
        hitTargets.push({type:'building',index:town.hallIndex,townId:id,left:px-Math.max(10,labelWidth/2),right:px+Math.max(10,labelWidth/2),top:py-28,bottom:py+9});
      }
    }
    ctx.restore();
  }
  for(const player of players){
    const [px,top]=point(player.x,player.y),py=top+tileH*.65;
    if(px<-100||px>rect.width+100||py<-30||py>rect.height+30)continue;
    const selected=selection?.type==='player'&&viewed===player.townId;
    const color=selected?'#fff0b1':player.townId===identity?.town_id?'#e4c980':'#b4d0c3';
    const label=player.townId===identity?.town_id?'You':world.towns[player.townId]?.name || 'Founder';
    // Fixed-size founder markers stay visible without preparing character animation.
    ctx.save();ctx.strokeStyle='#233b30';ctx.lineWidth=2;ctx.fillStyle=color;
    ctx.beginPath();ctx.ellipse(px,py,8,4,0,0,Math.PI*2);ctx.fill();ctx.stroke();
    ctx.fillRect(px-4,py-10,8,9);ctx.strokeRect(px-4,py-10,8,9);
    ctx.beginPath();ctx.arc(px,py-14,4,0,Math.PI*2);ctx.fill();ctx.stroke();
    ctx.font='bold 12px sans-serif';ctx.textAlign='center';ctx.lineWidth=3;
    ctx.strokeText(label,px,py-23);ctx.fillText(label,px,py-23);
    const labelWidth=ctx.measureText(label)?.width || 40;
    hitTargets.push({type:'player',index:0,townId:player.townId,left:px-Math.max(10,labelWidth/2),right:px+Math.max(10,labelWidth/2),top:py-37,bottom:py+5});
    ctx.restore();
  }
  overviewFrame={cache,key,towns};
}
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
    const layout=JSON.stringify(resources.map(({kind,x,y,footprint,farm_index,construction})=>[kind,x,y,footprint,farm_index,!!construction]));
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
    m:['#83945e','#899b65','#80915b'],s:['#b4a575','#bdad7d','#b8a878'],r:['#8b8d7c','#969785','#858978'],b:['#59674b','#626f50','#536247'],d:['#d6b575','#dfbf80','#cfac6b'],i:['#c6e2e8','#d4e9ed','#bcd9e3'],
    p:['#a18b67','#a58f6b','#9b8562'],c:['#a5aa94','#adb09c','#9ca38d'],w:['#a48a60','#b1986c','#9b8059']};
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
      if(detailed&&!wet&&['p','c','w'].includes(biome)){
        const q=(u,v)=>[px+(u-v)*tileW/2,py+(u+v)*tileH/2];
        const line=(a,b,color,width=1)=>{ctx.strokeStyle=color;ctx.lineWidth=width*tileW/46;ctx.beginPath();ctx.moveTo(...a);ctx.lineTo(...b);ctx.stroke();};
        if(biome==='w'){
          // Continuous raised planks cross the canal without blocking movement.
          for(let plank=1;plank<7;plank++)line(q(.02,plank/7),q(.98,plank/7),'#715d43',.8);
          for(const u of [.08,.92]){
            line(q(u,.04),q(u,.96),'#d0b582',1.4);
            for(const v of [.16,.83]){const [x,y]=q(u,v);ctx.fillStyle='#5d5945';ctx.fillRect(x,y,tileW/50,tileW/50);}
          }
        }else if(biome==='c'){
          for(let row=0;row<4;row++)for(let column=0;column<4;column++){
            const u=column/4,v=row/4;
            polygon([q(u+.025,v+.025),q(u+.23,v+.025),q(u+.23,v+.23),q(u+.025,v+.23)],
              (row+column+variation)%3?'#b9b9a1':'#939e8b','#7b8875');
          }
        }else{
          // Worn wheel tracks and pebbles make the main causeway read as a lane.
          const alongX=['p','c','w'].includes(landscape?.terrain?.[y]?.[x+1]);
          for(const v of [.27,.72])line(alongX?q(.08,v):q(v,.08),alongX?q(.92,v):q(v,.92),'#8e795b',.8);
          const [sx,sy]=q(.25+(variation%3)*.2,.5);ctx.fillStyle='#c4b18a';ctx.fillRect(sx,sy,tileW/35,tileW/55);
        }
        const neighbors=[[x,y-1],[x+1,y],[x,y+1],[x-1,y]];
        for(let edge=0;edge<4;edge++){
          const [nx,ny]=neighbors[edge];
          if(!['p','c','w'].includes(landscape?.terrain?.[ny]?.[nx]))line(diamond[edge],diamond[(edge+1)%4],biome==='w'?'#65523c':'#c2b28a',1.2);
        }
      }
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
      if(detailed&&!wet&&!['d','i','p','c','w'].includes(biome)&&variation===0){ctx.fillStyle='#8fa574';ctx.fillRect(px-5*tileW/46,py+tileH*.5,2*tileW/46,2*tileW/46);ctx.fillStyle='#d7c78a';ctx.fillRect(px+4*tileW/46,py+tileH*.65,2*tileW/46,2*tileW/46);}
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
        &&!(groundCache.landscape?.terrain===landscape?.terrain&&groundCache.water===water&&groundCache.forest===forest
          &&groundCache.landscape?.map_size===landscape?.map_size&&groundCache.landscape?.terrain_seed===landscape?.terrain_seed
          &&!!groundCache.landscape?.chunked===!!landscape?.chunked)
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
function drawFishingBoat(boat,target=ctx){
  const ctx=target,paintPolygon=(points,fill,stroke)=>polygon(points,fill,stroke,target);
  const [x,top]=point(boat.x,boat.y),y=top+tileH/2,s=tileW/46;
  ctx.save();ctx.lineWidth=1.5*s;
  ctx.strokeStyle='#b8dcda';ctx.beginPath();ctx.ellipse(x,y+6*s,23*s,8*s,0,0,Math.PI*2);ctx.stroke();
  paintPolygon([[x-23*s,y-3*s],[x-13*s,y+12*s],[x+12*s,y+12*s],[x+25*s,y-4*s]],boat.level===3?'#a27640':'#805738','#493627');
  paintPolygon([[x-23*s,y-3*s],[x-9*s,y-9*s],[x+25*s,y-4*s],[x+10*s,y+5*s],[x-12*s,y+5*s]],'#c29a63','#efcb8c');
  paintPolygon([[x-16*s,y-2*s],[x-8*s,y-5*s],[x+17*s,y-2*s],[x+8*s,y+2*s],[x-9*s,y+2*s]],'#574637');
  ctx.fillStyle='#e0bb80';ctx.fillRect(x-8*s,y-4*s,3*s,8*s);ctx.fillRect(x+6*s,y-3*s,3*s,7*s);
  ctx.strokeStyle='#efd49f';ctx.lineWidth=2*s;ctx.beginPath();ctx.moveTo(x-15*s,y+12*s);ctx.lineTo(x+14*s,y-10*s);ctx.stroke();
  if(boat.level>=2){
    ctx.fillStyle='#dfd6ac';ctx.fillRect(x,y-31*s,2*s,32*s);
    paintPolygon([[x+2*s,y-31*s],[x+2*s,y-7*s],[x+20*s,y-7*s]],boat.level===3?'#d4af6b':'#ece3c5','#805738');
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
function barnActive(b){return b.kind==='barn'&&(!b.construction||b.construction.upgrade);}
function barnPen(b,size=120,orientation=b.fence_orientation ?? 0){
  const left=Math.max(-.5,b.x-3.5),right=Math.min(size-.5,b.x+3.5),back=Math.max(-.5,b.y-3.5),front=Math.min(size-.5,b.y+3.5);
  const sides=[[right,back,right,front],[right,front,left,front],[left,front,left,back],[left,back,right,back]];
  const available=[right<size-.5,front<size-.5,left>-.5,back>-.5];
  if(!Number.isInteger(orientation)||!available[orientation])orientation=Math.max(0,available.indexOf(true));
  const [nx,ny]=[[1,0],[0,1],[-1,0],[0,-1]][orientation],tx=-ny,ty=nx,edge=sides[orientation];
  const ends=[(edge[0]-b.x)*tx+(edge[1]-b.y)*ty,(edge[2]-b.x)*tx+(edge[3]-b.y)*ty];
  const start=Math.max(Math.min(...ends)+2,Math.min(.5,Math.max(...ends)-2));
  const distance=(edge[0]-b.x)*nx+(edge[1]-b.y)*ny;
  const a={x:b.x+distance*nx+start*tx,y:b.y+distance*ny+start*ty},end={x:a.x+2*tx,y:a.y+2*ty};
  const segments=sides.filter((_,i)=>i!==orientation).concat([[edge[0],edge[1],a.x,a.y],[end.x,end.y,edge[2],edge[3]]]);
  return {orientation,a,end,nx,ny,tx,ty,segments,available};
}
function barnFenceSegments(buildings,size=120){return buildings.filter(barnActive).flatMap(b=>barnPen(b,size).segments);}
function barnFenceBlocked(x,y,segments,radius=.18){
  return segments.some(([x1,y1,x2,y2])=>x1===x2?
    Math.abs(x-x1)<radius&&y>Math.min(y1,y2)-radius&&y<Math.max(y1,y2)+radius:
    Math.abs(y-y1)<radius&&x>Math.min(x1,x2)-radius&&x<Math.max(x1,x2)+radius);
}
function barnFenceCrossed(a,b,segments=[]){
  return segments.some(([x1,y1,x2,y2])=>{
    if(x1===x2&&Math.abs(b.x-a.x)>1e-9){
      const t=(x1-a.x)/(b.x-a.x),y=a.y+t*(b.y-a.y);
      return t>=0&&t<=1&&y>=Math.min(y1,y2)-1e-9&&y<=Math.max(y1,y2)+1e-9;
    }
    if(y1===y2&&Math.abs(b.y-a.y)>1e-9){
      const t=(y1-a.y)/(b.y-a.y),x=a.x+t*(b.x-a.x);
      return t>=0&&t<=1&&x>=Math.min(x1,x2)-1e-9&&x<=Math.max(x1,x2)+1e-9;
    }
    return false;
  });
}
function drawBarnPath(b,size=120){
  const pen=barnPen(b,size),gate={x:(pen.a.x+pen.end.x)/2,y:(pen.a.y+pen.end.y)/2};
  const exit={x:gate.x+pen.nx*.35,y:gate.y+pen.ny*.35};
  let points=[{x:b.x,y:b.y+.6},{x:b.x,y:b.y+1.5}];
  if(pen.orientation===0)points.push({x:b.x,y:gate.y});
  if(pen.orientation===1)points.push({x:gate.x,y:b.y+1.5});
  if(pen.orientation===2)points.push({x:b.x-1.5,y:b.y+1.5},{x:b.x-1.5,y:gate.y});
  if(pen.orientation===3)points.push({x:gate.x,y:b.y+1.5});
  points.push(exit);
  const ground=(x,y)=>{const [px,py]=point(Math.max(-.5,Math.min(size-.5,x)),Math.max(-.5,Math.min(size-.5,y)));return [px,py+tileH/2];};
  for(let i=1;i<points.length;i++){
    const a=points[i-1],end=points[i],r=.45;
    if(Math.abs(a.x-end.x)+Math.abs(a.y-end.y)<1e-6)continue;
    const left=Math.min(a.x,end.x)-(a.x===end.x?r:0),right=Math.max(a.x,end.x)+(a.x===end.x?r:0);
    const back=Math.min(a.y,end.y)-(a.y===end.y?r:0),front=Math.max(a.y,end.y)+(a.y===end.y?r:0);
    polygon([ground(left,back),ground(right,back),ground(right,front),ground(left,front)],'#b1a078');
    if(i<points.length-1)polygon([ground(end.x-r,end.y-r),ground(end.x+r,end.y-r),ground(end.x+r,end.y+r),ground(end.x-r,end.y+r)],'#b1a078');
  }
}
function barnGatePassage(person,pen){
  if(!person)return false;
  const {a,nx,ny,tx,ty}=pen;
  const normal=p=>(p.x-a.x)*nx+(p.y-a.y)*ny,tangent=p=>(p.x-a.x)*tx+(p.y-a.y)*ty;
  const across=normal(person),along=tangent(person);
  if(Math.abs(across)>=1.7||along<=-.6||along>=2.6)return false;
  // Never close on somebody already occupying the doorway.
  if(Math.abs(across)<.25&&along>.18&&along<1.82)return true;
  const state=person.state || {};
  if(state.playbackQueue?.length&&!state.path?.length)return false;
  let route=state.path || [];
  if(state.playbackQueue?.length)route=state.playbackQueue.flatMap(chunk=>chunk.samples.slice(chunk.index+1));
  else if(state.segments?.length)route=state.segments.map(segment=>segment.target);
  else if(state.waypoints?.length)route=state.waypoints;
  let position=person,travelled=0;
  for(const waypoint of route){
    let next=Array.isArray(waypoint)?{x:waypoint[0],y:waypoint[1]}:waypoint;
    let distance=Math.hypot(next.x-position.x,next.y-position.y);
    if(distance<1e-9)continue;
    // Founders publish short input steps; use their current approach direction.
    if(person.kind==='player'&&route.length===1){
      next={x:position.x+(next.x-position.x)*2.4/distance,y:position.y+(next.y-position.y)*2.4/distance};distance=2.4;
    }
    const before=normal(position),after=normal(next),delta=after-before;
    if(Math.abs(delta)>1e-9){
      const fraction=-before/delta,crossing=tangent(position)+(tangent(next)-tangent(position))*fraction;
      if(fraction>=0&&fraction<=1&&travelled+distance*fraction<=2.4&&crossing>.18&&crossing<1.82)return true;
    }
    travelled+=distance;if(travelled>2.4)break;
    position=next;
  }
  return false;
}
function barnFenceSprites(buildings,landscape={},people=[],seconds=0){
  const sprites=[],size=landscape.map_size || 120,previous=barnFenceSprites.states || new Map(),states=new Map();
  for(const barn of buildings){
    if(!barnActive(barn))continue;
    const pen=barnPen(barn,size),{a,end,tx,ty}=pen;
    for(const [x,y,endX,endY] of pen.segments){
      const length=Math.max(Math.abs(endX-x),Math.abs(endY-y));
      for(let step=0;step<length-1e-6;step++){
        const start={x:x+(endX-x)*step/length,y:y+(endY-y)*step/length};
        const finish={x:x+(endX-x)*Math.min(length,step+1)/length,y:y+(endY-y)*Math.min(length,step+1)/length};
        sprites.push({kind:'barn_fence',x:(start.x+finish.x)/2,y:(start.y+finish.y)/2,start,end:finish});
      }
    }
    const nearby=people.some(person=>barnGatePassage(person,pen));
    const key=`${world.world_id}/${barn.townId || viewed}/${barn.x},${barn.y}/${pen.orientation}`;
    const old=previous.get(key),hold=nearby ? .5 : Math.max(0,(old?.hold || 0)-seconds),target=Number(nearby||hold>0);
    const opening=old?Math.max(0,Math.min(1,old.opening+Math.sign(target-old.opening)*seconds*4)):target;
    states.set(key,{opening,hold});
    const start={x:a.x-2.05*tx*opening,y:a.y-2.05*ty*opening},finish={x:end.x-2.05*tx*opening,y:end.y-2.05*ty*opening};
    sprites.push({kind:'barn_gate',x:(start.x+finish.x)/2,y:(start.y+finish.y)/2,start,end:finish,pen,opening});
  }
  barnFenceSprites.states=states;return sprites;
}
function drawBarnFence(section){
  const s=tileW/46;
  const ground=(p,height)=>{const [x,y]=point(p.x,p.y);return [x,y+tileH/2-height*s];};
  const rail=(a,b,height,color='#b68d59',width=2.8)=>{
    const [x,y]=ground(a,height),[endX,endY]=ground(b,height);
    ctx.strokeStyle='#624b35';ctx.lineWidth=(width+1.2)*s;ctx.beginPath();ctx.moveTo(x,y);ctx.lineTo(endX,endY);ctx.stroke();
    ctx.strokeStyle=color;ctx.lineWidth=width*s;ctx.beginPath();ctx.moveTo(x,y-.5*s);ctx.lineTo(endX,endY-.5*s);ctx.stroke();
  };
  const line=(a,az,b,bz,color,width)=>{const [x,y]=ground(a,az),[ex,ey]=ground(b,bz);ctx.strokeStyle=color;ctx.lineWidth=width*s;ctx.beginPath();ctx.moveTo(x,y);ctx.lineTo(ex,ey);ctx.stroke();};
  const post=(p,height=17)=>{const [x,y]=ground(p,0);ctx.lineWidth=.8*s;polygon([[x-2*s,y],[x-2*s,y-height*s],[x,y-(height+2)*s],[x+2*s,y-height*s],[x+2*s,y]],'#a57d4f','#624b35');line(p,1,p,height-1,'#dcba81',.8);};
  ctx.save();ctx.lineCap='round';
  const {start,end}=section;
  if(section.kind==='barn_gate'){
    const {a,end:closedEnd,tx,ty}=section.pen,guide={x:a.x-2.05*tx,y:a.y-2.05*ty};
    rail(guide,closedEnd,24,'#967749',2.3);post(guide,26);post(a,26);post(closedEnd,26);
    for(const height of [5,15])rail(start,end,height,'#d4b178',3.1);
    line(start,4,end,16,'#6b5036',3);line(start,5,end,16,'#d4b178',1.7);
    line(start,3,start,17,'#bb965e',2.7);line(end,3,end,17,'#bb965e',2.7);
    for(const t of [.1,.9]){
      const p={x:start.x+(end.x-start.x)*t,y:start.y+(end.y-start.y)*t};
      line(p,16,p,25,'#53605b',1.1);const [x,y]=ground(p,24);
      ctx.fillStyle='#53605b';ctx.beginPath();ctx.arc(x,y,2*s,0,Math.PI*2);ctx.fill();
    }
  }else{
    for(const height of [6,13])rail(start,end,height);
    line(start,4,end,15,'#6f573b',2);line(start,15,end,4,'#c4a477',1.5);post(start);post(end);
  }
  ctx.restore();
}
// Animals roam nearby open tiles; their motion is purely visual.
function barnAnimalSprites(buildings,time,landscape={}){
  const sprites=[],size=landscape.map_size || 120;
  const scope=typeof world==='undefined'?'':`${world.world_id}/${world.shared_map?'shared':viewed}`;
  const growthStates=barnAnimalSprites.growthStates || new Map(),nextGrowthStates=new Map();
  const key=p=>`${p.x},${p.y}`;
  const prepared=typeof ClientScene!=='undefined';
  const blocked=prepared?null:new Set([...buildings,...(landscape.map_resources || []),...(landscape.npcs || [])].map(key));
  for(const barn of buildings){
    if(!['barn','stable','village_hut'].includes(barn.kind)||(barn.construction&&!barn.construction.upgrade))continue;
    if(!Object.values(barn.animals || {}).some(n=>n>0))continue;
    const square=barn.kind==='barn',reach=square?3:2;
    // Outdoor trips stay connected to an accessible side of the shelter.
    const routes=prepared?ClientScene.animalRoutes(buildings,landscape,barn):[];
    if(!prepared)for(const [dx,dy] of [[1,0],[0,1],[-1,0],[0,-1]]){
      const open=p=>{
        const biome=landscape.terrain?.[p.y]?.[p.x];
        return (square?Math.max(Math.abs(p.x-barn.x),Math.abs(p.y-barn.y)):Math.abs(p.x-barn.x)+Math.abs(p.y-barn.y))<=reach&&key(p)!==key(barn)
          &&p.x>=0&&p.y>=0&&p.x<size&&p.y<size&&!blocked.has(key(p))
          &&biome!=='v'&&(!landscape.chunked||!!biome);
      };
      const start={x:barn.x+dx,y:barn.y+dy};if(!open(start))continue;
      const queue=[{...start,path:[start]}],seen=new Set([key(start)]);
      for(const tile of queue){
        routes.push(tile.path);if(!square&&tile.path.length>=3)continue;
        for(const [dx,dy] of [[1,0],[0,1],[-1,0],[0,-1]]){
          const next={x:tile.x+dx,y:tile.y+dy};
          if(!open(next)||seen.has(key(next)))continue;
          seen.add(key(next));queue.push({...next,path:[...tile.path,next]});
        }
      }
    }
    if(!routes.length)continue;
    routes.sort((a,b)=>b.length-a.length);
    const stock=[];
    for(const species of (barn.village?['chicken','sheep','pig','cow','horse']:barn.kind==='stable'?['horse']:['chicken','sheep','pig','cow']))
      for(let i=0;i<(barn.animals?.[species] || 0);i++){
        const duration=typeof world==='undefined'?300:world.progression?.animals?.[species]?.growth_seconds || world.progression?.animal_growth_seconds || 300;
        const target=Math.max(0,Math.min(1,(barn.animal_growth?.[species]?.[i] ?? duration)/duration));
        const id=`${scope}/${barn.townId || ''}/${barn.kind}/${barn.x},${barn.y}/${species}/${i}`;
        let state=growthStates.get(id);
        // Blend only confirmed growth; a replacement young animal starts at its own size.
        if(!state||target<state.target||time<state.time)state={from:target,target,started:time,seconds:0};
        const growth=state.seconds?state.from+(state.target-state.from)*Math.min(1,(time-state.started)/state.seconds):state.target;
        if(target!==state.target){
          state.from=growth;state.seconds=Math.min(3,(target-state.target)*duration);
          state.target=target;state.started=time;
        }
        state.time=time;nextGrowthStates.set(id,state);
        stock.push({species,growth});
      }
    for(const [index,{species,growth}] of stock.entries()){
      const path=routes[index%routes.length];
      const walkSeconds=Math.max(3,(path.length-1)*1.5),period=walkSeconds*2+10;
      const phase=time+(barn.x*7+barn.y*11+index*3.17)%13,cycle=phase%period;
      const outbound=cycle<walkSeconds,returning=cycle>=walkSeconds+5&&cycle<walkSeconds*2+5;
      const walking=path.length>1&&(outbound||returning);
      const progress=outbound?cycle/walkSeconds:returning?1-(cycle-walkSeconds-5)/walkSeconds:cycle<walkSeconds+5?1:0;
      const step=progress*(path.length-1),segment=Math.min(Math.floor(step),Math.max(0,path.length-2));
      const start=path[segment],end=path[Math.min(segment+1,path.length-1)],fraction=step-segment;
      const side=(index%4-1.5)*.06,front=.12+Math.min(5,Math.floor(index/4))*.04;
      sprites.push({kind:'animal',species,x:start.x+(end.x-start.x)*fraction+front+side,
        y:start.y+(end.y-start.y)*fraction+front-side,phase,walking,
        feeding:!walking&&((cycle>=walkSeconds+1&&cycle<walkSeconds+4)||cycle>=walkSeconds*2+6),
        facing:((end.x-start.x)-(end.y-start.y))*(returning?-1:1)>=0?1:-1,
        size:.8*(.65+.35*growth)});
    }
  }
  barnAnimalSprites.growthStates=nextGrowthStates;
  return sprites;
}
function drawBarnAnimal(animal){
  const speciesSize={chicken:.75,sheep:1,pig:1.05,cow:1.8,horse:1}[animal.species] || 1;
  const [x,top]=point(animal.x,animal.y),s=tileW/46*animal.size*speciesSize;
  const chicken=animal.species==='chicken',cow=animal.species==='cow';
  const stride=animal.walking?Math.sin(animal.phase*12):0;
  const bob=animal.walking?Math.abs(stride)*.6:Math.sin(animal.phase*2)*.2;
  const coat={chicken:'#f4deb0',sheep:'#e7e2ce',pig:'#d9968e',cow:'#e5d9bc'}[animal.species];
  ctx.save();ctx.translate(x,top+tileH/2);ctx.scale(s*animal.facing,s);
  if(animal.dragged)ctx.rotate(-.65);
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
  const resources=world.shared_map?.map_resources || town?.map_resources || [];
  const corners=[[-tileW,-100],[rect.width+tileW,-100],[-tileW,rect.height+100],[rect.width+tileW,rect.height+100]]
    .map(([px,py])=>({x:(px-originX)/tileW+(py-originY)/tileH,y:(py-originY)/tileH-(px-originX)/tileW}));
  const size=mapSize(town),minX=Math.max(0,Math.floor(Math.min(...corners.map(p=>p.x)))),maxX=Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.x))));
  const minY=Math.max(0,Math.floor(Math.min(...corners.map(p=>p.y)))),maxY=Math.min(size-1,Math.ceil(Math.max(...corners.map(p=>p.y))));
  const entries=world.shared_map?Object.entries(world.towns):town?[[viewed,town]]:[];
  const otherPlayers=frameActors?.world===world?frameActors.otherPlayers:world.shared_map?entries.filter(([id,t])=>t.player&&id!==hero?.townId).map(([id,t])=>remotePlayerSprite(id,t,seconds)):[];
  if(mapOverview(rect)){
    drawOverview(rect,dpr,world.shared_map || town,resources,{minX,maxX,minY,maxY},[...otherPlayers,...(hero?[hero]:[])]);return;
  }
  overviewFrame=null;
  ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,rect.width,rect.height);hitTargets=[];
  const resourceData=renderResources(resources,tileW>=32),water=resourceData.water;
  const showBuildingRange=town && (panel==='build' || (selection?.type==='building' && town.buildings[selection.index]?.kind==='hall'));
  const preview=hoverTile || keyboardTile;
  const previewKind=tool in costs?tool:tool==='move'&&moveIndex!==null?town?.buildings[moveIndex]?.kind:null;
  const selected=selection?.type==='building'?town?.buildings[selection.index]:selection?.type==='road'?town?.roads?.[selection.index]:selection?.type==='resource'?resources[selection.index]:null;
  const rangeBuildings=panel==='build'?(world.shared_map?Object.values(world.towns):[town]).flatMap(t=>t.buildings.filter(b=>gameplayRange(b,t)).map(b=>({building:b,town:t}))):selection?.type==='building'&&selected?[{building:selected,town}]:[];
  const rangePreview=previewKind&&preview?{...(tool==='move'&&moveIndex!==null?town.buildings[moveIndex]:{}),...placementBuilding(preview,previewKind)}:null;
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
  if(tool || showBuildingRange || reachable.size || rangeBuildings.length){
    for(let x=minX;x<=maxX;x++)for(let y=minY;y<=maxY;y++){
      const [px,py]=point(x,y);
      if(px+tileW/2<0 || px-tileW/2>rect.width || py+tileH+12<0 || py>rect.height)continue;
      if((world.shared_map || town)?.terrain?.[y]?.[x]==='v'||(town?.chunked&&!town.terrain?.[y]?.[x]))continue;
      const diamond=[[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]];
      if(tool)polygon(diamond,'#00000000','#91a37455');
      if(showBuildingRange && insideBuildingRange(town,{x,y}))polygon(diamond,'#e4c98022','#e4c98088');
      if(rangeBuildings.some(({building,town})=>building.kind!=='hall'&&rangeContains(building,{x,y},town)))polygon(diamond,'#82b9ae18','#82b9ae55');
      if(rangePreview&&rangeContains(rangePreview,{x,y},town))polygon(diamond,'#b8dcad33','#c6dfaa88');
      if(reachable.has(`${x},${y}`))polygon(diamond,'#b8dcad44','#c6dfaa');
      if(rangePreview&&occupies(rangePreview,x,y)&&town){const valid=placementValid(town,preview,previewKind)&&buildingUsable(town,{...preview,kind:previewKind});ctx.save();ctx.lineWidth=2;polygon(diamond,valid?'#d6d8a366':'#c76d6455',valid?'#eee5ab':'#f3a295');ctx.restore();}
    }
  }
  const highlights=[...(selected?footprintTiles(selected):[]),!previewKind?keyboardTile:null].filter(Boolean);
  for(let i=0;i<highlights.length;i++){
    const {x,y}=highlights[i];
    if(i&&x===highlights[0].x&&y===highlights[0].y)continue;
    const [px,py]=point(x,y),landscape=world.shared_map || town;
    if(x<minX||x>maxX||y<minY||y>maxY||px+tileW/2<0||px-tileW/2>rect.width||py+tileH+12<0||py>rect.height)continue;
    if(landscape?.terrain?.[y]?.[x]==='v'||(town?.chunked&&!landscape?.terrain?.[y]?.[x]))continue;
    ctx.save();ctx.lineWidth=3;
    polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],'#fff0b133','#fff0b1');ctx.restore();
  }
  const buildings=(typeof ClientScene!=='undefined'&&ClientScene.current?.world===world?(world.shared_map?ClientScene.current.buildings:town?.buildings):null) ?? (world.shared_map?entries.flatMap(([id,t])=>t.buildings.map((b,index)=>({...b,townId:id,objectIndex:index}))):town?.buildings || [{kind:'hall',x:5,y:5}]);
  for(const b of buildings)if(barnActive(b))drawBarnPath(b,mapSize(town));
  const workerLayout=(typeof ClientScene!=='undefined'&&ClientScene.current?.world===world?ClientScene.current.geometry:null) ?? ('['+JSON.stringify(buildings.map(({kind,x,y,footprint,farm_index,construction,fence_orientation})=>[kind,x,y,footprint,farm_index,!!construction,fence_orientation]))+','+resourceData.layout+']');
  const villagers=frameActors?.world===world?frameActors.villagers:entries.flatMap(([id,t])=>animatedVillagers(t,seconds,id,
    t?.chunked?townWorkerLayout(t,buildings,resources):workerLayout));
  const selectedWorkers=new Set(selection?.type==='building'&&selected?villagers.filter(worker=>{
    if(worker.townId!==undefined&&worker.townId!==viewed)return false;
    const index=settlerBuilding(town,worker.villager,worker.index);
    return index===selection.index || selected.kind==='farm'&&town.buildings[index]?.kind==='acre'
      &&town.buildings[index].farm_index===selection.index;
  }):[]);
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
      &&(field.townId===undefined||field.townId===worker.townId)
      &&(WorkerAnimation.fieldPresentation(worker.state,field)
        ||(worker.state.phase==='working'&&worker.state.source?.kind==='acre'
          &&worker.state.source.x===field.x&&worker.state.source.y===field.y)));
    // Soil and furrows stay below every character; stalks use their own depth.
    drawFarmCrops(field,farmer?.state,crop=>cropObjects.push(crop));
  }
  const visible=b=>{
    const [px,py]=point(b.x,b.y);
    return px>=-tileW&&px<=rect.width+tileW&&py>=-tileH&&py<=rect.height+80*tileW/46;
  };
  const boats=fishingBoatSprites(entries,resources,villagers);
  const passengers=new Set(boats.flatMap(b=>b.crew));
  const locals=npcSprites(world.shared_map || town,typeof ClientScene==='undefined'?seconds:0);
  const fences=barnFenceSprites(buildings,world.shared_map || town,[...villagers,...locals,...otherPlayers,...(hero?[hero]:[])],frameActors?.world===world?frameActors.seconds || 0:seconds);
  const objects=mergeDepthObjects([...buildings,...cropObjects,...boats,...fences].filter(visible),visibleResourceObjects(resourceData,rect),
    [...villagers.filter(v=>!passengers.has(v)),...locals,...barnAnimalSprites(villageAnimalBuildings(buildings,resourceData),animalClock,world.shared_map || town),...otherPlayers,...(hero?[hero]:[])].filter(visible));
  for(const b of objects){
    const position=b.kind==='wild_horse'?wildHorsePosition(b):[b.x,b.y];
    const [px,py]=point(position[0],position[1]),scale=tileW/46;
    if(px < -tileW || px > rect.width+tileW || py < -tileH || py > rect.height+80*scale)continue;
    if(b.kind==='animal'){drawBarnAnimal(b);continue;}
    if(b.kind==='barn_fence'||b.kind==='barn_gate'){drawBarnFence(b);continue;}
    if(b.kind==='crop'){drawFarmCrop(b);continue;}
    if(b.kind==='fishing_boat'){
      drawFishingBoat(b);
      for(const [seat,passenger] of b.crew.entries()){
        const offset=(seat-(b.crew.length-1)/2)*.28;
        drawVillager({...passenger,aboard:true,x:b.x+offset,y:b.y-offset,state:{...passenger.state,path:[]}},selectedWorkers.has(passenger));
      }
      // The near hull covers seated passengers' feet inside the boat.
      const boatY=py+tileH/2;
      polygon([[px-23*scale,boatY-3*scale],[px-12*scale,boatY+5*scale],[px+10*scale,boatY+5*scale],
        [px+25*scale,boatY-4*scale],[px+12*scale,boatY+12*scale],[px-13*scale,boatY+12*scale]],b.level===3?'#a27640':'#805738','#493627');
      hitTargets.push({type:'boat',index:b.objectIndex,townId:b.townId,left:px-25*scale,right:px+25*scale,top:py+(tileH/2)-(b.level>=2?32:10)*scale,bottom:py+tileH/2+13*scale,paint:target=>drawFishingBoat(b,target)});
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
      hitTargets.push({type:'player',index:0,townId:b.townId,left:px-(b.mounted?20:10)*scale,right:px+(b.mounted?24:10)*scale,top:baseY-(b.mounted?39:27)*scale,bottom:baseY+5*scale,paint:target=>{
        if(sailing)polygon([[px-20*scale,baseY],[px-10*scale,baseY+10*scale],[px+12*scale,baseY+10*scale],[px+21*scale,baseY-2*scale]],'#805838','#d4af6b',target);
        drawVillager({...b,mounted:b.mounted&&!sailing,leadingHorse:b.leadingHorse&&!sailing},false,target);
      }});
      continue;
    }
    const character=b.kind==='villager'||b.kind==='npc';
    const type=b.kind==='npc'?'npc':b.kind==='villager'?'villager':resourceData.indexes.has(b)?'resource':'building';
    const index=character?b.index:type==='resource'?resourceData.indexes.get(b):b.objectIndex ?? buildings.indexOf(b);
    const centerX=px+(character?(b.index%3-1)*3:0);
    const baseY=py+(character?tileH*.65+(b.kind==='npc'?0:Math.floor(b.index/3)):tileH/2);
    const owner=world.towns[b.townId] || town;
    const simplifiedResource=tileW<32&&(b.kind==='tree'||b.kind==='rock');
    const height=simplifiedResource?tileH/(2*scale):b.kind==='tree'?(b.species==='ancient'?94:70):character?26:b.kind==='acre'?0:b.kind==='rock'?80:type==='building'?buildingHeight(b,owner)+(b.kind==='hall'?(owner?.level>=4?35:33):18):45;
    const halfWidth=simplifiedResource?tileW*.45:character?9*scale:type==='resource'?30*scale:b.kind==='barn'?tileW*.9:b.kind==='orchard_tree'||decorationKinds.includes(b.kind)?tileW*.65:b.kind==='sawpit'&&footprintTiles(b).length===1?tileW*.65:detailedBuildingKinds.includes(b.kind)&&footprintTiles(b).length===1?tileW*.8:tileW*.45;
    const groundPoints=type==='building'&&footprintTiles(b).length>1?footprintTiles(b).map(p=>point(p.x,p.y)):[];
    hitTargets.push({type,index,townId:b.townId || viewed,
      left:Math.min(centerX-halfWidth,...groundPoints.map(p=>p[0]-tileW/2)),right:Math.max(centerX+halfWidth,...groundPoints.map(p=>p[0]+tileW/2)),
      top:type==='resource'&&!simplifiedResource?py-80*scale:baseY-height*scale,bottom:Math.max(baseY+(character?5*scale:b.kind==='sawpit'?22*scale:detailedBuildingKinds.includes(b.kind)||b.kind==='barn'||decorationKinds.includes(b.kind)||b.kind==='orchard_tree'?18*scale:tileH/2),...groundPoints.map(p=>p[1]+tileH)),
      paint:target=>{
        if(character){drawVillager(b,false,target);return;}
        if(type==='building'){
          if(b.kind==='acre'&&!b.construction){
            polygon([[px,py+tileH*.1],[px+tileW*.4,py+tileH*.5],[px,py+tileH*.9],[px-tileW*.4,py+tileH*.5]],'#80764a','#ab9561',target);
          }else paintBuilding(b,owner,null,target);
          return;
        }
        if(simplifiedResource){
          polygon([[px,py+tileH*.25],[px+tileW*.45,py+tileH*.5],[px,py+tileH*.75],[px-tileW*.45,py+tileH*.5]],'#fff',null,target);return;
        }
        target.translate(px,py);target.scale(scale,scale);
        const willow=b.kind==='tree'&&(world.shared_map || town)?.terrain?.[b.y]?.[b.x]==='b';
        paintResource(target,b,willow,tileW>=32,resourceData.rockJoins.get(b));
      }});
    if(character&&selection?.type===type&&selection.index===b.index&&(!world.shared_map||b.townId===viewed||type==='npc')){
      ctx.fillStyle='#fff0b144';ctx.strokeStyle='#fff0b1';ctx.lineWidth=2;ctx.beginPath();ctx.ellipse(centerX,baseY,12*scale,6*scale,0,0,Math.PI*2);ctx.fill();ctx.stroke();
    }
    if(simplifiedResource)continue;
    if(['tree','rock','wild_horse','berry_bush','herb_patch','chest','village_hut','village_market','village_decor'].includes(b.kind)){drawResource(b,resourceData.rockJoins.get(b));continue;}
    if(character){
      drawVillager(b,selectedWorkers.has(b));
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
    const lines=[hoverBuilding.kind==='orchard_tree'?`Fruit tree · ${orchardTreeStatus(hoverBuilding)}`:hoverBuilding.kind==='acre'?`${labels.acre} · ${hoverBuilding.field?.stage || 'unplowed'}`:`${labels[hoverBuilding.kind]} · Level ${level} · Capacity tier ${hoverBuilding.settler_level || 1}`,upgrades.join(' · ')].filter(Boolean);
    if(hoverBuilding.construction)lines.push('Under construction');
    ctx.save();ctx.font=`bold ${11*scale}px sans-serif`;ctx.textAlign='center';ctx.fillStyle='#fff0b1';
    lines.forEach((line,i)=>{ctx.strokeStyle='#233b30';ctx.lineWidth=3*scale;const y=py-(buildingHeight(hoverBuilding,hoverTown)+45+i*14)*scale;ctx.strokeText(line,px,y);ctx.fillText(line,px,y);});ctx.restore();
  }
  if(preview && previewKind && town && town.terrain?.[preview.y]?.[preview.x]!=='v' && viewed===identity?.town_id){
    const valid=placementValid(town,preview,previewKind);
    ctx.save();ctx.globalAlpha=valid ? .65 : .35;
    if(previewKind==='road')drawRoad(preview);
    else if(tool==='move'&&previewKind==='farm'){
      const farm=town.buildings[moveIndex];
      const group=farmMoveGroup(town).map(b=>({...b,x:b.x+preview.x-farm.x,y:b.y+preview.y-farm.y}));
      for(const b of group.sort((a,b)=>(a.x+a.y)-(b.x+b.y))){
        const [px,py]=point(b.x,b.y);
        polygon([[px,py],[px+tileW/2,py+tileH/2],[px,py+tileH],[px-tileW/2,py+tileH/2]],'#00000000',valid?'#eee5ab':'#f3a295');
        drawBuilding(b,town);
      }
    }
    else drawBuilding(placementBuilding(preview,previewKind),town);
    ctx.restore();
  }
}
function orchardTreeStatus(tree){
  const state=tree.tree || {stage:'sapling',elapsed:0},duration=(world.progression?.orchard_stage_seconds || {sapling:60,blossom:30,fruit:60,harvested:30})[state.stage];
  const label=({sapling:'Sapling growing',blossom:'In blossom',fruit:'Fruit growing',ripe:'Ready to pick',harvested:'Resting after harvest'})[state.stage] || state.stage;
  return duration?`${label} · ${Math.ceil(Math.max(0,duration-(state.elapsed || 0)))}s remaining`:label;
}
function orchardPlacementValid(town,tile,index){
  const orchard=town.buildings[index];
  return orchard?.kind==='orchard'&&!orchard.construction&&Math.max(Math.abs(tile.x-orchard.x),Math.abs(tile.y-orchard.y))<=(world.progression?.orchard_reach ?? 3);
}
function acrePlacementValid(town,tile,index){
  const farm=town.buildings[index];
  return farm?.kind==='farm'&&!farm.construction&&Math.max(Math.abs(tile.x-farm.x),Math.abs(tile.y-farm.y))<=3;
}
const buildingFootprints={hall:[3,3],mill:[2,2],sawpit:[3,2],workshop:[2,2],butcher:[2,2],bakery:[2,2],smokehouse:[2,2],tannery:[2,2]};
function footprintTiles(building){
  const [width,depth]=building.footprint || [1,1],tiles=[];
  for(let dx=0;dx<width;dx++)for(let dy=0;dy<depth;dy++)tiles.push({x:building.x+dx,y:building.y+dy});
  return tiles;
}
function occupies(building,x,y){return footprintTiles(building).some(p=>p.x===x&&p.y===y);}
function placementBuilding(tile,kind){
  const moving=tool==='move'&&decorationKinds.includes(kind)?world.towns[viewed]?.buildings[moveIndex]:null;
  return {...tile,kind,...(moving?{orientation:decorationOrientation(moving)}:{}),footprint:(world.progression?.building_footprints || buildingFootprints)[kind] || [1,1]};
}
function gameplayRange(building,town){
  if(building.kind==='hall')return {reach:buildingRange(town),square:false};
  if(['farm','barn','orchard'].includes(building.kind))return {reach:3,square:true};
  if(['woodcutter','fishing_hut','mine'].includes(building.kind))return {reach:resourceReach(building),square:false};
  return null;
}
function rangeContains(building,tile,town){
  const range=gameplayRange(building,town);if(!range)return false;
  const dx=Math.abs(tile.x-building.x),dy=Math.abs(tile.y-building.y);
  return (range.square?Math.max(dx,dy):dx+dy)<=range.reach
    && (building.kind!=='hall'||!town.town_origin||Math.abs(tile.x-town.town_origin.x)+Math.abs(tile.y-town.town_origin.y)<=Math.max(15,range.reach));
}
function buildingRange(town){return 30+5*(town.level-1);}
function insideBuildingRange(town,tile){
  const hall=town.buildings.find(b=>b.kind==='hall');
  const origin=town.town_origin;
  return !!hall && Math.abs(tile.x-hall.x)+Math.abs(tile.y-hall.y)<=buildingRange(town)
    && (!origin || Math.abs(tile.x-origin.x)+Math.abs(tile.y-origin.y)<=Math.max(15,buildingRange(town)));
}
function farmMoveGroup(town){
  return town.buildings.filter((b,i)=>i===moveIndex||b.kind==='acre'&&b.farm_index===moveIndex||b.kind==='orchard_tree'&&b.orchard_index===moveIndex);
}
function placementValid(town,tile,kind,moving=null){
  if(!moving&&tool==='move'&&['farm','orchard'].includes(kind)&&town.buildings[moveIndex]?.kind===kind){
    const farm=town.buildings[moveIndex],group=farmMoveGroup(town);
    return (tile.x!==farm.x||tile.y!==farm.y)
      && group.every(b=>placementValid(town,{x:b.x+tile.x-farm.x,y:b.y+tile.y-farm.y},b.kind,group));
  }
  if(!moving&&tool==='move'&&moveIndex!==null){
    const selected=town.buildings[moveIndex];
    if(selected?.x===tile.x&&selected?.y===tile.y)return false;
    moving=selected?[selected]:null;
  }
  const proposed=placementBuilding(tile,kind),tiles=footprintTiles(proposed);
  const buildings=world.shared_map?Object.values(world.towns).flatMap(t=>t.buildings):town.buildings;
  const obstacles=buildings.filter(b=>!moving?.includes(b));
  return tiles.every(p=>p.x>=0&&p.y>=0&&p.x<mapSize(town)&&p.y<mapSize(town)
    && insideBuildingRange(town,p)&&town.terrain?.[p.y]?.[p.x]!=='v'
    && !buildings.some(b=>!moving?.includes(b)&&occupies(b,p.x,p.y))
    && !obstacles.some(b=>b.kind==='barn'&&rangeContains(b,p,town))
    && !(town.npcs || []).some(n=>n.x===p.x&&n.y===p.y)
    && !town.map_resources.some(r=>!['berry_bush','herb_patch'].includes(r.kind)&&r.x===p.x&&r.y===p.y)
    && (tiles.length===1||!mapRoads(town).some(r=>r.x===p.x&&r.y===p.y))
    && !(world.shared_map?Object.values(world.towns):[town]).some(t=>t.player&&Math.abs(t.player.x-p.x)<.68&&Math.abs(t.player.y-p.y)<.68))
    && (kind!=='barn'||!obstacles.some(b=>footprintTiles(b).some(p=>rangeContains(proposed,p,town))))
    && (kind!=='orchard_tree'||moving?.some(b=>b.kind==='orchard')||orchardPlacementValid(town,tile,tool==='move'?town.buildings[moveIndex]?.orchard_index:orchardIndex))
    && (moving||kind!=='acre'||acrePlacementValid(town,tile,tool==='move'?town.buildings[moveIndex]?.farm_index:acreFarmIndex))
    && (!world.shared_map || kind!=='hall' || !Object.values(world.towns).some(t=>t!==town&&t.buildings.some(b=>b.kind==='hall'&&Math.hypot(b.x-tile.x,b.y-tile.y)<40)))
    && (tool==='move' || town.sandbox || town.wood>=costs[kind]);
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
  const field=(state&&WorkerAnimation.fieldPresentation(state,b)) || b.field || {stage:'unplowed',elapsed:0,progress:0};
  const elapsed=field.elapsed || 0;
  const growthSeconds=world?.progression?.farming_growth_seconds || 15;
  const stage=['unplowed','plowed'].includes(field.stage)?0:field.stage==='ripe'?3:2;
  const progress=['sprouting','growing'].includes(field.stage)?elapsed/growthSeconds:0;
  const plantedCrop=field.crop || b.crop;
  const cropType=['vegetables','wheat','carrots','fodder'].includes(plantedCrop)?plantedCrop:'vegetables';
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
    if(field.stage!=='unplowed'){
      ctx.strokeStyle='#655638';ctx.lineWidth=scale;
      ctx.beginPath();ctx.moveTo(ax,ay+tileH/2);ctx.lineTo(bx,by+tileH/2);ctx.stroke();
    }
    for(let crop=0;crop<4;crop++){
      const order=row*4+crop;
      if(stage===0 || (stage===3&&order<(field.progress || 0)/2*16))continue;
      const x=b.x-.3+row*.2,y=b.y-.3+crop*.2;
      // Characters' feet sit .15 tile heights ahead of a crop's ground anchor.
      const stalk={kind:'crop',cropType,x,y,depth:x+y-.3,stage,
        height:field.stage==='sprouting'?1+progress*2:stage===2?3+progress*3:6,
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
  const large=footprintTiles(b).length>1,width=large?256:128,height=large?224:144,offsetX=width/2,offsetY=large?160:112;
  const bytes=width*height*resolution*resolution*4;
  const key=JSON.stringify([b.kind,b.footprint,buildingVisualLevel(b,town),b.kind==='hall'?town?.level || 1:buildingLevel(b),b.settler_level || 1,
    [...(b.profession_upgrades || [])].sort(),
    b.kind==='orchard_tree'?b.tree?.stage || 'sapling':null,
    decorationKinds.includes(b.kind)?decorationOrientation(b):null,usable,resolution,tileW>=32]);
  let sprite=buildingSprites.get(key);
  const [x,top]=point(b.x,b.y);
  if(!sprite && bytes<=buildingSpriteByteLimit && typeof document!=='undefined'){
    const surface=document.createElement('canvas'),target=surface.getContext?.('2d');
    if(target){
      while(buildingSprites.size>=128 || buildingSpriteBytes+bytes>buildingSpriteByteLimit){
        const oldest=buildingSprites.keys().next().value,evicted=buildingSprites.get(oldest);
        buildingSpriteBytes-=evicted.width*evicted.height*4;buildingSprites.delete(oldest);
      }
      surface.width=width*resolution;surface.height=height*resolution;
      target.setTransform(resolution/s,0,0,resolution/s,(offsetX-x/s)*resolution,(offsetY-top/s)*resolution);
      paintBuilding(b,town,null,target,usable);
      sprite=surface;buildingSprites.set(key,sprite);buildingSpriteBytes+=bytes;
    }
  }
  if(sprite){
    buildingSprites.delete(key);buildingSprites.set(key,sprite);
    ctx.drawImage(sprite,x-offsetX*s,top-offsetY*s,width*s,height*s);
  }else paintBuilding(b,town,null,ctx,usable);
  drawBuildingAnimation(b,town,farmState?.elapsed || 0);
}
function paintBuildingGround(b,target,paintPolygon){
  const tiles=footprintTiles(b);if(tiles.length===1)return;
  if(!b.construction&&(detailedBuildingKinds.includes(b.kind)||b.kind==='sawpit'))return;
  const s=tileW/46,[width,depth]=b.footprint;
  const corners=[[b.x-.5,b.y-.5],[b.x+width-.5,b.y-.5],[b.x+width-.5,b.y+depth-.5],[b.x-.5,b.y+depth-.5]]
    .map(([x,y])=>{const [px,py]=point(x,y);return [px,py+tileH/2];});
  paintPolygon(corners,b.kind==='hall'?'#929387':'#a48d66','#6c715c');
  for(const tile of tiles){
    const [x,y]=point(tile.x,tile.y);
    if(b.kind==='hall'||b.kind==='sawpit'||b.construction||tile.y===b.y)continue;
    const base=y+tileH/2;
    if(b.kind==='mill'){
      // Flour sacks and grain bins distinguish the mill yard from the wood trades.
      if(tile.x===b.x){
        paintPolygon([[x-12*s,base],[x,base+6*s],[x+12*s,base],[x,base-6*s]],'#ac895d','#68563e');
        target.fillStyle='#735439';target.fillRect(x-12*s,base-8*s,12*s,8*s);target.fillRect(x,base-2*s,12*s,8*s);
        paintPolygon([[x-12*s,base-8*s],[x,base-2*s],[x+12*s,base-8*s],[x,base-14*s]],'#d3bc83','#68563e');
      }else for(let i=0;i<3;i++){
        const px=x-8*s+i*8*s,py=base-(i%2?8:3)*s;
        target.fillStyle='#d3bc83';target.beginPath();target.ellipse(px,py,5*s,7*s,0,0,Math.PI*2);target.fill();
        target.strokeStyle='#9b7a4b';target.lineWidth=s;target.beginPath();target.moveTo(px-3*s,py-5*s);target.lineTo(px+3*s,py-5*s);target.stroke();
      }
    }else if(tile.x===b.x){
      // A perspective workbench, vise, and loose boards form a usable workspace.
      for(const [px,py] of [[x-12*s,base],[x+10*s,base+4*s]]){target.fillStyle='#735439';target.fillRect(px,py-6*s,3*s,12*s);}
      paintPolygon([[x-15*s,base-7*s],[x+7*s,base+4*s],[x+16*s,base],[x-6*s,base-11*s]],'#c4a477','#735439');
      paintPolygon([[x-7*s,base-10*s],[x+6*s,base-4*s],[x+8*s,base-6*s],[x-5*s,base-12*s]],'#e0bd84');
      target.fillStyle='#84958e';target.fillRect(x+7*s,base-7*s,6*s,4*s);
      target.strokeStyle='#d5dfd8';target.lineWidth=s;target.beginPath();target.moveTo(x+10*s,base-7*s);target.lineTo(x+10*s,base-11*s);target.stroke();
      if(b.kind==='workshop'){
        target.fillStyle='#735439';target.fillRect(x-14*s,base-20*s,3*s,24*s);target.fillRect(x+11*s,base-9*s,3*s,17*s);
        paintPolygon([[x-17*s,base-21*s],[x+7*s,base-9*s],[x+16*s,base-13*s],[x-8*s,base-25*s]],'#9b825a','#735439');
        target.strokeStyle='#c4a477';target.lineWidth=s;target.beginPath();target.moveTo(x-10*s,base-21*s);target.lineTo(x+10*s,base-11*s);target.stroke();
      }
    }else{
      // Timber has cut ends and tied stacks, rather than anonymous yard boxes.
      for(let i=0;i<3;i++){
        const py=base-i*4*s;
        paintPolygon([[x-13*s,py-5*s],[x+7*s,py+5*s],[x+13*s,py+2*s],[x-7*s,py-8*s]],'#b48a56','#735439');
        target.fillStyle='#dbb580';target.beginPath();target.ellipse(x+10*s,py+3*s,3*s,2*s,-.45,0,Math.PI*2);target.fill();
      }
      target.strokeStyle='#68563e';target.lineWidth=2*s;target.beginPath();target.moveTo(x-3*s,base-15*s);target.lineTo(x-3*s,base+1*s);target.stroke();
    }
  }
}
function paintSawpit(b,town,target,paintPolygon,usable){
  const [width,depth]=b.footprint || [1,1],level=buildingVisualLevel(b,town);
  const [x,top]=point(b.x+(width-1)/2,b.y+(depth-1)/2);
  const large=width*depth>1,fit=Math.min(width/3,depth/2),s=tileW/46,scale=(large ? .6*fit : .4)*s;
  // Keep the open saw bench silhouette, with broad timber and no surface seams.
  target.save();target.translate(x,top+tileH/2);target.scale(scale,scale);target.translate(large ? -110 : -100,-103.5);
  const poly=(points,fill)=>{
    paintPolygon(points,usable?fill:(fill==='#8e7752'?'#a34f49':fill));
  };
  const line=(points,color,weight=1)=>{
    target.beginPath();points.forEach(([px,py],i)=>i?target.lineTo(px,py):target.moveTo(px,py));target.strokeStyle=color;target.lineWidth=Math.max(2.8,weight);target.stroke();
  };
  const oval=(cx,cy,rx,ry,angle,fill)=>{
    target.beginPath();target.ellipse(cx,cy,rx,ry,angle,0,Math.PI*2);
    if(fill!=='none'){target.fillStyle=fill;target.fill();}
  };
  target.save();target.globalAlpha=0.18;
  poly([[45,99],[99,126],[161,95],[107,68]],'#735439');
  target.restore();
  // Rear timber shed: shaded back panel and low open-front half wall.
  poly([[95,108],[144,84],[144,57],[95,81]],'#847053');
  poly([[48,94],[95,117],[95,98],[48,75]],'#b69a70');
  poly([[48,75],[95,98],[95,102],[48,79]],'#c4a477');
  // Posts and diagonal braces use the same material scale as the bench.
  poly([[47,56],[47,99],[51,101],[51,58]],'#735439');
  poly([[94,79],[94,117],[98,115],[98,77]],'#735439');
  poly([[143,54],[143,92],[147,90],[147,52]],'#735439');
  line([[50,67],[62,62]],'#b48a56',2.5);
  line([[143,63],[133,68]],'#b48a56',2.5);
  // Tool rack and a warm lantern sit behind the cutting station.
  line([[107,78],[136,64]],'#594d38',2.5);
  line([[111,76],[111,84]],'#c4a477',1.6);
  line([[123,70],[123,78]],'#c4a477',1.6);
  line([[131,67],[131,75]],'#c4a477',1.6);
  line([[108,83],[114,80]],'#98aaa7',2);
  line([[120,76],[126,73]],'#98aaa7',2);
  line([[128,73],[134,70]],'#98aaa7',2);
  line([[54,60],[54,68],[59,70]],'#735439',1.8);
  poly([[54,68],[59,70],[59,77],[54,75]],'#edc77d');
  // A compact pitched roof finishes the shed without covering the bench.
  poly([[95,80],[154,50],[124,45]],'#b69a70');
  poly([[100,23],[154,50],[124,45],[70,18]],'#a58d66');
  poly([[41,53],[95,80],[124,45],[70,18]],'#8e7752');
  line([[41,53],[95,80],[154,50]],'#c9a579',2);
  line([[70,18],[124,45]],'#c4a477',1.5);
  // Materials have separate input and output silhouettes.
  if(large){
    target.save();target.translate(40,88);target.scale(42/44,32/34);
    poly([[2,15],[27,27],[32,21],[7,9]],'#a47c4e');
    oval(29,24,4,5,-25*Math.PI/180,'#dbb580');
    poly([[10,11],[35,23],[40,17],[15,5]],'#b48a56');
    oval(37,20,4,5,-25*Math.PI/180,'#dbb580');
    poly([[5,7],[30,19],[35,13],[10,1]],'#b48a56');
    oval(32,16,4,5,-25*Math.PI/180,'#dbb580');
    target.restore();
    target.save();target.translate(157,68);target.scale(37/40,31/34);
    for(let row=0;row<3+level;row++){
      const y=19-row*4;
      poly([[2,y],[26,y+12],[37,y+7],[13,y-5]],'#c4a477');
      poly([[26,y+12],[37,y+7],[37,y+10],[26,y+15]],'#e0bd84');
    }
    line([[18,10],[18,23]],'#68563e',1.6);
    target.restore();
  }
  // Foreground sawing bench on two braced trestles, not a floating table.
  target.save();target.globalAlpha=0.16;
  poly([[78,112],[122,134],[163,114],[119,92]],'#223b29');
  target.restore();
  poly([[85,92],[83,116],[87,118],[90,94]],'#735439');
  poly([[105,81],[108,104],[112,102],[110,79]],'#735439');
  poly([[128,112],[126,132],[130,130],[133,110]],'#735439');
  poly([[150,100],[152,119],[156,117],[154,98]],'#735439');
  line([[87,115],[108,85]],'#947044',2.4);
  line([[130,128],[152,102]],'#947044',2.4);
  poly([[80,87],[130,112],[154,100],[104,75]],'#c4a477');
  poly([[80,87],[130,112],[130,116],[80,91]],'#947044');
  poly([[130,112],[154,100],[154,104],[130,116]],'#735439');
  // Raised log, visible end grain and a cut aligned with the saw teeth.
  poly([[86,79],[143,107],[145,100],[88,72]],'#947044');
  poly([[88,72],[92,70],[149,98],[145,100]],'#c4a477');
  oval(146,103,4,5.5,-25*Math.PI/180,'#dbb580');
  line([[136,93],[131,99],[131,104]],'#594d38',1.5);
  // Four larger teeth remain visible in the normal-size preview.
  target.save();target.translate(108,99);target.rotate(-24*Math.PI/180);target.translate(-108,-99);
  poly([[114,94],[144,101],[141,108],[135,105],[132,109],[127,106],[124,110],[119,107],[115,110],[111,105]],(b.profession_upgrades || []).includes('saw')?'#e8efdf':'#c4d2ce');
  poly([[104,90],[114,94],[111,106],[102,102]],'#c4a477');
  poly([[106,95],[110,96],[108,102],[105,100]],'#594d38');
  target.restore();
  // Existing structural and capacity upgrades retain their separate wall plaques.
  target.fillStyle='#344b42';target.fillRect(133,66,15,10);
  target.fillStyle='#fff0b1';target.font='bold 8px sans-serif';target.textAlign='center';target.fillText(String(buildingLevel(b)),140,74);
  if((b.settler_level || 1)>1){
    target.fillStyle='#6baeb5';target.fillRect(134,81,14,5);
    target.fillStyle='#eee0bd';for(let i=0;i<b.settler_level;i++){target.beginPath();target.arc(137+i*3,83.5,1,0,Math.PI*2);target.fill();}
  }
  target.restore();
  target.fillStyle='#fff0b1';target.font=`${10*s}px sans-serif`;target.textAlign='center';
  target.fillText(labels.sawpit,x,top+tileH/2-(103.5-18)*scale-7*s);
}
function paintDetailedBuilding(b,town,target,paintPolygon,usable){
  // Broad unoutlined faces and a few chunky props match the scenery and workers.
  // Compact legacy sites retain their saved footprints and upgrade identities.
  const [width,depth]=b.footprint || [1,1];
  const fit=['home','woodcutter','farm','fishing_hut','fish_pond','mine','smelter','steelworks','storehouse','market','orchard','stable','saddler','tradehouse','well'].includes(b.kind)?1:Math.min(width,depth)/2;
  const [x,top]=point(b.x+(width-1)/2,b.y+(depth-1)/2),s=tileW/46;
  const level=buildingVisualLevel(b,town),rise=4*(level-1),upgrades=b.profession_upgrades || [];
  target.save();target.translate(x,top+tileH/2);target.scale(.46*s*fit,.46*s*fit);
  // Ground uses the game's 2:1 projection; doors use broad readable shapes.
  const p=(x,y,z=0)=>[(x-y)*25,(x+y)*12.5-z*1.5];
  const poly=(points,fill,warning='#a34f49')=>{
    paintPolygon(points,usable?fill:warning);
  };
  const line=(points,color='#735439',weight=2.4)=>{
    target.beginPath();points.forEach(([x,y],i)=>i?target.lineTo(x,y):target.moveTo(x,y));
    target.strokeStyle=color;target.lineWidth=Math.max(2.4,weight);target.stroke();
  };
  const oval=(x,y,rx,ry,fill)=>{
    target.beginPath();target.ellipse(x,y,rx,ry,0,0,Math.PI*2);
    if(fill){target.fillStyle=fill;target.fill();}
  };
  const face=(coords,fill,warning='#a34f49')=>poly(coords.map(c=>p(...c)),fill,warning);
  const edge=(a,end,color='#735439',weight=2.4)=>line([p(...a),p(...end)],color,weight);
  oval(3,6,65,22,'#223b293d');
  const walls=(x1=-.95,x2=.95,y1=-1.1,y2=.65,h=43+rise,light='#c6b28d',dark='#9b8b6c')=>{
    face([[x1,y2,0],[x2,y2,0],[x2,y2,h],[x1,y2,h]],light,'#c76d64');
    face([[x2,y1,0],[x2,y2,0],[x2,y2,h],[x2,y1,h]],dark);
    for(const x of [x1,x2])edge([x,y2,3],[x,y2,h],'#796347',3.4);
    edge([x2,y1,3],[x2,y1,h],'#6a6048',3.4);
    if(level>=2){
      edge([x1,y2,h*.68],[x2,y2,h*.68],'#796347',3.4);
      edge([x2,y1,h],[x2,(y1+y2)/2,h*.68],'#796347',3.4);
    }
    const stone=level>=3?13:6;
    face([[x1,y2,0],[x2,y2,0],[x2,y2,stone],[x1,y2,stone]],'#a2a497');
    face([[x2,y1,0],[x2,y2,0],[x2,y2,stone],[x2,y1,stone]],'#767f73');
    const door=x1+.22;
    face([[door,y2+.015,3],[door+.6,y2+.015,3],[door+.6,y2+.015,33],[door,y2+.015,33]],'#515342');
    face([[x2+.01,-.8,23],[x2+.01,-.25,23],[x2+.01,-.25,35],[x2+.01,-.8,35]],'#edc77d');
  };
  const roof=(x1=-1.1,x2=1.1,y1=-1.25,y2=.82,h=43+rise,peak=19,light='#a17e55',dark='#826642')=>{
    // A steep far slope projects behind the near slope in this 2:1 view.
    // Cap its rise by the span so both roof faces retain visible area.
    peak=Math.min(peak,(x2-x1)*25/6);
    const mid=(x1+x2)/2;
    face([[x1,y2,h],[x2,y2,h],[mid,y2,h+peak]],'#bdab8c');
    face([[x1,y1,h],[x1,y2,h],[mid,y2,h+peak],[mid,y1,h+peak]],light);
    face([[mid,y1,h+peak],[mid,y2,h+peak],[x2,y2,h],[x2,y1,h]],dark);
    if(level>=3&&!['home','hall'].includes(b.kind)){
      const bottom=h+peak*.2,top=h+peak*.7;
      face([[mid-.1,y2+.02,bottom],[mid+.1,y2+.02,bottom],[mid+.1,y2+.02,top],[mid-.1,y2+.02,top]],'#4b4a3a');
    }
    return peak;
  };
  const chimney=(z)=>{
    const x=.55,y=-.85;
    face([[x,y,z-7],[x+.24,y,z-7],[x+.24,y,z+14],[x,y,z+14]],'#a2a497');
    face([[x+.24,y-.25,z-7],[x+.24,y,z-7],[x+.24,y,z+14],[x+.24,y-.25,z+14]],'#767f73');
    face([[x-.04,y-.3,z+14],[x+.29,y-.3,z+14],[x+.29,y+.04,z+14],[x-.04,y+.04,z+14]],'#c0bdab');
  };
  const bench=(x,y,width=1.1,depth=.4,height=13)=>{
    for(const [dx,dy] of [[0,0],[width,0],[0,depth],[width,depth]])edge([x+dx,y+dy,0],[x+dx,y+dy,height],'#735439',2.2);
    face([[x,y,height],[x+width,y,height],[x+width,y+depth,height],[x,y+depth,height]],'#c4a477');
    face([[x,y+depth,height-3],[x+width,y+depth,height-3],[x+width,y+depth,height],[x,y+depth,height]],'#947044');
  };
  const lantern=(x,y,z)=>{
    const [a,b]=p(x,y,z);line([[a,b-7],[a,b],[a+4,b]],'#735439',1.2);
    poly([[a+2,b],[a+7,b+2],[a+7,b+9],[a+2,b+7]],'#edc77d');
  };
  const sack=(x,y)=>{
    const [a,b]=p(x,y);oval(a,b-7,6,10,'#d3bc83');
    poly([[a-3,b-14],[a-4,b-18],[a+3,b-18],[a+2,b-14]],'#d3bc83');
  };
  const tub=(x,y,water='#787956',rx=10,ry=5)=>{
    const [a,b]=p(x,y);
    poly([[a-rx,b-14],[a-rx+1,b],[a,b+4],[a+rx-1,b],[a+rx,b-14]],'#a98756');
    oval(a,b-14,rx,ry,'#b69a70');oval(a,b-14,rx-2,ry-1.5,water);
    line([[a-rx+1,b-4],[a,b],[a+rx-1,b-4]],'#677064',1.3);
  };
  const awning=(color)=>{
    for(const x of [-.1,.95])edge([x,1.45,0],[x,1.45,30],'#735439',1.8);
    face([[-.2,.68,36],[1.05,.68,36],[1.05,1.5,29],[-.2,1.5,29]],color);
  };
  const crate=(x,y)=>{
    face([[x,y,0],[x+.45,y,0],[x+.45,y+.35,0],[x,y+.35,0]],'#ac895d');
    face([[x,y+.35,0],[x+.45,y+.35,0],[x+.45,y+.35,9],[x,y+.35,9]],'#ac895d');
    face([[x+.45,y,0],[x+.45,y+.35,0],[x+.45,y+.35,9],[x+.45,y,9]],'#947044');
    face([[x,y,9],[x+.45,y,9],[x+.45,y+.35,9],[x,y+.35,9]],'#c4a477');
  };
  const boards=(x,y)=>{
    for(const z of [2,5,8,11]){
      face([[x,y,z],[x+.75,y,z],[x+.75,y+.4,z],[x,y+.4,z]],'#c4a477');
      face([[x,y+.4,z-2],[x+.75,y+.4,z-2],[x+.75,y+.4,z],[x,y+.4,z]],'#e0bd84');
    }
    edge([x+.36,y+.4,0],[x+.36,y+.4,13],'#68563e',1.5);
  };
  const shed=(light,dark,wide=false)=>{
    const [x1,x2,y1,y2]=wide?[-1.05,1.05,-1.1,.35]:[-1.1,.55,-1.1,.3],h=38+rise;
    face([[x2,y1,0],[x2,y2,0],[x2,y2,13],[x2,y1,13]],'#947e60');
    face([[x1,y1,0],[x2,y1,0],[x2,y1,h],[x1,y1,h]],'#746e53','#c76d64');
    for(const x of [x1,x2])edge([x,y2,0],[x,y2,h],'#9c885e',level>=2?4.5:3.8);
    edge([x1,y1,0],[x1,y1,h],'#735439',2);
    face([[x1,y2,0],[x1+.25,y2,0],[x1+.25,y2,15],[x1,y2,15]],'#b69a70');
    if(level>=2){
      edge([x1,y2,h-16],[x1+.55,y2,h-2],'#b48a56',2.3);
      edge([x2,y2,h-16],[x2-.55,y2,h-2],'#b48a56',2.3);
      edge([x2,y1,15],[x2,y2,29],'#947044',1.7);
      edge([x1,y2,h-2],[x2,y2,h-2],'#c4a477',2);
    }else{
      edge([x1,y2,29],[x1+.35,y2,36],'#947044',1.6);edge([x2,y2,29],[x2-.35,y2,36],'#947044',1.6);
    }
    if(level>=3){
      face([[x2,y1,0],[x2,y2,0],[x2,y2,12],[x2,y1,12]],'#858e86');
      face([[x1,y2,0],[x1+.25,y2,0],[x1+.25,y2,12],[x1,y2,12]],'#b1b3a2');
      for(const x of [x1,x2])edge([x,y2,3],[x,y2,9],'#84958e',3.8);
    }
    roof(x1-.15,x2+.15,y1-.15,y2+.15,h,17,light,dark);
    edge([x1+.2,y2-.1,27],[x2-.15,y2-.1,27],'#735439',2);
    for(const [i,x] of [x1+.45,x2-.4].entries()){
      edge([x,y2-.1,27],[x,y2-.1,17],'#c4a477',1.4);
      if(i===0)face([[x-.08,y2-.1,18],[x+.12,y2-.1,18],[x+.12,y2-.1,14],[x-.08,y2-.1,14]],'#98aaa7');
      else edge([x-.13,y2-.1,17],[x+.14,y2-.1,17],i===1?'#98aaa7':'#c4d2ce',i===1?2:1);
    }
    lantern(x1,y2+.02,30);
  };
  if(b.kind==='storehouse'){
    const warehouse=buildingLevel(b)>=5,h=(warehouse?46:36)+rise;
    walls(warehouse?-1.2:-1,warehouse?1.2:1,-1,.65,h,'#b9b49a','#87917d');
    roof(warehouse?-1.35:-1.15,warehouse?1.35:1.15,-1.18,.83,h,16,'#738e7c','#506c60');
    face([[-.75,.68,2],[.65,.68,2],[.65,.68,31],[-.75,.68,31]],'#455646');
    edge([-.05,.7,3],[-.05,.7,30],'#a9956c',3);
    if(warehouse){
      face([[-.62,.68,36],[.52,.68,36],[.52,.68,45],[-.62,.68,45]],'#e4ca8a');
      edge([-.05,.7,36],[-.05,.7,45],'#79836a',3);
    }
    for(let i=0;i<Math.min(4,1+Math.floor((buildingLevel(b)-1)/2));i++)crate(.85-i*.35,1.05+i*.16);
    sack(-1.3,.75);lantern(-1,.7,28);
  }else if(b.kind==='mill'){
    const h=57+rise;
    walls(-.7,.7,-.8,.55,h,'#c4b894','#969b80');
    roof(-.88,.88,-.97,.72,h,13,'#99a081','#707f69');
    // The live sails rotate around this visible axle, including compact sites.
    const [a,py]=p(.1,.7,48+rise);oval(a,py,5,5,'#655b41');
    bench(.92,-.35,.65,.55,17);sack(.98,.3);sack(1.27,.75);sack(.55,1.1);
    face([[-1.2,.7,0],[-.85,.7,0],[-.85,.7,19],[-1.2,.7,19]],'#a68b5d');
    face([[-1.2,.7,19],[-.85,.7,19],[-.85,1,19],[-1.2,1,19]],'#dbc58b');
    lantern(-.7,.6,27);
  }else if(b.kind==='market'){
    const h=28+rise;
    face([[-1.1,-.65,0],[1.1,-.65,0],[1.1,-.65,12],[-1.1,-.65,12]],'#8a805d');
    for(const x of [-1.1,1.1]){
      edge([x,.72,0],[x,.72,h],'#9f8a5e',4.5);
      if(level>=2)edge([x,.72,h-9],[x+(x<0?.4:-.4),.72,h],'#c6ae7e',3.8);
    }
    if(level>=3)for(const x of [-1.1,.9])face([[x,.74,0],[x+.2,.74,0],[x+.2,.74,8],[x,.74,8]],'#a8ae98');
    roof(-1.3,1.3,-.88,.95,h,13,'#97a57b','#64816a');
    for(const x of [-.8,0,.8])face([[x,.96,h],[x+.35,.96,h],[x+.35,.96,h-5],[x,.96,h-5]],'#eee0bd');
    bench(-1,.78,2,.45,14);
    for(const [x,color] of [[-.65,'#d7ba73'],[0,'#c38664'],[.65,'#88a36b']]){
      const [a,py]=p(x,1,17);oval(a,py,7,4,color);
    }
    crate(-1.3,.2);lantern(1.1,.75,24);
  }else if(b.kind==='orchard'){
    const h=33+rise;
    face([[-1.05,-.9,0],[.9,-.9,0],[.9,-.9,h],[-1.05,-.9,h]],'#92966d');
    for(const x of [-1.05,.9])edge([x,.5,0],[x,.5,h],'#a18e61',4);
    if(level>=2)edge([-.99,.5,h-11],[-.5,.5,h],'#c4ad77',3.5);
    if(level>=3)face([[-1.05,.52,0],[.9,.52,0],[.9,.52,8],[-1.05,.52,8]],'#a3ad92');
    roof(-1.2,1.05,-1.08,.68,h,14,'#8d9e68','#637d52');
    bench(-.8,.83,1.35,.45,14);
    for(const [x,y] of [[-.5,1.05],[.2,1.05],[1,.1],[1,.65]]){
      crate(x,y);const [a,py]=p(x+.2,y+.17,12);oval(a,py,5,3,'#ce9566');
    }
    tub(-1.2,.65,'#a2b473',8,4);lantern(-1.05,.55,27);
  }else if(b.kind==='stable'){
    const h=35+rise;
    face([[-1.1,-1,0],[1.1,-1,0],[1.1,-1,h],[-1.1,-1,h]],'#8a8564');
    face([[1.1,-1,0],[1.1,.6,0],[1.1,.6,h],[1.1,-1,h]],'#7a8060');
    for(const x of [-1.1,0,1.1])edge([x,.6,0],[x,.6,h],'#ad956b',4);
    if(level>=2)for(const x of [-1.1,.65])edge([x,.6,h-10],[x+.42,.6,h],'#c7af7e',3.4);
    if(level>=3)face([[-1.1,.62,0],[1.1,.62,0],[1.1,.62,8],[-1.1,.62,8]],'#a5ad97');
    for(const x of [-1,.1]){
      face([[x,.63,8],[x+.9,.63,8],[x+.9,.63,18],[x,.63,18]],'#a58e62');
      edge([x,.65,18],[x+.9,.65,18],'#d0b783',3.3);
    }
    roof(-1.27,1.27,-1.17,.8,h,15,'#8c9b79','#637c62');
    tub(.5,1.12,'#5d8a83',13,5);sack(-1.35,.6);lantern(-1.1,.65,27);
  }else if(b.kind==='saddler'){
    const h=35+rise;
    walls(-1.05,.65,-1,.5,h,'#bca17b','#95886b');
    roof(-1.2,.8,-1.17,.68,h,14,'#ae8c70','#806b58');
    bench(-.65,.88,1.2,.4,14);
    const [a,py]=p(-.03,1.05,23);
    poly([[a-11,py+1],[a-8,py-8],[a-2,py-6],[a+3,py-7],[a+9,py-2],[a+13,py],[a+7,py+5],[a+2,py+3],[a-3,py+7]],'#bc8c57');
    poly([[a-2,py-6],[a+3,py-7],[a+9,py-2],[a+3,py+1],[a-3,py]],'#dbb884');
    for(const y of [-.65,-.15])edge([.8,y,8],[.8,y,30],'#b68755',4);
    crate(1,.55);lantern(-1.05,.55,26);
  }else if(b.kind==='tradehouse'){
    const h=40+rise;
    walls(-1.1,1.05,-1,.6,h,'#c3b48f','#8c987d');
    roof(-1.25,1.2,-1.17,.78,h,16,'#7b9590','#536f70');
    awning('#c3ad78');bench(-.75,.95,1.5,.45,14);
    for(const [x,y] of [[-.55,1.12],[.2,1.12],[1.1,.2]]){
      crate(x,y);edge([x+.22,y+.36,0],[x+.22,y+.36,9],'#eee0bd',3);
    }
    edge([1.09,-.3,20],[1.09,.16,20],'#d7bc78',4);
    lantern(-1.1,.65,28);
  }else if(b.kind==='well'){
    oval(0,4,32,15,'#7d8c7c');oval(0,-7,32,15,'#b6bba5');oval(0,-7,24,10,'#477a80');
    for(const x of [-.75,.75])edge([x,0,0],[x,0,32],'#a58c60',4.5);
    edge([-.75,0,25],[.75,0,25],'#c5ad7c',5);
    edge([0,0,25],[0,0,6],'#d3bc88',2.6);
    roof(-.95,.95,-.65,.65,34,12,'#a78c61','#7d7454');
    tub(.9,.8,'#78a4a0',8,4);
  }else if(b.kind==='fishing_hut'){
    // A low blue roof, open fish counter and chunky net distinguish the shore trade.
    const h=34+rise;
    face([[-1.05,-1.1,0],[1.05,-1.1,0],[1.05,.8,0],[-1.05,.8,0]],'#a38c63');
    walls(-.9,.9,-1,.6,h,'#bcad87','#8e957e');
    roof(-1.1,1.1,-1.18,.78,h,16,'#78968d','#526f6c');
    bench(-.45,.92,1.1,.4,13);
    for(const x of [-.15,.4]){
      const [a,b]=p(x,1.1,16);
      poly([[a-6,b],[a,b-3],[a+5,b],[a+9,b-3],[a+9,b+3],[a+5,b+1],[a,b+3]],'#c7d5ba');
    }
    tub(-1.2,.6,'#597e7b',9,4);lantern(-.9,.65,27);
    const net=upgrades.includes('nets')||upgrades.includes('wool_nets');
    if(net){
      const color=upgrades.includes('wool_nets')?'#eee1bc':'#b7c8b0';
      for(const y of [-.65,-.05])edge([1,y,3],[1,y,33],'#9c885e',3.6);
      face([[1,-.65,7],[1,-.05,7],[1,-.05,29],[1,-.65,29]],'#647d6b');
      for(const y of [-.65,-.35,-.05])edge([1.02,y,7],[1.02,y,29],color,2.8);
      for(const z of [7,18,29])edge([1.02,-.65,z],[1.02,-.05,z],color,2.8);
    }
    const boatTier=fishingBoatLevel(b);
    if(boatTier){
      const [a,py]=p(.1,.65,27);
      target.fillStyle=['','#c29a63','#ece3c5','#d4af6b'][boatTier];target.fillRect(a-5,py-5,10,10);
      target.fillStyle='#493627';target.font='bold 8px sans-serif';target.textAlign='center';target.fillText(String(boatTier),a,py+3);
    }
  }else if(b.kind==='fish_pond'){
    // The water is the main silhouette; the feed rack stays low and open.
    for(const x of [-1.05,.15])edge([x,-.75,0],[x,-.75,26+rise],'#9c885e',4);
    roof(-1.2,.3,-1.1,-.4,26+rise,10,'#78968d','#526f6c');
    sack(-1.15,-.3);
    oval(0,11,51,24,'#8b9580');oval(0,7,51,22,level>=3?'#b7baa5':'#a3aa91');
    oval(0,7,43,16,'#477d86');oval(-5,4,33,10,'#62999a');
    for(const [a,py] of [[-15,7],[16,11]])poly([[a-8,py],[a,py-4],[a+7,py],[a+12,py-4],[a+12,py+4],[a+7,py],[a,py+4]],'#dfbb78');
    if(level>=2)edge([-1.15,.8,4],[.8,.8,4],'#bca47b',4);
    lantern(-1.05,-.7,23+rise);
  }else if(b.kind==='mine'){
    // Broad rock faces match nearby boulders; the dark adit reads without a roof.
    const h=38+rise;
    face([[-1.3,.7,0],[1.1,.7,0],[1.1,.7,h-10],[.05,.7,h],[-1.1,.7,h-7]],'#929b88');
    face([[1.1,-1,0],[1.1,.7,0],[1.1,.7,h-10],[1.1,-.5,h-6]],'#707e70');
    face([[-1.1,.7,h-7],[.05,.7,h],[.4,-.65,h+2],[1.1,-.5,h-6],[1.1,.7,h-10]],'#b3b8a1');
    face([[-.85,.72,0],[.45,.72,0],[.45,.72,25],[-.2,.72,32],[-.85,.72,25]],'#354438');
    for(const x of [-.85,.45])edge([x,.75,0],[x,.75,29],'#ad9569',5);
    edge([-.96,.75,29],[.56,.75,29],'#c1a575',5.5);
    if(level>=2){
      edge([-.85,.76,20],[-.5,.76,29],'#c1a575',4);edge([.45,.76,20],[.1,.76,29],'#c1a575',4);
    }
    if(level>=3)for(const x of [-1.1,.65])face([[x,.78,0],[x+.25,.78,0],[x+.25,.78,13],[x,.78,13]],'#b7baa5');
    for(const x of [-.65,.25])edge([x,.8,0],[x,1.55,0],'#7b8477',3);
    const [a,py]=p(1.1,-.2,2);
    poly([[a-11,py-12],[a+9,py-12],[a+7,py],[a-9,py]],'#7f7353');
    poly([[a-10,py-12],[a-5,py-20],[a+1,py-17],[a+6,py-22],[a+10,py-12]],'#b38c61');
    for(const dx of [-6,6])oval(a+dx,py+1,3,3,'#465548');
    edge([-1.23,.4,0],[-1.12,.4,25],'#b39b70',3.4);
    const [pickX,pickY]=p(-1.12,.4,25);
    const steelPick=upgrades.includes('steel_pickaxe');
    poly([[pickX-9,pickY+2],[pickX-4,pickY-4],[pickX+7,pickY-4],[pickX+(steelPick?16:12),pickY+1],[pickX+3,pickY-1],[pickX-3,pickY-1]],steelPick?'#e0e9d5':'#9caca0');
    lantern(-.85,.8,25);
  }else if(b.kind==='smelter'||b.kind==='steelworks'){
    const steel=b.kind==='steelworks',h=(steel?38:32)+rise;
    // A sheltered anvil for the iron trade; a larger enclosed furnace for steel.
    if(steel){
      walls(-1,.8,-1,.6,h,'#a5aa96','#788574');
      roof(-1.15,.95,-1.15,.78,h,14,'#718a86','#4d6c6b');
    }else{
      walls(-1.1,-.05,-1,.6,h,'#b5a17d','#89866d');
      roof(-1.25,.1,-1.15,.78,h,14,'#a28c60','#7e7452');
    }
    const front=.65,x1=steel?-.5:.15,x2=steel?.65:1.2;
    face([[x1,front,0],[x2,front,0],[x2,front,30+rise],[x1,front,30+rise]],'#9fa48e');
    face([[x2,-.8,0],[x2,front,0],[x2,front,30+rise],[x2,-.8,30+rise]],'#707e70');
    face([[x1,-.8,30+rise],[x2,-.8,30+rise],[x2,front,30+rise],[x1,front,30+rise]],'#b4b7a0');
    face([[x1+.14,front+.02,3],[x2-.14,front+.02,3],[x2-.14,front+.02,20],[x1+.14,front+.02,20]],'#3c4638');
    face([[x1+.23,front+.03,4],[x2-.23,front+.03,4],[x2-.23,front+.03,10],[x1+.23,front+.03,15]],'#dcaa5e');
    chimney((steel?51:43)+rise);
    if(steel){
      edge([-.8,-.55,h-1],[-.8,-.55,h+17],'#7c8879',9);
      crate(1,-.2);
    }else{
      bench(-1.1,.95,.8,.4,12);
      const [a,py]=p(-.7,1.15,19);
      poly([[a-10,py-6],[a+9,py-6],[a+5,py-2],[a+2,py-2],[a+2,py+4],[a+7,py+5],[a-7,py+5],[a-2,py+2],[a-2,py-2],[a-7,py-2]],'#5e7168');
    }
    for(let i=0;i<level;i++){
      const [a,py]=p(.9,1.1,3+i*5);
      poly([[a-7,py],[a+4,py],[a+7,py-4],[a-4,py-4]],steel?'#a9bcb5':'#a6ab94');
      poly([[a-7,py],[a+4,py],[a+4,py+3],[a-7,py+3]],'#73867b');
    }
    lantern(-1,.65,27);
  }else if(b.kind==='butcher'){
    walls(-.95,.95,-1.1,.65,43+rise,'#c2aa90','#99816c');
    roof(-1.1,1.1,-1.25,.82,43+rise,19,'#aa7a78','#855d64');
    face([[-.02,.67,13],[.79,.67,13],[.79,.67,30],[-.02,.67,30]],'#473f32');
    awning('#a57670');bench(-.02,1,1.02,.42,14);
    for(const x of [.2,.72]){const [a,b]=p(x,1.19,16);oval(a,b,5,3.2,'#b66e65');}
    const [a,bottom]=p(-1.2,1.05);
    poly([[a-8,bottom-18],[a-8,bottom],[a,bottom+4],[a+8,bottom],[a+8,bottom-18]],'#947044');
    oval(a,bottom-18,8,4,'#dbb580');
    line([[a+2,bottom-19],[a+2,bottom-29]],'#735439',1.8);
    poly([[a+2,bottom-28],[a+9,bottom-28],[a+9,bottom-23],[a+2,bottom-23]],'#c4d2ce');
    const [signX,signY]=p(1.03,-.15,34);
    line([[signX,signY],[signX+12,signY-6],[signX+12,signY+2]],'#735439',1.3);
    poly([[signX+6,signY+2],[signX+19,signY-4],[signX+19,signY+9],[signX+6,signY+15]],'#674c33');
    poly([[signX+10,signY+5],[signX+16,signY+2],[signX+16,signY+6],[signX+10,signY+9]],'#c4d2ce');
    lantern(-.95,.7,30);
  }else if(b.kind==='bakery'){
    walls(-.95,.95,-1.1,.65,43+rise,'#d0bb95','#ac9574');
    roof(-1.1,1.1,-1.25,.82,43+rise,19,'#b48a56','#946d48');chimney(57+rise);
    face([[1,-.35,0],[1.6,-.35,0],[1.6,.45,0],[1,.45,0]],'#8b8976');
    face([[1,.45,0],[1.6,.45,0],[1.6,.45,21],[1.3,.45,28],[1,.45,21]],'#b5a486');
    face([[1.6,-.35,0],[1.6,.45,0],[1.6,.45,21],[1.6,-.35,21]],'#8d8976');
    face([[1,-.35,21],[1.3,-.35,28],[1.3,.45,28],[1,.45,21]],'#c3b496');
    face([[1.3,-.35,28],[1.6,-.35,21],[1.6,.45,21],[1.3,.45,28]],'#a3947c');
    const [a,bottom]=p(1.3,.47,3);
    poly([[a-5,bottom+2],[a-5,bottom-13],[a,bottom-18],[a+5,bottom-13],[a+5,bottom+2]],'#463b32');
    poly([[a-3,bottom],[a-1,bottom-9],[a+1,bottom-4],[a+3,bottom-12],[a+4,bottom]],'#e7af5d');
    for(const [i,key] of ['larger_oven','double_hearth_oven','masterwork_oven'].entries())if(upgrades.includes(key)){
      edge([1.61,-.25,5+i*5],[1.61,.35,5+i*5],'#edc77d',1.5);
    }
    awning('#bca67b');bench(-.12,1,1.12,.42,13);
    for(const [x,y] of [[.05,1.2],[.47,1.2],[.89,1.2]]){
      const [a,b]=p(x,y,15);oval(a,b,5,3.3,'#d9b36b');
    }
    sack(-1.32,.65);
    edge([1.6,.83,1],[1.35,.83,30],'#c4a477',1.7);
    face([[1.2,.83,29],[1.5,.83,29],[1.5,.83,36],[1.2,.83,36]],'#d3bc83');lantern(-.95,.7,30);
  }else if(b.kind==='smokehouse'){
    walls(-.95,.95,-1.1,.65,49+rise,'#a2977e','#7f7d6b');
    roof(-1.1,1.1,-1.25,.82,49+rise,19,'#7d8883','#596d69');chimney(63+rise);
    face([[-.21,.67,5],[.7,.67,5],[.7,.67,39],[-.21,.67,39]],'#554e40');
    for(const z of [18,25,32])edge([-.15,.69,z],[.63,.69,z],'#95866a',1.4);
    face([[.03,.7,7],[.47,.7,7],[.47,.7,12],[.03,.7,12]],'#332e27');edge([.12,.72,8],[.37,.72,8],'#c68b4d',1);
    for(const x of [-1.2,.25]){edge([x,1.3,0],[x,1.3,32],'#735439',2);edge([x,1.3,4],[x+.19,1.3,12],'#b48a56',1.3);}
    edge([-1.3,1.3,32],[.35,1.3,32],'#c4a477',2);
    for(const x of [-.93,-.5,-.07]){
      const [a,b]=p(x,1.3,27);line([[a,b-7],[a,b-3]],'#d1b37c',2.4);
      poly([[a,b-4],[a+3,b],[a+1,b+6],[a+3,b+9],[a-1,b+8],[a-3,b+9],[a-2,b+5],[a-3,b]],'#b28c75');
    }
    for(let i=0;i<3;i++){
      const [a,b]=p(1.4,-.35+i*.3);poly([[a-6,b-3],[a+4,b+2],[a+6,b-2],[a-4,b-7]],'#947044');oval(a+5,b,2.7,3,'#dbb580');
    }
    tub(1.25,.85,'#a39479',7,3.5);lantern(-.95,.7,34);
  }else if(b.kind==='tannery'){
    walls(-1.3,-.05,-1.1,.45,34+rise,'#bda482','#947e60');
    roof(-1.45,.1,-1.25,.6,34+rise,16,'#a99066','#817052');
    for(const x of [.4,1.55])edge([x,-.12,0],[x,-.12,47],'#735439',2.5);
    edge([.35,-.12,46],[1.6,-.12,46],'#c4a477',2.1);edge([.4,-.12,10],[1.55,-.12,10],'#947044',1.5);
    const [a,b]=p(.98,-.12,28);
    poly([[-8,-12],[-3,-9],[0,-12],[3,-9],[8,-12],[6,-4],[9,9],[3,7],[0,11],[-3,7],[-9,9],[-6,-4]].map(([x,y])=>[a+x*1.12,b+y*1.12]),'#bc8a57');
    for(const [dx,dy,ex,ey] of [[-9,-12,-14,-27],[9,-12,14,-13],[-9,9,-14,9],[9,9,14,23]])line([[a+dx,b+dy],[a+ex,b+ey]],'#e4cea4',.7);
    bench(-1.15,.78,.66,.35,13);
    face([[-1.12,.83,17],[-.52,.83,17],[-.52,1.05,12],[-1.12,1.05,12]],'#c69a64');edge([-.96,.94,17],[-.73,.94,19],'#c4d2ce',1.5);
    tub(.3,1.15,'#777a56',12,5.5);tub(1.25,.88,'#8c7755',10,5);
    edge([.35,1.18,4],[.45,1.18,26],'#c4a477',1.6);lantern(-1.3,.5,25);
  }else if(b.kind==='woodcutter'){
    shed('#8e8860','#706f4f');
    for(const [x,y] of [[.85,-.35]])for(const [dx,z] of [[0,3],[.34,3],[.17,8]]){
      face([[x+dx,y,z],[x+dx+.7,y,z],[x+dx+.7,y+.19,z],[x+dx,y+.19,z]],'#947044');
      const [a,b]=p(x+dx+.7,y+.09,z);oval(a,b,4,4.8,'#dbb580');
    }
    const [a,bottom]=p(-1.27,.85);
    poly([[a-8,bottom-18],[a-8,bottom],[a,bottom+4],[a+8,bottom],[a+8,bottom-18]],'#947044');
    oval(a,bottom-18,8,4,'#dbb580');
    edge([-1.25,.85,13],[-1.17,.85,30],'#c4a477',1.8);
    face([[-1.17,.85,30],[-.85,.85,33],[-.85,.85,25],[-1.17,.85,26]],upgrades.includes('axe')?'#e8efdf':'#c4d2ce');
    edge([.5,.75,0],[.5,.75,26],'#c4a477',1.7);
    face([[.5,.75,26],[.76,.75,29],[.76,.75,21],[.5,.75,22]],upgrades.includes('tools')?'#e8efdf':'#c4d2ce');
  }else if(b.kind==='workshop'){
    shed('#7f9990','#5d7c74',true);boards(1,-.7);crate(1,.1);bench(-.65,.85,1.45,.48,15);
    face([[-.43,.91,17],[.49,.91,17],[.49,1.12,17],[-.43,1.12,17]],'#e0bd84');
    face([[.48,1.1,16],[.72,1.1,16],[.72,1.1,23],[.48,1.1,23]],'#84958e');
    edge([.6,1.1,19],[.6,1.1,28],'#c4d2ce',1);edge([-.23,1.12,18],[0,1.12,21],'#735439',1.8);
    const [a,py]=p(-.1,1.1,20);poly([[a-3,py-2],[a+3,py-2],[a+4,py+2],[a-2,py+2]],'#98aaa7');
    const [cx,cy]=p(.86,1.42,6);oval(cx,cy-2,5,3,'#b48a56');oval(cx,cy-4,5,2.5,'#e0bd84');
  }else if(b.kind==='farm'){
    walls(-.95,.95,-1.1,.65,37+rise,'#c6b080','#a68f65');
    roof(-1.1,1.1,-1.25,.82,37+rise,18,'#b49b60','#958049');
    face([[-.08,.67,13],[.78,.67,13],[.78,.67,29],[-.08,.67,29]],'#5a583c');bench(-.07,.85,.9,.38,13);
    for(const [x,color] of [[.08,'#8da962'],[.3,'#d9bd70'],[.55,'#cc8962']]){const [a,b]=p(x,1.05,15);oval(a,b,3,2.3,color);}
    sack(-1.27,.45);sack(-1.22,.92);crate(1.08,.15);
    for(const y of [-.12,.12])edge([1.05,y,0],[1.05,y,30],'#c4a477',1.5);
    edge([1.05,-.32,30],[1.05,.09,30],'#98aaa7',1.4);
    for(const y of [-.25,-.03])edge([1.05,y,30],[1.05,y,26],'#98aaa7',.8);
    tub(.75,1.24,'#bba475',7,3.4);lantern(-.95,.7,29);
  }else if(b.kind==='home'){
    const h=34+3*(level-1),timber=upgrades.includes('timber_frame');
    const light=level===1?'#c4ae86':level===2?'#cbb68e':level===3?'#d1bea0':'#b8bcab';
    const dark=level===1?'#a18c69':level===2?'#a99572':level===3?'#b2a285':'#8f9a8d';
    walls(-.95,.95,-1.05,.65,h,light,dark);
    if(timber){
      for(const x of [-.95,0,.95])edge([x,.65,6],[x,.65,h],'#796347',3.2);
      edge([-.95,.65,h*.65],[.95,.65,h*.65],'#796347',3);
      edge([.95,-1,12],[.95,-.4,25],'#674c33',2);edge([.95,-1,25],[.95,-.4,12],'#674c33',2);
    }
    if(level>=3){
      const stone=level===3?12:h;
      face([[-.95,.65,0],[.95,.65,0],[.95,.65,stone],[-.95,.65,stone]],'#b8bcab');
      face([[.95,-1.05,0],[.95,.65,0],[.95,.65,stone],[.95,-1.05,stone]],'#8f9a8d');
      face([[-.73,.67,3],[-.13,.67,3],[-.13,.67,33],[-.73,.67,33]],'#594d38');
      face([[.96,-.8,23],[.96,-.4,23],[.96,-.4,33],[.96,-.8,33]],'#edc77d');
      if(timber){edge([.97,-1,12],[.97,-.4,25],'#674c33',2);edge([.97,-1,25],[.97,-.4,12],'#674c33',2);}
    }
    face([[.2,.67,16],[.72,.67,16],[.72,.67,28],[.2,.67,28]],'#edc77d');
    roof(-1.1,1.1,-1.2,.81,h,18,level<3?'#aa7b62':'#986d58',level<3?'#855c4d':'#795849');
    face([[.53,-.78,h+9],[.75,-.78,h+9],[.75,-.78,h+25],[.53,-.78,h+25]],'#b1b3a2');
    face([[.75,-1,h+9],[.75,-.78,h+9],[.75,-.78,h+25],[.75,-1,h+25]],'#858e86');
    face([[.49,-1.04,h+25],[.79,-1.04,h+25],[.79,-.74,h+25],[.49,-.74,h+25]],'#c8c7b4');
    if(level>=3){
      face([[-.5,.83,h-4],[.1,.83,h-4],[-.2,.83,h+7]],'#cbb68c');
      face([[-.58,.86,h-4],[-.2,.86,h+9],[.18,.86,h-4]],'#986d58');
      face([[-.3,.87,h-3],[-.1,.87,h-3],[-.1,.87,h+2],[-.3,.87,h+2]],'#edc77d');
    }
    face([[-.86,.78,.1],[-.18,.78,.1],[-.18,1.03,.1],[-.86,1.03,.1]],'#b8bcab');lantern(-.95,.7,29);
    const [a,py]=p(.6,1);oval(a,py,4.5,2.2,'#947044');oval(a,py-3,4,3.2,'#78945c');
  }else if(b.kind==='hall'){
    const h=43+3*(level-1),x1=-1.3,x2=1.3,y1=-1.2,y2=.8;
    face([[x1,y2,0],[x2,y2,0],[x2,y2,h],[x1,y2,h]],level<3?'#c5b08b':'#c6bca2');
    face([[x2,y1,0],[x2,y2,0],[x2,y2,h],[x2,y1,h]],level<3?'#9a896c':'#a39d87');
    const stone=level<3?7:14;
    face([[x1,y2,0],[x2,y2,0],[x2,y2,stone],[x1,y2,stone]],'#aeb1a0');
    face([[x2,y1,0],[x2,y2,0],[x2,y2,stone],[x2,y1,stone]],'#7f8d7c');
    for(const x of [x1,x2])edge([x,y2,5],[x,y2,h],'#796347',3.2);
    edge([x1,y2,h*.63],[x2,y2,h*.63],'#796347',3.2);
    if(level>=2){edge([x1,y2,h],[-.4,y2,h*.63],'#947044',1.6);edge([.4,y2,h*.63],[x2,y2,h],'#947044',1.6);}
    for(const y of [-1.1,.7])edge([x2,y,6],[x2,y,h],'#735439',1.1);
    face([[-.5,.82,3],[.22,.82,3],[.22,.82,34],[-.5,.82,34]],'#515342');
    for(const x of [-.96,.63]){
      face([[x,.82,22],[x+.3,.82,22],[x+.3,.82,32],[x,.82,32]],'#edc77d');
    }
    for(const y of [-.8,0]){
      face([[1.31,y,22],[1.31,y+.3,22],[1.31,y+.3,32],[1.31,y,32]],'#edc77d');
    }
    const roofColors=[['#7f9795','#5a747b'],['#748e91','#52767b'],['#6c8990','#476975'],['#637c8b','#405c70'],['#596f83','#374e65']][level-1];
    const roofPeak=roof(-1.47,1.47,-1.36,.96,h,23,...roofColors);
    edge([0,-.85,h+roofPeak],[0,-.85,h+42],'#c4a477',1.5);face([[0,-.85,h+42],[.65,-.85,h+38],[0,-.85,h+32]],'#d6c478');
    face([[-.65,.9,0],[.4,.9,0],[.4,1.18,0],[-.65,1.18,0]],'#aeb1a0');
    face([[-.7,1.15,0],[.45,1.15,0],[.45,1.43,0],[-.7,1.43,0]],'#c0c1ae');lantern(-1.3,.85,31);
    if(level>=3){
      face([[-.5,.98,h-3],[.3,.98,h-3],[-.1,.98,h+9]],'#cbb68c');face([[-.6,1,h-3],[-.1,1,h+11],[.4,1,h-3]],roofColors[1]);
      face([[-.2,1.01,h-2],[0,1.01,h-2],[0,1.01,h+3],[-.2,1.01,h+3]],'#edc77d');
    }
    if(level>=4){
      face([[.58,-.55,h+9],[1.03,-.55,h+9],[1.03,-.55,h+41],[.58,-.55,h+41]],'#b1b3a2');
      face([[1.03,-1,h+9],[1.03,-.55,h+9],[1.03,-.55,h+41],[1.03,-1,h+41]],'#858e86');
      roof(.48,1.13,-1.1,-.45,h+41,10,...roofColors);
      face([[.7,-.54,h+29],[.9,-.54,h+29],[.9,-.54,h+36],[.7,-.54,h+36]],'#473f32');
      if(level===5){
        face([[1.32,-.9,9],[1.32,-.62,9],[1.32,-.62,37],[1.32,-.9,37]],'#d6c478');
        for(const y of [-.4,-.1,.2])edge([1.33,y,11],[1.33,y,14],'#d6c478',1.7);
      }
    }
  }
  // Keep structural and worker-capacity upgrades independent, as on the barn.
  const plaqueAnchor={fish_pond:[-.25,-.75],tannery:[-.04,-.05],woodcutter:[.56,-.05],hall:[1.32,-.05],
    market:[1.1,.72],orchard:[.9,.5],mill:[.71,-.05],saddler:[.66,-.05]}[b.kind] || [.97,-.05];
  const [plaqueX,plaqueY]=p(...plaqueAnchor,20);
  if(b.kind!=='well'){
    target.fillStyle='#344b42';target.fillRect(plaqueX-5,plaqueY-7,13,9);
    target.fillStyle='#fff0b1';target.font='bold 7px sans-serif';target.textAlign='center';target.fillText(String(b.kind==='hall'?town?.level || 1:buildingLevel(b)),plaqueX+1.5,plaqueY);
    if((b.settler_level || 1)>1){
      target.fillStyle='#6baeb5';target.fillRect(plaqueX-5,plaqueY+4,14,6);
      target.fillStyle='#eee0bd';for(let i=0;i<b.settler_level;i++){target.beginPath();target.arc(plaqueX-2+i*3,plaqueY+7,1,0,Math.PI*2);target.fill();}
    }
  }
  target.restore();
  if(tileW>=32){
    target.save();target.fillStyle='#fff0b1';target.font=`${10*s}px sans-serif`;target.textAlign='center';
    target.fillText(labels[b.kind],x,top+tileH/2-(buildingHeight(b,town)+7)*s);target.restore();
  }
}
function paintBarn(b,town,target,paintPolygon,usable){
  const s=tileW/46,level=buildingVisualLevel(b,town),rise=4*(level-1);
  const p=(x,y,z=0)=>{const [px,py]=point(b.x+x,b.y+y);return [px,py+tileH/2-z*s];};
  const poly=(coords,fill)=>{paintPolygon(coords.map(c=>p(...c)),usable?fill:'#a34f49');};
  const line=(a,end,color,width=1)=>{const [x,y]=p(...a),[ex,ey]=p(...end);target.beginPath();target.moveTo(x,y);target.lineTo(ex,ey);target.strokeStyle=color;target.lineWidth=Math.max(1.6,width)*s;target.stroke();};
  target.save();
  const [shadowX,shadowY]=p(0,0);target.fillStyle='#223b293d';target.beginPath();target.ellipse(shadowX+3*s,shadowY+4*s,28*s,11*s,0,0,Math.PI*2);target.fill();
  poly([[-.6,.6,0],[.6,.6,0],[.6,.6,35+rise],[-.6,.6,35+rise]],'#bdab8c');
  poly([[.6,-.6,0],[.6,.6,0],[.6,.6,35+rise],[.6,-.6,35+rise]],'#8b8069');
  const stone=8+2*(level-1);
  poly([[-.6,.6,0],[.6,.6,0],[.6,.6,stone],[-.6,.6,stone]],'#a2a497');
  poly([[.6,-.6,0],[.6,.6,0],[.6,.6,stone],[.6,-.6,stone]],'#767f73');
  for(const [x,y] of [[-.6,.6],[.6,.6],[.6,-.6]])line([x,y,2],[x,y,36+rise],'#604d36',2);
  poly([[-.35,.612,0],[.35,.612,0],[.35,.612,24],[-.35,.612,24]],'#633f34');
  line([0,.62,0],[0,.62,24],'#e3cba1');
  for(const [a,end] of [[-.35,0],[0,.35]])line([a,.63,23],[end,.63,1],'#d6bd90');
  const roofPeak=44+rise;
  poly([[-.6,.6,35+rise],[.6,.6,35+rise],[0,.6,roofPeak-1.5]],'#bdab8c');
  poly([[-.75,-.76,35+rise],[-.75,.76,35+rise],[0,.76,roofPeak],[0,-.76,roofPeak]],'#6b8076');
  poly([[0,-.76,roofPeak],[0,.76,roofPeak],[.75,.76,35+rise],[.75,-.76,35+rise]],'#52685f');
  poly([[-.13,.64,34+rise],[.13,.64,34+rise],[.13,.64,40+rise],[-.13,.64,40+rise]],'#403f30');
  poly([[.42,.67,16],[.58,.67,16],[.58,.67,23],[.42,.67,23]],'#edc77d');
  for(const x of [-.8,-1.1]){
    poly([[x,.2,0],[x+.25,.2,0],[x+.25,.6,0],[x,.6,0]],'#aa8847');
    poly([[x,.2,7],[x+.25,.2,7],[x+.25,.6,7],[x,.6,7]],'#d6b765');
    poly([[x,.6,0],[x+.25,.6,0],[x+.25,.6,7],[x,.6,7]],'#bea15d');
  }
  const [x,y]=p(.6,-.2,22);
  target.fillStyle='#344b42';target.fillRect(x-4*s,y-6*s,9*s,6*s);
  target.fillStyle='#fff0b1';target.font=`bold ${5*s}px sans-serif`;target.textAlign='center';target.fillText(String(buildingLevel(b)),x+.5*s,y-1*s);
  if((b.settler_level || 1)>1){
    target.fillStyle='#6baeb5';target.fillRect(x-4*s,y+2*s,10*s,5*s);
    target.fillStyle='#eee0bd';target.fillText(String(b.settler_level),x+s,y+6*s);
  }
  if(tileW>=32){const [px,py]=p(0,0,65+rise);target.fillStyle='#fff0b1';target.font=`${10*s}px sans-serif`;target.fillText(labels.barn,px,py);}
  target.restore();
}
function paintBuilding(b,town,farmState=null,target=ctx,usable=buildingUsable(town,b)){
  const ctx=target;
  const paintPolygon=target===buildingMainContext?polygon:(points,fill,stroke)=>{
    ctx.beginPath();points.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));
    ctx.closePath();ctx.fillStyle=fill;ctx.fill();
    if(stroke){ctx.strokeStyle=stroke;ctx.stroke();}
  };

  if(b.kind==='orchard_tree'){
    const [x,top]=point(b.x,b.y),s=tileW/46,stage=b.tree?.stage || 'sapling',small=stage==='sapling',height=small?14:29;
    ctx.save();ctx.translate(x,top+tileH/2);ctx.scale(s,s);
    ctx.fillStyle='#223b293d';ctx.beginPath();ctx.ellipse(2,3,small?9:18,small?4:7,0,0,Math.PI*2);ctx.fill();
    paintPolygon([[-3,1],[3,1],[2,-height],[-1,-height-3]],'#8c7350');
    if(!small)paintPolygon([[-1,-13],[-9,-24],[-6,-25],[2,-16],[10,-25],[12,-22],[2,-10]],'#8c7350');
    const crown=small?.45:1;
    ctx.save();ctx.translate(0,-height);ctx.scale(crown,crown);
    paintPolygon([[-19,3],[-21,-5],[-13,-14],[1,-16],[16,-11],[20,1],[10,11],[-6,12]],'#64844d');
    paintPolygon([[-21,-5],[-13,-14],[1,-16],[16,-11],[7,-3],[-7,2]],'#8aa563');
    paintPolygon([[7,-3],[16,-11],[20,1],[10,11],[5,5]],'#507346');
    if(['blossom','fruit','ripe'].includes(stage))for(const [a,py] of [[-12,-5],[-2,-10],[10,-5],[-7,5],[8,5]]){
      ctx.fillStyle=stage==='blossom'?'#efd6c2':stage==='ripe'?'#d59865':'#a7b66d';
      ctx.beginPath();ctx.arc(a,py,stage==='blossom'?2.5:3,0,Math.PI*2);ctx.fill();
    }
    ctx.restore();
    if(stage==='harvested'){
      paintPolygon([[8,0],[18,-5],[25,-1],[15,4]],'#c4a477');
      paintPolygon([[8,0],[15,4],[15,10],[8,6]],'#ac895d');paintPolygon([[15,4],[25,-1],[25,5],[15,10]],'#8c7852');
    }
    ctx.restore();return;
  }
  paintBuildingGround(b,ctx,paintPolygon);
  if(b.construction){
    const [width,depth]=b.footprint || [1,1];
    const [x,top]=point(b.x+(width-1)/2,b.y+(depth-1)/2),y=top+tileH/2,s=tileW/46;
    const progress=b.construction.progress/b.construction.total,w=tileW*Math.min(1.2,.19*(width+depth)),d=w/2;
    paintPolygon([[x-w,y],[x,y+d],[x+w,y],[x,y-d]],b.kind==='acre'?'#80764a':'#8b795c');
    if(b.kind!=='acre'){
      // Loose timber and broad scaffold beams share the completed art's scale.
      paintPolygon([[x-w,y+2*s],[x-w+10*s,y+7*s],[x-w+18*s,y+3*s],[x-w+8*s,y-2*s]],'#c4a477');
      if(progress>=1/3){
        ctx.strokeStyle='#c1aa79';ctx.lineWidth=3*s;
        for(const [px,py] of [[x-w,y],[x,y+d],[x+w,y]]){
          ctx.beginPath();ctx.moveTo(px,py);ctx.lineTo(px,py-28*s);ctx.stroke();
        }
        paintPolygon([[x-w,y-25*s],[x,y+d-25*s],[x+w,y-25*s],[x,y-d-25*s]],'#b69a7044');
        ctx.beginPath();ctx.moveTo(x-w,y-10*s);ctx.lineTo(x,y+d-25*s);ctx.lineTo(x+w,y-10*s);ctx.stroke();
      }
      if(progress>=2/3){
        paintPolygon([[x-w,y],[x,y+d],[x,y+d-25*s],[x-w,y-25*s]],colors[b.kind] || '#b69a70');
        paintPolygon([[x,y+d],[x+w,y],[x+w,y-25*s],[x,y+d-25*s]],'#857659');
        const ridgeFront=[x-w/2,y+d/2-25*s-w/4],ridgeBack=[x+w/2,y-d/2-25*s-w/4];
        paintPolygon([[x-w,y-25*s],[x,y+d-25*s],ridgeFront],'#bba47a');
        paintPolygon([[x,y-d-25*s],[x-w,y-25*s],ridgeFront,ridgeBack],'#80574b');
        paintPolygon([ridgeBack,ridgeFront,[x,y+d-25*s],[x+w,y-25*s]],'#684f42');
      }
    }
    ctx.fillStyle='#6a5b43';ctx.fillRect(x-w,y+5*s,w*2,4*s);
    ctx.fillStyle='#e9c77e';ctx.fillRect(x-w,y+5*s,w*2*progress,4*s);
    return;
  }
  if(b.kind==='acre'){
    drawFarmCrops(b,farmState);
    return;
  }
  if(decorationKinds.includes(b.kind)){
    const [x,top]=point(b.x,b.y),y=top+tileH/2,s=tileW/46;
    ctx.save();ctx.translate(x,y);ctx.scale(s,s);
    ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(3,5,21,7,0,0,Math.PI*2);ctx.fill();
    // Turn local ground coordinates, leaving height upright and lighting fixed.
    const orientation=decorationOrientation(b),faces=[];
    const turn=(x,y,z)=>[...[ [x,y],[-y,x],[-x,-y],[y,-x] ][orientation],z];
    const wood=['#c0a674','#a58a5b','#806c49'],dark=['#9d855c','#806b49','#665a40'];
    const face=(points,color,layer=0)=>faces.push({points,color,layer,depth:points.reduce((sum,[x,y,z])=>sum+x+y+z,0)/points.length});
    const box=(x0,y0,z0,x1,y1,z1,colors=wood,layer=0)=>{
      const base=[[x0,y0],[x1,y0],[x1,y1],[x0,y1]].map(([x,y])=>turn(x,y,z0));
      const top=base.map(([x,y])=>[x,y,z1]);
      face(top,colors[0],layer);
      for(let i=0;i<4;i++){
        const j=(i+1)%4,nx=base[j][1]-base[i][1],ny=base[i][0]-base[j][0];
        if(nx+ny>0)face([base[i],base[j],top[j],top[i]],colors[nx>0?1:2],layer);
      }
    };
    const flower=(x,y,z,color,base=9)=>{
      box(x-.6,y-.6,base,x+.6,y+.6,z,['#8ba363','#6e8c50','#547144']);
      face([[x-3,y,z-5],[x-5,y,z-2],[x-3,y,z+2],[x+2,y,z+3],[x+5,y,z],[x+3,y,z-4]].map(p=>turn(...p)),color);
      face([[x-1,y+.1,z-2],[x+1,y+.1,z-2],[x+1,y+.1,z],[x-1,y+.1,z]].map(p=>turn(...p)),'#edcf84');
    };
    if(b.kind==='wooden_bench'){
      for(const x of [-12,9])for(const y of [-4,2])box(x,y,0,x+3,y+3,9,dark);
      box(-15,-5,8,15,5,11);
      for(const x of [-13,10])box(x,-5,10,x+3,-3,22,dark);
      box(-15,-5,15,15,-3,22);
    }else if(b.kind==='lantern'){
      const iron=['#818477','#60695e','#48554e'],glass=['#f2d38a','#e7bc69','#c9974f'];
      box(-2,-6,0,2,-2,37,dark);
      box(-2,-6,34,2,14,37,dark);
      box(-1,9,29,1,11,34,iron);
      box(-4,6,17,4,14,29,glass);
      box(-5,5,28,5,15,31,iron);
      box(-5,5,15,5,15,18,iron);
    }else if(b.kind==='flower_box'){
      box(-13,-5,0,13,5,9);
      box(-11,-3,9,11,3,10,['#68543c','#68543c','#68543c']);
      box(-14,-6,9,14,-4,12);box(-14,4,9,14,6,12);
      box(-14,-4,9,-12,4,12);box(12,-4,9,14,4,12);
      flower(-8,0,18,'#be7f77');flower(0,-1,23,'#d6b967');flower(8,1,19,'#c699a0');
    }else if(b.kind==='wooden_table'){
      for(const x of [-10,7])for(const y of [-6,3])box(x,y,0,x+3,y+3,14,dark);
      // The tabletop covers the leg joints; its plant sits above that surface.
      box(-12,-8,13,12,8,16,wood,1);
      box(-6,0,16,-1,5,22,['#c7926b','#ab7958','#8c624b'],2);
      box(-5,1,22,-2,4,25,['#8da66a','#6f8e53','#557447'],3);
    }else if(b.kind==='signpost'){
      box(-2,-2,0,2,2,32,dark);
      box(-12,-1,20,10,2,28);
      for(const points of [
        [[10,-1,20],[15,-1,24],[10,-1,28]],[[10,2,20],[15,2,24],[10,2,28]],
        [[10,-1,28],[15,-1,24],[15,2,24],[10,2,28]],
        [[10,-1,20],[15,-1,24],[15,2,24],[10,2,20]],
      ])face(points.map(p=>turn(...p)),'#a58a5b');
      box(-8,2,23,5,2.5,25,['#e1cc91','#d4bb80','#c0a674']);
    }else if(b.kind==='garden_arch'){
      for(const x of [-12,9])box(x,-2,0,x+3,2,33,dark);
      box(-15,-4,31,15,4,35);
      box(-13,2,5,-8,4,12,['#8ba363','#6e8c50','#547144']);
      box(-13,2,18,-8,4,26,['#8ba363','#6e8c50','#547144']);
      box(3,-3,35,11,3,38,['#8ba363','#6e8c50','#547144']);
      flower(-10,4,23,'#be7f77',18);flower(7,3,39,'#d6b967',35);
    }else{
      box(-9,-7,0,9,7,4,dark);
      box(-4,-3,4,-1,4,10);
      box(1,-3,4,4,4,10);
      box(-5,-3,10,5,3,23);
      box(-8,-2,13,-5,3,21);
      box(5,-2,13,8,3,21);
      box(-4,-3,24,4,4,33);
      box(-1,4,27,2,6,30); // A carved nose makes the statue's facing clear.
    }
    faces.sort((a,b)=>a.layer-b.layer||a.depth-b.depth);
    for(const {points,color} of faces)paintPolygon(points.map(([x,y,z])=>[x-y,(x+y)/2-z]),color);
    ctx.restore();return;
  }
  if(b.kind==='sawpit'){paintSawpit(b,town,ctx,paintPolygon,usable);return;}
  if(b.kind==='barn'){paintBarn(b,town,ctx,paintPolygon,usable);return;}
  if(detailedBuildingKinds.includes(b.kind)){paintDetailedBuilding(b,town,ctx,paintPolygon,usable);return;}
  const largeHall=b.kind==='hall'&&footprintTiles(b).length===4;
  const [x,top]=point(b.x+(largeHall ? .5 : 0),b.y+(largeHall ? .5 : 0)),y=top+tileH/2,s=tileW/46;
  if(b.kind==='well'){
    ctx.fillStyle='#223b2944';ctx.beginPath();ctx.ellipse(x+4*s,y+5*s,20*s,9*s,0,0,Math.PI*2);ctx.fill();
    // A stone ring, timber windlass, and bucket mark the water collection point.
    ctx.fillStyle='#828c85';ctx.beginPath();ctx.ellipse(x,y,16*s,8*s,0,0,Math.PI*2);ctx.fill();
    ctx.fillRect(x-16*s,y-9*s,32*s,9*s);
    ctx.fillStyle='#b5b8a9';ctx.beginPath();ctx.ellipse(x,y-9*s,16*s,8*s,0,0,Math.PI*2);ctx.fill();
    ctx.fillStyle='#356275';ctx.beginPath();ctx.ellipse(x,y-9*s,11*s,5*s,0,0,Math.PI*2);ctx.fill();
    ctx.strokeStyle='#67756e';ctx.lineWidth=s;
    for(const offset of [-8,0,8]){ctx.beginPath();ctx.moveTo(x+offset*s,y-5*s);ctx.lineTo(x+offset*s,y+3*s);ctx.stroke();}
    ctx.fillStyle='#705337';ctx.fillRect(x-14*s,y-32*s,3*s,25*s);ctx.fillRect(x+11*s,y-32*s,3*s,25*s);
    ctx.fillRect(x-14*s,y-25*s,28*s,3*s);
    ctx.strokeStyle='#d1b37c';ctx.beginPath();ctx.moveTo(x,y-24*s);ctx.lineTo(x,y-10*s);ctx.stroke();
    ctx.fillStyle='#b79865';ctx.fillRect(x-3*s,y-12*s,6*s,6*s);ctx.fillStyle='#83bac5';ctx.fillRect(x-2*s,y-12*s,4*s,2*s);
    paintPolygon([[x-20*s,y-32*s],[x,y-42*s],[x+20*s,y-32*s],[x,y-22*s]],'#98634f','#705337');
    return;
  }
  const level=buildingVisualLevel(b,town),upgrades=b.profession_upgrades || [];
  const w=46*s*(largeHall?.65:b.kind==='storehouse'?.43:b.kind==='workshop'&&footprintTiles(b).length>1?.43:.38),depth=w*tileH/tileW,h=buildingHeight(b,town)*s;
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
    const masonry=b.kind==='home'&&level>=4?h:9*s;
    paintPolygon([[x-w,y],[x,y+depth],[x,y+depth-masonry],[x-w,y-masonry]],usable?'#b1b3a2':'#c76d64');
    paintPolygon([[x,y+depth],[x+w,y],[x+w,y-masonry],[x,y+depth-masonry]],usable?'#858e86':'#a34f49');
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
  if(largeHall){
    ctx.strokeStyle='#849594';ctx.lineWidth=s;
    for(const f of [.3,.6]){ctx.beginPath();ctx.moveTo(x-w*(1-f),y-h-15*s*f);ctx.lineTo(x+w*f,y-h+(depth+s)*(1-f));ctx.stroke();}
  }
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
  if(largeHall){
    // Human-sized doors and windows stay in proportion to the wider civic hall.
    paintPolygon([[x-9*s,y+depth-4*s],[x-2*s,y+depth-s],[x-2*s,y+depth-20*s],[x-9*s,y+depth-23*s]],'#594d38','#c4a477');
    paintPolygon([[x-12*s,y+depth-3*s],[x,y+depth+3*s],[x+7*s,y+depth],[x-5*s,y+depth-6*s]],'#b1b3a2','#797665');
    paintPolygon([[x-12*s,y+depth-3*s],[x,y+depth+3*s],[x,y+depth+6*s],[x-12*s,y+depth]],'#858e86');
    for(const [wx,wy] of [[-22,-h/s+10],[19,-h/s+10]]){
      ctx.fillStyle='#594d38';ctx.fillRect(x+wx*s,y+wy*s,6*s,9*s);
      ctx.fillStyle='#edc77d';ctx.fillRect(x+(wx+1)*s,y+(wy+1)*s,4*s,6*s);
      ctx.fillStyle='#c4a477';ctx.fillRect(x+(wx+2.5)*s,y+(wy+1)*s,s,7*s);
    }
  }
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
  const [x,top]=point(b.x,b.y),s=tileW/46;
  ctx.save();ctx.translate(x,top+tileH/2);ctx.scale(s,s);
  if(b.kind==='mill'){
    const [width,depth]=b.footprint || [1,1],fit=Math.min(width,depth)/2;
    const [cx,cy]=point(b.x+(width-1)/2,b.y+(depth-1)/2);
    ctx.translate((cx-x)/s,(cy-top)/s);ctx.scale(fit,fit);
    ctx.translate(-15*.46,(10-(48+4*(buildingVisualLevel(b,town)-1))*1.5)*.46);
    ctx.strokeStyle='#c5ad79';ctx.lineWidth=3;
    for(let i=0;i<4;i++){
      const a=i*Math.PI/2+Math.PI/4+time*2;
      ctx.beginPath();ctx.moveTo(0,0);ctx.lineTo(Math.cos(a)*21,Math.sin(a)*21);ctx.stroke();
      const sail=(along,across)=>[Math.cos(a)*along-Math.sin(a)*across,Math.sin(a)*along+Math.cos(a)*across];
      polygon([sail(8,0),sail(21,0),sail(21,5),sail(9,4)],'#e8dab4');
    }
  }else if(b.kind==='fish_pond'){
    ctx.strokeStyle='#b8d8d5';ctx.lineWidth=1;
    ctx.beginPath();ctx.ellipse(0,5,4+Math.sin(time*4),2,0,0,Math.PI*2);ctx.stroke();
  }else if(time){
    const [width,depth]=b.footprint || [1,1],fit=Math.min(width,depth)/2;
    const [cx,cy]=point(b.x+(width-1)/2,b.y+(depth-1)/2);
    ctx.translate((cx-x)/s,(cy-top)/s);ctx.scale(fit,fit);
    const chimneyX=(.67+.975)*25*.46;
    const chimneyY=((.67-.975)*12.5-((b.kind==='bakery'?71:77)+4*(buildingVisualLevel(b,town)-1))*1.5)*.46;
    ctx.fillStyle='#c7c6af66';
    for(let i=0;i<3;i++){
      const rise=(time*.4+i/3)%1;
      ctx.beginPath();ctx.arc(chimneyX+Math.sin(rise*5)*3,chimneyY-rise*12,2+rise*3,0,Math.PI*2);ctx.fill();
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
    if(footprintTiles(b).length===1){
      ctx.fillStyle='#ac895d';ctx.fillRect(-15,-5,30,5);ctx.fillRect(-12,0,3,7);ctx.fillRect(10,0,3,7);
      polygon([[-5,-10],[8,-10],[4,-6],[-2,-6]],'#84958e');
    }
    ctx.strokeStyle='#d5dfd8';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(9,-height+8);ctx.lineTo(9,-height+18);ctx.moveTo(5,-height+8);ctx.lineTo(13,-height+8);ctx.stroke();
    ctx.fillStyle='#735439';ctx.fillRect(-16,-height+11,13,2);
    for(let i=0;i<3;i++){ctx.fillStyle='#c4a477';ctx.fillRect(-14+i*4,-height+11,2,8);ctx.fillStyle='#98aaa7';ctx.fillRect(-14+i*4,-height+18,2,3);}
  }else if(b.kind==='market'){
    for(let i=0;i<5;i++)polygon([[-22+i*9,-height],[-13+i*9,-height],[-9+i*9,-height+13],[-18+i*9,-height+13]],i%2?'#eee0bd':'#698a72');
    ctx.fillStyle='#ac895d';ctx.fillRect(-18,-2,36,5);
    for(let i=0;i<5;i++){ctx.fillStyle=i%2?'#cc8962':'#d9bd70';ctx.beginPath();ctx.arc(-13+i*6,-4,3,0,Math.PI*2);ctx.fill();}
  }else if(b.kind==='fish_pond'){
    ctx.strokeStyle='#b1b3a2';ctx.lineWidth=3;ctx.beginPath();ctx.ellipse(0,5,18,7,0,0,Math.PI*2);ctx.stroke();
    polygon([[-8,5],[-3,3],[2,5],[-3,7]],'#d5dfd8');
  }else if(b.kind==='orchard'){
    ctx.fillStyle='#ac895d';ctx.fillRect(-16,-7,13,9);ctx.fillRect(4,-3,12,6);
    ctx.strokeStyle='#725333';ctx.lineWidth=2;for(const cx of [-12,-6,8,13]){ctx.beginPath();ctx.moveTo(cx,-6);ctx.lineTo(cx,2);ctx.stroke();}
  }else if(b.kind==='butcher'){
    polygon([[-22,-height+2],[17,-height+2],[22,-height+12],[-17,-height+12]],'#b66e65');
    ctx.fillStyle='#eadac1';ctx.fillRect(-5,-height+3,3,8);ctx.fillRect(-9,-height+6,11,2);
  }
}
async function selectTile(x,y){
  const town=world.towns[viewed];
  if(!town || x<0||y<0||x>=mapSize(town)||y>=mapSize(town))return;
  if(town.terrain?.[y]?.[x]==='v')return;
  if(mapOverview()){
    const entries=world.shared_map?Object.entries(world.towns):[[viewed,town]];
    for(const [id,t] of entries){const i=t.buildings.findIndex(b=>b.kind==='hall'&&occupies(b,x,y));if(i>=0){inspect('building',i,id);return;}}
    openPanel(null);return;
  }
  const npc=(town.npcs || []).findIndex(n=>{const p=npcPosition(n);return Math.round(p.x)===x&&Math.round(p.y)===y;});
  const resource=town.map_resources.findIndex(r=>r.x===x&&r.y===y);
  const index=town.buildings.findIndex(b=>occupies(b,x,y));
  if(!tool){
    if(world.shared_map){
      for(const [id,t] of Object.entries(world.towns)){const i=t.buildings.findIndex(b=>occupies(b,x,y));if(i>=0){inspect('building',i,id);return;}}
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
  if(resource>=0&&!['berry_bush','herb_patch'].includes(town.map_resources[resource].kind)){notice('Choose open grass, away from resources.');return;}
  if(tool==='move'){
    if(moveIndex===null){if(index<0){notice('Click the tile beneath a building.');return;}moveIndex=index;render();}
    else{
      if(!insideBuildingRange(town,{x,y})){notice(`Move within ${buildingRange(town)} tile steps of your town hall.`);return;}
      await act({action:'move',index:moveIndex,x,y});moveIndex=null;render();
    }
  }else{
    if(building)return;
    if(index>=0){notice('Choose open grass, away from existing buildings.');return;}
    if(!town.sandbox&&town.wood<costs[tool]){notice('Not enough wood.');return;}
    await buildAt(x,y);
  }
}
function tileAt(event){
  const rect=canvas.getBoundingClientRect(),dx=event.clientX-rect.left-originX,dy=event.clientY-rect.top-originY;
  return {x:Math.floor(dx/tileW+dy/tileH),y:Math.floor(dy/tileH-dx/tileW)};
}
let pointerHitCanvas=null;
function hitTargetContains(hit,px,py){
  if(px<hit.left||px>hit.right||py<hit.top||py>hit.bottom)return false;
  if(!hit.paint)return true; // Overview glyphs and labels use their screen bounds.
  const surface=pointerHitCanvas ||= document.createElement('canvas');
  const target=surface.getContext?.('2d',{willReadFrequently:true});
  if(!target)return true;
  // Sample only the pointer pixel using the same artwork and pose as the scene.
  // Resetting the tiny canvas also resets transforms, styles and clipping.
  surface.width=1;surface.height=1;
  target.translate(.5-px,.5-py);
  hit.paint(target);
  return target.getImageData(0,0,1,1).data[3]>=128;
}
function hitTargetAt(px,py){
  for(let i=hitTargets.length-1;i>=0;i--)if(hitTargetContains(hitTargets[i],px,py))return hitTargets[i];
  return null;
}
canvas.onmousemove=event=>{
  const rect=canvas.getBoundingClientRect(),px=event.clientX-rect.left,py=event.clientY-rect.top;
  const hit=hitTargetAt(px,py);
  buildingHover=!tool&&hit?.type==='building'?{index:hit.index,townId:hit.townId}:null;
  const town=world.towns[viewed],tile=tileAt(event);
  buildTile=(tool in costs)&&viewed===identity?.town_id&&town&&placementValid(town,tile,tool)?tile:null;
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
    // Ground roads and water use their visible tile; sprite misses never fall back to it.
    const tile=tileAt(event),towns=world.shared_map?Object.entries(world.towns):[[viewed,world.towns[viewed]]];
    if(!mapOverview())for(const [id,town] of towns){
      const index=(town?.roads || []).findIndex(r=>r.x===tile.x&&r.y===tile.y);
      if(index>=0){inspect('road',index,id);return;}
    }
    if(!mapOverview()){
      const resources=world.shared_map?.map_resources || world.towns[viewed]?.map_resources || [];
      const index=resources.findIndex(r=>r.kind==='water'&&r.x===tile.x&&r.y===tile.y);
      if(index>=0){inspect('resource',index);return;}
    }
    openPanel(null);return;
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
  const otherPlayers=world.shared_map?entries.filter(([id,t])=>t.player&&id!==hero?.townId).map(([id,t])=>remotePlayerSprite(id,t,seconds)):[];
  const overview=mapOverview();
  if(overview!==overviewActive){
    // Drop suspended visual jobs so detailed playback resumes from current state.
    ClientScene.queue.clear();workers.clear();overviewActive=overview;
  }
  if(overview){
    buildingEffects.length=0;frameActors={world,hero,villagers:[],otherPlayers};return;
  }
  const buildings=ClientScene.current?.buildings || [];
  const villagers=entries.flatMap(([id,t])=>animatedVillagers(t,seconds,id,
    ClientScene.region(t)?.layout ?? townWorkerLayout(t,buildings,t.map_resources || [])));
  animalClock+=seconds;npcClock+=seconds;
  for(let i=buildingEffects.length-1;i>=0;i--){buildingEffects[i].age+=seconds;if(buildingEffects[i].age>=1.2)buildingEffects.splice(i,1);}
  frameActors={world,hero,villagers,otherPlayers,seconds};
}
function animate(now){
  updateBarnTimers(now);
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
setInterval(()=>{refresh().catch(()=>notice('Connection lost. Waiting for the host…'));},2000);

setInterval(()=>refreshPlayers().catch(()=>{}),50);
