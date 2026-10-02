import init, { WebGame, game_conformance_trace, script_conformance_trace, advanced_conformance_trace } from './pkg/grazer.js';
const canvas = document.querySelector('#game'), status = document.querySelector('#status');
const query = new URLSearchParams(location.search), keys = new Set();
let runtime, audio, sounds, last = null, paused = false, focused = true, bombPending = false;
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
    if (event.target.matches('select,input')) return;
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
try {
    await init();
    const project = new URL(query.get('project') ?? './assets/demo/project.json',location.href);
    if (project.origin !== location.origin) throw new Error('Project assets must use this origin');
    const response = await fetch(project); if (!response.ok) throw new Error(`Project load failed (${response.status})`);
    const manifest = await response.text(), parsed = JSON.parse(manifest), atlasURL = new URL(parsed.atlas.file,project);
    if(atlasURL.origin!==location.origin) throw new Error('Atlas must use this origin');
    const atlasResponse = await fetch(atlasURL); if(!atlasResponse.ok) throw new Error(`Atlas load failed (${atlasResponse.status})`);
    const health = Number(query.get('health') ?? 0); if(!Number.isInteger(health)||health<0||health>10000) throw new Error('health must be 0..10,000');
    const scriptURL=new URL(query.get('script')??'./advanced_showcase.graze',project);
    if(scriptURL.origin!==location.origin)throw new Error('Script must use this origin');
    const scriptResponse=await fetch(scriptURL);if(!scriptResponse.ok)throw new Error(`Script load failed (${scriptResponse.status})`);
    const difficulty=['easy','normal','hard'].indexOf(query.get('difficulty')??'normal');if(difficulty<0)throw new Error('difficulty must be easy, normal or hard');
    runtime = await WebGame.create_with_difficulty(canvas,backend,manifest,new Uint8Array(await atlasResponse.arrayBuffer()),health,await scriptResponse.text(),difficulty);
    sounds = new Map(parsed.sounds.map(sound => [sound.id,sound]));
    Object.assign(state,{ready:true,adapter:runtime.adapter(),initialHash:runtime.state_hash(),width:canvas.width,height:canvas.height});
    document.querySelector('#diagnostics').textContent = state.adapter;
    for (const id of ['audio','pause','restart']) document.querySelector(`#${id}`).disabled=false;
    window.grazerGameMetrics=metrics;
    window.grazerGameTrace=(frames=100000)=>Array.from(script_conformance_trace(frames),n=>n.toString(16).padStart(16,'0'));
    window.grazerNativeGameTrace=(frames=100000)=>Array.from(game_conformance_trace(frames),n=>n.toString(16).padStart(16,'0'));
    window.grazerAdvancedGameTrace=(frames=100000)=>Array.from(advanced_conformance_trace(frames),n=>n.toString(16).padStart(16,'0'));
    window.grazerGameLasers=()=>Array.from(runtime.laser_segments());
    window.grazerSetPaused=setPaused; window.grazerDrawForProbe=()=>runtime.draw();
    window.grazerGameAdvance=(steps,x=0,y=0,flags=1)=>{
        if(!Number.isInteger(steps)||steps<0||steps>20000) throw new Error('steps must be 0..20,000');
        for(let i=0;i<steps;i++) for(const id of runtime.step(x,y,flags)) playSound(id);
        runtime.draw();updateStatus();return metrics();
    };
    window.grazerGameRestart=()=>{runtime.restart();resetClock();updateStatus();runtime.draw();return metrics();};
    const frame = now => {
        try {
            const active=!document.hidden&&focused&&!paused, elapsed=last===null ? 0 : now-last; last=active ? now : null;
            const due=runtime.ticks_due(elapsed,active);
            for(let i=0;i<due;i++) {
                const x=Number(keys.has('ArrowRight')||keys.has('KeyD'))-Number(keys.has('ArrowLeft')||keys.has('KeyA'));
                const y=Number(keys.has('ArrowDown')||keys.has('KeyS'))-Number(keys.has('ArrowUp')||keys.has('KeyW'));
                const flags=Number(keys.has('KeyZ')||keys.has('Space')||query.get('autoplay')==='1') | Number(keys.has('KeyX')||bombPending)<<1 | Number(keys.has('ShiftLeft')||keys.has('ShiftRight'))<<2 | Number(keys.has('KeyR')||keys.has('Enter'))<<3;
                for(const id of runtime.step(x,y,flags)) playSound(id); bombPending=false;
            }
            if(runtime.draw()) state.frames++; if(state.frames%30===0) updateStatus(); requestAnimationFrame(frame);
        } catch(error) {fail(error);}
    };
    updateStatus(); requestAnimationFrame(frame);
} catch(error) {fail(error);}
