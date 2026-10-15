const assert=require('node:assert/strict');
const app=require('./client_harness')();
const results=JSON.parse(app.run(`
  const artTown={level:5,map_size:32,terrain:Array(32).fill('g'.repeat(32)),buildings:[],map_resources:[],villagers:[]};
  world={world_id:'production-art',towns:{own:artTown}};viewed='own';
  const cases=[];
  for(const kind of ['butcher','bakery','smokehouse','tannery','woodcutter','workshop','farm','home','hall','barn','sawpit','fishing_hut','fish_pond','mine','smelter','steelworks','storehouse','mill','market','orchard','stable','saddler','tradehouse','well','orchard_tree',...decorationKinds])
  for(const zoom of [.7,1,2])
  for(const footprint of (['woodcutter','farm','home','barn','fishing_hut','fish_pond','mine','smelter','steelworks','storehouse','market','orchard','stable','saddler','tradehouse','well','orchard_tree',...decorationKinds].includes(kind)?[[1,1]]:kind==='sawpit'?[[3,2],[1,1]]:[[2,2],[1,1]]))
  for(const profession_upgrades of [[],...({home:[['timber_frame']],woodcutter:[['axe','tools']],bakery:[['larger_oven','double_hearth_oven','masterwork_oven']],sawpit:[['saw']],mine:[['steel_pickaxe']],fishing_hut:[['nets'],['wool_nets'],['boat'],['boat','improved_boat'],['nets','boat','improved_boat','master_boat']]}[kind] || [])])
  for(const stage of kind==='orchard_tree'?['sapling','blossom','fruit','ripe','harvested']:[null])
  for(const orientation of decorationKinds.includes(kind)?[0,1,2,3]:[0])
  for(const level of (kind==='storehouse'?[1,2,3,4,5,6,7,8]:['well','orchard_tree',...decorationKinds].includes(kind)?[1]:kind==='hall'?[1,2,3,4,5,10]:kind==='home'?[1,2,3,4]:[1,2,3])){
    artTown.level=kind==='hall'?level:5;
    const building={kind,x:5,y:5,footprint,level,orientation,settler_level:3,profession_upgrades,...(stage?{tree:{stage,fruit:8}}:{})};
    const snapshot=JSON.stringify(building);
    artTown.buildings=[building];cameraTown='production-art/own';camera={x:5,y:5,zoom:46/54*zoom};animationFrameRunning=true;
    const steps=ClientScene.prepare(world);let prepared;do{prepared=steps.next();}while(!prepared.done);ClientScene.commit(prepared.value);
    const images=[],oldDrawImage=ctx.drawImage;
    ctx.drawImage=(surface,x,y,width,height)=>images.push({surface,x,y,width,height});draw();ctx.drawImage=oldDrawImage;
    const image=images.find(image=>[...buildingSprites.values()].includes(image.surface));
    const hit=hitTargets.find(hit=>hit.type==='building'&&hit.index===0);
    const points=[],stack=[];let m=[1,0,0,1,0,0],pathStart=0,closed=false;
    const record=(x,y)=>points.push([m[0]*x+m[2]*y+m[4],m[1]*x+m[3]*y+m[5]]);
    const target={
      save(){stack.push([...m]);},restore(){m=stack.pop();},
      translate(x,y){m[4]+=m[0]*x+m[2]*y;m[5]+=m[1]*x+m[3]*y;},
      scale(x,y){m[0]*=x;m[1]*=x;m[2]*=y;m[3]*=y;},
      rotate(angle){const [a,b,c,d]=m,cos=Math.cos(angle),sin=Math.sin(angle);m[0]=a*cos+c*sin;m[1]=b*cos+d*sin;m[2]=c*cos-a*sin;m[3]=d*cos-b*sin;},
      moveTo:record,lineTo:record,beginPath(){pathStart=points.length;closed=false;},closePath(){closed=true;},fill(){},
      stroke(){
        if(closed)throw Error(kind+': building faces should use flat shading without polygon outlines');
        const paddingX=(this.lineWidth || 1)*Math.hypot(m[0],m[2])/2;
        const paddingY=(this.lineWidth || 1)*Math.hypot(m[1],m[3])/2;
        for(const [x,y] of points.slice(pathStart))points.push([x-paddingX,y-paddingY],[x+paddingX,y+paddingY]);
      },fillText(){},
      fillRect(x,y,w,h){for(const [a,b] of [[x,y],[x+w,y],[x+w,y+h],[x,y+h]])record(a,b);},
      ellipse(x,y,rx,ry){for(const dx of [-rx,rx])for(const dy of [-ry,ry])record(x+dx,y+dy);},
      arc(x,y,r){this.ellipse(x,y,r,r);}
    };
    paintBuilding(building,artTown,null,target);
    cases.push({kind,footprint,level,zoom,orientation,profession_upgrades,unchanged:snapshot===JSON.stringify(building),balanced:stack.length===0,
      extent:{left:Math.min(...points.map(p=>p[0])),right:Math.max(...points.map(p=>p[0])),top:Math.min(...points.map(p=>p[1])),bottom:Math.max(...points.map(p=>p[1]))},
      sprite:{left:image.x,right:image.x+image.width,top:image.y,bottom:image.y+image.height},hit});
  }
  JSON.stringify(cases);
`));
for(const result of results){
  const name=`${result.kind} orientation ${result.orientation} ${result.footprint.join('×')} level ${result.level} ${result.profession_upgrades.join(',')} at ${result.zoom}× zoom`;
  assert(result.unchanged,`${name}: artwork preserves the authoritative building`);
  assert(result.balanced,`${name}: artwork restores canvas state`);
  for(const key of ['sprite','hit']){
    const bound=result[key],extent=result.extent;
    assert(extent.left>=bound.left&&extent.right<=bound.right&&extent.top>=bound.top&&extent.bottom<=bound.bottom,
      `${name}: artwork ${JSON.stringify(extent)} must fit ${key} ${JSON.stringify(bound)}`);
  }
}
console.log('Detailed building artwork fits cache and selection bounds without mutating buildings');

// Compare live smoke to the cap actually painted, including compact saved sites.
app.run(`
  const original={};
  for(const key of ['save','restore','translate','scale','beginPath','moveTo','lineTo','closePath','fill','stroke','fillRect','ellipse','arc','fillText'])original[key]=ctx[key];
  for(const kind of ['bakery','smokehouse'])for(const footprint of [[2,2],[1,1]])for(const level of [1,3]){
    let matrix=[1,0,0,1,0,0],stack=[],path=[],caps=[],smoke=[];
    const record=(x,y)=>[matrix[0]*x+matrix[2]*y+matrix[4],matrix[1]*x+matrix[3]*y+matrix[5]];
    Object.assign(ctx,{
      save(){stack.push([...matrix]);},restore(){matrix=stack.pop();},
      translate(x,y){matrix[4]+=matrix[0]*x+matrix[2]*y;matrix[5]+=matrix[1]*x+matrix[3]*y;},
      scale(x,y){matrix[0]*=x;matrix[1]*=x;matrix[2]*=y;matrix[3]*=y;},
      beginPath(){path=[];},moveTo(x,y){path.push(record(x,y));},lineTo(x,y){path.push(record(x,y));},
      closePath(){},stroke(){},fillRect(){},ellipse(){},fillText(){},
      fill(){if(this.fillStyle==='#c0bdab')caps.push([...path]);},
      arc(x,y){if(this.fillStyle==='#c7c6af66')smoke.push(record(x,y));}
    });
    const building={kind,footprint,level,x:5,y:5};
    paintBuilding(building,artTown);drawBuildingAnimation(building,artTown,2.5);
    if(caps.length!==1||smoke.length!==3)throw Error('Chimney and smoke must both render');
    const center=caps[0].reduce((sum,p)=>[sum[0]+p[0]/4,sum[1]+p[1]/4],[0,0]);
    if(Math.hypot(center[0]-smoke[0][0],center[1]-smoke[0][1])>tileW/46)throw Error('Smoke must start at the painted chimney cap');
    if(stack.length)throw Error('Live smoke must restore the drawing state');
  }
  Object.assign(ctx,original);
`);
console.log('Live smoke stays aligned with upgraded and compact chimney caps');

// The rotor is live artwork; its centre must stay on the cached tower axle.
app.run(`
  const rotorOriginal={};
  for(const key of ['save','restore','translate','scale','beginPath','moveTo','lineTo','closePath','fill','stroke','fillRect','ellipse','arc','fillText'])rotorOriginal[key]=ctx[key];
  for(const footprint of [[2,2],[1,1]])for(const level of [1,2,3])for(const zoom of [.7,1,2]){
    tileW=46*zoom;tileH=23*zoom;
    let m=[1,0,0,1,0,0],stack=[],start=null,ellipse=null;const hubs=[],arms=[];
    const screen=(x,y)=>[m[0]*x+m[2]*y+m[4],m[1]*x+m[3]*y+m[5]];
    Object.assign(ctx,{
      save(){stack.push([...m]);},restore(){m=stack.pop();},
      translate(x,y){m[4]+=m[0]*x+m[2]*y;m[5]+=m[1]*x+m[3]*y;},
      scale(x,y){m[0]*=x;m[1]*=x;m[2]*=y;m[3]*=y;},
      beginPath(){start=null;ellipse=null;},moveTo(x,y){start ||= screen(x,y);},lineTo(){},closePath(){},
      fill(){if(this.fillStyle==='#655b41'&&ellipse)hubs.push(ellipse);},
      stroke(){if(this.strokeStyle==='#c5ad79')arms.push(start);},
      ellipse(x,y){ellipse=screen(x,y);},arc(){},fillRect(){},fillText(){}
    });
    const mill={kind:'mill',x:5,y:5,footprint,level};
    paintBuilding(mill,artTown);drawBuildingAnimation(mill,artTown,1);
    if(hubs.length!==1||arms.length!==4)throw Error('Mill must paint an axle and four live sail arms');
    for(const arm of arms)if(Math.hypot(arm[0]-hubs[0][0],arm[1]-hubs[0][1])>1e-6)throw Error('Mill sails must rotate on the painted axle');
    if(stack.length)throw Error('Mill animation must restore canvas state');
  }
  Object.assign(ctx,rotorOriginal);
`);
console.log('Live mill sails stay attached to the tower axle at full and compact sizes');
