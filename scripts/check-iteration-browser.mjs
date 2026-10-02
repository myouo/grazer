import {writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const backend=process.argv[2]??'webgl',port=Number(process.argv[3]??9226);
const page=await fetch(`http://127.0.0.1:${port}/json/new?about:blank`,{method:'PUT'}).then(r=>r.json());
const ws=new WebSocket(page.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});
let next=0;const pending=new Map();ws.onmessage=({data})=>{const m=JSON.parse(data);if(!m.id)return;const p=pending.get(m.id);pending.delete(m.id);if(m.error)p.reject(new Error(JSON.stringify(m.error)));else p.resolve(m.result);};
function call(method,params={}){const id=++next;return new Promise((resolve,reject)=>{pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});}
async function evaluate(expression){const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw new Error(JSON.stringify(r.exceptionDetails));return r.result.value;}
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function ready(health=10000){await call('Page.navigate',{url:`http://127.0.0.1:8080/?backend=${backend}&health=${health}`});for(let i=0;i<180;i++){const s=await evaluate('window.grazerGameValidation');if(s?.error)throw new Error(s.error);if(s?.ready&&s.frames>=3){await evaluate('window.grazerSetPaused(true)');await evaluate('document.querySelector("#overlay").hidden=true');return await evaluate('window.grazerGameRestart()');}await sleep(250);}throw new Error('M5 init timeout');}
async function screenshot(name){const r=await call('Page.captureScreenshot',{format:'png'});writeFileSync(`target/m5-${backend}-${name}.png`,Buffer.from(r.data,'base64'));}
async function pixel(x,y){if(backend!=='webgpu')return null;return evaluate(`(async()=>{const c=document.querySelector('#game').getContext('webgpu'),cfg=c.getConfiguration(),d=cfg.device;d.pushErrorScope('validation');c.configure({...cfg,usage:cfg.usage|GPUTextureUsage.COPY_SRC});window.grazerDrawForProbe();const b=d.createBuffer({size:256,usage:GPUBufferUsage.COPY_DST|GPUBufferUsage.MAP_READ}),e=d.createCommandEncoder();e.copyTextureToBuffer({texture:c.getCurrentTexture(),origin:[${x},${y},0]},{buffer:b,bytesPerRow:256},[1,1,1]);d.queue.submit([e.finish()]);const error=await d.popErrorScope();if(error)throw new Error(error.message);await b.mapAsync(GPUMapMode.READ);const p=Array.from(new Uint8Array(b.getMappedRange()).slice(0,4));b.unmap();b.destroy();return p;})()`);}
try {
    await call('Page.enable');await call('Emulation.setDeviceMetricsOverride',{width:1280,height:1600,deviceScaleFactor:1,mobile:false});
    const initial=await ready();await evaluate('window.grazerDebug.record(10)');
    const at20=await evaluate('window.grazerGameAdvance(20,1,0,1)');await evaluate('window.savedCheckpoint=window.grazerDebug.checkpoint()');
    const at30=await evaluate('window.grazerGameAdvance(10,-1,0,5)');await evaluate('window.grazerGameRestart()');const final=await evaluate('window.grazerGameAdvance(10,0,0,1)');
    const length=await evaluate('window.savedReplay=window.grazerDebug.stop();window.grazerDebug.status()');assert.equal(length.recording,false);
    await evaluate('window.grazerDebug.load(window.savedReplay)');assert.equal((await evaluate('window.grazerDebug.status()')).length,41);
    const played=await evaluate('window.grazerGameAdvance(41,0,0,0)');assert.equal(played.hash,final.hash);assert.equal((await evaluate('window.grazerDebug.status()')).frame,41);
    assert.equal((await evaluate('window.grazerDebug.seek(20)')).hash,at20.hash);
    const intact=await evaluate('window.grazerGameMetrics().hash');const seekError=await evaluate('(()=>{try{window.grazerDebug.seek(42);return null;}catch(e){return String(e);}})()');assert.ok(seekError);assert.equal(await evaluate('window.grazerGameMetrics().hash'),intact);
    await evaluate('window.grazerDebug.restore(window.savedCheckpoint)');assert.equal((await evaluate('window.grazerGameMetrics()')).hash,at20.hash);assert.equal((await evaluate('window.grazerGameAdvance(10,-1,0,5)')).hash,at30.hash);
    const beforeBad=await evaluate('window.grazerGameMetrics().hash');const corrupt=await evaluate('(()=>{const b=window.savedReplay.slice();b[20]^=1;try{window.grazerDebug.load(b);return null;}catch(e){return String(e);}})()');assert.ok(corrupt.includes('fingerprint'));assert.equal(await evaluate('window.grazerGameMetrics().hash'),beforeBad);
    await ready(0);const practice=await evaluate('window.grazerDebug.practice(2)');assert.equal(practice.health,3);assert.equal(practice.bossPhase,2);assert.equal(practice.ticks,'28801');
    const hash=practice.hash;await evaluate('window.grazerDebug.boxes(true)');await evaluate('window.grazerDebug.panel(true)');assert.equal(await evaluate('window.grazerGameMetrics().hash'),hash);
    const inspector=await evaluate('window.grazerDebug.inspect()');assert.ok(inspector.tasks.length>=3);assert.ok(inspector.entities.some(e=>e.kind===0));assert.ok(inspector.tasks.some(t=>t.frames.some(f=>f.line>0&&f.registers.length>0)));
    await evaluate('document.querySelector("#studio").open=true;document.querySelector("#inspector").textContent=JSON.stringify(window.grazerDebug.inspect(),null,2)');await screenshot('practice-studio');
    await evaluate('window.grazerDebug.panel(false)');const ringPixel=await pixel(483,1248);if(ringPixel)assert.ok(ringPixel[0]>ringPixel[1]+25,JSON.stringify(ringPixel));
    await evaluate('window.grazerGameAdvance(1)');assert.equal((await evaluate('window.grazerGameMetrics()')).ticks,'28802');assert.equal((await evaluate('window.grazerGameRestart()')).hash,hash);
    const failed=await evaluate('(()=>{try{window.grazerDebug.reload("task main() { let n: int = true; }");return null;}catch(e){return String(e);}})()');assert.ok(failed.includes('stage.graze:1:'));assert.equal(await evaluate('window.grazerGameMetrics().hash'),hash);
    const source='task main() { wave(77); wait(100); }';await evaluate(`window.grazerDebug.reload(${JSON.stringify(source)})`);const reloaded=await evaluate('window.grazerGameAdvance(1,0,0,0)');assert.equal(reloaded.wave,77);assert.equal(reloaded.ticks,'1');
    await evaluate('window.grazerDebug.record(0)');await evaluate('window.grazerGameAdvance(1,0,0,0)');await evaluate('window.preAssetReplay=window.grazerDebug.stop()');
    const assetReload=await evaluate(`(async()=>{const m=await fetch('./assets/demo/project.json').then(r=>r.text()),p=JSON.parse(m),a=new Uint8Array(await fetch('./assets/demo/'+p.atlas.file).then(r=>r.arrayBuffer()));const i=(8*128+8)*4;a.set([0,0,255,255],i);window.grazerDebug.reloadAssets(${JSON.stringify(source)},m,a);window.grazerDrawForProbe();return window.grazerGameMetrics();})()`);assert.notEqual(assetReload.resources,practice.resources);
    const bluePixel=await pixel(480,1248);if(bluePixel)assert.ok(bluePixel[2]>150&&bluePixel[0]<20&&bluePixel[1]<20,JSON.stringify(bluePixel));
    const wrongAsset=await evaluate('(()=>{try{window.grazerDebug.load(window.preAssetReplay);return null;}catch(e){return String(e);}})()');assert.ok(wrongAsset.includes('resource summary'),wrongAsset);
    const metrics={backend,recordingFrames:41,at20,at30,played,practice,taskCount:inspector.tasks.length,ringPixel,reloaded,assetReload,bluePixel,corrupt,wrongAsset};writeFileSync(`target/m5-${backend}-metrics.json`,JSON.stringify(metrics,null,2));console.log(JSON.stringify(metrics));
    console.log(`PASS M5 ${backend}: replay/reset/seek, full checkpoint, normal-health practice, pause/step, hitboxes/performance/inspection, atomic source/resource reload, compatibility rejection`);
} finally {await call('Page.close').catch(()=>{});ws.close();}
