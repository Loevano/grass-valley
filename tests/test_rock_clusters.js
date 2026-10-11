// Run with: node tests/test_rock_clusters.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const sprites=source.slice(source.indexOf('const resourceSprites='),source.indexOf('function animatedVillagers('));
const resources=source.slice(source.indexOf('function renderResources('),source.indexOf('function visibleResourceObjects('));
new Function('assert',`
  let resourceCache=null;
  ${sprites}
  ${resources}
  const patch=[];
  for(let y=20;y<23;y++)for(let x=20;x<23;x++)patch.push({kind:'rock',x,y});
  patch.push({kind:'rock',x:40,y:40},{kind:'tree',x:19,y:20});
  const original=JSON.stringify(patch),data=renderResources(patch);
  assert.strictEqual(renderResources(patch),data,'Stationary frames reuse topology');
  assert.equal(data.rockJoins.size,9,'A touching patch joins every rock tile');
  assert(!data.rockJoins.has(patch[9]),'Isolated rocks retain standalone artwork');
  const at=(x,y)=>patch.find(r=>r.x===x&&r.y===y);
  for(const r of patch.slice(0,9)){
    const join=data.rockJoins.get(r);
    assert.equal(data.indexes.get(r),patch.indexOf(r),'Joined tiles retain individual selection indexes');
    for(const height of join.heights)assert(height>=15&&height<=43,'Outcrops fit inside the resource texture');
    const right=at(r.x+1,r.y),front=at(r.x,r.y+1);
    if(right){
      const next=data.rockJoins.get(right);
      assert.deepEqual([join.heights[1],join.heights[2]],[next.heights[0],next.heights[3]],'Horizontal neighbors have gap-free shared edges');
      assert(join.mask&2,'Internal horizontal walls are hidden');
    }
    if(front){
      const next=data.rockJoins.get(front);
      assert.deepEqual([join.heights[3],join.heights[2]],[next.heights[0],next.heights[1]],'Vertical neighbors have gap-free shared edges');
      assert(join.mask&4,'Internal vertical walls are hidden');
    }
  }
  const center=data.rockJoins.get(at(21,21)),edge=data.rockJoins.get(at(20,20));
  assert.equal(center.mask,15,'Interior tiles have no exposed walls');
  assert(Math.min(...center.heights)>edge.heights[0],'Dense patches rise toward the middle');
  assert.equal(JSON.stringify(patch),original,'Visual grouping leaves saved resources unchanged');
  const again=renderResources(JSON.parse(original).reverse());
  for(const r of again.objects.filter(r=>data.rockJoins.has(at(r.x,r.y))))
    assert.deepEqual(again.rockJoins.get(r),data.rockJoins.get(at(r.x,r.y)),'Snapshot order and reloads preserve outcrop shapes');
  const missing=patch.filter(r=>r!==at(21,21)),changed=renderResources(missing);
  assert(!(changed.rockJoins.get(at(20,21)).mask&2),'Removing a tile exposes its neighbor wall');
  assert(changed.rockJoins.get(at(20,21)).heights[1]<data.rockJoins.get(at(20,21)).heights[1],'Changed neighborhoods refresh elevations');
  const diagonal=renderResources([{kind:'rock',x:0,y:0},{kind:'rock',x:1,y:1}]);
  assert.equal(diagonal.rockJoins.size,0,'Diagonal rocks do not bridge empty walkable tiles');
  const faces=[];
  const ctx={save(){},restore(){},beginPath(){},moveTo(){},lineTo(){},closePath(){},fill(){faces.push(this.fillStyle);}};
  paintResource(ctx,at(21,21),false,false,center);
  assert.equal(faces.length,3,'Interior rocks draw only continuous top facets');
  faces.length=0;
  paintResource(ctx,at(22,22),false,false,data.rockJoins.get(at(22,22)));
  assert.equal(faces.length,5,'Exposed front edges draw both cliff faces');
`)(assert);
console.log('Connected rock outcrop checks passed');
