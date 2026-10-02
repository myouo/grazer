import init,{WebFrameBenchmark} from './pkg/grazer.js';
const q=new URLSearchParams(location.search),backend=q.get('backend')??'webgpu',count=Number(q.get('count')??30000),samples=Number(q.get('samples')??1200),warmup=120;
const state=window.grazerFrameBenchmark={ready:false,error:null,metrics:null};
function p95(a){a.sort((a,b)=>a-b);return a[Math.ceil(a.length*0.95)-1];}
const status=document.querySelector('#status'),canvas=document.querySelector('#game');
try {
    if(!Number.isInteger(samples)||samples<1||samples>100000)throw new Error('samples must be 1..100,000');await init();const runtime=await WebFrameBenchmark.create(canvas,backend,count);
    state.adapter=runtime.adapter();state.ready=true;let running=false;
    state.run=async()=>{
        if(running)throw new Error('benchmark already running');running=true;const simulation=[],recycle=[],draw=[],total=[];let recoverable=0;
        try {
            const gl=backend==='webgl'?canvas.getContext('webgl2'):null;
            const device=backend==='webgpu'?canvas.getContext('webgpu').getConfiguration().device:null;
            for(let i=0;i<warmup+samples;i++){
                const start=performance.now();runtime.step();const afterStep=performance.now();runtime.replenish();const afterRecycle=performance.now();
                if(!runtime.draw()){recoverable++;i--;continue;}if(device)await device.queue.onSubmittedWorkDone();else gl.finish();const end=performance.now();
                if(runtime.count()!==count)throw new Error('count differs at draw');if(i>=warmup){simulation.push(afterStep-start);recycle.push(afterRecycle-afterStep);draw.push(end-afterRecycle);total.push(end-start);}
                if(i%60===0)status.textContent=`${state.adapter}\n${count} visible moving bullets · ${i+1}/${warmup+samples} frames`;
                if(i%30===0)await new Promise(requestAnimationFrame);
            }
            const metrics={workload:'M6-full-frame',backend,adapter:state.adapter,width:canvas.width,height:canvas.height,count,samples,warmup,gpuCompletion:true,simulationP95:p95(simulation),replenishP95:p95(recycle),drawGpuCompleteP95:p95(draw),frameP95:p95(total),recoverable,...JSON.parse(runtime.metrics()),userAgent:navigator.userAgent};state.metrics=metrics;status.textContent=JSON.stringify(metrics,null,2);return metrics;
        } finally {running=false;}
    };
    state.draw=()=>runtime.draw();status.textContent=`${state.adapter}\nReady: ${count} moving bullets, 1920 × 1080; call grazerFrameBenchmark.run()`;
    if(q.get('autorun')==='1')state.run().catch(e=>{state.error=String(e);status.textContent=state.error;});
}catch(e){state.error=String(e);status.textContent=state.error;}
