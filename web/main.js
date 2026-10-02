import init, { WebGame, game_conformance_trace, script_conformance_trace, advanced_conformance_trace } from './pkg/grazer.js';
const canvas = document.querySelector('#game'), status = document.querySelector('#status');
const query = new URLSearchParams(location.search), keys = new Set();
let runtime, audio, sounds, last = null, paused = false, focused = true, bombPending = false;
let projectURL,stageURL,manifestCache,atlasCache;
const voices = new Set();
const state = window.grazerGameValidation = {ready:false,frames:0,audio:'locked',scheduledAudio:0,error:null};
const backend = query.get('backend') ?? 'auto';
document.querySelector('#backend').value = backend;
document.querySelector('#backend').onchange = event => { query.set('backend',event.target.value); location.search = query.toString(); };
document.querySelector('#difficulty').value = query.get('difficulty') ?? 'normal';
document.querySelector('#difficulty').onchange = event => {query.set('difficulty',event.target.value);location.search=query.toString();};
function metrics() {
    const h = runtime?.hud();
    const a=runtime?.advanced_hud();
    return {...state,paused,ticks:runtime?.tick().toString(),score:runtime?.score().toString(),health:h?.[0],bombs:h?.[1],phase:h?.[2],wave:h?.[3],bossHealth:h?.[4],bossMax:h?.[5],projectiles:h?.[6],enemies:h?.[7],bombFlash:h?.[8],difficulty:a?.length?Number(a[0]):null,power:a?.length?Number(a[1]):null,drops:a?.length?Number(a[2]):null,bossPhase:a?.length?Number(a[3]):null,phaseTicks:a?.length?Number(a[4]):null,phasesStarted:a?.length?Number(a[5]):null,collected:a?.length?a[6].toString():null,cancelled:a?.length?a[7].toString():null,phaseBonus:a?.length?a[8].toString():null,hash:runtime?.state_hash(),resources:runtime?.resource_hash(),program:runtime?.program_hash(),diagnostic:runtime?.diagnostic()};
}
function playSound(id) {
    if (!audio || audio.state !== 'running' || voices.size >= 16) return;
    const sound = sounds.get(id); if (!sound) throw new Error(`Missing sound ${id}`);
    const oscillator = audio.createOscillator(), gain = audio.createGain(), now = audio.currentTime;
    oscillator.type = ['square','triangle','sine'][sound.waveform]; oscillator.frequency.value = sound.frequency;
    gain.gain.setValueAtTime(0,now); gain.gain.linearRampToValueAtTime(sound.gain_q8 / 256,now+0.005); gain.gain.linearRampToValueAtTime(0,now+sound.duration_ms/1000);
    oscillator.connect(gain).connect(audio.destination); voices.add(oscillator);
    oscillator.onended = () => { voices.delete(oscillator); oscillator.disconnect(); gain.disconnect(); };
    oscillator.start(); oscillator.stop(now+sound.duration_ms/1000+0.01); state.scheduledAudio++;
}
function resetClock() {last = null; runtime?.reset_clock();}
async function setPaused(value) {
    paused = value; keys.clear(); bombPending = false; resetClock();
    runtime?.set_paused(value);updateStudio();
    document.querySelector('#pause').textContent = value ? 'Resume' : 'Pause'; document.querySelector('#overlay').hidden = !value;
    if (audio) { if (value) await audio.suspend(); else await audio.resume(); state.audio = audio.state; }
}
document.querySelector('#pause').onclick = () => setPaused(!paused).catch(fail);
document.querySelector('#restart').onclick = () => { runtime.restart(); resetClock(); keys.clear(); bombPending=false; canvas.focus(); };
document.querySelector('#audio').onclick = async () => {
    try { audio ??= new AudioContext(); await audio.resume(); state.audio = audio.state; playSound(1); document.querySelector('#audio').textContent = 'Sound enabled'; }
    catch (error) { status.textContent = `Sound unavailable: ${error}`; state.audio = String(error); }
};
const gameKeys = new Set(['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','KeyA','KeyD','KeyW','KeyS','KeyZ','Space','KeyX','ShiftLeft','ShiftRight','KeyR','Enter']);
window.addEventListener('keydown', event => {
    if (event.target.matches('select,input,textarea')) return;
    if(event.code==='F5'&&!event.repeat){event.preventDefault();reloadFiles().catch(toolFailure);return;}
    if(event.code==='KeyN'&&!event.repeat&&paused){event.preventDefault();studioStep();return;}
    if (event.code === 'KeyP' && !event.repeat) {event.preventDefault(); setPaused(!paused).catch(fail);}
    if (gameKeys.has(event.code)) {event.preventDefault(); keys.add(event.code);}
});
window.addEventListener('keyup', event => keys.delete(event.code));
window.addEventListener('blur', () => { focused=false; keys.clear(); bombPending=false; resetClock(); });
window.addEventListener('focus', () => { focused=true; resetClock(); });
document.addEventListener('visibilitychange', () => { keys.clear(); bombPending=false; resetClock(); if(document.hidden) {for(const voice of voices) voice.stop();} });
canvas.addEventListener('pointerdown', () => canvas.focus());
for (const button of document.querySelectorAll('[data-key]')) {
    button.onpointerdown = event => {event.preventDefault(); button.setPointerCapture(event.pointerId); keys.add(button.dataset.key);};
    button.onpointerup = button.onpointercancel = () => keys.delete(button.dataset.key);
    button.onlostpointercapture = () => keys.delete(button.dataset.key);
}
document.querySelector('#touch-bomb').onclick = () => bombPending=true;
function fail(error) {state.error=String(error);status.textContent=`Stage paused: ${error}`;try{runtime?.draw();}catch{}}
function updateStatus() {
    const m=metrics(); status.textContent = m.phase===1 ? 'Ship lost. Press R or Restart for a new run.' : m.phase===2 ? 'Stage clear. Press R to fly again.' : `${m.bossPhase?`Boss phase ${m.bossPhase} · ${Math.ceil(m.phaseTicks/60)}s`:`Wave ${m.wave || 1}`} · ${m.health} lives · ${m.bombs} bombs${m.power!==null?` · Power ${m.power}`:''}`;
}
function debugStatus(){return runtime?JSON.parse(runtime.debug_status()):{};}
function updateStudio(){if(!runtime)return;const d=debugStatus();document.querySelector('#step').disabled=!paused;document.querySelector('#record').textContent=d.recording?'Stop and save replay':'Record replay';document.querySelector('#seek').disabled=!d.playback||!paused;document.querySelector('#replay-status').textContent=`${d.playback?'Playback':d.recording?'Recording':'Live'} · frame ${d.frame}/${d.length} · CPU update ${Number(d.updateMs??0).toFixed(2)} ms · draw ${Number(d.drawMs??0).toFixed(2)} ms · p95 ${Number(d.frameP95??0).toFixed(2)} ms`;
    document.querySelector('#hitboxes').setAttribute('aria-pressed',String(d.hitboxes));document.querySelector('#performance').setAttribute('aria-pressed',String(d.performance));}
function download(bytes,name){const url=URL.createObjectURL(new Blob([bytes],{type:'application/octet-stream'})),a=document.createElement('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
function archive(bytes){if(bytes.length)download(bytes,'archived-run.grz');}
function silence(){for(const voice of voices)voice.stop();keys.clear();bombPending=false;}
function toolFailure(error){document.querySelector('#studio-error').textContent=String(error);setPaused(true).catch(fail);}
function studioStep(){try{if(!paused)throw new Error('Pause before stepping');for(const id of runtime.step(0,0,1))playSound(id);runtime.draw();updateStatus();updateStudio();}catch(error){toolFailure(error);}}
async function sourceReload(source,manifest=manifestCache,atlas=atlasCache){archive(runtime.reload(source,manifest,atlas));manifestCache=manifest;atlasCache=atlas;sounds=new Map(JSON.parse(manifest).sounds.map(s=>[s.id,s]));document.querySelector('#source-editor').value=source;document.querySelector('#studio-error').textContent='';silence();await setPaused(true);runtime.draw();updateStatus();}
async function reloadFiles(){const response=await fetch(projectURL,{cache:'no-store'});if(!response.ok)throw new Error(`Reload project failed (${response.status})`);const manifest=await response.text(),url=new URL(JSON.parse(manifest).atlas.file,projectURL);if(url.origin!==location.origin)throw new Error('Atlas must use this origin');const [atlas,source]=await Promise.all([fetch(url,{cache:'no-store'}),fetch(stageURL,{cache:'no-store'})]);if(!atlas.ok||!source.ok)throw new Error('Reload asset/source fetch failed');await sourceReload(await source.text(),manifest,new Uint8Array(await atlas.arrayBuffer()));}
document.querySelector('#step').onclick=studioStep;
document.querySelector('#forward').onclick=()=>{try{runtime.fast_forward(600,1);runtime.draw();updateStatus();updateStudio();}catch(e){toolFailure(e);}};
document.querySelector('#hitboxes').onclick=()=>{runtime.set_hitboxes(!debugStatus().hitboxes);runtime.draw();updateStudio();};
document.querySelector('#performance').onclick=()=>{runtime.set_performance_panel(!debugStatus().performance);runtime.draw();updateStudio();};
document.querySelector('#record').onclick=()=>{try{if(debugStatus().recording)download(runtime.stop_recording(),'grazer-run.grz');else runtime.start_recording(600);updateStudio();}catch(e){toolFailure(e);}};
document.querySelector('#save-checkpoint').onclick=()=>{try{download(runtime.checkpoint(),'grazer-checkpoint.gcp');}catch(e){toolFailure(e);}};
document.querySelector('#inspect').onclick=()=>{document.querySelector('#inspector').textContent=JSON.stringify(JSON.parse(runtime.inspect()),null,2);};
document.querySelector('#practice').onclick=async()=>{try{silence();archive(runtime.practice(Number(document.querySelector('#practice-phase').value)));await setPaused(true);runtime.draw();updateStatus();}catch(e){toolFailure(e);}};
document.querySelector('#seek').onclick=()=>{try{runtime.seek(Number(document.querySelector('#seek-frame').value));silence();runtime.draw();updateStatus();updateStudio();}catch(e){toolFailure(e);}};
document.querySelector('#apply-source').onclick=()=>sourceReload(document.querySelector('#source-editor').value).catch(toolFailure);
document.querySelector('#reload-files').onclick=()=>reloadFiles().catch(toolFailure);
document.querySelector('#replay-file').onchange=async event=>{try{const file=event.target.files[0];if(!file)return;if(file.size>256*1024*1024)throw new Error('Replay file exceeds 256 MiB');const bytes=new Uint8Array(await file.arrayBuffer());archive(file.name.endsWith('.gcp')?runtime.restore_checkpoint(bytes):runtime.load_replay(bytes));silence();await setPaused(true);runtime.draw();updateStatus();}catch(e){toolFailure(e);}};
try {
    await init();
    const project = new URL(query.get('project') ?? './assets/demo/project.json',location.href);
    projectURL=project;
    if (project.origin !== location.origin) throw new Error('Project assets must use this origin');
    const response = await fetch(project); if (!response.ok) throw new Error(`Project load failed (${response.status})`);
    const manifest = await response.text(), parsed = JSON.parse(manifest), atlasURL = new URL(parsed.atlas.file,project);
    if(atlasURL.origin!==location.origin) throw new Error('Atlas must use this origin');
    const atlasResponse = await fetch(atlasURL); if(!atlasResponse.ok) throw new Error(`Atlas load failed (${atlasResponse.status})`);
    const health = Number(query.get('health') ?? 0); if(!Number.isInteger(health)||health<0||health>10000) throw new Error('health must be 0..10,000');
    const scriptURL=new URL(query.get('script')??'./advanced_showcase.graze',project);
    stageURL=scriptURL;
    if(scriptURL.origin!==location.origin)throw new Error('Script must use this origin');
    const scriptResponse=await fetch(scriptURL);if(!scriptResponse.ok)throw new Error(`Script load failed (${scriptResponse.status})`);
    const difficulty=['easy','normal','hard'].indexOf(query.get('difficulty')??'normal');if(difficulty<0)throw new Error('difficulty must be easy, normal or hard');
    manifestCache=manifest;atlasCache=new Uint8Array(await atlasResponse.arrayBuffer());const source=await scriptResponse.text();
    runtime = await WebGame.create_with_difficulty(canvas,backend,manifest,atlasCache,health,source,difficulty);
    document.querySelector('#source-editor').value=source;
    for(const id of ['forward','hitboxes','performance','practice','record','save-checkpoint','inspect','replay-file','apply-source','reload-files'])document.querySelector(`#${id}`).disabled=false;
    sounds = new Map(parsed.sounds.map(sound => [sound.id,sound]));
    Object.assign(state,{ready:true,adapter:runtime.adapter(),initialHash:runtime.state_hash(),width:canvas.width,height:canvas.height});
    document.querySelector('#diagnostics').textContent = state.adapter;
    for (const id of ['audio','pause','restart']) document.querySelector(`#${id}`).disabled=false;
    window.grazerGameMetrics=metrics;
    window.grazerGameTrace=(frames=100000)=>Array.from(script_conformance_trace(frames),n=>n.toString(16).padStart(16,'0'));
    window.grazerNativeGameTrace=(frames=100000)=>Array.from(game_conformance_trace(frames),n=>n.toString(16).padStart(16,'0'));
    window.grazerAdvancedGameTrace=(frames=100000)=>Array.from(advanced_conformance_trace(frames),n=>n.toString(16).padStart(16,'0'));
    window.grazerGameLasers=()=>Array.from(runtime.laser_segments());
    window.grazerDebug={status:debugStatus,checkpoint:()=>runtime.checkpoint(),restore:bytes=>runtime.restore_checkpoint(bytes),record:interval=>runtime.start_recording(interval??600),stop:()=>runtime.stop_recording(),load:bytes=>runtime.load_replay(bytes),seek:frame=>{runtime.seek(frame);runtime.draw();return metrics();},practice:phase=>{runtime.practice(phase);paused=true;runtime.draw();updateStatus();return metrics();},boxes:value=>{runtime.set_hitboxes(value);runtime.draw();},panel:value=>{runtime.set_performance_panel(value);runtime.draw();},inspect:()=>JSON.parse(runtime.inspect()),reload:source=>{runtime.reload(source,manifestCache,atlasCache);paused=true;runtime.draw();return metrics();},reloadAssets:(source,manifest,atlas)=>runtime.reload(source,manifest,atlas),forward:steps=>{runtime.fast_forward(steps,1);runtime.draw();return metrics();}};
    window.grazerSetPaused=setPaused; window.grazerDrawForProbe=()=>runtime.draw();
    window.grazerGameAdvance=(steps,x=0,y=0,flags=1)=>{
        if(!Number.isInteger(steps)||steps<0||steps>20000) throw new Error('steps must be 0..20,000');
        for(let i=0;i<steps;i++) for(const id of runtime.step(x,y,flags)) playSound(id);
        runtime.draw();updateStatus();return metrics();
    };
    window.grazerGameRestart=()=>{runtime.restart();resetClock();updateStatus();runtime.draw();return metrics();};
    const frame = now => {
        try {
            const cpuStart=performance.now();
            const active=!document.hidden&&focused&&!paused, elapsed=last===null ? 0 : now-last; last=active ? now : null;
            const due=runtime.ticks_due(elapsed,active);
            for(let i=0;i<due;i++) {
                const x=Number(keys.has('ArrowRight')||keys.has('KeyD'))-Number(keys.has('ArrowLeft')||keys.has('KeyA'));
                const y=Number(keys.has('ArrowDown')||keys.has('KeyS'))-Number(keys.has('ArrowUp')||keys.has('KeyW'));
                const flags=Number(keys.has('KeyZ')||keys.has('Space')||query.get('autoplay')==='1') | Number(keys.has('KeyX')||bombPending)<<1 | Number(keys.has('ShiftLeft')||keys.has('ShiftRight'))<<2 | Number(keys.has('KeyR')||keys.has('Enter'))<<3;
                if(flags&8){runtime.restart();keys.delete('KeyR');keys.delete('Enter');resetClock();break;}
                for(const id of runtime.step(x,y,flags)) playSound(id); bombPending=false;
            }
            const drawStart=performance.now();if(runtime.draw())state.frames++;const cpuEnd=performance.now();runtime.observe_frame(drawStart-cpuStart,cpuEnd-drawStart,cpuEnd-cpuStart);if(runtime.debug_status&&debugStatus().paused&&!paused)setPaused(true).catch(fail);
            if(state.frames%30===0){updateStatus();updateStudio();}requestAnimationFrame(frame);
        } catch(error) {fail(error);}
    };
    updateStatus(); requestAnimationFrame(frame);
} catch(error) {fail(error);}
