// Run with Node.js or JavaScriptCore's jsc shell.
const source=typeof readFile==='function'?readFile('web/app.js'):require('fs').readFileSync('web/app.js','utf8');
const elements=new Map(),fills=[];
function element(){return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const context=new Proxy({fill(){fills.push(this.fillStyle);}}, {get:(target,key)=>key in target?target[key]:()=>{}});
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>context;
canvasMock.getBoundingClientRect=()=>({left:0,top:0,width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','fills',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  function check(condition,message){if(!condition)throw Error(message);}
  const town={map_size:12,buildings:[],map_resources:[],villagers:[],terrain:Array(12).fill('gfmsrggggggg')};
  viewed='own';world={world_id:'test',towns:{own:town}};
  animatedVillagers=()=>[];draw();
  for(const palette of [['#68865a','#69875b','#668358'],['#506f4a','#55764c','#5a7b50'],
    ['#83945e','#899b65','#80915b'],['#b4a575','#bdad7d','#b8a878'],['#8b8d7c','#969785','#858978']]){
    check(palette.some(color=>fills.includes(color)),'Each ground type has a visible palette');
  }
  town.map_resources=[{kind:'water',x:2,y:2}];fills.length=0;draw();
  check(fills.includes('#6299a1'),'Shallow water is rendered along shores');
  town.map_resources=[{kind:'water',x:2,y:2},{kind:'water',x:1,y:2},
    {kind:'water',x:3,y:2},{kind:'water',x:2,y:1},{kind:'water',x:2,y:3}];fills.length=0;draw();
  check(fills.includes('#447d91'),'Interior water has a deeper color');
  town.terrain=Array(12).fill('bbbbbbbbbbbb');fills.length=0;draw();
  check(fills.includes('#59674b'),'Swamp ground has its own palette');
  check(fills.includes('#718565')&&fills.includes('#4e6857'),'Swamp water has shallow and deep green colors');
  for(const [x,y,color] of [[6,5,'#536247'],[6,7,'#626f50'],[6,8,'#59674b']]){
    fills.length=0;paintGround(ctx,town,new Set(),{minX:x,maxX:x,minY:y,maxY:y});
    check(fills.includes(color),'Swamp movement fixtures match the visible tile shade');
  }
  town.map_resources.push({kind:'water',x:0,y:0});fills.length=0;draw();
  check(fills.includes('#a3ad74'),'Swamp water contains lily pads');
  fills.length=0;drawResource({kind:'tree',x:5,y:5});
  check(fills.includes('#64784c'),'Swamp trees render as willows');
  town.terrain=Array(12).fill('vvvvvvvvvvvv');fills.length=0;draw();
  check(!fills.includes('#68865a')&&!fills.includes('#6299a1'),'Tiles outside the outline are not rendered');
  check(!placementValid(town,{x:5,y:5},'home'),'Cannot preview a building outside the outline');
  delete town.terrain;fills.length=0;draw();
  check(fills.includes('#68865a'),'Maps without terrain still render grass');
`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},fills);
if(typeof print==='function')print('Terrain rendering checks passed');else console.log('Terrain rendering checks passed');
