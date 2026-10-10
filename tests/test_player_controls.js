// Run with Node.js or JavaScriptCore's jsc shell.
const read=path=>typeof readFile==='function'?readFile(path):require('fs').readFileSync(path,'utf8');
const source=read('web/app.js');
const elements=new Map(),listeners={};
function element(){return {children:[],style:{},classList:{toggle(){}},append(child){this.children.push(child);},replaceChildren(){this.children=[];},setAttribute(){},addEventListener(){},focus(){},showModal(){},close(){}};}
const documentMock={getElementById(id){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element,querySelectorAll:()=>[]};
const canvasMock=documentMock.getElementById('world');
canvasMock.getContext=()=>new Proxy({}, {get:()=>()=>{}});
const run=new Function('document','window','localStorage','setTimeout','clearTimeout','listeners',
  source.slice(0,source.lastIndexOf('requestAnimationFrame(animate);'))+`
  return (async()=>{
    function check(condition,message){if(!condition)throw Error(message);}
    const town={player:{x:5,y:7,facing:1}};
    world={world_id:'test',towns:{own:town,friend:{player:{x:8,y:8,facing:1}}}};
    identity={town_id:'own'};viewed='own';
    const commands=[];
    request=async(path,data)=>{commands.push(data);return {player:{x:5.2,y:6.8,facing:1}};};
    playerSprite(town,0);
    const key=key=>listeners.keydown({key,preventDefault(){}});
    key('D');playerSprite(town,.1);await Promise.resolve();await Promise.resolve();
    check(commands[0].action==='walk'&&commands[0].dx===1&&commands[0].dy===-1,'D walks right on the isometric screen');
    listeners.keyup({key:'D'});
    const hero=playerSprite(town,.1);
    check(hero.x===5.2&&hero.y===6.8,'Sprite advances smoothly to host position');
    check(playerView.following&&(typeof camera==='undefined'||camera.x===hero.x&&camera.y===hero.y),'Camera follows movement');
    key('w');key('d');
    check(walkKeys.has('w')&&walkKeys.has('d'),'Multiple movement keys can be held');
    listeners.blur();check(walkKeys.size===0,'Losing focus releases all movement keys');
    for(const tag of ['INPUT','TEXTAREA','SELECT','BUTTON']){
      document.activeElement={tagName:tag};key('w');
      check(walkKeys.size===0,'Typing or focused controls do not move player');
    }
    document.activeElement=null;
    $('world-menu').open=true;key('w');check(walkKeys.size===0,'World menu blocks movement');$('world-menu').open=false;
    tool='home';key('w');check(walkKeys.size===0,'Building placement blocks movement');tool=null;
    viewed='friend';key('w');check(walkKeys.size===0,'Visitors cannot control another player');
    const friend=playerSprite(world.towns.friend,0);
    check(friend.x===8&&friend.y===8,'Town switch resets the player sprite');
    viewed='own';playerSprite(town,0);key('s');
    check(walkKeys.has('s'),'Movement resumes in own town');
    $('delete-world-dialog').open=true;playerSprite(town,.1);
    check(walkKeys.size===0&&commands.length===1,'Opening a modal releases held keys without sending movement');
  })();`);
run(documentMock,{addEventListener(key,callback){listeners[key]=callback;}},{getItem:()=>null},()=>0,()=>{},listeners)
  .then(()=>{if(typeof print==='function')print('Player control checks passed');else console.log('Player control checks passed');})
  .catch(error=>{if(typeof print==='function'){print(String(error));print(error.stack);quit(1);}else{console.error(error);process.exitCode=1;}});
