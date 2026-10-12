// Execute the actual browser scripts; tests never cut functions out of source text.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
module.exports=function client(){
  const elements=new Map(),frames=[],listeners=new Map();let paints=0;
  const context2d=new Proxy({clearRect(){paints++;}}, {get:(target,key)=>key in target?target[key]:()=>{}});
  function element(){return {open:false,children:[],style:{setProperty(){}},classList:{toggle(){},add(){},remove(){}},
    append(...children){this.children.push(...children);},replaceChildren(...children){this.children=children;},
    setAttribute(){},addEventListener(){},focus(){},showModal(){this.open=true;},close(){this.open=false;},
    getContext(){return new Proxy({}, {get:()=>()=>{}});},getBoundingClientRect(){return {left:0,top:0,width:960,height:660};}};}
  const document={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},
    createElement:element,querySelectorAll:()=>[],addEventListener(name,callback){listeners.set(name,callback);}};
  document.getElementById('world').getContext=()=>context2d;
  const sandbox={document,console,performance:{now:()=>0},AbortController,setTimeout,clearTimeout,setInterval:()=>0,
    localStorage:{getItem:()=>null,setItem(){}},requestAnimationFrame:callback=>frames.push(callback),
    fetch:()=>new Promise(()=>{})};
  sandbox.window={devicePixelRatio:1,addEventListener(name,callback){listeners.set(name,callback);}};
  const context=vm.createContext(sandbox);
  for(const file of ['scene.js','animations.js','app.js','quarry.js']){
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../web',file),'utf8'),context,{filename:file});
  }
  return {run:code=>vm.runInContext(code,context),frame(time){frames.shift()(time);},listeners,elements,get paints(){return paints;}};
};
