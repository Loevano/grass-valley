const assert=require('node:assert/strict');
const app=require('./client_harness')();
const results=JSON.parse(app.run(`
  const artTown={level:5,map_size:32,terrain:Array(32).fill('g'.repeat(32)),buildings:[],map_resources:[],villagers:[]};
  world={world_id:'sawpit-art',towns:{own:artTown}};viewed='own';
  const cases=[];
  for(const zoom of [1,2])for(const footprint of [[3,2],[1,1]])for(const level of [1,2,3]){
    const building={kind:'sawpit',x:5,y:5,footprint,level,settler_level:2,profession_upgrades:['saw']};
    const snapshot=JSON.stringify(building);
    artTown.buildings=[building];cameraTown='sawpit-art/own';camera={x:5,y:5,zoom:46/54*zoom};animationFrameRunning=true;
    const steps=ClientScene.prepare(world);let prepared;do{prepared=steps.next();}while(!prepared.done);ClientScene.commit(prepared.value);
    const images=[],oldDrawImage=ctx.drawImage;
    ctx.drawImage=(surface,x,y,width,height)=>images.push({surface,x,y,width,height});draw();ctx.drawImage=oldDrawImage;
    const image=images.find(image=>[...buildingSprites.values()].includes(image.surface));
    const hit=hitTargets.find(hit=>hit.type==='building'&&hit.index===0);
    const points=[],stack=[];let m=[1,0,0,1,0,0];
    const record=(x,y)=>points.push([m[0]*x+m[2]*y+m[4],m[1]*x+m[3]*y+m[5]]);
    const target={
      save(){stack.push([...m]);},restore(){m=stack.pop();},
      translate(x,y){m[4]+=m[0]*x+m[2]*y;m[5]+=m[1]*x+m[3]*y;},
      scale(x,y){m[0]*=x;m[1]*=x;m[2]*=y;m[3]*=y;},
      rotate(angle){const [a,b,c,d]=m,cos=Math.cos(angle),sin=Math.sin(angle);m[0]=a*cos+c*sin;m[1]=b*cos+d*sin;m[2]=c*cos-a*sin;m[3]=d*cos-b*sin;},
      moveTo:record,lineTo:record,beginPath(){},closePath(){},fill(){},stroke(){},fillText(){},
      fillRect(x,y,w,h){for(const [a,b] of [[x,y],[x+w,y],[x+w,y+h],[x,y+h]])record(a,b);},
      ellipse(x,y,rx,ry){for(const dx of [-rx,rx])for(const dy of [-ry,ry])record(x+dx,y+dy);},
      arc(x,y,r){this.ellipse(x,y,r,r);}
    };
    paintBuilding(building,artTown,null,target);
    cases.push({footprint,level,zoom,unchanged:snapshot===JSON.stringify(building),balanced:stack.length===0,
      extent:{left:Math.min(...points.map(p=>p[0])),right:Math.max(...points.map(p=>p[0])),top:Math.min(...points.map(p=>p[1])),bottom:Math.max(...points.map(p=>p[1]))},
      sprite:{left:image.x,right:image.x+image.width,top:image.y,bottom:image.y+image.height},hit});
  }
  JSON.stringify(cases);
`));
for(const result of results){
  const name=`${result.footprint.join('×')} level ${result.level} at ${result.zoom}× zoom`;
  assert(result.unchanged,`${name}: artwork preserves the authoritative building`);
  assert(result.balanced,`${name}: artwork restores canvas state`);
  for(const key of ['sprite','hit']){
    const bound=result[key],extent=result.extent;
    assert(extent.left>=bound.left&&extent.right<=bound.right&&extent.top>=bound.top&&extent.bottom<=bound.bottom,
      `${name}: shed, bench, saw and materials fit the ${key} bounds`);
  }
}
console.log('Approved sawpit artwork fits cache and selection bounds at both lot sizes and zoom levels');
