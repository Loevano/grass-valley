// Run with: node tests/test_resource_sprites.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const helpers=source.slice(source.indexOf('const resourceSprites='),source.indexOf('function animatedVillagers('));
const surfaces=[],images=[];
const main={drawImage(...args){images.push(args);}};
const documentMock={createElement(){
  const surface={paints:0,colors:[],points:[],paths:[],ellipses:[]};
  surface.getContext=()=>new Proxy({fill(){surface.paints++;surface.colors.push(this.fillStyle);surface.paths.push(surface.path || []);},
    beginPath(){surface.path=[];},
    ellipse(...args){surface.ellipses.push(args);},
    moveTo(x,y){surface.points.push([x,y]);surface.path.push([x,y]);},lineTo(x,y){surface.points.push([x,y]);surface.path.push([x,y]);},
    setTransform(...args){surface.transform=args;}},{get:(target,key)=>key in target?target[key]:()=>{}});
  surfaces.push(surface);return surface;
}};
new Function('document','ctx','window','assert','surfaces','images',`
  let tileW=54,world={shared_map:{terrain:{10:{10:'g',12:'b'}}}},viewed='own';
  const point=(x,y)=>[100+(x-y)*tileW/2,100+(x+y)*tileW/4];
  ${helpers}
  const tree={kind:'tree',x:10,y:10};
  drawResource(tree);
  const sprite=surfaces[0],painted=sprite.paints;
  assert(painted>0,'Tree artwork is painted once');
  assert.equal(images.length,1,'A resource needs one image draw on the main canvas');
  for(let i=0;i<100;i++)drawResource({...tree,x:20+i*2});
  assert.equal(surfaces.length,1,'Trees with the same artwork share a sprite across tiles');
  assert.equal(sprite.paints,painted,'Subsequent frames do not rebuild tree paths');
  const last=images.at(-1),position=point(218,10),scale=tileW/46;
  assert.equal(last[1],position[0]-30*scale,'Sprites follow horizontal world positioning');
  assert.equal(last[2],position[1]-80*scale,'Canopies retain their vertical anchor');
  drawResource({...tree,x:11});
  assert.equal(surfaces.length,2,'Alternating tree colors have distinct sprites');
  drawResource({...tree,x:12});
  assert(surfaces.at(-1).colors.includes('#64784c'),'Swamp trees retain willow artwork');
  drawResource({...tree,species:'ancient'});
  assert.equal(surfaces.length,4,'Ancient trees retain distinct enlarged artwork');
  for(const kind of ['rock','berry_bush','herb_patch','chest']){
    drawResource({...tree,kind});const surface=surfaces.at(-1),paints=surface.paints;
    drawResource({...tree,kind,supply:1});
    assert.equal(surface.paints,paints,'Supply changes reuse '+kind+' artwork');
  }
  const formations=new Map();
  for(let x=0;x<500;x++){
    const rock={kind:'rock',x,y:20};drawResource(rock);
    const sprite=images.at(-1)[0],variant=rockVariant(rock)%48;
    if(formations.has(variant))assert.strictEqual(sprite,formations.get(variant),'Matching rock formations share cached artwork');
    formations.set(variant,sprite);
    drawResource(JSON.parse(JSON.stringify(rock)));
    assert.strictEqual(images.at(-1)[0],sprite,'Reloaded coordinates preserve rock appearance');
  }
  assert.equal(formations.size,48,'The landscape contains all 48 rock variations');
  assert.equal(new Set([...formations.values()].map(sprite=>JSON.stringify(sprite.points))).size,48,'Each variation has a distinct silhouette');
  const heights=[...formations.values()].map(sprite=>Math.max(...sprite.points.map(p=>p[1]))-Math.min(...sprite.points.map(p=>p[1])));
  assert(Math.max(...heights)>=50,'Large boulders have an enlarged silhouette');
  assert(Math.min(...heights)<=20,'Some tiles contain only small stones');
  assert(Math.max(...heights)>Math.min(...heights)*3,'Boulders and small stones have clearly different sizes');
  for(const sprite of formations.values()){
    assert(sprite.colors.includes('#d4cbb2')||sprite.colors.includes('#d6ceb9'),'Low-poly rocks have a bright upper-left face');
    assert(sprite.colors.includes('#686f69')||sprite.colors.includes('#626c69'),'Low-poly rocks have a contrasting shadow face');
  }
  for(const sprite of formations.values()){
    const silhouettes=sprite.paths.filter(path=>path.length===7);
    assert.equal(sprite.ellipses.length,silhouettes.length,'Every stone casts a shadow matching its own footprint');
    sprite.ellipses.forEach(([x,y,w,h],i)=>{
      const stone=silhouettes[i],left=Math.min(...stone.map(p=>p[0])),right=Math.max(...stone.map(p=>p[0]));
      assert(Math.abs(w-(right-left)*.425)<1e-8,'Shadow width scales with the actual stone width');
      assert(x-w>=left&&x+w<=right,'Contact shadows stay beneath their stone');
      assert(Math.abs(y-(Math.max(...stone.map(p=>p[1]))-1))<1e-8,'Contact shadows stay anchored at the stone base');
      assert(h<=Math.max(1,w*.3),'Contact shadows follow the ground perspective');
    });
  }
  for(const sprite of formations.values())
    assert(sprite.points.every(([x,y])=>x>=-30&&x<=30&&y>=-80&&y<=24),'Rock formations fit inside their cached texture');
  tileW=31;
  for(const [variant,detailedSprite] of formations){
    const x=Array.from({length:500},(_,x)=>x).find(x=>rockVariant({x,y:20})%48===variant);
    drawResource({kind:'rock',x,y:20});const simple=images.at(-1)[0];
    assert.deepEqual(simple.paths.filter(path=>path.length===7),detailedSprite.paths.filter(path=>path.length===7),'Every stone retains its silhouette across zoom detail tiers');
    assert(simple.paints<detailedSprite.paints,'Distant rocks keep primary facets but skip secondary faces and moss');
    assert(simple.colors.length>=6,'Even small distant rocks retain contrasting low-poly faces');
  }
  tileW=54;drawResource(tree);
  const count=surfaces.length;
  tileW=50;drawResource(tree);
  assert.equal(surfaces.length,count,'Nearby zoom levels reuse a resolution bucket');
  tileW=18.9;drawResource(tree);
  assert(surfaces.at(-1).width<sprite.width,'Distant zoom uses a smaller texture');
  tileW=135;drawResource(tree);
  assert(surfaces.at(-1).width>sprite.width,'Close zoom uses a sharper texture');
  const close=surfaces.at(-1);
  assert(close.transform[0]>=tileW/46*window.devicePixelRatio,'Textures cover the display resolution');
  tileW=31;drawResource(tree);const simple=images.at(-1)[0];
  tileW=33;drawResource(tree);const detailed=images.at(-1)[0];
  assert.notStrictEqual(simple,detailed,'Detail tiers use distinct sprites even within the same resolution bucket');
  assert(simple.paints<detailed.paints,'Distant trees use fewer artwork layers');
  tileW=31;drawResource(tree);
  assert.strictEqual(images.at(-1)[0],simple,'Zooming back out reuses the simpler tree sprite');
  tileW=135;
  window.devicePixelRatio=1;drawResource(tree);
  assert.notEqual(images.at(-1)[0],close,'Display scale changes select an appropriate texture');
  world.shared_map.terrain[10][10]='b';tileW=54;drawResource(tree);
  assert(images.at(-1)[0].colors.includes('#64784c'),'Biome changes select the correct tree sprite');
  const rock={kind:'rock',x:201,y:20};drawResource(rock);
  const originalRock=images.at(-1)[0];resourceSprites.clear();drawResource(rock);
  assert.deepEqual(images.at(-1)[0].paths,originalRock.paths,'Evicted rock artwork regenerates with the same shapes');
  assert.deepEqual(images.at(-1)[0].colors,originalRock.colors,'Evicted rock artwork regenerates with the same colors');
  const join={mask:6,heights:[23,24,38,25]};drawResource(rock,join);
  const joinedSprite=images.at(-1)[0];
  assert.notStrictEqual(joinedSprite,images.at(-2)[0],'Joined rocks have distinct artwork from standalone rocks');
  drawResource({...rock},{...join,heights:[...join.heights]});
  assert.strictEqual(images.at(-1)[0],joinedSprite,'Identical topology reuses joined rock artwork');
  drawResource(rock,{...join,heights:[23,24,31,25]});
  assert.notStrictEqual(images.at(-1)[0],joinedSprite,'Changed neighboring rocks invalidate the joined sprite');
  for(let resolution=1;resolution<=128;resolution*=2){
    tileW=46*resolution;
    for(const kind of ['tree','rock','berry_bush','herb_patch','chest'])
      for(const species of ['ordinary','ancient'])
        for(let x=10;x<=13;x++)drawResource({kind,species,x,y:10});
  }
  assert.equal(resourceSprites.size,64,'The sprite cache stays bounded across resolution changes');
`)(documentMock,main,{devicePixelRatio:2},assert,surfaces,images);
console.log('Resource sprite cache checks passed');
