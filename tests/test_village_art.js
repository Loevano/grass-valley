const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const paint=new Function(source.slice(source.indexOf('function paintResource('),source.indexOf('function animatedVillagers('))+'return paintResource;')();
function artwork(kind,biome,detailed,variant,species){
  const commands=[];
  const context=new Proxy({}, {get(target,key){
    if(key in target)return target[key];
    return (...args)=>commands.push([key,target.fillStyle,...args]);
  }});
  paint(context,{kind,biome,variant,species,x:0,y:0},false,detailed);
  assert.equal(commands.filter(c=>c[0]==='save').length,commands.filter(c=>c[0]==='restore').length);
  assert(commands.some(c=>['fill','fillRect','stroke'].includes(c[0])),'Village artwork paints at every zoom');
  return commands;
}
for(const detailed of [false,true]){
  const styles=new Set();
  for(const biome of ['b','d','i']){
    const hut=artwork('village_hut',biome,detailed);
    const market=artwork('village_market',biome,detailed);
    styles.add(JSON.stringify(hut));
    for(const variant of ['inn','workshop','hall','barn']){
      assert.notDeepEqual(artwork('village_hut',biome,detailed,variant),hut,'Landmarks have distinct artwork');
    }
    assert.notDeepEqual(market,hut,'Markets have open stalls and produce instead of cottage walls');
  }
  const palm=artwork('tree','d',detailed,undefined,'palm');
  assert(palm.some(command=>command[1]==='#b28b5e'),'Desert courtyards have distinct palm trunks');
  assert.equal(styles.size,3,'Each biome has distinct cottage artwork');
}

for(const detailed of [false,true]){
  const styles=new Set(['reed_house','net_house','moss_house','tavern'].map(variant=>
    JSON.stringify(artwork('village_hut','b',detailed,variant))));
  assert.equal(styles.size,4,'Swamp quarters have four distinct silhouettes and materials');
  for(const variant of ['bench','candle_lantern','barrels','ferns','cattails','mushrooms','reeds'])
    artwork('village_decor','b',detailed,variant);
}

const client=require('./client_harness')();
client.run(`
  const sleeper={name:'Gus',biome:'b',x:6,y:6,pose:'passed_out',
    routine:{x:6,y:6,path:[],phase:'sleeping',job:'idle',elapsed:0,facing:1}};
  const before=JSON.stringify(sleeper),town={npcs:[sleeper]};
  const sprite=npcSprites(town,1)[0],commands=[];
  const target=new Proxy({}, {get:(object,key)=>key in object?object[key]:(...args)=>commands.push([key,...args])});
  drawVillager(sprite,false,target);
  if(!commands.some(c=>c[0]==='fillText'&&c[1]==='Z'))throw Error('Sleeping patron lacks its lying-down pose');
  if(JSON.stringify(sleeper)!==before)throw Error('Rendering mutated the saved patron');
`);
console.log('Village buildings, decorations and sleeping patrons paint without mutating snapshots');
