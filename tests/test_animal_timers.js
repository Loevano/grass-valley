const assert=require('node:assert/strict');
const intervals=[];
const client=require('./client_harness')({setInterval:(callback,ms)=>intervals.push({callback,ms})});
client.run(`
  let clock=0,renderCalls=0;
  performance.now=()=>clock;
  world={world_id:'animals',simulation_clock:{tick_seconds:3,next_tick_in:3},
    progression:{animals:{chicken:{level:2,growth_seconds:30,output:'eggs'}}},
    towns:{own:{level:2,buildings:[{kind:'barn',x:5,y:5,animals:{chicken:1},animal_growth:{chicken:[0]}}],
      villagers:[{job:'husbandry',building_index:0},{job:'husbandry',building_index:0}],
      map_resources:[],npcs:[],roads:[],terrain:Array(12).fill('g'.repeat(12))}}};
  identity={town_id:'own',token:'test'};identities.animals=identity;
  viewed='own';panel='selection';selection={type:'building',index:0};worldReceivedAt=0;
  renderBarnAnimals(world.towns.own,world.towns.own.buildings[0],0,document.createElement('div'));
  const timer=renderBarnAnimals.timers.rows[0];
  const before=JSON.stringify(world.towns);
  draw=()=>{};updateActors=()=>{frameActors={villagers:[]};};
`);
assert.equal(client.run('timer.time.textContent'),'0m 12s until fully grown','A 10-second average cycle completes on the fourth host tick');
client.frame(0);
client.frame(1100);
assert.equal(client.run('timer.time.textContent'),'0m 11s until fully grown','The countdown ticks between snapshots');
assert.equal(client.run('JSON.stringify(world.towns)'),client.run('before'),'Countdowns never advance authoritative growth');
assert.equal(client.run('timer.percent.textContent'),'0%','Growth stays at its confirmed value');

client.run('world.towns.own.buildings[0].animal_growth.chicken=[27];worldReceivedAt=9000;clock=9000;');
client.frame(9000);
assert.equal(client.run('timer.time.textContent'),'0m 3s until fully grown','Near maturity, completion still waits for the next host tick');
client.frame(12000);
assert.equal(client.run('timer.time.textContent'),'Awaiting host update…','An overdue snapshot cannot announce an unconfirmed adulthood');
client.frame(60000);
assert.equal(client.run('timer.time.textContent'),'Awaiting host update…','A stalled connection does not replay imaginary cycles');

(async()=>{
  client.run(`
    refreshing=null;clock=12000;
    const dropdown={tagName:'SELECT'};document.activeElement=dropdown;
    render=()=>{renderCalls++;};renderWorldMenu=()=>{};
    request=async path=>{
      if(path==='/api/worlds')return {worlds:[]};
      const next=JSON.parse(JSON.stringify(world));
      next.towns.own.buildings[0].animal_growth.chicken=[6];
      return next;
    };
  `);
  intervals.find(interval=>interval.ms===2000).callback();
  await client.run('refreshing');
  assert.equal(client.run('world.towns.own.buildings[0].animal_growth.chicken[0]'),6,'Polling continues with a dropdown focused');
  assert.equal(client.run('document.activeElement===dropdown'),true);
  assert.equal(client.run('renderCalls'),0,'Snapshot refresh preserves an open dropdown');
  assert.equal(client.run('timer.time.textContent'),'0m 9s until fully grown','The confirmed young animal uses a tick-aligned countdown');
  assert.equal(client.run('timer.percent.textContent'),'20%','Existing growth bars receive confirmed snapshots without rebuilding controls');
  client.run('world.towns.own.villagers[1].construction_target={kind:"road"};');
  client.frame(12000);
  assert.equal(client.run('timer.time.textContent'),'0m 12s until fully grown','Construction helpers no longer speed the countdown');
  client.run('document.activeElement=null;');
  await client.run('refresh()');
  assert.equal(client.run('renderCalls'),1,'Normal rendering resumes after leaving the dropdown');
  assert.equal(client.run('barnGrowthSeconds(300,30,3,clock)'),0,'Legacy adults are already fully grown');
  assert.equal(client.run('barnGrowthSeconds(0,180,3,clock)'),60,'Exact multiples retain their full cycle time');
  assert.equal(client.run('barnGrowthSeconds(0,30,5,clock)'),6,'Four keepers retain their six-second cycle');
  client.run('world.towns.own.buildings[0].animal_growth.chicken=[30];');
  client.frame(12000);
  assert.equal(client.run('timer.time.textContent'),'Fully grown · Ready for butchery');
  assert.equal(client.run('timer.label.textContent'),'Chicken 1 · Fully grown');
  assert.equal(client.run('renderBarnAnimals.timers.state.textContent'),'0 growing · 1 fully grown · 2× speed');
  client.frame(60000);
  assert.equal(client.run('timer.time.textContent'),'Fully grown · Ready for butchery','Adults wait without restarting a countdown');
  client.run("viewed='elsewhere';timer.time.textContent='Detached';");
  client.frame(13000);
  assert.equal(client.run('timer.time.textContent'),'Detached','Leaving a town stops updating its old controls');
  client.run(`
    viewed='own';
    const sliderTarget=document.createElement('div');
    renderBarnAnimals(world.towns.own,world.towns.own.buildings[0],0,sliderTarget);
    const descendants=node=>[node,...node.children.flatMap(descendants)];
    const slider=descendants(sliderTarget).find(node=>node.id?.startsWith('barn-count-'));
    document.activeElement=slider;slider.onpointerdown();
  `);
  const beforeDrag=client.run('renderCalls');
  await client.run('refresh()');
  assert.equal(client.run('renderCalls'),beforeDrag,'Polling preserves a slider during a drag');
  client.run('slider.onpointercancel();document.activeElement=null;');
  await client.run('refresh()');
  assert.equal(client.run('renderCalls'),beforeDrag+1,'Polling resumes rendering after dragging ends');
  console.log('Animal countdown and focused-dropdown polling checks passed.');
})().catch(error=>{console.error(error);process.exitCode=1;});
