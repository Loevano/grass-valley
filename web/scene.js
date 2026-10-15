// Derived browser data. The host remains authoritative; drawing only reads this data.
const ClientScene = (() => {
  const tile = p => `${p.x},${p.y}`;
  const footprint = p => {
    const [width,depth]=p.footprint || [1,1],tiles=[];
    for(let dx=0;dx<width;dx++)for(let dy=0;dy<depth;dy++)tiles.push({x:p.x+dx,y:p.y+dy});
    return tiles;
  };

  const geometry = items => JSON.stringify(items.map(p => [p.kind,p.x,p.y,p.footprint,p.farm_index,!!p.construction,p.fence_orientation]));
  class Overlay {
    constructor(base){this.base=base;this.added=new Set();this.removed=new Set();}
    has(key){return this.added.has(key)||(!this.removed.has(key)&&this.base.has(key));}
    add(key){this.removed.delete(key);this.added.add(key);return this;}
    delete(key){this.added.delete(key);this.removed.add(key);return true;}
    *[Symbol.iterator](){for(const key of this.base)if(!this.removed.has(key))yield key;yield* this.added;}
  }
  // A job is a resumable generator. Replacing its key cancels obsolete work.
  class Queue {
    constructor(clock=()=>performance.now()){this.clock=clock;this.jobs=new Map();this.completed=0;}
    schedule(key,version,steps,done){
      if(this.jobs.get(key)?.version===version)return;
      this.jobs.set(key,{version,steps,done});
    }
    cancel(key){this.jobs.delete(key);}
    clear(){this.jobs.clear();}
    run(budget=2){
      const start=this.clock();let steps=0;
      while(this.jobs.size&&this.clock()-start<budget){
        const [key,job]=this.jobs.entries().next().value;
        const result=job.steps.next();steps++;
        if(result.done){this.jobs.delete(key);this.completed++;job.done(result.value);}
        else{this.jobs.delete(key);this.jobs.set(key,job);}
        // Also bound work under fake clocks and unusually cheap jobs.
        if(steps>=2048)break;
      }
      return this.clock()-start;
    }
  }
  const queue=new Queue();
  let current=null;
  function* prepare(world,previous=current){
    const landscape=world.shared_map;
    const entries=Object.entries(world.towns);
    const resources=landscape?.map_resources || [];
    const buildings=landscape?entries.flatMap(([townId,t])=>t.buildings.map((b,objectIndex)=>({...b,townId,objectIndex}))):[];
    const scene={world,buildings,regions:new Map(),capacities:new WeakMap(),buckets:new Map(),occupied:new Set(),playerBuildings:new Set(),water:new Set(),
      geometry:geometry(buildings),resources,tasks:new WeakMap(),resetTowns:[],roads:new Set()};
    const reuse=previous?.world.world_id===world.world_id&&previous.resources===resources;
    if(reuse){scene.buckets=previous.buckets;scene.water=previous.water;}
    else for(let i=0;i<resources.length;i++){
      const r=resources[i],key=`${Math.floor(r.x/16)},${Math.floor(r.y/16)}`;
      if(!scene.buckets.has(key))scene.buckets.set(key,[]);
      scene.buckets.get(key).push(r);
      if(r.kind==='water')scene.water.add(tile(r));
      if(i%256===255)yield;
    }
    for(const building of buildings)footprint(building).forEach(p=>scene.playerBuildings.add(tile(p)));
    for(const [,town] of entries)for(const road of town.roads || [])if(!road.construction)scene.roads.add(tile(road));
    for(const p of [...buildings,...(landscape?.npcs || [])])footprint(p).forEach(p=>scene.occupied.add(tile(p)));
    // Only affected towns rebuild their local geometry. No global scans per worker.
    for(const [id,town] of entries){
      const source=landscape || town;
      const center=town.town_origin || town.buildings.find(b=>b.kind==='hall') || {x:0,y:0};
      const bounds=source.chunked?{minX:Math.max(0,center.x-60),maxX:Math.min((source.map_size || 120)-1,center.x+60),
        minY:Math.max(0,center.y-60),maxY:Math.min((source.map_size || 120)-1,center.y+60)}:{minX:0,minY:0,maxX:(source.map_size || 120)-1,maxY:(source.map_size || 120)-1};
      const near=p=>p.x>=bounds.minX&&p.x<=bounds.maxX&&p.y>=bounds.minY&&p.y<=bounds.maxY;
      const localResources=[];
      if(landscape){
        for(let x=Math.floor(bounds.minX/16);x<=Math.floor(bounds.maxX/16);x++)
          for(let y=Math.floor(bounds.minY/16);y<=Math.floor(bounds.maxY/16);y++)localResources.push(...(scene.buckets.get(`${x},${y}`) || []).filter(near));
      }else localResources.push(...(town.map_resources || []).filter(near));
      const obstacles=(landscape?buildings:town.buildings).filter(near),npcs=(source.npcs || []).filter(near);
      const rows=[];
      for(let y=bounds.minY;y<=bounds.maxY;y++){
        let row='';for(let x=bounds.minX;x<=bounds.maxX;x++)row+=source.terrain?.[y]?.[x] || 'v';
        rows.push(row);if(y%8===0)yield;
      }
      const localRoads=(landscape?[...scene.roads]:(town.roads || []).filter(r=>!r.construction).map(tile)).filter(k=>{
        const [x,y]=k.split(',').map(Number);return near({x,y});
      }).sort();
      const occupancy=JSON.stringify([geometry(obstacles),geometry(localResources),geometry(npcs),rows,bounds]);
      // Avoid escaping the large occupancy signature a second time: layout
      // strings also appear in every worker's resumable planning job key.
      const layout=localRoads.length?occupancy+'|roads:'+localRoads.join(';'):occupancy;
      const old=previous?.world.world_id===world.world_id?previous.regions.get(id):null;
      const roster=JSON.stringify(town.villagers.map(v=>v.name));
      if(old&&old.roster!==roster)scene.resetTowns.push(id);
      let blocked;
      if(old?.occupancy===occupancy)blocked=old.blocked;
      else{
        blocked=new Set([...obstacles,...localResources,...npcs].flatMap(footprint).map(tile));
        for(let y=bounds.minY;y<=bounds.maxY;y++){
          for(let x=bounds.minX;x<=bounds.maxX;x++){
            const biome=source.terrain?.[y]?.[x];
            if(biome==='v'||(source.chunked&&!biome))blocked.add(`${x},${y}`);
          }
          if(y%8===0)yield;
        }
      }
      const routing={...town,...source,buildings:town.buildings,obstacles,map_resources:localResources,npcs,resourceBuckets:landscape?scene.buckets:null,
        home_hall:town.buildings.find(b=>b.kind==='hall'),navigation:{blocked,bounds,roads:new Set(localRoads),overlay:()=>new Overlay(blocked)}};
      const tiles=items=>items.map(({kind,x,y,farm_index,construction})=>[kind,x,y,farm_index,!!construction]);
      const crew=town.villagers.map(v=>[v.job,v.building_index]);
      const huts=town.buildings.filter(b=>b.kind==='fishing_hut').map(b=>[b.level,b.profession_upgrades]);
      for(const villager of town.villagers){
        const task=JSON.stringify([villager.job,villager.building_index,villager.work_target?tiles([villager.work_target]):null,
          villager.construction_target?tiles([villager.construction_target]):null,villager.job==='fishing'?[crew,huts]:null]);
        const trip=villager.logistics_trip;
        scene.tasks.set(villager,{task,signature:JSON.stringify([task,trip?[trip.id,trip.operation,trip.workplace,trip.storage,trip.path]:villager.logistics_target,!!villager.route_waiting])});
      }
      // Compare geometry once per snapshot, then use identity in worker frames.
      const layoutVersion=old?.layout===layout?old.layoutVersion:{};
      scene.regions.set(id,{layout,layoutVersion,occupancy,blocked,routing,roster});
      yield;
    }
    scene.geometry=world.world_id+'|'+scene.geometry+'|'+[...scene.regions].map(([id,r])=>id+':'+r.layout).join('|');
    return scene;
  }
  function commit(scene){if(current?.world.world_id!==scene.world.world_id)queue.clear();else queue.cancel('resource-art');current=scene;}
  function region(town){
    if(current?.world.towns)for(const [id,t] of Object.entries(current.world.towns))if(t===town)return current.regions.get(id);
    return null;
  }
  function animalRoutes(buildings,landscape,barn){
    const key=tile(barn),square=barn.kind==='barn',radius=square?3:2,width=radius*2+1,resources=landscape.map_resources || [];
    let cache=animalRoutes.cache;
    if(!cache||cache.resources!==resources||cache.buildings!==buildings||cache.terrain!==landscape.terrain){
      const buckets=current?.resources===resources?current.buckets:new Map();
      if(current?.resources!==resources)for(const p of resources){
        const bucket=`${Math.floor(p.x/16)},${Math.floor(p.y/16)}`;
        if(!buckets.has(bucket))buckets.set(bucket,[]);buckets.get(bucket).push(p);
      }
      cache=animalRoutes.cache={resources,buildings,terrain:landscape.terrain,buckets,occupied:new Set([...buildings,...(landscape.npcs || [])].flatMap(footprint).map(tile)),routes:new Map()};
    }
    if(cache.routes.has(key))return cache.routes.get(key).routes;
    const nearby=[];
    for(let x=Math.floor((barn.x-radius)/16);x<=Math.floor((barn.x+radius)/16);x++)
      for(let y=Math.floor((barn.y-radius)/16);y<=Math.floor((barn.y+radius)/16);y++)nearby.push(...(cache.buckets.get(`${x},${y}`) || []));
    const blocked=new Set(nearby.filter(p=>Math.abs(p.x-barn.x)<=radius&&Math.abs(p.y-barn.y)<=radius).flatMap(footprint).map(tile));
    const available=[];
    for(let x=barn.x-radius;x<=barn.x+radius;x++)for(let y=barn.y-radius;y<=barn.y+radius;y++){
      const biome=landscape.terrain?.[y]?.[x],k=`${x},${y}`;
      available.push(!cache.occupied.has(k)&&!blocked.has(k)&&biome!=='v'&&(!landscape.chunked||!!biome)&&x>=0&&y>=0&&x<(landscape.map_size || 120)&&y<(landscape.map_size || 120));
    }
    const signature=available.join('');
    const old=cache.routes.get(key);if(old?.signature===signature)return old.routes;
    const open=p=>(square?Math.max(Math.abs(p.x-barn.x),Math.abs(p.y-barn.y)):Math.abs(p.x-barn.x)+Math.abs(p.y-barn.y))<=radius
      &&tile(p)!==key&&available[(p.x-barn.x+radius)*width+p.y-barn.y+radius];
    const routes=[];
    // Start outside each accessible side; the barn itself is never a waypoint.
    for(const [dx,dy] of [[1,0],[0,1],[-1,0],[0,-1]]){
      const start={x:barn.x+dx,y:barn.y+dy};if(!open(start))continue;
      const work=[{...start,path:[start]}],seen=new Set([tile(start)]);
      for(const p of work){
        routes.push(p.path);if(!square&&p.path.length>=3)continue;
        for(const [dx,dy] of [[1,0],[0,1],[-1,0],[0,-1]]){
          const next={x:p.x+dx,y:p.y+dy},k=tile(next);
          if(!open(next)||seen.has(k))continue;
          seen.add(k);work.push({...next,path:[...p.path,next]});
        }
      }
    }
    routes.sort((a,b)=>b.length-a.length);cache.routes.set(key,{signature,routes});return routes;
  }
  function resourceAt(x,y){
    return current?.buckets.get(`${Math.floor(x/16)},${Math.floor(y/16)}`)?.find(r=>r.x===x&&r.y===y);
  }
  return {Overlay,Queue,queue,prepare,commit,region,animalRoutes,resourceAt,get current(){return current;}};
})();
if(typeof module!=='undefined')module.exports=ClientScene;
