// A compact adventure board uses the same authenticated commands as the valley.
let quarryBusy=false, quarryWorld=null;
const quarryDirections=[['↑',0,-1,'up'],['←',-1,0,'left'],['↓',0,1,'down'],['→',1,0,'right']];
const quarryObjects=[['inscription','Read inscription',7,4],['sun','Touch ☀ sun',7,2],
  ['leaf','Touch ♣ leaf',8,2],['moon','Touch ☾ moon',9,2],['treasure','Open lantern chest',9,4]];

function openQuarry(){
  clearWalk();quarryWorld=world.world_id;
  $('quarry-error').textContent='';$('quarry-dialog').showModal();renderQuarry();
}
async function quarryCommand(data){
  if(quarryBusy)return;
  quarryBusy=true;$('quarry-error').textContent='';renderQuarry();
  try{
    await request('/api/command',{...data,world_id:quarryWorld});
    await refresh();
  }catch(error){$('quarry-error').textContent=error.message;}
  finally{quarryBusy=false;renderQuarry();}
}
function renderQuarry(){
  const dialog=$('quarry-dialog');
  if(quarryWorld!==null&&quarryWorld!==world.world_id){dialog.close();quarryWorld=null;return;}
  if(!dialog.open)return;
  const spec=world.quarry_catalog;
  if(!spec)return;
  const quest=world.quarry,member=quest?.members?.[identity?.town_id];
  const inside=!!member?.inside,playing=inside&&quest.started&&!quest.completed;
  const gateOpen=!!quest?.gate_latched||Object.values(quest?.members || {}).some(m=>m.inside&&m.x===2&&m.y===2);
  $('quarry-status').textContent=quest?.message || 'Flint’s map leads beneath the ridge. Enter and ask a friend to enter from their You panel.';
  $('quarry-objective').textContent=quest?.completed?'Lantern stone recovered. Leave the expedition to return to the valley.'
    :!quest?.started?'Objective: two founders must enter to begin.'
    :!quest.gate_latched?'Objective: hold P while a companion crosses G to secure the gate.'
    :!quest.runes_solved?`Objective: read I and awaken the runes in order · ${quest.sequence.length} / 3 glowing.`
    :'Objective: open T to recover the lantern stone.';
  $('quarry-party').textContent='Expedition: '+(Object.entries(quest?.members || {}).map(([id,m])=>
    `${world.towns[id]?.name || 'Founder'}${id===identity?.town_id?' (you)':''} · ${m.inside?'inside':'returned to valley'}`).join(' / ') || 'Nobody yet');
  $('quarry-reward').textContent=`Each expedition member earns ${spec.reward} coins and a ${spec.keepsake.toLowerCase()}.${quest?.completed?' Rewards received.':''}`;
  const board=$('quarry-board');board.replaceChildren();
  const names={'#':'Wall','.':'Floor',P:'Floor plate',G:gateOpen?'Open gate':'Closed gate',S:'Sun rune',L:'Leaf rune',M:'Moon rune',I:'Inscription',T:'Lantern chest'};
  const symbols={'#':'','.':'',P:'P',G:gateOpen?'·':'G',S:'☀',L:'♣',M:'☾',I:'I',T:'T'};
  // Keep the cells descriptive so the map is usable without relying on color.
  spec.layout.forEach((row,y)=>Array.from(row).forEach((tile,x)=>{
    const cell=document.createElement('div');cell.className='quarry-cell';
    if(tile==='#')cell.classList.add('wall');
    if(tile==='G'&&!gateOpen)cell.classList.add('sealed');
    if(quest?.sequence?.includes({S:'sun',L:'leaf',M:'moon'}[tile]))cell.classList.add('lit');
    cell.textContent=symbols[tile];
    const occupants=Object.entries(quest?.members || {}).filter(([,m])=>m.inside&&m.x===x&&m.y===y);
    for(const [id] of occupants){
      const marker=document.createElement('span');marker.className='quarry-marker'+(id===identity?.town_id?' own':'');
      marker.textContent=id===identity?.town_id?'You':(world.towns[id]?.name || '?').slice(0,1);cell.append(marker);
    }
    const description=`${names[tile]} at column ${x+1}, row ${y+1}${occupants.length?': '+occupants.map(([id])=>world.towns[id]?.name).join(', '):''}`;
    cell.title=description;cell.setAttribute('aria-label',description);board.append(cell);
  }));
  quarryDirections.forEach(([,dx,dy],i)=>{
    const button=$('quarry-directions').children[i];button.disabled=quarryBusy||!playing;
  });
  quarryObjects.forEach(([target,,x,y],i)=>{
    const button=$('quarry-objects').children[i];
    button.disabled=quarryBusy||!playing||Math.abs(member.x-x)+Math.abs(member.y-y)>1
      ||target==='treasure'&&!quest.runes_solved||['sun','leaf','moon'].includes(target)&&quest.runes_solved;
  });
  $('quarry-enter').hidden=inside||!!quest?.completed;
  $('quarry-enter').disabled=quarryBusy||!identity;
  $('quarry-enter').textContent=member?'Rejoin quarry':'Enter quarry';
  $('quarry-leave').hidden=!inside;$('quarry-leave').disabled=quarryBusy;
}
quarryDirections.forEach(([label,dx,dy,direction])=>{
  const button=document.createElement('button');button.textContent=label;button.setAttribute('aria-label',`Walk ${direction}`);
  button.onclick=()=>quarryCommand({action:'quarry_walk',dx,dy});$('quarry-directions').append(button);
});
quarryObjects.forEach(([target,label])=>{
  const button=document.createElement('button');button.textContent=label;
  button.onclick=()=>quarryCommand({action:'quarry_interact',target});$('quarry-objects').append(button);
});
$('quarry-enter').onclick=()=>quarryCommand({action:'join_quarry'});
$('quarry-leave').onclick=()=>quarryCommand({action:'leave_quarry'});
$('quarry-close').onclick=()=>$('quarry-dialog').close();
window.addEventListener('keydown',event=>{
  if(!$('quarry-dialog').open||event.ctrlKey||event.metaKey||event.altKey
    ||['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName)||document.activeElement?.isContentEditable)return;
  const key=event.key.toLowerCase(),direction=quarryDirections.find(([,dx,dy,name])=>key===keybindings[name]);
  if(!direction)return;
  event.preventDefault();
  if(!event.repeat&&world.quarry?.members?.[identity?.town_id]?.inside&&world.quarry.started&&!world.quarry.completed)
    quarryCommand({action:'quarry_walk',dx:direction[1],dy:direction[2]});
});
