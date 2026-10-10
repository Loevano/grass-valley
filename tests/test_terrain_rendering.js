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
  check(fills.includes('#518697'),'Water is rendered over its ground type');
  delete town.terrain;fills.length=0;draw();
  check(fills.includes('#68865a'),'Maps without terrain still render grass');
`);
run(documentMock,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},fills);
if(typeof print==='function')print('Terrain rendering checks passed');else console.log('Terrain rendering checks passed');
