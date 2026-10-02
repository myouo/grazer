import {writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const backend=process.argv[2]??'webgpu',port=Number(process.argv[3]??9230);
const page=await fetch(`http://127.0.0.1:${port}/json/new?about:blank`,{method:'PUT'}).then(r=>r.json()),ws=new WebSocket(page.webSocketDebuggerUrl);await new Promise((r,j)=>{ws.onopen=r;ws.onerror=j;});let id=0;const pending=new Map();ws.onmessage=({data})=>{const m=JSON.parse(data);if(!m.id)return;const p=pending.get(m.id);pending.delete(m.id);if(m.error)p.reject(new Error(JSON.stringify(m.error)));else p.resolve(m.result);};
function call(method,params={}){const n=++id;return new Promise((resolve,reject)=>{pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params}));});}
async function evaluate(expression){const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw new Error(JSON.stringify(r.exceptionDetails));return r.result.value;}
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
try {
 await call('Page.enable');await call('Emulation.setDeviceMetricsOverride',{width:1280,height:1000,deviceScaleFactor:1,mobile:false});await call('Page.navigate',{url:`http://127.0.0.1:8080/?backend=${backend}&health=10000`});
 for(let i=0;i<180;i++){const s=await evaluate('window.grazerGameValidation');if(s?.error)throw new Error(s.error);if(s?.ready&&s.frames>=3)break;await sleep(250);}await evaluate('window.grazerSetPaused(true)');await evaluate('window.grazerGameRestart()');
 const before=await evaluate('window.grazerGameAdvance(240)');
 await call('Emulation.setDeviceMetricsOverride',{width:760,height:900,deviceScaleFactor:2,mobile:false});await evaluate('document.querySelector("#game").width=1200;document.querySelector("#game").height=1920;window.grazerResizeGame(1200,1920);window.grazerDrawForProbe()');const resized=await evaluate('window.grazerGameMetrics()');assert.equal(resized.hash,before.hash);
 await evaluate('window.dispatchEvent(new Event("blur"))');await sleep(300);assert.equal((await evaluate('window.grazerGameMetrics()')).hash,before.hash);await evaluate('window.dispatchEvent(new Event("focus"))');
 let recovered=null;
 if(backend==='webgpu') {await evaluate('document.querySelector("#game").getContext("webgpu").getConfiguration().device.destroy()');for(let i=0;i<60;i++){const s=await evaluate('window.grazerGameValidation');if(s?.error?.includes('GPU device lost'))break;await sleep(100);}const error=await evaluate('window.grazerGameValidation.error');assert.ok(error?.includes('GPU device lost'),error);recovered=await evaluate('window.grazerRecoverGraphics()');assert.equal(recovered.hash,before.hash);assert.equal(recovered.error,null);assert.equal(recovered.paused,true);const stepped=await evaluate('window.grazerGameAdvance(1)');assert.equal(Number(stepped.ticks),Number(before.ticks)+1);}
 const metadata={backend,before,resized,recovered};writeFileSync(`target/m6-${backend}-lifecycle.json`,JSON.stringify(metadata,null,2));console.log(JSON.stringify(metadata));console.log(`PASS M6 ${backend}: scaling/focus, state-preserving presentation ${recovered?'device recovery':'checks'}`);
} finally {await call('Page.close').catch(()=>{});ws.close();}
