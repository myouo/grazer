import {readFileSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
const backend=process.argv[2]??'webgl',port=Number(process.argv[3]??9226);
const page=await fetch(`http://127.0.0.1:${port}/json/new?about:blank`,{method:'PUT'}).then(r=>r.json());
const ws=new WebSocket(page.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});
let next=0;const pending=new Map();
ws.onmessage=({data})=>{const message=JSON.parse(data);if(!message.id)return;const p=pending.get(message.id);pending.delete(message.id);if(message.error)p.reject(new Error(JSON.stringify(message.error)));else p.resolve(message.result);};
function call(method,params={}){const id=++next;return new Promise((resolve,reject)=>{pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});}
async function evaluate(expression){const result=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails));return result.result.value;}
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function ready(health=0){await call('Page.navigate',{url:`http://127.0.0.1:8080/?backend=${backend}&health=${health}`});for(let i=0;i<180;i++){const state=await evaluate('window.grazerGameValidation');if(state?.error)throw new Error(state.error);if(state?.ready&&state.frames>=3)return;if(i%30===0)console.log(`${backend}: waiting for stage resources/rendering`);await sleep(250);}throw new Error('stage initialization timeout');}
async function screenshot(name){const shot=await call('Page.captureScreenshot',{format:'png'});writeFileSync(`target/m2-${backend}-${name}.png`,Buffer.from(shot.data,'base64'));}
try {
    await call('Page.enable');await call('Emulation.setDeviceMetricsOverride',{width:1280,height:1100,deviceScaleFactor:1,mobile:false});
    await ready();
    const rect=await evaluate('(()=>{const r=document.querySelector("#audio").getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()');
    await call('Input.dispatchMouseEvent',{type:'mousePressed',...rect,button:'left',clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',...rect,button:'left',clickCount:1});await sleep(150);
    const unlocked=await evaluate('window.grazerGameMetrics()');assert.equal(unlocked.audio,'running');assert.ok(unlocked.scheduledAudio>0);
    await evaluate('window.grazerSetPaused(true)');
    const before=await evaluate('window.grazerGameRestart()');assert.equal(before.health,3);assert.equal(before.ticks,'0');
    await sleep(100);assert.equal((await evaluate('window.grazerGameMetrics()')).ticks,'0','pause stops simulation');
    await evaluate('document.querySelector("#overlay").hidden=true');await screenshot('start');
    const moved=await evaluate('window.grazerGameAdvance(10,1,0,5)');assert.equal(moved.ticks,'10');assert.ok(moved.projectiles>0,'shooting produces resource-backed sprites');
    const bomb=await evaluate('window.grazerGameAdvance(1,0,0,3)');assert.equal(bomb.bombs,2);assert.equal(bomb.bombFlash,30);
    const held=await evaluate('window.grazerGameAdvance(3,0,0,3)');assert.equal(held.bombs,2,'bomb edge only');
    await evaluate('window.grazerGameRestart()');const dead=await evaluate('window.grazerGameAdvance(1500,0,0,0)');assert.equal(dead.phase,1);assert.equal(dead.health,0);
    const frozen=await evaluate('window.grazerGameAdvance(100,0,0,0)');assert.equal(frozen.ticks,dead.ticks);await screenshot('death');
    const restarted=await evaluate('window.grazerGameRestart()');assert.equal(restarted.hash,before.initialHash);assert.equal(restarted.phase,0);
    await ready(10000);await evaluate('window.grazerSetPaused(true)');await evaluate('window.grazerGameRestart()');
    const boss=await evaluate('window.grazerGameAdvance(7201,0,0,1)');assert.equal(boss.bossMax,1000);assert.ok(boss.bossHealth>0);assert.equal(boss.phase,0);
    await evaluate('document.querySelector("#overlay").hidden=true');await screenshot('boss');
    let pixels=null;
    if(backend==='webgpu') {
        pixels=await evaluate(`(async()=>{
            const canvas=document.querySelector('#game'),context=canvas.getContext('webgpu'),cfg=context.getConfiguration(),device=cfg.device;
            device.pushErrorScope('validation');context.configure({...cfg,usage:cfg.usage|GPUTextureUsage.COPY_SRC});window.grazerDrawForProbe();
            const buffer=device.createBuffer({size:768,usage:GPUBufferUsage.COPY_DST|GPUBufferUsage.MAP_READ}),encoder=device.createCommandEncoder(),texture=context.getCurrentTexture();
            for(const [i,p] of [[0,[480,1248,0]],[1,[520,288,0]],[2,[2,260,0]]])encoder.copyTextureToBuffer({texture,origin:p},{buffer,offset:i*256,bytesPerRow:256},[1,1,1]);
            device.queue.submit([encoder.finish()]);const error=await device.popErrorScope();if(error)throw new Error(error.message);await buffer.mapAsync(GPUMapMode.READ);
            const bytes=new Uint8Array(buffer.getMappedRange()),out={player:Array.from(bytes.slice(0,4)),boss:Array.from(bytes.slice(256,260)),background:Array.from(bytes.slice(512,516))};buffer.unmap();buffer.destroy();return out;
        })()`);
        assert.ok(pixels.player.slice(0,3).some(n=>n>100),'textured player pixels');assert.ok(pixels.boss.slice(0,3).some(n=>n>100),'textured boss pixels');assert.equal(pixels.player[3],255);
    }
    const won=await evaluate('window.grazerGameAdvance(4000,0,0,1)');assert.equal(won.phase,2);assert.equal(won.bossHealth,0);assert.ok(Number(won.ticks)>=9000&&Number(won.ticks)<=10800);await screenshot('clear');
    const expected=createHash('sha256').update(readFileSync('target/native-game-trace.txt')).digest('hex');
    const actual=await evaluate(`(async()=>{const text=window.grazerGameTrace(100000).join('\\n')+'\\n',hash=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(text));return Array.from(new Uint8Array(hash),v=>v.toString(16).padStart(2,'0')).join('');})()`);
    assert.equal(actual,expected,'100,000 native/browser Game hashes');
    const metrics={backend,unlocked,boss,won,pixels,traceSha256:actual,userAgent:await evaluate('navigator.userAgent')};
    writeFileSync(`target/m2-${backend}-metrics.json`,JSON.stringify(metrics,null,2));console.log(JSON.stringify(metrics));
    await call('Page.navigate',{url:`http://127.0.0.1:8080/?backend=${backend}&project=./missing.json`});
    for(let i=0;i<80;i++){if(await evaluate('Boolean(window.grazerGameValidation?.error)'))break;await sleep(100);}
    assert.ok(await evaluate('window.grazerGameValidation.error'),'missing resources fail visibly');
    console.log(`PASS M2 ${backend}: sprites/HUD, audio unlock, input/bomb, pause, death/restart, full stage clear, resource failure, 100000 browser/native hashes`);
} finally {await call('Page.close').catch(()=>{});ws.close();}
