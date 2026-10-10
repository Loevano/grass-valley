// Visual worker state stays in the browser; the host owns production and skills.
const WorkerAnimation = (() => {
  const key = p => `${p.x},${p.y}`;
  const distance = (a,b) => Math.abs(a.x-b.x)+Math.abs(a.y-b.y);
  const neighbors = p => [{x:p.x+1,y:p.y},{x:p.x,y:p.y+1},{x:p.x-1,y:p.y},{x:p.x,y:p.y-1}];
  function route(start, target, blocked, size=120, adjacent=true) {
    const queue = [start], previous = new Map([[key(start),null]]);
    for(let i=0;i<queue.length;i++){
      const p=queue[i];
      if(distance(p,target)===(adjacent?1:0)){
        const path=[];
        for(let step=p;previous.get(key(step));step=previous.get(key(step)))path.unshift(step);
        return path;
      }
      for(const next of neighbors(p)){
        if(next.x<0||next.y<0||next.x>=size||next.y>=size||blocked.has(key(next))||previous.has(key(next)))continue;
        previous.set(key(next),p);queue.push(next);
      }
    }
    return null;
  }
  function plan(town, villager, index, state) {
    const blocked=new Set([...(town.obstacles || town.buildings),...town.map_resources,...(town.npcs || [])].map(key));
    const hall=town.home_hall || town.buildings.find(b=>b.kind==='hall');
    const preferred=hall?{x:hall.x-2+index%5,y:hall.y+3+Math.floor(index/5)}:{x:3+index%5,y:8+Math.floor(index/5)};
    const open=[];
    const size=town.map_size || 120;
    for(let x=0;x<size;x++)for(let y=0;y<size;y++){if(town.terrain?.[y]?.[x]==='v')blocked.add(`${x},${y}`);else if(!blocked.has(`${x},${y}`))open.push({x,y});}
    const current=state?{x:Math.round(state.x),y:Math.round(state.y)}:villager.construction_position || preferred;
    const start=open.sort((a,b)=>distance(a,current)-distance(b,current))[0] || preferred;
    const source=villager.construction_target || (villager.job==='idle'?null:villager.work_target);
    const kind=villager.job==='woodcutting'?'woodcutter':villager.job==='fishing'?'fishing_hut':'farm';
    // Match the host's workplace selection, including multiple assigned workers.
    const workplaces=town.buildings.filter(b=>b.kind===kind&&!b.construction&&town.map_resources.some(r=>
      r.kind===(kind==='woodcutter'?'tree':'water')&&distance(b,r)<=3));
    const workplace=source?.kind==='acre'?town.buildings[source.farm_index]:workplaces[index%workplaces.length];
    const home=villager.construction_target || workplace || town.buildings.find(b=>b.kind==='hall');
    let pier=[],workSpot=null;
    if(!villager.construction_target&&villager.job==='fishing'&&source&&home?.kind==='fishing_hut'){
      // A hut can occupy the only bank tile; its pier provides access over water.
      const pierBlocked=new Set(blocked);
      pierBlocked.delete(key(home));
      for(const water of town.map_resources.filter(r=>r.kind==='water'&&distance(home,r)<=3)){
        if(town.terrain?.[water.y]?.[water.x]!=='v')pierBlocked.delete(key(water));
      }
      const deck=route(home,source,pierBlocked,size,false);
      if(deck&&deck.length<=3){
        pier=[home,...deck];workSpot=source;
        for(const tile of pier)blocked.delete(key(tile));
      }
    }
    const path=source?route(start,workSpot || source,blocked,size,!workSpot):home?route(start,home,blocked,size):[];
    return {...start,phase:path===null?'waiting':path.length?'walking':source?'working':'idle',
      path:path || [],source,home,pier,workSpot,blocked,size,workDuration:villager.job==='farming'?8:3.2,
      construction:!!villager.construction_target,elapsed:0,clock:state?.clock || index*.37,facing:state?.facing || 1};
  }
  function advance(state, seconds, active) {
    state.clock+=seconds;
    if(state.path.length){
      let travel=seconds*1.8;
      while(state.path.length&&travel>0){
        const next=state.path[0],dx=next.x-state.x,dy=next.y-state.y,length=Math.hypot(dx,dy);
        if(dx-dy!==0)state.facing=dx-dy>0?1:-1;
        if(length<=travel){state.x=next.x;state.y=next.y;state.path.shift();travel-=length;}
        else{state.x+=dx/length*travel;state.y+=dy/length*travel;travel=0;}
      }
      if(!state.path.length){state.phase=state.phase==='returning'?'depositing':state.source?'working':'idle';state.elapsed=0;}
      return;
    }
    if(state.construction){state.elapsed+=seconds;return;}
    if(!active || !state.source || state.phase==='waiting')return;
    state.elapsed+=seconds;
    if(state.phase==='working'&&state.elapsed>=(state.workDuration || 3.2)){
      const path=state.home?route(state,state.home,state.blocked,state.size):null;
      if(path===null)return;
      state.path=path;state.phase=path.length?'returning':'depositing';state.elapsed=0;
    }else if(state.phase==='depositing'&&state.elapsed>=.65){
      const path=route(state,state.workSpot || state.source,state.blocked,state.size,!state.workSpot);
      state.path=path || [];state.phase=path===null?'waiting':path.length?'walking':'working';state.elapsed=0;
    }
  }
  return {route,plan,advance};
})();
if(typeof module!=='undefined')module.exports=WorkerAnimation;
