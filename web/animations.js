// Visual worker state stays in the browser; the host owns production and skills.
const WorkerAnimation = (() => {
  const key = p => `${p.x},${p.y}`;
  const distance = (a,b) => Math.abs(a.x-b.x)+Math.abs(a.y-b.y);
  const neighbors = p => [{x:p.x+1,y:p.y},{x:p.x,y:p.y+1},{x:p.x-1,y:p.y},{x:p.x,y:p.y-1}];
  const boatLevel = b => b?.profession_upgrades?.includes('master_boat')?3:b?.profession_upgrades?.includes('improved_boat')?2:b?.profession_upgrades?.includes('boat')?1:0;
  const fishingReach = b => 2+(b.level || 1)+(boatLevel(b)?3+2*(boatLevel(b)-1):0);
  function fishingBoating(town,home,index){
    const huts=town.buildings.filter(b=>b.kind==='fishing_hut'&&(!b.construction||b.construction.upgrade)
      &&town.map_resources.some(r=>r.kind==='water'&&distance(b,r)<=fishingReach(b)));
    const crew=(town.villagers || []).map((v,i)=>({v,i})).filter(({v,i})=>v.job==='fishing'
      &&(Number.isInteger(v.building_index)?town.buildings[v.building_index]:huts[i%huts.length])===home);
    return crew.length?crew.slice(0,boatLevel(home)).some(worker=>worker.i===index):index<boatLevel(home);
  }
  function fishingPier(town,home){
    const occupied=new Set([...(town.obstacles || town.buildings),...(town.npcs || [])].map(key));
    const waters=town.map_resources.filter(r=>r.kind==='water'&&distance(home,r)<=3&&!occupied.has(key(r)))
      .sort((a,b)=>distance(home,a)-distance(home,b)||a.y-b.y||a.x-b.x);
    const blocked=new Set([...occupied,...town.map_resources.map(key)]);
    blocked.delete(key(home));
    for(const water of waters)if(town.terrain?.[water.y]?.[water.x]!=='v')blocked.delete(key(water));
    const size=town.map_size || 120,bounds={minX:Math.max(0,home.x-3),maxX:Math.min(size-1,home.x+3),
      minY:Math.max(0,home.y-3),maxY:Math.min(size-1,home.y+3)};
    for(let x=bounds.minX;x<=bounds.maxX;x++)for(let y=bounds.minY;y<=bounds.maxY;y++)
      if(town.terrain?.[y]?.[x]==='v'||(town.chunked&&!town.terrain?.[y]?.[x]))blocked.add(`${x},${y}`);
    for(const water of waters){
      const deck=route(home,water,blocked,size,false,bounds);
      if(deck&&deck.length<=3)return [home,...deck];
    }
    return [];
  }
  function waterDepths(resources){
    const water=new Set(resources.filter(r=>r.kind==='water').map(key)),depths=new Map(),queue=[];
    for(const tile of water){
      const [x,y]=tile.split(',').map(Number),p={x,y};
      if(neighbors(p).some(next=>!water.has(key(next)))){depths.set(tile,1);queue.push(p);}
    }
    for(const tile of queue)for(const next of neighbors(tile)){
      if(water.has(key(next))&&!depths.has(key(next))){depths.set(key(next),depths.get(key(tile))+1);queue.push(next);}
    }
    return depths;
  }
  function route(start, target, blocked, size=120, adjacent=true, bounds=null) {
    const queue = [start], previous = new Map([[key(start),null]]);
    for(let i=0;i<queue.length;i++){
      const p=queue[i];
      if(target instanceof Set?target.has(key(p)):distance(p,target)===(adjacent?1:0)){
        const path=[];
        for(let step=p;previous.get(key(step));step=previous.get(key(step)))path.unshift(step);
        return path;
      }
      for(const next of neighbors(p)){
        if(next.x<0||next.y<0||next.x>=size||next.y>=size||blocked.has(key(next))||previous.has(key(next)))continue;
        if(bounds&&(next.x<bounds.minX||next.x>bounds.maxX||next.y<bounds.minY||next.y>bounds.maxY))continue;
        previous.set(key(next),p);queue.push(next);
      }
    }
    return null;
  }
  function travel(start,target,state,adjacent=true){
    if(!state.sailing)return route(start,target,state.blocked,state.size,adjacent,state.bounds);
    const {water,berth,landBlocked,seaBlocked,seaBounds}=state.sailing;
    const afloat=water.has(key({x:Math.round(start.x),y:Math.round(start.y)}));
    if(target===state.source&&afloat)return route(start,target,seaBlocked,state.size,false,seaBounds);
    const outbound=target===state.source;
    const approach=outbound?route(start,berth,landBlocked,state.size,false,state.bounds):
      afloat?route(start,berth,seaBlocked,state.size,false,seaBounds):[];
    const sailing=outbound?route(berth,target,seaBlocked,state.size,false,seaBounds):
      route(afloat?berth:start,target,landBlocked,state.size,true,state.bounds);
    return approach===null||sailing===null?null:[...approach,...sailing];
  }
  function plan(town, villager, index, state) {
    const blocked=new Set([...(town.obstacles || town.buildings),...town.map_resources,...(town.npcs || [])].map(key));
    if(villager.logistics_target || (villager.job==='farming'&&!villager.construction_target)){
      for(const acre of (town.obstacles || town.buildings).filter(b=>b.kind==='acre'&&!b.construction))blocked.delete(key(acre));
    }
    const hall=town.home_hall || town.buildings.find(b=>b.kind==='hall');
    const preferred=hall?{x:hall.x-2+index%5,y:hall.y+3+Math.floor(index/5)}:{x:3+index%5,y:8+Math.floor(index/5)};
    const size=town.map_size || 120;
    const center=town.town_origin || hall || preferred;
    const bounds=town.chunked?{minX:Math.max(0,center.x-60),maxX:Math.min(size-1,center.x+60),
      minY:Math.max(0,center.y-60),maxY:Math.min(size-1,center.y+60)}:null;
    const current=state?(state.path[0] || {x:Math.round(state.x),y:Math.round(state.y)}):villager.logistics_position || villager.construction_position || preferred;
    const minX=bounds?.minX || 0,maxX=bounds?.maxX ?? size-1,minY=bounds?.minY || 0,maxY=bounds?.maxY ?? size-1;
    const currentBiome=town.terrain?.[current.y]?.[current.x];
    const currentOpen=Number.isInteger(current.x)&&Number.isInteger(current.y)
      &&current.x>=minX&&current.x<=maxX&&current.y>=minY&&current.y<=maxY
      &&!blocked.has(key(current))&&currentBiome!=='v'&&(!town.chunked||!!currentBiome);
    // Most replans already stand on an open tile. Otherwise retain the first
    // closest tile in column order without allocating every open map tile.
    let start=currentOpen?current:preferred,best=currentOpen?0:Infinity;
    for(let x=minX;x<=maxX;x++)for(let y=minY;y<=maxY;y++){
      const biome=town.terrain?.[y]?.[x];
      if(biome==='v'||(town.chunked&&!biome))blocked.add(`${x},${y}`);
      else if(best>0&&!blocked.has(`${x},${y}`)){
        const delta=Math.abs(x-current.x)+Math.abs(y-current.y);
        if(delta<best){start={x,y};best=delta;}
      }
    }
    let position=state?{x:state.x,y:state.y}:start;
    let approach=distance(position,start)>1e-9?[start]:[];
    if(villager.logistics_target){
      const target=villager.logistics_target,trip=villager.logistics_trip;
      const remaining=trip?.path.slice(state?0:(trip.step || 0)).map(([x,y])=>({x,y}));
      const path=route(start,remaining?.[0] || target,blocked,size,false,bounds);
      const deliveryPath=path===null?[]:[...approach,...path,...(remaining?.slice(1) || [])];
      return {...position,path:deliveryPath,phase:path===null?'waiting':'walking',source:target,home:null,blocked,size,bounds,transport:true,
        transportCargo:trip?.cargo || {},pickupRemaining:(trip?.path.length || 1)-1-(trip?.pickup_step ?? 0),
        deliveryRemaining:(trip?.path.length || 1)-1-(trip?.delivery_step ?? 0),
        transportSpeed:1+.25*Math.floor((villager.hauling || 0)/20),elapsed:0,clock:state?.clock || index*.37,facing:state?.facing || 1};
    }
    const source=villager.construction_target || (villager.job==='idle'?null:villager.work_target);
    const kind=villager.job==='woodcutting'?'woodcutter':villager.job==='fishing'?'fishing_hut':'farm';
    // Match the host's workplace selection, including multiple assigned workers.
    const workplaces=town.buildings.filter(b=>b.kind===kind&&!b.construction&&(kind==='farm'||town.map_resources.some(r=>
      r.kind===(kind==='woodcutter'?'tree':'water')&&distance(b,r)<=(kind==='woodcutter'?10:kind==='fishing_hut'?fishingReach(b):3))));
    const assigned=Number.isInteger(villager.building_index)?town.buildings[villager.building_index]:null;
    const workplace=(assigned?.kind==='acre'?town.buildings[assigned.farm_index]:assigned) || (source?.kind==='acre'?town.buildings[source.farm_index]:workplaces[index%workplaces.length]);
    const home=villager.construction_target || workplace || town.buildings.find(b=>b.kind==='hall');
    let pier=[],workSpot=null,sailing=null;
    if(!villager.construction_target&&villager.job==='farming'&&source?.kind==='acre'){
      // Farmers tend the field itself rather than working outside its boundary.
      for(const acre of town.buildings.filter(b=>b.kind==='acre'&&!b.construction))blocked.delete(key(acre));
      workSpot=source;
    }
    const boating=!villager.construction_target&&villager.job==='fishing'&&source&&home?.kind==='fishing_hut'&&fishingBoating(town,home,index);
    if(!villager.construction_target&&villager.job==='fishing'&&home?.kind==='fishing_hut')pier=fishingPier(town,home);
    if(boating){
      const depths=waterDepths(town.map_resources);
      const water=new Set(town.map_resources.filter(r=>r.kind==='water'&&distance(home,r)<=fishingReach(home)
        &&depths.get(key(r))<=boatLevel(home)+1&&town.terrain?.[r.y]?.[r.x]!=='v').map(key));
      const occupied=new Set([...(town.obstacles || town.buildings),...(town.npcs || [])].map(key));
      const queue=[source],seen=new Set([key(source)]);
      for(const tile of queue){
        if(!water.has(key(tile))||occupied.has(key(tile)))continue;
        blocked.delete(key(tile));
        for(const next of neighbors(tile))if(water.has(key(next))&&!occupied.has(key(next))&&!seen.has(key(next))){
          seen.add(key(next));queue.push(next);
        }
      }
      const connected=queue.filter(tile=>water.has(key(tile))&&!occupied.has(key(tile)));
      const berth=connected.sort((a,b)=>distance(home,a)-distance(home,b)||a.y-b.y||a.x-b.x)[0] || source;
      const seaBounds={minX:Math.max(0,home.x-fishingReach(home)),maxX:Math.min(size-1,home.x+fishingReach(home)),
        minY:Math.max(0,home.y-fishingReach(home)),maxY:Math.min(size-1,home.y+fishingReach(home))};
      const seaBlocked=new Set(),landBlocked=new Set([...(town.obstacles || town.buildings),...town.map_resources,...(town.npcs || [])].map(key));
      landBlocked.delete(key(home));landBlocked.delete(key(berth));
      const connectedWater=new Set(connected.map(key));
      for(let x=seaBounds.minX;x<=seaBounds.maxX;x++)for(let y=seaBounds.minY;y<=seaBounds.maxY;y++){
        if(!connectedWater.has(`${x},${y}`))seaBlocked.add(`${x},${y}`);
      }
      // Preserve terrain blockers for the land approach to the berth.
      for(const tile of blocked)if(!water.has(tile))landBlocked.add(tile);
      landBlocked.delete(key(home));
      sailing={water:connectedWater,berth,landBlocked,seaBlocked,seaBounds};
      blocked.delete(key(home));workSpot=source;
    }else if(!villager.construction_target&&villager.job==='fishing'&&source&&home?.kind==='fishing_hut'){
      // A hut can occupy the only bank tile; its pier provides access over water.
      const pierBlocked=new Set(blocked);
      pierBlocked.delete(key(home));
      for(const water of town.map_resources.filter(r=>r.kind==='water'&&distance(home,r)<=3)){
        if(town.terrain?.[water.y]?.[water.x]!=='v')pierBlocked.delete(key(water));
      }
      const deck=route(home,source,pierBlocked,size,false,bounds);
      if(deck&&deck.length<=3){
        pier=[home,...deck];workSpot=source;
        for(const tile of pier)blocked.delete(key(tile));
      }
    }
    // Fishing opens water and pier tiles after the general spawn search.
    // Replanning offshore must continue from the current waypoint, not a bank.
    if((boating||pier.length)&&!blocked.has(key(current))&&Number.isInteger(current.x)&&Number.isInteger(current.y)
      &&current.x>=minX&&current.x<=maxX&&current.y>=minY&&current.y<=maxY){
      start=current;
      if(!state)position=start;
      approach=distance(position,start)>1e-9?[start]:[];
    }
    const routing={blocked,size,bounds,sailing,source};
    let path=source?travel(start,workSpot || source,routing,!workSpot):home?route(start,home,blocked,size,true,bounds):[];
    if(path!==null)path=[...approach,...path];
    let castTarget=villager.job==='fishing'?source:null;
    if(villager.job==='fishing'&&source){
      const water=new Set(town.map_resources.filter(r=>r.kind==='water').map(key));
      const occupied=new Set([...(town.obstacles || town.buildings),...(town.npcs || [])].map(key));
      // Keep the entire cast over connected water, preferring farther water away from the hut.
      for(const direction of neighbors({x:0,y:0})){
        for(let reach=1;reach<=3;reach++){
          const tile={x:source.x+direction.x*reach,y:source.y+direction.y*reach};
          if(!water.has(key(tile))||occupied.has(key(tile))||town.terrain?.[tile.y]?.[tile.x]==='v')break;
          if(distance(tile,source)>distance(castTarget,source)||
            (distance(tile,source)===distance(castTarget,source)&&home&&distance(tile,home)>distance(castTarget,home)))castTarget=tile;
        }
      }
    }
    return {...position,phase:path===null?'waiting':path.length?'walking':source?'working':'idle',
      path:path || [],source,home,pier,workSpot,castTarget,fishingCaught:null,blocked,size,bounds,boating,sailing,workDuration:villager.job==='farming'?8:3.2,
      farming:villager.job==='farming'&&!villager.construction_target,water:town.map_resources.filter(r=>r.kind==='water'),waterReady:false,
      stationary:!!source&&source.kind===workplace?.kind&&distance(source,workplace)===0&&!villager.construction_target,
      construction:!!villager.construction_target,elapsed:0,clock:state?.clock || index*.37,facing:state?.facing || 1};
  }
  function fetchWater(state) {
    // Search all reachable banks in one pass; water need not be near the farm.
    const banks=new Map();
    for(const water of state.water)for(const bank of neighbors(water)){
      if(bank.x>=0&&bank.y>=0&&bank.x<state.size&&bank.y<state.size&&!state.blocked.has(key(bank)))banks.set(key(bank),water);
    }
    const path=route(state,new Set(banks.keys()),state.blocked,state.size,false,state.bounds);
    if(path===null){state.phase='waiting_water';return;}
    const bank=path.at(-1) || state;
    state.waterSource=banks.get(key(bank));state.path=path;state.waterElapsed=0;
    state.phase=path.length?'fetching_water':'collecting_water';
  }
  function advance(state, seconds, active) {
    if(state.storagePaused&&!state.transport&&!state.construction)return;
    const cargo=()=>state.transport&&state.path.length<=state.pickupRemaining&&state.path.length>state.deliveryRemaining?state.transportCargo:{};
    state.cargo=cargo();
    // A completed host-owned drop-off waits for the next authoritative cycle.
    if(state.hostWork&&state.phase==='depositing'&&state.elapsed>=.65)return;
    if(active || state.path.length || !state.source)state.clock+=seconds;
    if(state.path.length){
      let remaining=seconds;
      while(state.path.length&&remaining>0){
        const next=state.path[0],dx=next.x-state.x,dy=next.y-state.y,length=Math.hypot(dx,dy);
        if(dx-dy!==0)state.facing=dx-dy>0?1:-1;
        const speed=1.8*(state.transport?(state.transportSpeed || 1):1)*(state.roads?.has(`${Math.floor(state.x+.5)},${Math.floor(state.y+.5)}`)?1.5:1);
        const travel=Math.min(remaining*speed,.05,length);
        remaining-=travel/speed;
        if(length<=travel+1e-9){state.x=next.x;state.y=next.y;state.path.shift();}
        else{state.x+=dx/length*travel;state.y+=dy/length*travel;}
      }
      if(!state.path.length){
        if(state.phase==='fetching_water'){state.phase='collecting_water';state.waterElapsed=0;}
        else if(state.phase==='returning_water'){state.phase='working';state.waterReady=true;}
        else{state.phase=state.phase==='returning'?'depositing':state.source?'working':'idle';state.elapsed=0;}
      }
      state.cargo=cargo();
      return;
    }
    if(state.transport){state.phase='waiting';return;}
    if(state.construction){state.elapsed+=seconds;return;}
    if(!active || !state.source || state.phase==='waiting')return;
    if(state.phase==='waiting_water')return;
    if(state.phase==='collecting_water'){
      state.waterElapsed+=seconds;
      if(state.waterElapsed>=.65){
        const path=route(state,state.workSpot || state.source,state.blocked,state.size,!state.workSpot,state.bounds);
        state.path=path || [];state.phase=path===null?'waiting_water':path.length?'returning_water':'working';
        state.waterReady=path!==null&&!path.length;
      }
      return;
    }
    state.elapsed+=seconds;
    if(state.farming&&state.phase==='working'&&state.elapsed>=4&&!state.waterReady){
      state.elapsed=4;fetchWater(state);return;
    }
    if(state.phase==='working'&&state.castTarget&&state.elapsed>=2.35&&state.fishingCaught===null){
      state.fishingCaught=state.latestFishingCatch?.caught ?? true;
    }
    if(state.phase==='working'&&state.elapsed>=(state.workDuration || 3.2)){
      if(state.fishingCaught===false){state.elapsed=0;state.fishingCaught=null;return;}
      const path=state.stationary?[]:state.home?travel(state,state.home,state):null;
      if(path===null)return;
      state.path=path;state.phase=path.length?'returning':'depositing';state.elapsed=0;
    }else if(state.phase==='depositing'&&state.elapsed>=.65){
      if(state.hostWork){state.elapsed=.65;return;}
      const path=travel(state,state.workSpot || state.source,state,!state.workSpot);
      state.path=path || [];state.phase=path===null?'waiting':path.length?'walking':'working';state.elapsed=0;state.fishingCaught=null;state.waterReady=false;
    }
  }
  return {route,plan,advance,waterDepths,fishingPier};
})();
if(typeof module!=='undefined')module.exports=WorkerAnimation;
