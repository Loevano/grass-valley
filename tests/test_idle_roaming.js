const assert=require('node:assert/strict');
const Animation=require('../web/animations');
const client=require('./client_harness')();

client.run(`
  const finish=steps=>{let result;do{result=steps.next();}while(!result.done);return result.value;};
  const town={map_size:24,terrain:Array(24).fill('g'.repeat(24)),food:40,wood:40,
    buildings:[{kind:'hall',x:10,y:10,footprint:[2,2]},{kind:'farm',x:18,y:18}],
    map_resources:[{kind:'tree',x:9,y:12},{kind:'water',x:13,y:12}],
    npcs:[{x:8,y:10}],villagers:[{name:'Ada',job:'idle'},{name:'Bo',job:'idle'}]};
  town.terrain[13]='g'.repeat(12)+'v'+'g'.repeat(11);
  world={world_id:'idle-roaming',towns:{own:town}};viewed='own';
  const commit=()=>ClientScene.commit(finish(ClientScene.prepare(world,ClientScene.current)));
  const frame=seconds=>{const sprites=animatedVillagers(world.towns.own,seconds);ClientScene.queue.run();return sprites;};
  commit();frame(0);
  const original=JSON.stringify(world),states=[workers.get(0),workers.get(1)];
  const positions=[new Set(),new Set()],phases=new Set();
  for(let tick=0;tick<1200;tick++){
    if(tick%30===0){world=JSON.parse(JSON.stringify(world));commit();}
    const sprites=frame(.1);
    sprites.forEach((sprite,i)=>{
      const state=sprite.state;
      if(state!==states[i])throw Error('Polling discards an idle stroll');
      if(state.home.kind!=='hall')throw Error('Idle villagers roam around a workplace');
      for(const p of state.path){
        if(p.x<6||p.x>15||p.y<6||p.y>15)throw Error('Idle stroll leaves the hall neighborhood');
        if(state.blocked.has(p.x+','+p.y)||town.terrain[p.y][p.x]==='v')throw Error('Idle stroll crosses an obstacle');
      }
      positions[i].add(state.x.toFixed(2)+','+state.y.toFixed(2));phases.add(state.phase);
    });
  }
  if(positions.some(visited=>visited.size<40))throw Error('New villagers stop roaming');
  if(!phases.has('walking')||!phases.has('idle'))throw Error('Strolls do not alternate walking and resting');
  if(states[0].roamSeed===states[1].roamSeed)throw Error('Villagers share synchronized strolls');
  if(JSON.stringify(world)!==original)throw Error('Ambient roaming mutates authoritative state');

  const builder=world.towns.own.villagers[0];
  builder.construction_target={kind:'home',x:20,y:10};
  world.towns.own.buildings.push({...builder.construction_target,construction:{progress:0,total:30}});
  const before={x:states[0].x,y:states[0].y};
  commit();frame(0);
  const construction=workers.get(0);
  if(construction.roaming||!construction.construction)throw Error('Automatic construction does not cancel roaming');
  if(construction.x!==before.x||construction.y!==before.y)throw Error('Construction teleports a roaming villager');
  if(!construction.path.length)throw Error('Builder retains its ambient route');

  // Moving a hall refreshes the neighborhood for residents still resting.
  world.towns.own.buildings[0].x=16;commit();frame(0);
  if(workers.get(1).home.x!==16)throw Error('Idle villager keeps roaming around the old hall');
`);

// A tiny enclosed pocket has no safe stroll; retrying stays put without teleporting.
const pocket={map_size:6,terrain:Array(6).fill('g'.repeat(6)),
  buildings:[{kind:'hall',x:0,y:0,footprint:[2,2]}],
  map_resources:[{kind:'tree',x:1,y:2},{kind:'water',x:0,y:3}]};
let state=Animation.plan(pocket,{job:'idle',logistics_position:{x:0,y:2}},0);
for(let i=0;i<200;i++)Animation.advance(state,.1,false);
assert.equal(state.x,0);assert.equal(state.y,2);assert.equal(state.path.length,0);

// Unknown chunks, map edges, and newly blocked next steps remain impassable.
const edge={map_size:8,chunked:true,terrain:Array(8).fill('gggg'),
  buildings:[{kind:'hall',x:0,y:0,footprint:[2,2]}],map_resources:[]};
state=Animation.plan(edge,{job:'idle'},0);
for(let i=0;i<400;i++){
  Animation.advance(state,.1,false);
  assert(state.x>=0&&state.y>=0&&state.x<4&&state.y<8);
  assert(state.path.every(p=>p.x<4&&p.y<8));
}
while(!state.path.length)Animation.advance(state,.1,false);
const position={x:state.x,y:state.y};
state.blocked.add(`${state.path[0].x},${state.path[0].y}`);
Animation.advance(state,.1,false);
assert(state.needsPlan,'An obstructed next step requests a safe replan');
assert.equal(state.x,position.x);assert.equal(state.y,position.y);

const worker=Animation.plan(edge,{job:'building'},0);
assert.equal(worker.roaming,false,'Unassigned professions do not acquire idle roaming');
console.log('Idle roaming, pauses, collision safety, reassignment, and snapshot isolation passed');
