const assert=require('node:assert/strict'),client=require('./client_harness');
const app=client();
app.run(`
  const town={wood:50,food:20,coins:100,goods:{wheat:10,bread:4},
    buildings:[{kind:'hall',stored_goods:{wood:50,wheat:10,bread:4}},
      {kind:'mill',input_buffer:{wheat:8},pending_output:{flour:6},orphaned_output:{flour:2}},
      {kind:'market'}],
    villagers:[{pending_output:{flour:4},logistics_trip:{operation:'sell',picked_up:true,delivered:false,cargo:{bread:2}}}],
    rules:{market_purchases:{wheat:30},market_sales:{bread:{cap:10,enabled:true}}},
    resource_activity:{seconds:90,buckets:[{at:30,in:{wood:99},out:{}},{at:33,in:{wood:4},out:{wheat:3}},{at:90,in:{wood:2},out:{wheat:1}}]}};
  world={world_id:'test',towns:{own:town},progression:{storage_keys:['wood','food','wheat','flour','bread','vegetables'],edible:{bread:1.5,vegetables:1},buyable_goods:['wheat','bread','vegetables'],sellable_goods:['bread','vegetables']}};
  identity={town_id:'own'};viewed='friend';const commands=[];act=data=>commands.push(data);
`);
const stocks=app.run('resourceStocks(town)');
assert.equal(stocks.wheat.total,18);assert.equal(stocks.wheat.free,10);assert.equal(stocks.wheat.reserved,8);
assert.equal(stocks.flour.total,6,'Carried and orphaned output count once through building pending totals');
assert.equal(stocks.bread.total,6);assert.equal(stocks.bread.reserved,2);
assert.equal(stocks.food.total,23,'Food summarizes reserved edible goods by food value');
const before=app.run('JSON.stringify(town)');app.run('renderResourceList(town)');
assert.equal(app.run('JSON.stringify(town)'),before,'Rendering leaves authoritative snapshots untouched');
const get=key=>app.run(`renderResourceList.rows.get('${key}')`);
assert.equal(get('wood').cells.in.textContent,'6');assert.equal(get('wheat').cells.out.textContent,'4');
const wheat=get('wheat').inputs.buy;assert.equal(wheat.value,'30');assert(!wheat.disabled);
wheat.value='40';app.run('document.activeElement=renderResourceList.rows.get("wheat").inputs.buy;town.wood=51;renderResourceList(town)');
assert.strictEqual(get('wheat').inputs.buy,wheat);assert.equal(wheat.value,'40','Snapshot refresh preserves focused draft');
wheat.onchange();assert.equal(app.run('JSON.stringify(commands.pop())'),JSON.stringify({action:'configure_resource_trade',resource:'wheat',buy_below:40,sell_above:null}));
wheat.value='';wheat.onchange();assert.equal(app.run('commands.pop().buy_below'),null,'Blank disables buying');
const vegetables=get('vegetables').inputs.sell;assert(vegetables,'Vegetables have a Sell at control');
vegetables.value='50';vegetables.onchange();
assert.equal(app.run('JSON.stringify(commands.pop())'),JSON.stringify({action:'configure_resource_trade',resource:'vegetables',buy_below:null,sell_above:50}));
assert(app.run('marketSaleGoods({}).includes("vegetables")'),'The fallback market catalog supports vegetables');
app.run('document.activeElement=null;town.buildings.pop();renderResourceList(town)');assert(wheat.disabled,'A completed market is required');
app.run('world.world_id="another"');wheat.value='50';wheat.onchange();assert.equal(app.run('commands.length'),0,'Stale controls cannot modify another world');
console.log('Resource stock, activity, and popup controls passed');
