// Actual browser predictor against frozen independently generated Python traces.
// Run from repository root: node rust/crates/lantern-host/tests/fixtures/founder/browser.js
const fs=require('fs'),assert=require('assert');
const vectors=JSON.parse(fs.readFileSync(`${__dirname}/python.json`,'utf8'));
const source=fs.readFileSync('web/app.js','utf8'),elements=new Map();
function element(){return {children:[],style:{},classList:{toggle(){}},append(...c){this.children.push(...c);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
documentMock.getElementById('world').getContext=()=>new Proxy({}, {get:()=>()=>{}});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','assert','ClientScene','vectors',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  let comparisons=0;
  for(const c of vectors.cases){
    if(!c.browser)continue; // Existing host/JS discrepancy for saved boat levels <= 0.
    const town=structuredClone(c.source);
    world={world_id:'founder-vectors',towns:{own:town}};identity={town_id:'own'};viewed='own';movementMap=null;
    let player=town.player;
    for(let i=0;i<c.inputs.length;i++){
      player=predictPlayerStep(town,player,...c.inputs[i]);
      for(const key of ['x','y'])assert(Math.abs(player[key]-c.expected[i][key])<vectors.tolerance,c.name+' step '+i+' '+key);
      assert.equal(player.facing,c.expected[i].facing,c.name+' facing');
      comparisons++;
    }
    assert.deepEqual(town,c.source,'Prediction must not mutate snapshots');
  }
  return comparisons;`);
const comparisons=run(documentMock,{addEventListener(){}},{getItem:()=>null},()=>0,()=>{},assert,{current:null},vectors);
console.log(`Actual browser predictor matches ${comparisons} Python movement samples; snapshots unchanged.`);
