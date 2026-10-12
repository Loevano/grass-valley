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
  assert(commands.some(c=>c[0]==='fill'),'Village buildings paint at every zoom');
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
    assert(market.length>hut.length,'Markets include their stalls');
  }
  const palm=artwork('tree','d',detailed,undefined,'palm');
  assert(palm.some(command=>command[1]==='#b28b5e'),'Desert courtyards have distinct palm trunks');
  assert.equal(styles.size,3,'Each biome has distinct cottage artwork');
}
console.log('Village artwork checks passed');
