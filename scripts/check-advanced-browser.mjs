import {readFileSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
const backend=process.argv[2]??'webgl',port=Number(process.argv[3]??9226);
const page=await fetch(`http://127.0.0.1:${port}/json/new?about:blank`,{method:'PUT'}).then(r=>r.json());
const ws=new WebSocket(page.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});
let next=0;const pending=new Map();
ws.onmessage=({data})=>{const m=JSON.parse(data);if(!m.id)return;const p=pending.get(m.id);pending.delete(m.id);if(m.error)p.reject(new Error(JSON.stringify(m.error)));else p.resolve(m.result);};
function call(method,params={}){const id=++next;return new Promise((resolve,reject)=>{pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});}
async function evaluate(expression){const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw new Error(JSON.stringify(r.exceptionDetails));return r.result.value;}
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function ready(script='',difficulty='normal',health=10000){
    await call('Page.navigate',{url:`http://127.0.0.1:8080/?backend=${backend}&health=${health}&difficulty=${difficulty}${script?`&script=./${script}.graze`:''}`});
    for(let i=0;i<180;i++) {const s=await evaluate('window.grazerGameValidation');if(s?.error)throw new Error(s.error);if(s?.ready&&s.frames>=3){await evaluate('window.grazerSetPaused(true)');await evaluate('document.querySelector("#overlay").hidden=true');return await evaluate('window.grazerGameRestart()');}await sleep(250);}
    throw new Error('M4 initialization timeout');
}
async function screenshot(name){const shot=await call('Page.captureScreenshot',{format:'png'});writeFileSync(`target/m4-${backend}-${name}.png`,Buffer.from(shot.data,'base64'));}
async function probeBeam(){
    if(backend!=='webgpu')return null;
    return evaluate(`(async()=>{
        const s=window.grazerGameLasers(),index=Math.floor(s.length/20)*10;
        const x=Math.round(s[index+4]+(s[index+6]-s[index+4])*0.5)*2,y=Math.round(s[index+5]+(s[index+7]-s[index+5])*0.5+64)*2;
        const canvas=document.querySelector('#game'),context=canvas.getContext('webgpu'),cfg=context.getConfiguration(),device=cfg.device;
        device.pushErrorScope('validation');context.configure({...cfg,usage:cfg.usage|GPUTextureUsage.COPY_SRC});window.grazerDrawForProbe();
        const buffer=device.createBuffer({size:512,usage:GPUBufferUsage.COPY_DST|GPUBufferUsage.MAP_READ}),encoder=device.createCommandEncoder(),texture=context.getCurrentTexture();
        encoder.copyTextureToBuffer({texture,origin:[x,y,0]},{buffer,offset:0,bytesPerRow:256},[1,1,1]);
        encoder.copyTextureToBuffer({texture,origin:[2,260,0]},{buffer,offset:256,bytesPerRow:256},[1,1,1]);
        device.queue.submit([encoder.finish()]);const error=await device.popErrorScope();if(error)throw new Error(error.message);
        await buffer.mapAsync(GPUMapMode.READ);const b=new Uint8Array(buffer.getMappedRange()),out={x,y,beam:Array.from(b.slice(0,4)),background:Array.from(b.slice(256,260))};buffer.unmap();buffer.destroy();return out;
    })()`);
}
try {
    await call('Page.enable');await call('Emulation.setDeviceMetricsOverride',{width:1280,height:1100,deviceScaleFactor:1,mobile:false});
    const initial=await ready();assert.equal(initial.difficulty,1);assert.equal(initial.power,0);
    const waves=await evaluate('window.grazerGameAdvance(20000)');assert.equal(waves.phase,0);assert.equal(waves.power,4);assert.ok(Number(waves.collected)>0);
    const phase1=await evaluate('window.grazerGameAdvance(5201)');assert.equal(phase1.bossPhase,1);assert.equal(phase1.bossMax,6000);assert.equal(phase1.phase,0);
    await screenshot('phase1');
    await evaluate('window.grazerGameAdvance(1,0,0,3)');const phase2=await evaluate('window.grazerGameAdvance(3599)');assert.equal(phase2.bossPhase,2);assert.equal(phase2.phasesStarted,2);
    const warning=await evaluate('window.grazerGameLasers()');assert.equal(warning.length,10);assert.equal(warning[3],0);
    await screenshot('straight-warning');await evaluate('window.grazerGameAdvance(90)');const active=await evaluate('window.grazerGameLasers()');assert.equal(active[3],1);
    await screenshot('straight-active');const straightPixels=await probeBeam();if(straightPixels){assert.ok(straightPixels.beam.slice(0,3).some(n=>n>180),JSON.stringify(straightPixels));assert.equal(straightPixels.beam[3],255);}
    const phase3=await evaluate('window.grazerGameAdvance(3510)');assert.equal(phase3.bossPhase,3);assert.equal(phase3.phasesStarted,3);
    const curveWarning=await evaluate('window.grazerGameLasers()');assert.equal(curveWarning.length,150);assert.equal(curveWarning[143],0);assert.equal(curveWarning[142],14);
    await screenshot('curve-warning');await evaluate('window.grazerGameAdvance(90)');const curveActive=await evaluate('window.grazerGameLasers()');assert.equal(curveActive[3],1);
    await screenshot('curve-active');const curvePixels=await probeBeam();if(curvePixels){assert.ok(curvePixels.beam.slice(0,3).some(n=>n>180),JSON.stringify(curvePixels));assert.equal(curvePixels.beam[3],255);}
    const won=await evaluate('window.grazerGameAdvance(4000)');assert.equal(won.phase,2);assert.equal(won.ticks,'36001');assert.equal(won.phasesStarted,3);assert.ok(Number(won.cancelled)>0);await screenshot('clear');
    const restarted=await evaluate('window.grazerGameRestart()');assert.equal(restarted.hash,initial.hash);assert.equal(restarted.power,0);assert.equal(restarted.phasesStarted,0);
    const expected=createHash('sha256').update(readFileSync('target/native-advanced-trace.txt')).digest('hex');
    const actual=await evaluate(`(async()=>{const t=window.grazerAdvancedGameTrace(100000).join('\\n')+'\\n',h=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(t));return Array.from(new Uint8Array(h),v=>v.toString(16).padStart(2,'0')).join('');})()`);assert.equal(actual,expected);
    for(const [name,value] of [['easy',0],['hard',2]]) {const m=await ready('examples/ring',name);assert.equal(m.difficulty,value);const r=await evaluate('window.grazerGameAdvance(1)');assert.equal(r.projectiles,18);}
    const metrics={backend,waves,phase1,phase2,phase3,won,straightPixels,curvePixels,traceSha256:actual,userAgent:await evaluate('navigator.userAgent')};
    writeFileSync(`target/m4-${backend}-metrics.json`,JSON.stringify(metrics,null,2));console.log(JSON.stringify(metrics));
    console.log(`PASS M4 ${backend}: ten minutes, three Boss phases, both warning/active lasers, drops/power/scoring, difficulties, restart, 100000 native/browser hashes`);
} finally {await call('Page.close').catch(()=>{});ws.close();}
