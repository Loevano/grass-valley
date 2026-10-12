const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const {sprites,position}=new Function('tileW',`
  ${source.slice(source.indexOf('function npcPosition('),source.indexOf('function localVisitKey('))}
  ${source.slice(source.indexOf('let npcClock='),source.indexOf('function villagerAccessories('))}
  return {sprites:npcSprites,position:npcPosition};
`)(54);
const local={name:'Hakon',village:'Frostbrook',biome:'i',x:10,y:10,
  routine:{x:10,y:10,path:[[11,10],[11,11]],phase:'walking',job:'woodcutting',elapsed:0,facing:1}};
const snapshot=JSON.stringify(local);
const town={npcs:[local]};
sprites(town,0);
const walking=sprites(town,1)[0];
assert(walking.x>10&&walking.x<11,'Host route moves smoothly between snapshots');
assert(walking.state.path.length,'Walking residents animate their stride');
assert.equal(JSON.stringify(local),snapshot,'Drawing never mutates the host snapshot');
assert.deepEqual([position(local).x,position(local).y],[10,10],'Interaction checks the latest authoritative position');
local.routine={...local.routine,paused:true};
const paused=sprites(town,1)[0];
assert.equal(paused.x,10);assert.equal(paused.y,10);
assert.equal(paused.state.path.length,0,'Residents stop beside visitors');
local.routine={...local.routine,paused:false,path:[],phase:'working',source:{x:9,y:10}};
const working=sprites(town,1)[0];
assert(working.active&&working.state.phase==='working');
assert.equal(working.villager.job,'woodcutting');
assert.deepEqual(working.state.source,{x:9,y:10},'Tool animation faces its actual work source');
const gear=new Function(source.slice(source.indexOf('function villagerAccessories('),source.indexOf('function paintHorse('))+'return villagerAccessories;')();
assert(gear(working).axe,'Village woodcutters carry visible axes');
console.log('NPC routine checks passed');
