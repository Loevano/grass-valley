const assert=require('node:assert');
const client=require('./client_harness')();

client.run(`
  const sawTown={name:'Sawyers',level:3,buildings:[{kind:'hall',x:8,y:8},{kind:'sawpit',x:5,y:5}],
    villagers:[{name:'Ada',job:'woodworking',building_index:1,woodworking:7,woodcutting:10}],map_resources:[]};
  world={world_id:'sawing',towns:{own:sawTown},progression:{
    buildings:{sawpit:{level:3,job:'woodworking'}},job_requirements:{crafting:{woodworking:2}},recipes:{}}};
  identity={town_id:'own'};viewed='own';
  const sawCommands=[];act=data=>sawCommands.push(data);
  const sawCard=villagerCard(sawTown,sawTown.villagers[0],0);
  const sawSelect=sawCard.children.at(-1);
`);
assert.equal(client.run(`sawSelect.children.find(option=>option.value==='woodworking').textContent`),'Sawing');
assert.equal(client.run('sawSelect.value'),'woodworking','Saved assignments retain their job identifier');
client.run('sawSelect.onchange()');
assert.equal(client.run('sawCommands.at(-1).job'),'woodworking','Sawing assignments use the existing host contract');
assert(client.run(`jobRequirement({},'crafting').includes('2 sawing practice')`));
assert.equal(client.run(`goodName(workerTools.crafted_saw.job)`),'Sawing');
assert(client.run(`departureMessage({last_departure:{name:'Ada',job:'woodworking',practice:{woodworking:7}}}).includes('7 sawing practice')`));
assert(client.run(`sawCard.children.find(node=>node.className==='villager-profile').children.some(node=>node.textContent==='Sawing')`));
assert(client.run(`villagerRow(sawTown,sawTown.villagers[0],0).children[3].children[0].children.some(node=>node.title==='Sawing · 7 practice')`));

client.run(`
  const sawMarks=[];
  for(const method of ['fillRect','strokeRect','moveTo','lineTo','translate','scale']){
    ctx[method]=(...args)=>sawMarks.push({method,args,color:method==='fillRect'?ctx.fillStyle:ctx.strokeStyle});
  }
  polygon=(points,color)=>sawMarks.push({method:'polygon',points,color});
  const sawSprite={kind:'villager',index:0,x:4,y:5,active:true,villager:sawTown.villagers[0],
    state:{x:4,y:5,path:[],phase:'working',elapsed:0,clock:0,facing:1,source:{kind:'sawpit',x:5,y:5}}};
  function renderSaw(time,phase='working',active=true,walking=false){
    Object.assign(sawSprite,{active});
    Object.assign(sawSprite.state,{elapsed:time,phase,path:walking?[{x:5,y:5}]:[]});
    const before=JSON.stringify(sawSprite);
    sawMarks.length=0;ctx.fillStyle='';ctx.strokeStyle='';drawVillager(sawSprite);
    if(JSON.stringify(sawSprite)!==before)throw Error('Drawing mutated authoritative worker state');
    return JSON.stringify(sawMarks);
  }
`);
const render=(...args)=>JSON.parse(client.run(`renderSaw(${args.map(value=>JSON.stringify(value)).join(',')})`));
const blade=marks=>marks.find(mark=>mark.method==='polygon'&&mark.color==='#c4d2ce');
const timber=marks=>marks.find(mark=>mark.method==='fillRect'&&mark.color==='#b78b57');
const pull=render(Math.PI/14),push=render(3*Math.PI/14);
assert.equal(blade(pull).points[0][0]-blade(push).points[0][0],8,'Saw moves horizontally in both stroke directions');
assert.deepEqual(timber(pull),timber(push),'Timber stays fixed through the stroke');
assert(blade(pull).points.length>10,'Saw has visible teeth');
for(const marks of [pull,push]){
  const handle=marks.find(mark=>mark.method==='strokeRect'&&mark.color==='#8b5e3b');
  assert(marks.some(mark=>mark.method==='lineTo'&&mark.args[0]===handle.args[0]+1&&mark.args[1]===-12),
    'Working hand follows the saw handle');
  assert(!marks.some(mark=>mark.method==='fillRect'&&mark.args.join(',')==='-13,-11,8,3'),
    'Sawyer puts away the spare waist saw during work');
}
assert.deepEqual(render(.4,'working',false),render(.4,'working',false),'Paused rendering stays still');
for(const [phase,active,walking] of [['working',false,false],['idle',true,false],['walking',true,true],['depositing',true,false]]){
  assert(!blade(render(.4,phase,active,walking)),`No sawing while ${phase}, active=${active}`);
}
client.run(`sawSprite.villager={...sawSprite.villager,job:'idle'}`);
assert(!blade(render(.4)),'Changing jobs removes the saw action');
console.log('Sawing job labels, saved assignments, and working animation checks passed');
