const assert=require('node:assert/strict');
const app=require('./client_harness')();
const results=JSON.parse(app.run(`
  const roofTown={level:1,buildings:[],map_resources:[],villagers:[]};
  const palettes={butcher:['#aa7a78','#855d64'],bakery:['#b48a56','#946d48'],
    smokehouse:['#7d8883','#596d69'],tannery:['#a99066','#817052'],
    woodcutter:['#8e8860','#706f4f'],workshop:['#7f9990','#5d7c74'],
    farm:['#b49b60','#958049'],fishing_hut:['#78968d','#526f6c'],
    fish_pond:['#78968d','#526f6c'],smelter:['#a28c60','#7e7452'],
    steelworks:['#718a86','#4d6c6b'],storehouse:['#738e7c','#506c60'],mill:['#99a081','#707f69'],
    market:['#97a57b','#64816a'],orchard:['#8d9e68','#637d52'],stable:['#8c9b79','#637c62'],
    saddler:['#ae8c70','#806b58'],tradehouse:['#7b9590','#536f70'],well:['#a78c61','#7d7454'],barn:['#6b8076','#52685f']};
  const roofs=[];
  for(const kind of [...Object.keys(palettes),'home','hall'])
  for(const level of kind==='hall'?[1,2,3,4,5]:kind==='home'?[1,2,3,4]:kind==='storehouse'?[1,2,3,4,5,6,7,8]:kind==='well'?[1]:[1,2,3])
  for(const footprint of kind==='hall'?[[3,3],[2,2],[1,1]]:['workshop','butcher','bakery','smokehouse','tannery','mill'].includes(kind)?[[2,2],[1,1]]:[[1,1]]){
    roofTown.level=level;
    const colors=kind==='home'?(level<3?['#aa7b62','#855c4d']:['#986d58','#795849']):
      kind==='hall'?[['#7f9795','#5a747b'],['#748e91','#52767b'],['#6c8990','#476975'],['#637c8b','#405c70'],['#596f83','#374e65']][level-1]:palettes[kind];
    let path=[];const faces=[];
    const target=new Proxy({
      beginPath(){path=[];},moveTo(x,y){path.push([x,y]);},lineTo(x,y){path.push([x,y]);},
      fill(){if(path.length===4&&colors.includes(this.fillStyle))faces.push({color:this.fillStyle,points:[...path]});}
    },{get:(target,key)=>key in target?target[key]:()=>{}});
    paintBuilding({kind,x:5,y:5,level,footprint},roofTown,null,target,true);
    roofs.push({kind,level,footprint,left:faces.filter(f=>f.color===colors[0]).map(f=>f.points),right:faces.filter(f=>f.color===colors[1]).map(f=>f.points)});
  }
  JSON.stringify(roofs);
`));
const area=points=>Math.abs(points.reduce((sum,[x,y],i)=>{
  const [nx,ny]=points[(i+1)%points.length];return sum+x*ny-y*nx;
},0)/2);
const equal=(a,b)=>Math.abs(a[0]-b[0])<1e-8&&Math.abs(a[1]-b[1])<1e-8;
for(const {kind,level,footprint,left,right} of results){
  const label=`${kind} level ${level}, ${footprint.join('×')}`;
  assert(left.length&&left.length===right.length,`${label}: each roof has two slopes`);
  for(let i=0;i<left.length;i++){
    const a=left[i],b=right[i],shared=a.filter(p=>b.some(q=>equal(p,q)));
    assert.equal(shared.length,2,`${label}: roof slopes meet along their ridge`);
    const side=p=>(shared[1][0]-shared[0][0])*(p[1]-shared[0][1])-(shared[1][1]-shared[0][1])*(p[0]-shared[0][0]);
    const outer=points=>points.find(p=>!shared.some(q=>equal(p,q)));
    assert(side(outer(a))*side(outer(b))<0,`${label}: the near slope must not cover the far slope`);
    assert(Math.min(area(a),area(b))/Math.max(area(a),area(b))>=.2,
      `${label}: both slopes must have readable projected area`);
  }
}
console.log('Pitched roofs retain two visible slopes, including narrow sheds and the hall belfry');
