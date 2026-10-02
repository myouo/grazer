import {writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const backend=process.argv[2]??'webgpu',port=Number(process.argv[3]??9230),count=Number(process.argv[4]??30000),samples=Number(process.argv[5]??1200);
const page=await fetch(`http://127.0.0.1:${port}/json/new?about:blank`,{method:'PUT'}).then(r=>r.json()),ws=new WebSocket(page.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});let id=0;const pending=new Map();
ws.onmessage=({data})=>{const m=JSON.parse(data);if(!m.id)return;const p=pending.get(m.id);pending.delete(m.id);if(m.error)p.reject(new Error(JSON.stringify(m.error)));else p.resolve(m.result);};
function call(method,params={}){const n=++id;return new Promise((resolve,reject)=>{pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params}));});}
async function evaluate(expression){const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw new Error(JSON.stringify(r.exceptionDetails));return r.result.value;}
try {
 await call('Page.enable');await call('Emulation.setDeviceMetricsOverride',{width:1920,height:1080,deviceScaleFactor:1,mobile:false});await call('Page.navigate',{url:`http://127.0.0.1:8080/frame-benchmark.html?backend=${backend}&count=${count}&samples=${samples}`});
 for(let i=0;i<180;i++){const s=await evaluate('window.grazerFrameBenchmark');if(s?.error)throw new Error(s.error);if(s?.ready)break;await new Promise(r=>setTimeout(r,250));}
 const metrics=await evaluate('window.grazerFrameBenchmark.run()');assert.equal(metrics.count,count);assert.equal(metrics.samples,samples);assert.equal(metrics.width,1920);assert.equal(metrics.height,1080);assert.ok(metrics.hits>0&&metrics.grazes>0);writeFileSync(`target/m6-${backend}-frame.json`,JSON.stringify(metrics,null,2));console.log(JSON.stringify(metrics));
 console.log(`M6 ${backend}: simulation ${metrics.simulationP95.toFixed(3)} ms; GPU-complete frame ${metrics.frameP95.toFixed(3)} ms; ${metrics.simulationP95<=8&&metrics.frameP95<=16.7?'PASS':'BUDGET NOT MET'}`);
} finally {await call('Page.close').catch(()=>{});ws.close();}
