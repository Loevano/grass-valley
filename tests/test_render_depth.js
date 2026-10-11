// Run with: node tests/test_render_depth.js
const fs=require('fs'),assert=require('assert');
const source=fs.readFileSync('web/app.js','utf8');
const merge=new Function(source.slice(source.indexOf('function mergeDepthObjects('),source.indexOf('function paintGround('))+'return mergeDepthObjects;')();
const depth=object=>object.depth ?? object.x+object.y;
const reference=(before,resources,after)=>[...before,...resources,...after].sort((a,b)=>depth(a)-depth(b));
assert.deepEqual(merge([],[],[]),[]);
// Dense equal depths exercise artwork and click-target precedence. Explicit
// crop depths must interleave correctly with resource and character positions.
for(let seed=0;seed<30;seed++){
  const make=(count,kind)=>Array.from({length:count},(_,i)=>({kind,id:i,x:(i*7+seed)%11,y:(i*3+seed)%13,
    ...(i%5===0?{depth:i%9}:{})}));
  const before=make(20,'building'),resources=make(200,'resource').sort((a,b)=>depth(a)-depth(b)),after=make(15,'villager');
  const expected=reference(before,resources,after),original=[...resources];
  resources.sort=()=>{throw Error('Cached resources must not be sorted again');};
  assert.deepEqual(merge(before,resources,after),expected,'Merge preserves the previous stable painter order');
  assert.deepEqual([...resources],original,'The cached resource list remains untouched');
}
// Count depth reads to check scaling without a machine-dependent time limit.
let reads=0;
const resources=Array.from({length:20000},(_,i)=>({get depth(){reads++;return i;}}));
const result=merge([{depth:100}],resources,[{depth:100},{depth:19999}]);
assert.equal(result.length,20003);
assert(reads<resources.length*8,'Dense maps merge with linear work');
assert.equal(result[100].depth,100);
assert.strictEqual(result[101],resources[100]);
console.log('Render depth checks passed');
