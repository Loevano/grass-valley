// Real Chromium and Canvas, deterministic HTTP fixtures, no user saves.
import {chromium} from 'playwright';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import os from 'node:os';

const root=path.resolve(process.argv[2] || '.'),output=process.argv[3];
const headed=process.argv.includes('--headed'),diagnostic=process.argv.includes('--trace');
function fixture(large){
  const size=480,count=large?4:1,world={world_id:'performance',name:'Performance fixture',towns:{},hall_ready:{}};
  const terrain_chunks={},resource_chunks={};
  for(let i=0;i<128;i++){
    const key=`${i%12},${Math.floor(i/12)}`;
    terrain_chunks[key]=Array(40).fill('g'.repeat(40));
    resource_chunks[key]={version:'initial',map_resources:[],tree_stumps:[]};
  }
  const centers=Array.from({length:count},(_,i)=>({x:80+(i%2)*120,y:80+Math.floor(i/2)*120}));
  let added=0;
  for(let y=0;y<400;y++)for(let x=0;x<size;x++){
    if(added>=(large?50000:2000)||centers.some(p=>Math.abs(x-p.x)<9&&Math.abs(y-p.y)<14))continue;
    if((x*17+y*31)%3)continue;
    resource_chunks[`${Math.floor(x/40)},${Math.floor(y/40)}`].map_resources.push({kind:'tree',x,y,supply:8});added++;
  }
  for(const [i,center] of centers.entries()){
    const buildings=[{kind:'hall',...center},{kind:'barn',x:center.x+3,y:center.y+3,animals:{chicken:4,sheep:4,pig:4,cow:4}},
      {kind:'storehouse',x:center.x-3,y:center.y,storage_enabled:{wood:true,food:true}}];
    for(let b=0;b<(large?18:2);b++)buildings.push({kind:'home',x:center.x-8+b%8*2,y:center.y+12+Math.floor(b/8)*2});
    world.towns[String(i)]={name:`Town ${i}`,level:3,wood:65,food:40,coins:20,town_origin:center,buildings,roads:[],goods:{},
      villagers:Array.from({length:large?50:5},(_,v)=>({name:`Worker ${i}/${v}`,job:'idle',woodcutting:0,farming:0,fishing:0})),
      player:{x:center.x-1,y:center.y-1,facing:1,equipment:{}}};
  }
  const npcs=Array.from({length:8},(_,index)=>({name:`Local ${index}`,village:'Fixture village',
    role:'Village craftsperson',biome:'b',x:74+index%4,y:74+Math.floor(index/4),
    routine:{x:74+index%4,y:74+Math.floor(index/4),job:index%2?'woodcutting':'husbandry',
      phase:index%2?'working':'walking',elapsed:2,facing:1,paused:false,
      path:index%2?[]:[[76+index%4,74+Math.floor(index/4)]],source:{x:73,y:73}}}));
  resource_chunks['1,1'].map_resources.push(
    {kind:'village_hut',variant:'barn',village:'Fixture village',biome:'b',x:75,y:78,animals:{chicken:3,pig:1}},
    {kind:'village_hut',variant:'hall',village:'Fixture village',biome:'b',x:73,y:78});
  world.shared_map={chunked:true,chunk_size:40,map_size:size,terrain_seed:42,npcs,visible_chunks:Object.keys(terrain_chunks),terrain_chunks,resource_chunks};
  return world;
}
const server=http.createServer((req,res)=>{
  const name=new URL(req.url,'http://localhost').pathname;
  const file=path.join(root,'web',name==='/'?'index.html':name);
  if(!file.startsWith(path.join(root,'web')+path.sep)){res.writeHead(403);res.end();return;}
  try{
    let body=fs.readFileSync(file);
    if(name==='/app.js')body=Buffer.from(body.toString()+`\n
      window.__perf={frames:[],draws:[],intervals:[],longTasks:[],requests:[],inputDelays:[],snapshotDelays:[],workerDelays:[],canvasRedraws:0};
      const measuredClear=ctx.clearRect.bind(ctx);ctx.clearRect=function(...args){window.__perf.canvasRedraws++;return measuredClear(...args);};
      const measuredDraw=draw;draw=function(...args){if(typeof ClientScene!=='undefined'&&!animationFrameRunning)return measuredDraw(...args);const t=performance.now();try{return measuredDraw(...args);}finally{window.__perf.draws.push(performance.now()-t);
        const p=window.__perf;if(p.snapshotReady!==undefined&&world===p.snapshotWorld){p.snapshotDelays.push(performance.now()-p.snapshotReady);delete p.snapshotReady;}
      }};
      const measuredAnimate=animate;let measuredLast=null;
      animate=function(now){const t=performance.now();if(measuredLast!==null)window.__perf.intervals.push(now-measuredLast);measuredLast=now;try{return measuredAnimate(now);}finally{window.__perf.frames.push(performance.now()-t);
        const p=window.__perf;
        if(p.workerStart!==undefined&&world===p.snapshotWorld&&(typeof ClientScene==='undefined'||ClientScene.queue.jobs.size===0)){p.workerDelays.push(performance.now()-p.workerStart);delete p.workerStart;}
        if(p.inputStart!==undefined&&playerView&&(playerView.x!==p.inputX||playerView.y!==p.inputY)){p.inputDelays.push(performance.now()-p.inputStart);delete p.inputStart;}
      }};
      const measuredRequest=request;request=async function(...args){const t=performance.now();try{const result=await measuredRequest(...args);if(args[0].split('?')[0]==='/api/world'){window.__perf.snapshotReady=performance.now();window.__perf.workerStart=window.__perf.snapshotReady;window.__perf.snapshotWorld=result;}return result;}finally{window.__perf.requests.push({path:args[0].split('?')[0],ms:performance.now()-t});}};
      new PerformanceObserver(list=>window.__perf.longTasks.push(...list.getEntries().map(e=>e.duration))).observe({type:'longtask',buffered:true});
    `);
    res.setHeader('Content-Type',name.endsWith('.js')?'text/javascript':name.endsWith('.css')?'text/css':'text/html');res.end(body);
  }catch{res.writeHead(404);res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const metrics={},checks=[],errors=[],details={},timers=new Set();
let browser;
const percentile=(values,q)=>{const a=values.slice().sort((a,b)=>a-b);return a[Math.ceil((a.length-1)*q)] || 0;};
try{
  browser=await chromium.launch({headless:!headed,channel:'chromium'});
  const browserVersion=browser.version();
  for(const large of [false,true]){
    const label=large?'large':'village',data=fixture(large);
    const context=await browser.newContext({viewport:{width:1280,height:800},deviceScaleFactor:1});
    await context.addInitScript(()=>localStorage.setItem('lantern-identities',JSON.stringify({performance:{town_id:'0',token:'fixture'}})));
    const page=await context.newPage();page.on('pageerror',error=>errors.push(`${label}: ${error.message}`));
    await page.route('**/api/**',async route=>{
      const url=new URL(route.request().url());let body;
      if(url.pathname==='/api/world'){
        body=structuredClone(data);
        const known=new Set((url.searchParams.get('known') || '').split(';'));
        const versions=new Map((url.searchParams.get('resources') || '').split(';').map(k=>k.split(':')));
        for(const key of Object.keys(body.shared_map.terrain_chunks))if(known.has(key))delete body.shared_map.terrain_chunks[key];
        for(const [key,value] of Object.entries(body.shared_map.resource_chunks))if(versions.get(key)===value.version)delete body.shared_map.resource_chunks[key];
        body.players_sampled_at=performance.now();
      }else if(url.pathname==='/api/worlds')body={active_world:'performance',worlds:[{id:'performance',name:'Fixture',town_count:Object.keys(data.towns).length}],can_manage:true};
      else if(url.pathname==='/api/login')body={town_id:'0',token:'fixture'};
      else if(url.pathname==='/api/players')body={world_id:'performance',sampled_at:performance.now(),players:Object.fromEntries(Object.entries(data.towns).map(([id,t])=>[id,t.player]))};
      else if(url.pathname==='/api/command'){
        const command=route.request().postDataJSON();
        if(command.action==='walk')for(const step of command.steps || [command]){data.towns['0'].player.x+=step.dx*.15;data.towns['0'].player.y+=step.dy*.15;}
        body={player:data.towns['0'].player};
      }else body={};
      await route.fulfill({json:body});
    });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForFunction(()=>world.world_id==='performance'&&world.towns[viewed]&&window.__perf.frames.length>15);
    await page.evaluate(async()=>{
      if($('world-menu').open)$('world-menu').close();
      if(document.getElementById('login-dialog')?.open){
        $('player-name').value='Fixture';$('player-pin').value='4826';
        await $('join-form').onsubmit({preventDefault(){}});
      }
    });
    await page.waitForFunction(()=>typeof ClientScene==='undefined'||ClientScene.queue.jobs.size===0);
    // Warm shaders, sprite artwork, and terrain separately from measured work.
    await page.waitForTimeout(500);
    for(const scenario of ['idle','walk','pan','zoom','refresh','obstacle','eviction','panel','overview_idle','overview_pan','overview_refresh']){
      if(scenario.startsWith('overview_'))await page.evaluate(()=>{camera.zoom=.35;});
      await page.evaluate(()=>{window.__perf.frames=[];window.__perf.draws=[];window.__perf.intervals=[];window.__perf.longTasks=[];window.__perf.requests=[];window.__perf.inputDelays=[];window.__perf.snapshotDelays=[];window.__perf.workerDelays=[];window.__perf.canvasRedraws=0;});
      let timer,panStart;
      if(scenario==='walk'){await page.evaluate(()=>{window.__perf.inputStart=performance.now();window.__perf.inputX=playerView.x;window.__perf.inputY=playerView.y;});await page.keyboard.down('d');}
      if(scenario==='pan'||scenario==='overview_pan'){
        const bounds=await page.locator('#world').boundingBox();
        if(!bounds)throw new Error('Map canvas is unavailable for the pan scenario');
        await page.mouse.move(bounds.x+bounds.width/2,bounds.y+bounds.height/2);
        panStart=await page.evaluate(()=>({x:camera.x,y:camera.y}));
        timer=setInterval(()=>page.mouse.wheel(45,20).catch(()=>{}),40);
      }
      if(timer)timers.add(timer);
      if(scenario==='zoom')timer=setInterval(()=>page.evaluate(()=>{camera.zoom=camera.zoom===.7?.75:.7;}).catch(()=>{}),100);
      if(timer)timers.add(timer);
      if(scenario==='panel')await page.evaluate(()=>openPanel('villagers'));
      if(scenario==='obstacle'){
        const c=data.towns['0'].town_origin;
        data.shared_map.resource_chunks['2,2'].map_resources.push({kind:'rock',x:c.x+4,y:c.y+4,supply:8});
        data.shared_map.resource_chunks['2,2'].version='changed';
      }
      if(scenario==='eviction'){
        for(const key of ['0,11','1,11']){data.shared_map.terrain_chunks[key]=Array(40).fill('g'.repeat(40));data.shared_map.resource_chunks[key]={version:'new',map_resources:[],tree_stumps:[]};}
        data.shared_map.visible_chunks=['0,11','1,11'];
      }
      if(['refresh','obstacle','eviction','overview_refresh'].includes(scenario)){
        const latency=await page.evaluate(async()=>{const t=performance.now();await refresh();return performance.now()-t;});
        metrics[`${label}.${scenario}.refresh_ms`]=latency;
      }
      await page.waitForTimeout(1200);
      if(timer){clearInterval(timer);timers.delete(timer);}
      if(scenario==='walk')await page.keyboard.up('d');
      const result=await page.evaluate(()=>({frames:window.__perf.frames,draws:window.__perf.draws,intervals:window.__perf.intervals,longTasks:window.__perf.longTasks,requests:window.__perf.requests,inputDelays:window.__perf.inputDelays,snapshotDelays:window.__perf.snapshotDelays,workerDelays:window.__perf.workerDelays,canvasRedraws:window.__perf.canvasRedraws,
        pending:typeof ClientScene==='undefined'?0:ClientScene.queue.jobs.size,terrain:chunkTerrainCache.size,resources:chunkResourceCache.size,camera:{x:camera.x,y:camera.y}}));
      for(const [key,values] of Object.entries({frame_ms:result.frames,callback_interval_ms:result.intervals,long_task_ms:result.longTasks})){
        metrics[`${label}.${scenario}.${key}_p95`]=percentile(values,.95);
        metrics[`${label}.${scenario}.${key}_max`]=Math.max(0,...values);
      }
      if(scenario==='walk')metrics[`${label}.${scenario}.input_to_movement_ms`]=percentile(result.inputDelays,.95);
      if(['refresh','obstacle','eviction','overview_refresh'].includes(scenario)){metrics[`${label}.${scenario}.snapshot_to_draw_ms`]=percentile(result.snapshotDelays,.95);metrics[`${label}.${scenario}.workers_ready_ms`]=percentile(result.workerDelays,.95);}
      const acknowledgements=result.requests.filter(r=>r.path==='/api/command').map(r=>r.ms);
      if(scenario==='walk')metrics[`${label}.${scenario}.command_ack_ms`]=percentile(acknowledgements,.95);
      metrics[`${label}.${scenario}.draw_calls_per_frame`]=result.draws.length/Math.max(1,result.frames.length);
      metrics[`${label}.${scenario}.canvas_draw_calls_per_frame`]=result.canvasRedraws/Math.max(1,result.frames.length);
      checks.push({scenario:`${label}.${scenario}`,ok:result.frames.length>=10&&result.pending===0&&result.terrain<=128&&result.resources<=128,frames:result.frames.length,pending:result.pending});
      if(panStart)checks.push({scenario:`${label}.${scenario}.camera`,ok:Math.hypot(result.camera.x-panStart.x,result.camera.y-panStart.y)>1});
      if(scenario==='walk')checks.push({scenario:`${label}.input`,ok:result.inputDelays.length>0&&acknowledgements.length>0});
      if(['refresh','obstacle','eviction','overview_refresh'].includes(scenario))checks.push({scenario:`${label}.snapshot`,ok:result.snapshotDelays.length>0&&result.workerDelays.length>0});
      details[`${label}.${scenario}`]={samples:result.frames.length,requests:result.requests};
      if(timer)timers.add(timer);
      if(scenario==='panel')await page.evaluate(()=>{panel=null;render();});
    }
    // Diagnostic traces are separate from scored timings to avoid profiler overhead.
    if(diagnostic){
      const cdp=await context.newCDPSession(page);const events=[];
      cdp.on('Tracing.dataCollected',event=>events.push(...event.value));
      await cdp.send('Tracing.start',{categories:'devtools.timeline,disabled-by-default-devtools.timeline.frame,blink.user_timing',transferMode:'ReportEvents'});
      await page.evaluate(()=>refresh());await page.waitForTimeout(1000);
      const done=new Promise(resolve=>cdp.once('Tracing.tracingComplete',resolve));await cdp.send('Tracing.end');await done;
      fs.writeFileSync(`${output}.${label}.trace.json`,JSON.stringify({traceEvents:events}));
    }
    await context.close();
  }
  fs.writeFileSync(output,JSON.stringify({schema:1,environment:{browser:browserVersion,headed,platform:process.platform,arch:process.arch,cpu:os.cpus()[0].model,viewport:'1280x800',dpr:1},metrics,checks,errors,details},null,2));
  if(errors.length||checks.some(check=>!check.ok))process.exitCode=1;
}catch(error){console.error(error.stack);process.exitCode=2;}
finally{for(const timer of timers)clearInterval(timer);await browser?.close();await new Promise(resolve=>server.close(resolve));}
