const assert=require('assert'),fs=require('fs');
const source=fs.readFileSync('web/app.js','utf8');
const document={createElement(tag){return {tag,value:'',children:[],attributes:{},append(...items){this.children.push(...items);},setAttribute(key,value){this.attributes[key]=value;}};}};
const basicResources=['vegetables','berries','carrots','fodder','manure','stone','wheat','wood'];
const town={goods:{planks:250,smoked_sturgeon:200,...Object.fromEntries(basicResources.map(key=>[key,250]))}},commands=[];
const world={world_id:'market',towns:{own:town},progression:{sellable_goods:['planks','smoked_sturgeon',...basicResources],manual_sellable_goods:['planks','smoked_sturgeon',...basicResources],sale_prices:{planks:2,smoked_sturgeon:6}}};
const identity={town_id:'own'},view={town:'own'};
const create=new Function('document','world','identity','view','goodName','amountOf','act',
  'let viewed=view.town;'+
  source.slice(source.indexOf('function resourceCategories('),source.indexOf('function resourceGroup('))+
  source.slice(source.indexOf('function marketSaleGoods('),source.indexOf('function renderMarketSales('))+
  'return (town,target)=>{viewed=view.town;renderQuickMarketSale(town,target);};');
const render=create(document,world,identity,view,key=>key.replaceAll('_',' ').replace(/^./,c=>c.toUpperCase()),
  (town,key)=>town.goods[key] || 0,data=>commands.push(data));
const nodes=node=>[node,...node.children.flatMap(nodes)];
const panel=document.createElement('div');render(town,panel);
const form=panel.children[0],fields=nodes(form),amount=fields.find(n=>n.id==='sale-quantity'),resource=fields.find(n=>n.id==='sale-resource');
const sell=fields.find(n=>n.id==='sale-submit'),options=fields.find(n=>n.tag==='datalist');
const stock=fields.find(n=>n.tag==='small');
const submit=()=>form.onsubmit({preventDefault(){}});
assert.deepEqual(amount.children.map(n=>n.value),['1','5','10','50','100','200']);
assert.equal(resource.attributes.list,options.id,'Typing and dropdown selection share the host resource catalog');
assert.deepEqual(options.children.map(n=>n.value).sort(),['Planks','Smoked sturgeon',...basicResources.map(key=>key[0].toUpperCase()+key.slice(1))].sort());
assert(sell.disabled,'A resource must be selected');
resource.value='Planks';resource.oninput();
for(const quantity of [1,5,10,50,100,200]){
  amount.value=String(quantity);amount.onchange();assert(!sell.disabled);
  assert.equal(sell.textContent,`Sell ${quantity} · ${quantity*2} coins`);
  submit();assert.deepEqual(commands.pop(),{action:'sell_goods',resource:'planks',amount:quantity});
}
resource.value='  SMOKED STURGEON  ';resource.oninput();
assert.equal(sell.textContent,'Sell 200 · 1200 coins');submit();
assert.deepEqual(commands.pop(),{action:'sell_goods',resource:'smoked_sturgeon',amount:200});
resource.value='sturgeon';resource.oninput();assert(sell.disabled);submit();assert.equal(commands.length,0,'Partial names cannot sell a different good');
assert.equal(stock.textContent,'This resource cannot be sold. Choose a resource from the list.');
for(const key of basicResources){
  resource.value=key;resource.oninput();assert(!sell.disabled,`Typing ${key} enables sales when stock is sufficient`);
  assert.equal(sell.textContent,'Sell 200 · 200 coins');submit();
  assert.deepEqual(commands.pop(),{action:'sell_goods',resource:key,amount:200});
}
resource.value='Planks';resource.onchange();
const latest={goods:{planks:199}};world.towns.own=latest;
const refreshed=document.createElement('div');render(latest,refreshed);
assert.strictEqual(refreshed.children[0],form,'Snapshots preserve the active form');
assert.equal(amount.value,'200');assert.equal(resource.value,'Planks');assert(sell.disabled,'Latest stock disables oversized batches');
assert.equal(stock.textContent,'199 available · Need 200 to sell this amount.');
amount.value='100';amount.onchange();assert(!sell.disabled);
world.towns.own={goods:{planks:0}};submit();assert.equal(commands.length,0,'Submission rechecks the current snapshot');
view.town='friend';render(town,document.createElement('div'));submit();assert.equal(commands.length,0,'A retained form cannot sell another town stock');
view.town='own';world.world_id='another-world';submit();assert.equal(commands.length,0,'A retained form cannot sell in a different world');
const otherWorldPanel=document.createElement('div');render(town,otherWorldPanel);
assert.notStrictEqual(otherWorldPanel.children[0],form,'World changes rebuild the editor');
const app=require('./client_harness')();
app.run(`
  const createElement=document.createElement;
  document.createElement=tag=>Object.assign(createElement(tag),{value:''});
  world={world_id:'market-panel',towns:{own:{level:5,wood:50,food:20,coins:10,goods:{planks:10},
    buildings:[{kind:'market',x:2,y:2}],villagers:[],player:{},map_resources:[]}},
    progression:{...${JSON.stringify(world.progression)},buildings:{},upgrades:{},animals:{},profession_recipes:{}}};
  identity={town_id:'own'};viewed='own';selection={type:'building',index:0};
  renderSelection(world.towns.own);
  function panelNodes(node=$('selection-details')){return node.children.flatMap(child=>[child,...panelNodes(child)]);}
`);
assert.equal(app.run(`panelNodes().filter(node=>node.textContent==='Quick sale').length`),1,'Market retains one quick-sale form');
assert(app.run(`!panelNodes().some(node=>node.textContent==='Sell goods'||node.textContent?.startsWith('Sell 1 '))`),'Market omits the duplicate single-item sale list');
assert(app.run(`panelNodes().some(node=>node.textContent==='Automatic sales')&&panelNodes().some(node=>node.textContent==='Trade with another town')`),'Automatic sales and town trades remain available');
console.log('Quick market sales passed');
