const fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8');
const animation=new Function(fs.readFileSync('web/animations.js','utf8')+';return WorkerAnimation;')();
const elements=new Map(),listeners={};
function element(tag){return {tag,children:[],style:{},attributes:{},classList:{toggle(){}},append(...items){this.children.push(...items);},replaceChildren(){this.children=[];},setAttribute(k,v){this.attributes[k]=v;},addEventListener(){},focus(){},showModal(){},close(){}};}
const document={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvas=document.getElementById('world');canvas.getContext=()=>new Proxy({},{get:()=>()=>{}});canvas.getBoundingClientRect=()=>({width:960,height:660});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','WorkerAnimation',
 source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
 function panelNodes(node=$('selection-details')){return Array.from(node.children || []).flatMap(child=>[child,...panelNodes(child)]);}


 function check(ok,message){if(!ok)throw Error(message);}
 const town={name:'Town',wood:65,food:40,coins:20,level:5,goods:{},player:{x:5,y:7},
 buildings:[{kind:'hall',x:5,y:5}],villagers:[],map_resources:[]};
 const other={...town,name:'Other',rules:{production_caps:{wood:999}}};
 world={world_id:'test',towns:{own:town,friend:other},progression:{buildings:{},upgrades:{},job_requirements:{},crops:{wheat:1},edible:{fish:1},recipes:{sawpit:[{wood:2},'planks',1]}}};
 identity={town_id:'own'};viewed='own';draw=()=>{};
 const commands=[];act=data=>commands.push(data);
 $('rules-button').onclick();
 check(panel==='rules'&&!$('rules-panel').hidden,'Rules opens from toolbar');
 const rows=()=>$('rules-list').children;
 const field=key=>rows().flatMap(row=>row.children).find(node=>node.id==='cap-'+key);
 const save=key=>{const row=rows().find(row=>row.children.includes(field(key)));row.reportValidity=()=>true;row.onsubmit({preventDefault(){}});};
 check(field('planks').value===''&&field('wood').placeholder==='Unlimited','Caps start unlimited');
 field('wood').value='0';save('wood');
 check(commands.at(-1).resource==='wood'&&commands.at(-1).cap===0,'Zero pauses production');
 field('planks').value='100';save('planks');
 check(commands.at(-1).action==='configure_rule'&&commands.at(-1).cap===100,'Save submits the town resource cap');
 render();check(field('planks').value==='100','Snapshot polling preserves an unfinished edit');
 town.rules={production_caps:{wood:0,planks:100}};render();
 check(field('wood').value===0&&field('planks').value===100,'Host-confirmed rules render');
 field('wood').value='';save('wood');check(commands.at(-1).cap===null,'Blank removes the cap');
 viewed='friend';render();check(field('wood').value==='','Rules still controls your own town when visiting');
 check(other.rules.production_caps.wood===999,'Other town rules remain untouched');
 identity=null;render();check($('rules-button').disabled&&!rows().length,'Unjoined visitors cannot set rules');
 `);
run(document,{devicePixelRatio:1,addEventListener(){}},{getItem:()=>null},()=>0,()=>{},animation);
console.log('Town production rules controls passed');
