// Visual worker state stays in the browser; the host owns production and skills.
const WorkerAnimation = (() => {
  const key = p => `${p.x},${p.y}`;
  const footprint = p => {
    const [width,depth]=p.footprint || [1,1],tiles=[];
    for(let dx=0;dx<width;dx++)for(let dy=0;dy<depth;dy++)tiles.push({x:p.x+dx,y:p.y+dy});
    return tiles;
  };

  const distance = (a,b) => Math.abs(a.x-b.x)+Math.abs(a.y-b.y);
  const neighbors = p => [{x:p.x+1,y:p.y},{x:p.x,y:p.y+1},{x:p.x-1,y:p.y},{x:p.x,y:p.y-1}];
  const boatLevel = b => b?.profession_upgrades?.includes('master_boat')?3:b?.profession_upgrades?.includes('improved_boat')?2:b?.profession_upgrades?.includes('boat')?1:0;
  const fishingReach = b => 2+(b.level || 1)+(boatLevel(b)?3+2*(boatLevel(b)-1):0);
  // Scene occupancy is immutable and reused only while local geometry matches.
  // Per-worker exceptions form part of the key; plain mutable Sets bypass caching.
  const routeCaches=new WeakMap();
  function cachedRoutes(blocked){
    if(!blocked.base||!blocked.added||!blocked.removed)return null;
    if(!routeCaches.has(blocked.base))routeCaches.set(blocked.base,{paths:new Map(),steps:0});
    return routeCaches.get(blocked.base);
  }
  function fishingBoating(town,home,index){
    const huts=town.buildings.filter(b=>b.kind==='fishing_hut'&&(!b.construction||b.construction.upgrade)
      &&town.map_resources.some(r=>r.kind==='water'&&distance(b,r)<=fishingReach(b)));
    const crew=(town.villagers || []).map((v,i)=>({v,i})).filter(({v,i})=>v.job==='fishing'
      &&(Number.isInteger(v.building_index)?town.buildings[v.building_index]:huts[i%huts.length])===home);
    return crew.length?crew.slice(0,boatLevel(home)).some(worker=>worker.i===index):index<boatLevel(home);
  }
  function* fishingPierSteps(town,home){
    // Pier searches never leave this square. Avoid copying the entire map's
    // resources into a blocking set on each snapshot or worker replan.
    const size=town.map_size || 120,bounds={minX:Math.max(0,home.x-3),maxX:Math.min(size-1,home.x+3),
      minY:Math.max(0,home.y-3),maxY:Math.min(size-1,home.y+3)};
    let resources=town.map_resources;
    if(town.resourceBuckets){
      resources=[];
      for(let x=Math.floor(bounds.minX/16);x<=Math.floor(bounds.maxX/16);x++)
        for(let y=Math.floor(bounds.minY/16);y<=Math.floor(bounds.maxY/16);y++)
          resources.push(...(town.resourceBuckets.get(`${x},${y}`) || []));
    }
    resources=resources.filter(r=>r.x>=bounds.minX&&r.x<=bounds.maxX&&r.y>=bounds.minY&&r.y<=bounds.maxY);
    const occupied=new Set([...(town.obstacles || town.buildings),...(town.npcs || [])].flatMap(footprint).map(key));
    const waters=resources.filter(r=>r.kind==='water'&&distance(home,r)<=3&&!occupied.has(key(r)))
      .sort((a,b)=>distance(home,a)-distance(home,b)||a.y-b.y||a.x-b.x);
    const blocked=new Set([...occupied,...resources.map(key)]);
    blocked.fences=typeof barnFenceSegments!=='undefined'?barnFenceSegments(town.obstacles || town.buildings,size):[];
    blocked.delete(key(home));
    for(const water of waters)if(town.terrain?.[water.y]?.[water.x]!=='v')blocked.delete(key(water));
    for(let x=bounds.minX;x<=bounds.maxX;x++)for(let y=bounds.minY;y<=bounds.maxY;y++)
      if(town.terrain?.[y]?.[x]==='v'||(town.chunked&&!town.terrain?.[y]?.[x]))blocked.add(`${x},${y}`);
    for(const water of waters){
      const deck=(yield* routeSteps(home,water,blocked,size,false,bounds));
      if(deck&&deck.length<=3)return [home,...deck];
    }
    return [];
  }
  function fishingBerth(home,pier,water){
    const dock=pier.at(-1) || home,deck=new Set(pier.map(key));
    return water.filter(tile=>distance(tile,dock)===1&&!deck.has(key(tile)))
      .sort((a,b)=>distance(home,a)-distance(home,b)||a.y-b.y||a.x-b.x)[0] || dock;
  }
  function* waterDepthsSteps(resources){
    const water=new Set(resources.filter(r=>r.kind==='water').map(key)),depths=new Map(),queue=[];
    let visited=0;
    for(const tile of water){
      if(++visited%128===0)yield;
      const [x,y]=tile.split(',').map(Number),p={x,y};
      if(neighbors(p).some(next=>!water.has(key(next)))){depths.set(tile,1);queue.push(p);}
    }
    for(const tile of queue){
      if(++visited%128===0)yield;
      for(const next of neighbors(tile)){
        if(water.has(key(next))&&!depths.has(key(next))){depths.set(key(next),depths.get(key(tile))+1);queue.push(next);}
      }
    }
    return depths;
  }
  function* routeSteps(start, target, blocked, size=120, adjacent=true, bounds=null, roads=null, roadMode='smooth', diagonal=false) {
    if(adjacent&&!(target instanceof Set)&&target.footprint){
      const positions=footprint(target),tiles=new Set(positions.map(key));
      target=new Set(positions.flatMap(neighbors).filter(p=>!tiles.has(key(p))&&!blocked.has(key(p))
        &&positions.some(q=>distance(p,q)===1&&(typeof barnFenceCrossed==='undefined'||!barnFenceCrossed(p,q,blocked.fences)))).map(key));
    }
    const fences=blocked.fences || [],crosses=(a,b)=>typeof barnFenceCrossed!=='undefined'&&barnFenceCrossed(a,b,fences);
    const cache=cachedRoutes(blocked);
    const cacheKey=cache?JSON.stringify([key(start),target instanceof Set?[...target].sort():key(target),size,adjacent,
      bounds?[bounds.minX,bounds.maxX,bounds.minY,bounds.maxY]:null,[...blocked.added].sort(),[...blocked.removed].sort(),
      roads?.size?[...roads].sort():[],roads?.size?roadMode:null,diagonal,fences]):null;
    const copy=path=>path===null?null:path.map(p=>({...p}));
    if(cache?.paths.has(cacheKey)){
      const path=cache.paths.get(cacheKey);cache.paths.delete(cacheKey);cache.paths.set(cacheKey,path);
      return copy(path);
    }
    const remember=path=>{
      if(cache&&(path?.length || 1)<=32768){
        // Concurrent resumable searches can finish the same query.
        const old=cache.paths.get(cacheKey);
        if(cache.paths.has(cacheKey))cache.steps-=old?.length || 1;
        cache.paths.delete(cacheKey);cache.paths.set(cacheKey,copy(path));cache.steps+=path?.length || 1;
        while(cache.paths.size>256||cache.steps>32768){
          const oldest=cache.paths.keys().next().value;
          cache.steps-=cache.paths.get(oldest)?.length || 1;cache.paths.delete(oldest);
        }
      }
      return path;
    };
    const weighted=!!roads?.size||diagonal,previous=new Map([[key(start),null]]),costs=new Map([[key(start),0]]);
    const heuristic=p=>{
      if(target instanceof Set)return 0;
      const dx=Math.abs(p.x-target.x),dy=Math.abs(p.y-target.y);
      const length=diagonal?Math.max(dx,dy)+(Math.SQRT2-1)*Math.min(dx,dy):dx+dy;
      return (roads?.size?4:6)*Math.max(0,length-(adjacent?1:0));
    };
    const open=p=>p.x>=0&&p.y>=0&&p.x<size&&p.y<size&&!blocked.has(key(p))
      &&(!bounds||p.x>=bounds.minX&&p.x<=bounds.maxX&&p.y>=bounds.minY&&p.y<=bounds.maxY);
    const queue=weighted?[{p:start,cost:0,rank:heuristic(start),order:0}]:[start];
    let cursor=0,inspected=0,order=0;
    // Stable min-heap for travel-time A*. Cardinal uniform maps use BFS.
    const less=(a,b)=>a.rank<b.rank||(a.rank===b.rank&&a.order<b.order);
    const push=entry=>{
      let i=queue.length;queue.push(entry);
      while(i>0){const parent=(i-1)>>1;if(!less(entry,queue[parent]))break;queue[i]=queue[parent];i=parent;}
      queue[i]=entry;
    };
    const pop=()=>{
      const first=queue[0],last=queue.pop();
      if(queue.length){
        let i=0;
        while(i*2+1<queue.length){
          let child=i*2+1;if(child+1<queue.length&&less(queue[child+1],queue[child]))child++;
          if(!less(queue[child],last))break;queue[i]=queue[child];i=child;
        }
        queue[i]=last;
      }
      return first;
    };
    while(weighted?queue.length:cursor<queue.length){
      if(++inspected%128===0)yield;
      const entry=weighted?pop():null,p=weighted?entry.p:queue[cursor++];
      if(weighted&&entry.cost!==costs.get(key(p)))continue;
      if((target instanceof Set?target.has(key(p)):distance(p,target)===(adjacent?1:0))
          &&(!adjacent||target instanceof Set||!crosses(p,target))){
        const path=[];
        for(let step=p;previous.get(key(step));step=previous.get(key(step)))path.push(step);
        return remember(path.reverse());
      }
      const candidates=neighbors(p);
      if(diagonal)candidates.push({x:p.x+1,y:p.y+1},{x:p.x-1,y:p.y+1},{x:p.x-1,y:p.y-1},{x:p.x+1,y:p.y-1});
      for(const next of candidates){
        if(!open(next)||crosses(p,next)||(!weighted&&previous.has(key(next))))continue;
        const slant=next.x!==p.x&&next.y!==p.y;
        if(slant&&(!open({x:next.x,y:p.y})||!open({x:p.x,y:next.y})))continue;
        if(weighted){
          // Six cost units per normal step; road tiles cost four. Smooth
          // walkers cross halfway through each tile. Match host trip timing.
          const departure=roads?.has(key(p))?2:3,arrival=roads?.has(key(next))?2:3;
          const weight=(roadMode==='departure'?2*departure:roadMode==='arrival'?2*arrival:departure+arrival)*(slant?Math.SQRT2:1);
          const cost=entry.cost+weight;
          if(cost>=(costs.get(key(next)) ?? Infinity))continue;
          costs.set(key(next),cost);previous.set(key(next),p);
          push({p:next,cost,rank:cost+heuristic(next),order:++order});
        }else{previous.set(key(next),p);queue.push(next);}
      }
    }
    return remember(null);
  }
  function* travelSteps(start,target,state,adjacent=true){
    if(!state.sailing)return (yield* routeSteps(start,target,state.blocked,state.size,adjacent,state.bounds,state.roads,state.roadMode,true));
    const {water,berth,dock,landBlocked,boardingBlocked,seaBlocked,seaBounds}=state.sailing;
    const tile={x:Math.round(start.x),y:Math.round(start.y)};
    const afloat=water.has(key(tile))&&(key(tile)!==key(dock)||key(dock)===key(berth));
    if(target===state.source&&afloat)return (yield* routeSteps(start,target,seaBlocked,state.size,false,seaBounds));
    const outbound=target===state.source;
    const approach=outbound?(yield* routeSteps(start,dock,boardingBlocked,state.size,false,state.bounds,state.roads,'smooth',true)):
      afloat?(yield* routeSteps(start,berth,seaBlocked,state.size,false,seaBounds)):[];
    const sailing=outbound?(yield* routeSteps(berth,target,seaBlocked,state.size,false,seaBounds)):
      afloat?[]:(yield* routeSteps(start,berth,landBlocked,state.size,false,state.bounds,state.roads,'smooth',true));
    return approach===null||sailing===null?null:[...approach,...(outbound&&key(dock)!==key(berth)?[berth]:[]),...sailing];
  }
  function* planSteps(town, villager, index, state) {
    const roads=town.navigation?.roads || new Set((town.roads || []).filter(r=>!r.construction).map(key));
    const roadMode=villager.construction_target?'departure':'smooth';
    const blocked=town.navigation?town.navigation.overlay():new Set([...(town.obstacles || town.buildings),...town.map_resources,...(town.npcs || [])].flatMap(footprint).map(key));
    blocked.fences=typeof barnFenceSegments!=='undefined'?barnFenceSegments(town.obstacles || town.buildings,town.map_size || 120):[];
    if(villager.construction_target || villager.logistics_target || villager.job==='farming'){
      for(const acre of (town.obstacles || town.buildings).filter(b=>b.kind==='acre'&&!b.construction))blocked.delete(key(acre));
    }
    const hall=town.home_hall || town.buildings.find(b=>b.kind==='hall');
    const preferred=hall?{x:hall.x-2+index%5,y:hall.y+3+Math.floor(index/5)}:{x:3+index%5,y:8+Math.floor(index/5)};
    const size=town.map_size || 120;
    const center=town.town_origin || hall || preferred;
    const bounds=town.navigation?.bounds || (town.chunked?{minX:Math.max(0,center.x-60),maxX:Math.min(size-1,center.x+60),
      minY:Math.max(0,center.y-60),maxY:Math.min(size-1,center.y+60)}:null);
    const hostWork=!villager.construction_target&&!villager.logistics_target?villager.work_animation:null;
    const workPosition=hostWork&&(Number.isFinite(hostWork.x)&&Number.isFinite(hostWork.y)?hostWork:hostWork.playback?.at(-1));
    const savedPosition=(villager.construction_target?villager.construction_position:
      villager.logistics_target?villager.logistics_position:workPosition)
      || villager.logistics_position || villager.construction_position || preferred;
    if(villager.route_waiting){
      const position=state?{x:state.x,y:state.y}:savedPosition;
      const trip=villager.logistics_trip;
      return {...position,path:[],phase:'waiting',source:villager.construction_target || villager.work_target,
        home:null,blocked,size,bounds,roads,terrain:town.terrain,routeWaiting:true,construction:!!villager.construction_target,
        transport:!!villager.logistics_target,transportCargo:trip?.cargo || {},
        cargo:trip?.picked_up&&!trip.delivered?trip.cargo:villager.pending_output || {},
        elapsed:0,clock:state?.clock || 0,facing:state?.facing || hostWork?.facing || 1};
    }
    // A reload starts at the active host position, not a previous transport trip.
    // Existing sprites keep their visible position when a snapshot changes.
    const current=state?(!state.needsPlan&&state.path[0] || {x:Math.round(state.x),y:Math.round(state.y)}):
      workPosition?{x:Math.round(workPosition.x),y:Math.round(workPosition.y)}:savedPosition;
    const minX=bounds?.minX || 0,maxX=bounds?.maxX ?? size-1,minY=bounds?.minY || 0,maxY=bounds?.maxY ?? size-1;
    const currentBiome=town.terrain?.[current.y]?.[current.x];
    const currentOpen=Number.isInteger(current.x)&&Number.isInteger(current.y)
      &&current.x>=minX&&current.x<=maxX&&current.y>=minY&&current.y<=maxY
      &&!blocked.has(key(current))&&currentBiome!=='v'&&(!town.chunked||!!currentBiome);
    // Most replans already stand on an open tile. Otherwise retain the first
    // closest tile in column order without allocating every open map tile.
    let start=currentOpen?current:preferred,best=currentOpen?0:Infinity;
    let inspected=0;
    if(!town.navigation||best>0)for(let x=minX;x<=maxX;x++)for(let y=minY;y<=maxY;y++){
      if(++inspected%128===0)yield;
      const biome=town.terrain?.[y]?.[x];
      if(biome==='v'||(town.chunked&&!biome)){if(!town.navigation)blocked.add(`${x},${y}`);}
      else if(best>0&&!blocked.has(`${x},${y}`)){
        const delta=Math.abs(x-current.x)+Math.abs(y-current.y);
        if(delta<best){start={x,y};best=delta;}
      }
    }
    let position=state?{x:state.x,y:state.y}:workPosition?{x:workPosition.x,y:workPosition.y}:start;
    let approach=distance(position,start)>1e-9?[start]:[];
    if(villager.logistics_target&&!villager.construction_target){
      const target=villager.logistics_target,trip=villager.logistics_trip;
      const remaining=trip?.path.slice(state?0:(trip.step || 0)).map(([x,y])=>({x,y}));
      const path=(yield* routeSteps(start,remaining?.[0] || target,blocked,size,false,bounds,roads,'arrival',true));
      const deliveryPath=path===null?[]:[...approach,...path,...(remaining?.slice(1) || [])];
      return {...position,path:deliveryPath,phase:path===null?'waiting':'walking',source:target,home:null,blocked,size,bounds,roads,roadMode:'arrival',transport:true,boating:false,sailing:null,
        transportCargo:trip?.cargo || {},pickupRemaining:(trip?.path.length || 1)-1-(trip?.pickup_step ?? 0),
        deliveryRemaining:(trip?.path.length || 1)-1-(trip?.delivery_step ?? 0),
        transportHolding:trip?.picked_up&&!trip.delivered&&(trip.step || 0)>=trip.delivery_step,
        transportSpeed:1+.25*Math.floor((villager.hauling || 0)/20),elapsed:0,clock:state?.clock || index*.37,facing:state?.facing || 1};
    }
    const source=villager.construction_target || (villager.job==='idle'?null:villager.work_target);
    const kind=villager.job==='woodcutting'?'woodcutter':villager.job==='fishing'?'fishing_hut':'farm';
    // Match the host's workplace selection, including multiple assigned workers.
    const workplaces=town.buildings.filter(b=>b.kind===kind&&!b.construction&&(kind==='farm'||town.map_resources.some(r=>
      r.kind===(kind==='woodcutter'?'tree':'water')&&distance(b,r)<=(kind==='woodcutter'?10:kind==='fishing_hut'?fishingReach(b):3))));
    const assigned=Number.isInteger(villager.building_index)?town.buildings[villager.building_index]:null;
    const workplace=(assigned?.kind==='acre'?town.buildings[assigned.farm_index]:assigned) || (source?.kind==='acre'?town.buildings[source.farm_index]:workplaces[index%workplaces.length]);
    const roaming=villager.job==='idle'&&!villager.construction_target&&!!hall;
    const home=villager.construction_target || (roaming?hall:workplace || hall);
    let pier=[],workSpot=null,sailing=null;
    if(!villager.construction_target&&villager.job==='farming'&&source?.kind==='acre'){
      // Farmers tend the field itself rather than working outside its boundary.
      for(const acre of town.buildings.filter(b=>b.kind==='acre'&&!b.construction))blocked.delete(key(acre));
      workSpot=source;
    }
    const boating=!villager.construction_target&&villager.job==='fishing'&&source&&home?.kind==='fishing_hut'&&fishingBoating(town,home,index);
    if(!villager.construction_target&&villager.job==='fishing'&&home?.kind==='fishing_hut')pier=(yield* fishingPierSteps(town,home));
    if(boating){
      const depths=(yield* waterDepthsSteps(town.map_resources));
      const water=new Set(town.map_resources.filter(r=>r.kind==='water'&&distance(home,r)<=fishingReach(home)
        &&depths.get(key(r))<=boatLevel(home)+1&&town.terrain?.[r.y]?.[r.x]!=='v').map(key));
      const occupied=new Set([...(town.obstacles || town.buildings),...(town.npcs || [])].flatMap(footprint).map(key));
      const queue=[source],seen=new Set([key(source)]);
      for(const tile of queue){
        if(!water.has(key(tile))||occupied.has(key(tile)))continue;
        blocked.delete(key(tile));
        for(const next of neighbors(tile))if(water.has(key(next))&&!occupied.has(key(next))&&!seen.has(key(next))){
          seen.add(key(next));queue.push(next);
        }
      }
      const connected=queue.filter(tile=>water.has(key(tile))&&!occupied.has(key(tile)));
      const dock=pier.at(-1) || home,berth=fishingBerth(home,pier,connected);
      const seaBounds={minX:Math.max(0,home.x-fishingReach(home)),maxX:Math.min(size-1,home.x+fishingReach(home)),
        minY:Math.max(0,home.y-fishingReach(home)),maxY:Math.min(size-1,home.y+fishingReach(home))};
      const seaBlocked=new Set(),landBlocked=new Set([...(town.obstacles || town.buildings),...town.map_resources,...(town.npcs || [])].flatMap(footprint).map(key));
      seaBlocked.fences=blocked.fences;
      for(const tile of pier)landBlocked.delete(key(tile));
      landBlocked.delete(key(home));landBlocked.delete(key(berth));
      const connectedWater=new Set(connected.map(key));
      for(let x=seaBounds.minX;x<=seaBounds.maxX;x++)for(let y=seaBounds.minY;y<=seaBounds.maxY;y++){
        if(!connectedWater.has(`${x},${y}`))seaBlocked.add(`${x},${y}`);
      }
      // Preserve terrain blockers for the land approach to the berth.
      for(const tile of blocked)if(!water.has(tile))landBlocked.add(tile);
      landBlocked.delete(key(home));
      landBlocked.fences=blocked.fences;
      const boardingBlocked=new Set(landBlocked);boardingBlocked.fences=blocked.fences;
      if(key(dock)!==key(berth))boardingBlocked.add(key(berth));
      sailing={water:connectedWater,berth,dock,landBlocked,boardingBlocked,seaBlocked,seaBounds};
      blocked.delete(key(home));workSpot=source;
    }else if(!villager.construction_target&&villager.job==='fishing'&&source&&home?.kind==='fishing_hut'){
      // A hut can occupy the only bank tile; its pier provides access over water.
      const pierBlocked=new Set(blocked);pierBlocked.fences=blocked.fences;
      pierBlocked.delete(key(home));
      for(const water of town.map_resources.filter(r=>r.kind==='water'&&distance(home,r)<=3)){
        if(town.terrain?.[water.y]?.[water.x]!=='v')pierBlocked.delete(key(water));
      }
      const deck=(yield* routeSteps(home,source,pierBlocked,size,false,bounds));
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
      if(!state&&!workPosition)position=start;
      approach=distance(position,start)>1e-9?[start]:[];
    }
    const routing={blocked,size,bounds,sailing,source,roads,roadMode};
    const confirmed=!!villager.work_animation?.playback?.length&&!villager.construction_target&&!villager.logistics_target;
    let path=confirmed?[]:source?(yield* travelSteps(start,workSpot || source,routing,!workSpot)):home?(yield* routeSteps(start,home,blocked,size,true,bounds,roads,'smooth',true)):[];
    if(path!==null&&!confirmed)path=[...approach,...path];
    let castTarget=villager.job==='fishing'?source:null;
    if(villager.job==='fishing'&&source){
      const water=new Set(town.map_resources.filter(r=>r.kind==='water').map(key));
      const occupied=new Set([...(town.obstacles || town.buildings),...(town.npcs || [])].flatMap(footprint).map(key));
      // Keep the entire cast over connected water, preferring farther water away from the hut.
      for(const direction of neighbors({x:0,y:0})){
        for(let reach=1;reach<=(boating?1:3);reach++){
          const tile={x:source.x+direction.x*reach,y:source.y+direction.y*reach};
          if(!water.has(key(tile))||occupied.has(key(tile))||town.terrain?.[tile.y]?.[tile.x]==='v')break;
          if(distance(tile,source)>distance(castTarget,source)||
            (distance(tile,source)===distance(castTarget,source)&&home&&distance(tile,home)>distance(castTarget,home)))castTarget=tile;
        }
      }
    }
    return {...position,phase:path===null?'waiting':path.length?'walking':source?'working':'idle',
      path:path || [],source,home,pier,workSpot,castTarget,fishingCaught:null,blocked,size,bounds,roads,roadMode,boating,sailing,boatCapacity:[0,5,10,20][boatLevel(home)],boatLoad:0,workDuration:villager.job==='farming'?8:3.2,
      farming:villager.job==='farming'&&!villager.construction_target,
      fieldCycle:villager.job==='farming'&&!!source?.field,
      water:[...town.map_resources.filter(r=>r.kind==='water'),...(town.obstacles || town.buildings).filter(b=>b.kind==='well'&&!b.construction).map(b=>({kind:'well',x:b.x,y:b.y}))],waterReady:false,
      stationary:!!source&&source.kind===workplace?.kind&&distance(source,workplace)===0&&!villager.construction_target,
      roaming,roamSeed:state?.roamSeed ?? index+1,roamWait:1.5+index%5*.6,
      construction:!!villager.construction_target,terrain:town.terrain,elapsed:0,clock:state?.clock || index*.37,facing:state?.facing || 1};
  }
  function pathBlocked(state, next=state.path[0]) {
    if(!next)return false;
    const biome=state.terrain?.[next.y]?.[next.x];
    return (typeof barnFenceCrossed!=='undefined'&&barnFenceCrossed(state,next,state.blocked?.fences)) || state.blocked?.has(key(next)) || next.x<0 || next.y<0 || next.x>=state.size || next.y>=state.size
      || biome==='v' || (state.bounds&&(!biome || next.x<state.bounds.minX || next.x>state.bounds.maxX
        || next.y<state.bounds.minY || next.y>state.bounds.maxY));
  }
  function roam(state) {
    // Short ambient strolls need only adjacent collision checks, never a map search.
    const random=()=>state.roamSeed=(Math.imul(state.roamSeed,1664525)+1013904223)>>>0;
    const hall=state.home,[width,depth]=hall.footprint || [1,1];
    let position={x:Math.round(state.x),y:Math.round(state.y)};
    const visited=new Set([key(position)]),steps=2+random()%3;
    for(let i=0;i<steps;i++){
      const choices=neighbors(position).filter(p=>p.x>=hall.x-4&&p.x<=hall.x+width+3
        &&p.y>=hall.y-4&&p.y<=hall.y+depth+3&&!visited.has(key(p))&&!pathBlocked({...state,...position},p));
      if(!choices.length)break;
      position=choices[random()%choices.length];visited.add(key(position));state.path.push(position);
    }
    state.phase=state.path.length?'walking':'idle';
    state.roamWait=2+random()%300/100;
  }
  function refreshNavigation(state, town, villager) {
    // Geometry updates refresh collision data without changing an ongoing route.
    const blocked=town.navigation?town.navigation.overlay():new Set([...(town.obstacles || town.buildings),
      ...town.map_resources,...(town.npcs || [])].flatMap(footprint).map(key));
    blocked.fences=typeof barnFenceSegments!=='undefined'?barnFenceSegments(town.obstacles || town.buildings,town.map_size || 120):[];
    if(state.farming || state.construction || state.transport)
      for(const acre of (town.obstacles || town.buildings).filter(b=>b.kind==='acre'&&!b.construction))blocked.delete(key(acre));
    const occupied=new Set([...(town.obstacles || town.buildings),...(town.npcs || [])].flatMap(footprint).map(key));
    const water=new Set(town.map_resources.filter(r=>r.kind==='water').map(key));
    const home=state.home&&town.buildings.find(b=>b.kind===state.home.kind&&key(b)===key(state.home));
    if(state.home&&!home)return false; // The destination moved or disappeared.
    for(const p of state.pier || []){
      const biome=town.terrain?.[p.y]?.[p.x];
      if(biome!=='v'&&(!town.chunked||biome)
        &&((home&&key(p)===key(home))||(!occupied.has(key(p))&&water.has(key(p)))))blocked.delete(key(p));
    }
    if(state.sailing){
      const sailing=state.sailing,landBlocked=new Set(blocked);
      landBlocked.delete(key(sailing.berth));
      landBlocked.fences=blocked.fences;
      const boardingBlocked=new Set(landBlocked);boardingBlocked.fences=blocked.fences;
      if(key(sailing.dock)!==key(sailing.berth))boardingBlocked.add(key(sailing.berth));
      const available=new Set([...sailing.water].filter(k=>{
        const [x,y]=k.split(',').map(Number),biome=town.terrain?.[y]?.[x];
        return water.has(k)&&!occupied.has(k)&&biome!=='v'&&(!town.chunked||biome);
      }));
      const seaBlocked=new Set(),bounds=sailing.seaBounds;
      seaBlocked.fences=blocked.fences;
      for(let x=bounds.minX;x<=bounds.maxX;x++)for(let y=bounds.minY;y<=bounds.maxY;y++)
        if(!available.has(`${x},${y}`))seaBlocked.add(`${x},${y}`);
      for(const k of available)blocked.delete(k);
      state.sailing={...sailing,water:available,landBlocked,boardingBlocked,seaBlocked};
    }
    Object.assign(state,{blocked,terrain:town.terrain,size:town.map_size || 120,
      bounds:town.navigation?.bounds || state.bounds,home:home || state.home,
      source:villager.construction_target || (state.transport?state.source:villager.work_target) || state.source,
      water:[...town.map_resources.filter(r=>r.kind==='water'),...(town.obstacles || town.buildings)
        .filter(b=>b.kind==='well'&&!b.construction).map(b=>({kind:'well',x:b.x,y:b.y}))]});
    return !pathBlocked(state)&&!(state.phase==='waiting'&&!state.path.length&&!villager.work_animation?.playback?.length);
  }
  function* fetchWaterSteps(state) {
    // Search all reachable banks in one pass; water need not be near the farm.
    const banks=new Map();
    for(const water of state.water)for(const bank of neighbors(water)){
      if(bank.x>=0&&bank.y>=0&&bank.x<state.size&&bank.y<state.size&&!state.blocked.has(key(bank)))banks.set(key(bank),water);
    }
    const path=(yield* routeSteps(state,new Set(banks.keys()),state.blocked,state.size,false,state.bounds,state.roads,'smooth',true));
    if(path===null){state.phase='waiting_water';return;}
    const bank=path.at(-1) || state;
    state.waterSource=banks.get(key(bank));state.path=path;state.waterElapsed=0;
    state.phase=path.length?'fetching_water':'collecting_water';
  }
  function* advanceSteps(state, seconds, active) {
    if(state.routeWaiting)return;
    if(state.storagePaused&&!state.transport&&!state.construction)return;
    const cargo=()=>state.transport&&((state.transportHolding&&state.path.length<=state.deliveryRemaining)
      ||(state.path.length<=state.pickupRemaining&&state.path.length>state.deliveryRemaining))?state.transportCargo:{};
    state.cargo=cargo();
    // A completed host-owned drop-off waits for the next authoritative cycle.
    if(state.hostWork&&state.phase==='depositing'&&state.elapsed>=.65)return;
    if(active || state.path.length || !state.source)state.clock+=seconds;
    if(state.path.length){
      let remaining=seconds;
      while(state.path.length&&remaining>0){
        if(state.transportHolding&&state.path.length<=state.deliveryRemaining){
          state.phase='waiting';state.cargo=state.transportCargo;return;
        }
        if(pathBlocked(state)){state.needsPlan=true;state.phase='waiting';return;}
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
    if(state.roaming){
      state.roamWait-=seconds;
      if(state.roamWait<=0&&seconds>0)roam(state);
      return;
    }
    // Field actions and growth are confirmed by the host, never predicted here.
    if(state.fieldCycle)return;
    if(!active || !state.source || state.phase==='waiting')return;
    if(state.phase==='waiting_water')return;
    if(state.phase==='collecting_water'){
      state.waterElapsed+=seconds;
      if(state.waterElapsed>=.65){
        const path=(yield* routeSteps(state,state.workSpot || state.source,state.blocked,state.size,!state.workSpot,state.bounds,state.roads,'smooth',true));
        state.path=path || [];state.phase=path===null?'waiting_water':path.length?'returning_water':'working';
        state.waterReady=path!==null&&!path.length;
      }
      return;
    }
    state.elapsed+=seconds;
    if(state.farming&&state.phase==='working'&&state.elapsed>=4&&!state.waterReady){
      state.elapsed=4;(yield* fetchWaterSteps(state));return;
    }
    if(state.phase==='working'&&state.castTarget&&state.elapsed>=2.35&&state.fishingCaught===null){
      state.fishingCaught=state.latestFishingCatch?.caught ?? true;
    }
    if(state.phase==='working'&&state.elapsed>=(state.workDuration || 3.2)){
      if(state.boating&&state.boatLoad<(state.boatCapacity || 5)&&!state.boatReturnRequested){state.elapsed=0;state.fishingCaught=null;return;}
      if(!state.boating&&state.fishingCaught===false){state.elapsed=0;state.fishingCaught=null;return;}
      const path=state.stationary?[]:state.home?(yield* travelSteps(state,state.home,state)):null;
      if(path===null)return;
      state.path=path;state.phase=path.length?'returning':'depositing';state.elapsed=0;
    }else if(state.phase==='depositing'&&state.elapsed>=.65){
      if(state.hostWork){state.elapsed=.65;return;}
      const path=(yield* travelSteps(state,state.workSpot || state.source,state,!state.workSpot));
      state.path=path || [];state.phase=path===null?'waiting':path.length?'walking':'working';state.elapsed=0;state.fishingCaught=null;state.waterReady=false;
    }
  }
  function joinPlayback(samples, position) {
    const start=samples[0],duration=Math.hypot(start.x-position.x,start.y-position.y)/1.8;
    if(duration<1e-9)return samples;
    // Missed updates and job changes can leave a gap before the first confirmed
    // waypoint. Walk that gap separately; changing just its starting position
    // compresses the distance into the next waypoint's short (or zero) time.
    return [{...start,x:position.x,y:position.y,time:0,phase:'walking',elapsed:0,farmAction:null},
      ...samples.map(sample=>({...sample,time:sample.time+duration}))];
  }
  // Play confirmed host waypoints one simulation cycle behind. No route searches.
  function playback(state, host, seconds) {
    const task=JSON.stringify(host.task);
    if(state.hostTask===task&&host.version<state.hostVersion)return;
    const first=state.playbackQueue===undefined;
    if(!state.playbackQueue)state.playbackQueue=[];
    if(state.hostVersion!==host.version || state.hostTask!==task){
      if(state.hostVersion===undefined&&first){
        Object.assign(state,{x:host.x,y:host.y,phase:host.phase,elapsed:host.elapsed,
          farmAction:host.farmAction,farmField:host.playback.at(-1)?.farmField,
          butcherAnimal:host.butcherAnimal,butcherPickup:host.butcherPickup,
          waterReady:host.waterReady,waterSource:host.waterSource,path:[]});
      }else if(host.playback?.length){
        const hostSamples=host.playback.map(p=>({...p}));
        const tail=state.playbackQueue.at(-1)?.samples.at(-1) || state;
        state.playbackQueue.push({samples:joinPlayback(hostSamples,tail),hostSamples,time:0,index:0});
        // A background tab may miss updates. Bound retained presentation history.
        if(state.playbackQueue.length>3){
          state.playbackQueue.splice(0,state.playbackQueue.length-3);
          const next=state.playbackQueue[0];
          Object.assign(next,{samples:joinPlayback(next.hostSamples,state),time:0,index:0});
        }
      }
      state.hostVersion=host.version;state.hostTask=task;state.hostWork=true;
    }
    let remaining=seconds;
    while(state.playbackQueue.length){
      const chunk=state.playbackQueue[0],end=chunk.samples.at(-1).time;
      const take=Math.min(remaining,Math.max(0,end-chunk.time));chunk.time+=take;remaining-=take;
      while(chunk.index+1<chunk.samples.length&&chunk.samples[chunk.index+1].time<=chunk.time)chunk.index++;
      const a=chunk.samples[chunk.index],b=chunk.samples[chunk.index+1] || a;
      const fraction=b.time>a.time?(chunk.time-a.time)/(b.time-a.time):0;
      Object.assign(state,{x:a.x+(b.x-a.x)*fraction,y:a.y+(b.y-a.y)*fraction,
        phase:a.phase,elapsed:(a.elapsed || 0)+(a.x===b.x&&a.y===b.y?chunk.time-a.time:0),
        waterElapsed:(a.elapsed || 0)+chunk.time-a.time,waterReady:a.waterReady,waterSource:a.waterSource,farmAction:a.farmAction,farmField:a.farmField,
        butcherAnimal:a.butcherAnimal,butcherPickup:a.butcherPickup,
        facing:b.x-b.y!==a.x-a.y?(b.x-b.y>a.x-a.y?1:-1):a.facing,
        path:a.x!==b.x||a.y!==b.y?[{x:b.x,y:b.y}]:[]});
      state.clock+=take;
      if(chunk.time<end)break;
      state.playbackQueue.shift();
      if(remaining<=0)break;
    }
  }
  function fieldPresentation(state, field) {
    // Keep each acre at its earliest unplayed state, even after the host has
    // assigned the farmer another acre. Finished playback uses live growth.
    for(const chunk of state.playbackQueue || []){
      let sample;
      for(let i=chunk.index;i<chunk.samples.length;i++){
        const candidate=chunk.samples[i];
        if(candidate.farmField?.x===field.x&&candidate.farmField?.y===field.y){sample=candidate;break;}
      }
      const view=sample?.farmField;
      if(view?.x===field.x&&view?.y===field.y){
        // The crop clears when the sickle action finishes, not before arrival.
        return {...view,progress:sample.phase==='working'&&sample.farmAction==='harvesting'?0:view.progress};
      }
    }
    const view=state.farmField;
    if(state.phase==='working'&&state.farmAction==='harvesting'&&view?.stage==='ripe'
      &&view.x===field.x&&view.y===field.y)return {...view,progress:0};
  }
  function finish(steps){let result;do{result=steps.next();}while(!result.done);return result.value;}
  const route=(...args)=>finish(routeSteps(...args));
  const plan=(...args)=>finish(planSteps(...args));
  const advance=(...args)=>finish(advanceSteps(...args));
  const waterDepths=(...args)=>finish(waterDepthsSteps(...args));
  const fishingPier=(...args)=>finish(fishingPierSteps(...args));
  return {playback,fieldPresentation,refreshNavigation,route,plan,advance,waterDepths,fishingPier,fishingBoating,routeSteps,planSteps,advanceSteps};
})();
if(typeof module!=='undefined')module.exports=WorkerAnimation;
