import init, { WebDemo, conformance_trace } from './pkg/grazer.js';
const status = document.querySelector('#status'), canvas = document.querySelector('#game');
const query = new URLSearchParams(location.search), keys = new Set();
let paused = false, last = null, accumulator = 0, audio;
window.grazerValidation = { ready: false, frames: 0, audio: 'locked', error: null };
document.querySelector('#audio').onclick = async () => {
    try {
        audio ??= new AudioContext(); await audio.resume();
        const oscillator = audio.createOscillator(), gain = audio.createGain();
        oscillator.frequency.value = 440;
        gain.gain.setValueAtTime(0.08, audio.currentTime);
        gain.gain.exponentialRampToValueAtTime(0.001, audio.currentTime + 0.15);
        oscillator.connect(gain).connect(audio.destination);
        oscillator.start(); oscillator.stop(audio.currentTime + 0.16);
        window.grazerValidation.audio = audio.state;
        document.querySelector('#audio').textContent = `Audio ${audio.state}`;
    } catch (error) { status.textContent = `Audio failed: ${error}`; window.grazerValidation.audio = String(error); }
};
document.querySelector('#pause').onclick = (event) => {
    paused = !paused; last = null; event.target.textContent = paused ? 'Resume' : 'Pause';
};
document.addEventListener('visibilitychange', () => { last = null; keys.clear(); });
window.addEventListener('blur', () => keys.clear());
window.addEventListener('keydown', (event) => { if (event.key.startsWith('Arrow')) { event.preventDefault(); keys.add(event.key); } });
window.addEventListener('keyup', (event) => keys.delete(event.key));
const samples = [], intervals = [], simTimes = [];
const p95 = (values) => { if (!values.length) return null; const sorted = [...values].sort((a,b) => a-b); return sorted[Math.ceil(sorted.length * 0.95)-1]; };
try {
    const count = Number(query.get('count') ?? 30000);
    if (!Number.isInteger(count) || count < 0 || count > 1000000) throw new Error('count must be 0..1,000,000');
    await init();
    const runtime = await WebDemo.create(canvas, count, query.get('backend') ?? 'auto');
    const adapter = runtime.adapter();
    Object.assign(window.grazerValidation, { ready: true, adapter, count, width: canvas.width, height: canvas.height });
    window.grazerTrace = (ticks = 100000) => Array.from(conformance_trace(ticks), n => n.toString(16).padStart(16,'0'));
    window.grazerMetrics = () => ({ ...window.grazerValidation, ticks: runtime.tick().toString(), cpuSubmitP95Ms: p95(samples), rafIntervalP95Ms: p95(intervals), simulationStepP95Ms: p95(simTimes), samples: samples.length, hash: runtime.state_hash(), userAgent: navigator.userAgent });
    window.grazerSetPaused = (value) => { paused = value; last = null; };
    window.grazerDrawForProbe = () => runtime.draw();
    let measuredFrames = 0;
    const frame = (now) => {
        if (document.hidden || paused) { last = null; requestAnimationFrame(frame); return; }
        const dt = last === null ? 0 : now-last; last = now; accumulator += dt;
        const start = performance.now();
        try {
            for (let i=0; accumulator >= 1000/60 && i<8; i++) {
                const simStart = performance.now();
                runtime.step(Number(keys.has('ArrowRight'))-Number(keys.has('ArrowLeft')), Number(keys.has('ArrowDown'))-Number(keys.has('ArrowUp')));
                if (measuredFrames > 120 && simTimes.length < 2000) simTimes.push(performance.now()-simStart);
                accumulator -= 1000/60;
            }
            if (runtime.draw()) window.grazerValidation.frames++;
            measuredFrames++;
            if (measuredFrames > 120 && samples.length < 1200) { samples.push(performance.now()-start); intervals.push(dt); }
            if (measuredFrames % 60 === 0) status.textContent = `${adapter}\n${count.toLocaleString()} bullets · tick ${runtime.tick()} · CPU submit p95 ${p95(samples)?.toFixed(2) ?? 'warming up'} ms`;
            requestAnimationFrame(frame);
        } catch (error) { status.textContent = `Runtime failed: ${error}`; window.grazerValidation.error = String(error); }
    };
    status.textContent = `${adapter}\n${count.toLocaleString()} bullets`; requestAnimationFrame(frame);
} catch (error) { status.textContent = `Initialization failed: ${error}`; window.grazerValidation.error = String(error); }
