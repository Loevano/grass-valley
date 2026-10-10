// Run with: node tests/test_acre_rendering.js
const source=require('fs').readFileSync('web/app.js','utf8');
const elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const context=new Proxy({}, {get:()=>()=>{}});
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>context;
canvasMock.getBoundingClientRect=()=>({left:0,top:0,width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  function check(condition,message){if(!condition)throw Error(message);}
  const field={kind:'acre',x:5,y:5};
  const town={map_size:12,buildings:[field],map_resources:[],villagers:[],player:{x:5,y:5}};
  identity={town_id:'own'};viewed='own';
  const marks=[];
  const paintPolygon=polygon;
  polygon=(points,fill,stroke)=>{if(fill==='#80764a')marks.push({kind:'soil'});paintPolygon(points,fill,stroke);};
  drawFarmCrop=crop=>marks.push(crop);
  drawVillager=character=>marks.push(character);
  animatedVillagers=()=>[];npcSprites=()=>[];
  let hero={kind:'player',townId:'own',index:0,x:5,y:5};
  playerSprite=()=>hero;
  for(const shared of [false,true]){
    world={world_id:'test',towns:{own:town},...(shared?{shared_map:town}:{})};
    for(let step=-8;step<=8;step++){
      hero={...hero,x:5+step/10};marks.length=0;draw();
      const heroIndex=marks.indexOf(hero);
      check(marks[0]?.kind==='soil','Acre soil is always below the player');
      const crops=marks.filter(mark=>mark.kind==='crop');
      check(crops.length===16,'All mature stalks are rendered');
      for(const crop of crops){
        const cropIndex=marks.indexOf(crop),feetDepth=hero.x+hero.y;
        if(crop.depth<feetDepth)check(cropIndex<heroIndex,'Stalk behind the feet renders before the player');
        if(crop.depth>feetDepth)check(cropIndex>heroIndex,'Stalk in front of the feet renders after the player');
      }
      check(hitTargets.some(target=>target.type==='building'&&target.index===0),'Acre remains selectable');
      check(!hitTargets.some(target=>target.type==='crop'),'Individual stalks do not create selection targets');
      if(step===0){
        check(crops.some(crop=>marks.indexOf(crop)<heroIndex)&&crops.some(crop=>marks.indexOf(crop)>heroIndex),'Player inside an acre has crops in front and behind');
      }
    }
  }
  const planted=[];
  drawFarmCrops(field,{elapsed:3},crop=>planted.push(crop));
  check(planted.length===8&&planted.every(crop=>crop.stage===1),'Sowing retains its progress');
  const harvested=[];
  drawFarmCrops(field,{elapsed:7},crop=>harvested.push(crop));
  check(harvested.length===8&&harvested.every(crop=>crop.stage===3),'Harvesting retains its progress');
`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{});
console.log('Acre rendering checks passed');
