// Lightweight course polling keeps real-time hazards out of full scene refreshes.
let lanternWorld=null,lanternQuest=null,lanternBusy=false,lanternPolling=false,lanternGeneration=0,lanternLastMove=-Infinity;
const lanternDirections=[['↑',0,-1,'up'],['←',-1,0,'left'],['↓',0,1,'down'],['→',1,0,'right']];
function openLanterns(){
  clearWalk();lanternWorld=world.world_id;lanternQuest=world.lanterns || null;lanternGeneration++;
  $('lanterns-error').textContent='';$('lanterns-dialog').showModal();renderLanterns();pollLanterns();
}
async function readLanterns(generation,worldId){
  const result=await request('/api/lanterns');
  if(generation!==lanternGeneration)return;
  if(result.world_id!==worldId||world.world_id!==worldId){$('lanterns-dialog').close();lanternWorld=null;return;}
  lanternQuest=result.quest;world.lanterns=result.quest;renderLanterns();
}
async function pollLanterns(){
  if(lanternPolling||!$('lanterns-dialog').open)return;
  const generation=lanternGeneration,worldId=lanternWorld;
  lanternPolling=true;
  try{await readLanterns(generation,worldId);}
  catch(error){if(generation===lanternGeneration)$('lanterns-error').textContent=error.message;}
  finally{lanternPolling=false;}
}
async function lanternCommand(data){
  if(lanternBusy)return false;
  const generation=lanternGeneration,worldId=lanternWorld;
  if(data.action==='lanterns_walk'){
    if(performance.now()-lanternLastMove<150)return false;
    lanternLastMove=performance.now();
  }
  lanternBusy=true;$('lanterns-error').textContent='';renderLanterns();
  try{
    await request('/api/command',{...data,world_id:worldId});
    if(generation!==lanternGeneration)return false;
    await readLanterns(generation,worldId);
    if(['leave_lanterns','lanterns_collect'].includes(data.action))await refresh();
    return true;
  }catch(error){if(generation===lanternGeneration)$('lanterns-error').textContent=error.message;return false;}
  finally{lanternBusy=false;renderLanterns();}
}
async function closeLanterns(){
  if(lanternBusy)return;
  if(lanternQuest?.members?.[identity?.town_id]?.inside){
    if(!await lanternCommand({action:'leave_lanterns'}))return;
  }
  $('lanterns-dialog').close();lanternGeneration++;
}
function renderLanterns(){
  const dialog=$('lanterns-dialog');
  if(lanternWorld!==world.world_id){dialog.close();return;}
  if(!dialog.open)return;
  const quest=lanternQuest,member=quest?.members?.[identity?.town_id];
  const inside=!!member?.inside,playing=inside&&!!quest?.playing;
  $('lanterns-status').textContent=quest?.completed?'Satchel recovered! Leave to return to the valley.'
    :quest&&!quest.playing?'Waiting for two founders inside. Movement and hazards are paused.'
    :quest?.message || 'Enter the courier path and ask a friend to enter from their You panel.';
  $('lanterns-party').textContent='Expedition: '+(Object.entries(quest?.members || {}).map(([id,m])=>
    `${world.towns[id]?.name || 'Founder'}${id===identity?.town_id?' (you)':''} · ${m.inside?'inside':'in valley'} · ${m.hits} hits`).join(' / ') || 'Nobody yet');
  $('lanterns-reward').textContent='Reward: 25 coins and a courier lantern badge for each expedition member.';
  const board=$('lanterns-board');board.replaceChildren();
  const danger=new Map((quest?.hazards || []).map(h=>[`${h.x},${h.y}`,h.state]));
  for(let y=0;y<5;y++)for(let x=0;x<13;x++){
    const cell=document.createElement('div'),wall=x===0||x===12||y===0||y===4,state=danger.get(`${x},${y}`);
    cell.className='quarry-cell';if(wall)cell.classList.add('wall');if(state)cell.classList.add(state);
    cell.textContent=wall?'':state==='strike'?'×':state==='warning'?'!':x===6?'C':x===11?'T':'';
    let description=wall?'Wall':state==='strike'?'Falling debris':state==='warning'?'Warning: debris soon':x===6?'Safe checkpoint':x===11?'Courier satchel':'Safe floor';
    for(const [id,m] of Object.entries(quest?.members || {}))if(m.inside&&m.x===x&&m.y===y){
      const marker=document.createElement('span');marker.className='quarry-marker'+(id===identity?.town_id?' own':'');
      marker.textContent=id===identity?.town_id?'You':(world.towns[id]?.name || '?').slice(0,1);cell.append(marker);
      description+=` · ${world.towns[id]?.name || 'Founder'}`;
    }
    cell.setAttribute('aria-label',`${description} at column ${x+1}, row ${y+1}`);board.append(cell);
  }
  for(const button of $('lanterns-directions').children)button.disabled=lanternBusy||!playing;
  $('lanterns-enter').hidden=inside||!!quest?.completed;$('lanterns-enter').disabled=lanternBusy||!identity;
  $('lanterns-enter').textContent=member?'Rejoin courier path':'Enter courier path';
  $('lanterns-leave').hidden=!inside;$('lanterns-leave').disabled=lanternBusy;
  $('lanterns-collect').disabled=lanternBusy||!playing||member?.x!==11
    ||Object.values(quest?.members || {}).filter(m=>m.inside&&m.x===11).length<2;
  $('lanterns-close').disabled=lanternBusy;
}
lanternDirections.forEach(([label,dx,dy,name])=>{
  const button=document.createElement('button');button.textContent=label;button.setAttribute('aria-label',`Walk ${name}`);
  button.onclick=()=>lanternCommand({action:'lanterns_walk',dx,dy});$('lanterns-directions').append(button);
});
$('lanterns-enter').onclick=()=>lanternCommand({action:'join_lanterns'});
$('lanterns-leave').onclick=()=>lanternCommand({action:'leave_lanterns'});
$('lanterns-collect').onclick=()=>lanternCommand({action:'lanterns_collect'});
$('lanterns-close').onclick=closeLanterns;
$('lanterns-dialog').addEventListener('cancel',event=>{event.preventDefault();closeLanterns();});
window.addEventListener('keydown',event=>{
  if(!$('lanterns-dialog').open||event.ctrlKey||event.metaKey||event.altKey
    ||document.activeElement?.isContentEditable||['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName))return;
  const direction=lanternDirections.find(([,dx,dy,name])=>event.key.toLowerCase()===keybindings[name]);
  if(!direction)return;
  event.preventDefault();
  if(lanternQuest?.playing&&lanternQuest.members?.[identity?.town_id]?.inside)
    lanternCommand({action:'lanterns_walk',dx:direction[1],dy:direction[2]});
});
setInterval(()=>{if($('lanterns-dialog').open)pollLanterns();},100);
