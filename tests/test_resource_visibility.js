// Run with: node tests/test_resource_visibility.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
new Function('assert',`
  let resourceCache=null,tileW=18.9,tileH=tileW/2,originX=480,originY=-900,projections=0;
  const point=(x,y)=>{projections++;return [originX+(x-y)*tileW/2,originY+(x+y)*tileH/2];};
  ${source.slice(source.indexOf('function renderResources('),source.indexOf('function paintGround('))}
  const resources=[];
  for(let y=0;y<280;y++)for(let x=0;x<280;x+=4)
    resources.push({kind:(x+y)%7===0?'water':'tree',x,y});
  // Scramble the snapshot order to check equal-depth ordering and selection indexes.
  resources.reverse();
  const rect={width:960,height:660};
  const expected=items=>items.filter(r=>{
    if(r.kind==='water')return false;
    const px=originX+(r.x-r.y)*tileW/2,py=originY+(r.x+r.y)*tileH/2;
    return px>=-tileW&&px<=rect.width+tileW&&py>=-tileH&&py<=rect.height+80*tileW/46;
  }).sort((a,b)=>(a.depth ?? a.x+a.y)-(b.depth ?? b.x+b.y));
  let data=renderResources(resources),visible=visibleResourceObjects(data,rect);
  assert.deepEqual(visible,expected(resources),'Spatial culling preserves screen bounds and equal-depth snapshot order');
  assert(projections<resources.length/2,'Distant loaded resources skip projection');
  projections=0;
  assert.strictEqual(visibleResourceObjects(data,rect),visible,'Stationary frames reuse visible resources');
  assert.equal(projections,0,'Stationary frames do not rescan resources');
  for(const resource of visible)assert.strictEqual(resources[data.indexes.get(resource)],resource,'Selection retains snapshot indexes');
  const region=data.region;
  originX+=40;
  assert.deepEqual(visibleResourceObjects(data,rect),expected(resources),'Fast panning updates visibility');
  assert.strictEqual(data.region,region,'Nearby camera movement reuses the sorted region');
  originX-=40;visibleResourceObjects(data,rect);
  assert.strictEqual(data.region,region,'Reversing direction does not rebuild the sorted region');
  for(const zoom of [.35,.36,.6,1,2.5])for(const position of [[0,0],[30,30],[140.25,139.75],[279,279]]){
    tileW=54*zoom;tileH=tileW/2;
    originX=rect.width/2-(position[0]-position[1])*tileW/2;
    originY=rect.height/2-(position[0]+position[1])*tileH/2-tileH/2;
    assert.deepEqual(visibleResourceObjects(data,rect),expected(resources),'Panning and zooming preserve visibility');
  }
  rect.width=1440;rect.height=900;
  assert.deepEqual(visibleResourceObjects(data,rect),expected(resources),'Resizing refreshes visible resources');
  const next=resources.filter((_,index)=>index%2).map(r=>({...r}));
  data=renderResources(next);
  assert.deepEqual(visibleResourceObjects(data,rect),expected(next),'New snapshots replace stale visible objects');
  next.push({kind:'rock',x:279,y:279});data=renderResources(next);
  assert.deepEqual(visibleResourceObjects(data,rect),expected(next),'Resource additions invalidate the visibility cache');
  const edges=[[-tileW,-tileH],[rect.width+tileW,-tileH],[-tileW,rect.height+80*tileW/46],
    [rect.width+tileW,rect.height+80*tileW/46]].map(([px,py])=>({kind:'tree',
      x:(px-originX)/tileW+(py-originY)/tileH,y:(py-originY)/tileH-(px-originX)/tileW}));
  assert.deepEqual(visibleResourceObjects(renderResources(edges),rect),expected(edges),'Objects at viewport boundaries use the original inclusive bounds');
`)(assert);
console.log('Resource visibility checks passed');
