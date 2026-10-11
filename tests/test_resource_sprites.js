// Run with: node tests/test_resource_sprites.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const helpers=source.slice(source.indexOf('const resourceSprites='),source.indexOf('function animatedVillagers('));
const surfaces=[],images=[];
const main={drawImage(...args){images.push(args);}};
const documentMock={createElement(){
  const surface={paints:0,colors:[]};
  surface.getContext=()=>new Proxy({fill(){surface.paints++;surface.colors.push(this.fillStyle);},
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
  for(let resolution=1;resolution<=128;resolution*=2){
    tileW=46*resolution;
    for(const kind of ['tree','rock','berry_bush','herb_patch','chest'])
      for(const species of ['ordinary','ancient'])
        for(let x=10;x<=13;x++)drawResource({kind,species,x,y:10});
  }
  assert.equal(resourceSprites.size,64,'The sprite cache stays bounded across resolution changes');
`)(documentMock,main,{devicePixelRatio:2},assert,surfaces,images);
console.log('Resource sprite cache checks passed');
