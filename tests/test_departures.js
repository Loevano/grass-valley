const assert=require('assert');
const source=require('fs').readFileSync('web/app.js','utf8');
const logic=source.slice(source.indexOf('function departureMessage('),source.indexOf('async function refresh('));
const harness=new Function(`
  let world, identity={town_id:'own'}, viewed='own', selection={type:'villager',index:0};
  const workers=new Map([[0,{}]]), notices=[];
  const notice=message=>notices.push(message);
  ${logic}
  return {departureMessage, updateDepartures, setWorld(value){world=value;},
    state(){return {selection,workers,notices};}};
`)();
const before={world_id:'one',towns:{own:{departure_count:0},friend:{departure_count:0}}};
const after=JSON.parse(JSON.stringify(before));
after.towns.own={departure_count:1,last_departure:{name:'Ada',job:'baking',practice:{baking:80,milling:20}}};
harness.setWorld(before);
harness.updateDepartures(after);
assert.equal(harness.state().selection,null,'Departure clears an index-based villager selection');
assert.equal(harness.state().workers.size,1,'Departure preserves surviving villager animation state');
assert(harness.state().notices[0].includes('Ada, your baker'),'Notice names the lost specialist');
assert(harness.state().notices[0].includes('80 baking practice'),'Notice includes lost experience');
assert(harness.state().notices[0].includes('food shortages'),'Old departures default to food shortages');
assert(harness.departureMessage({last_departure:{name:'Finn',job:'idle',reason:'housing'}}).includes('housing shortages'));
assert(harness.departureMessage({last_departure:{name:'Finn',job:'idle',reason:'food and housing'}}).includes('food and housing shortages'));
harness.setWorld(after);
harness.updateDepartures(after);
assert.equal(harness.state().notices.length,1,'Polling does not repeat a departure notice');
const other=JSON.parse(JSON.stringify(after));other.towns.friend.departure_count=1;
harness.updateDepartures(other);
assert.equal(harness.state().notices.length,1,'Other towns do not show personal loss notices');
harness.updateDepartures({...after,world_id:'two'});
assert.equal(harness.state().notices.length,1,'Changing worlds does not show stale losses');
console.log('Settler departure UI checks passed');
