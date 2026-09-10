// Run against an isolated Chrome --remote-debugging-port=9223 and web server :8080.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
const backend = process.argv[2] ?? 'webgpu';
const count = Number(process.argv[3] ?? 30000);
const page = await fetch('http://127.0.0.1:9223/json/new?about:blank', { method: 'PUT' }).then(r => r.json());
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
let next = 0;
const pending = new Map();
ws.onmessage = ({data}) => {
    const message = JSON.parse(data);
    if (!message.id) return;
    const callback = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) callback.reject(new Error(JSON.stringify(message.error)));
    else callback.resolve(message.result);
};
function call(method, params = {}) {
    const id = ++next;
    return new Promise((resolve, reject) => { pending.set(id, {resolve, reject}); ws.send(JSON.stringify({id, method, params})); });
}
async function evaluate(expression) {
    const result = await call('Runtime.evaluate', {expression, returnByValue: true, awaitPromise: true});
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
}
const sleep = ms => new Promise(r => setTimeout(r, ms));
try {
    await call('Page.enable');
    await call('Emulation.setDeviceMetricsOverride', {width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false});
    await call('Page.navigate', {url: `http://127.0.0.1:8080/?backend=${backend}&count=${count}`});
    for (let attempt = 0; ; attempt++) {
        const state = await evaluate('window.grazerValidation');
        if (state?.error) throw new Error(state.error);
        if (state?.ready && state.frames >= 180) break;
        if (attempt >= 600) throw new Error('timed out waiting for 180 rendered frames');
        if (attempt % 30 === 0) console.log(`${backend}: frames=${state?.frames ?? 0}`);
        await sleep(500);
    }
    const rect = await evaluate('(() => { const r = document.querySelector("#audio").getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()');
    await call('Input.dispatchMouseEvent', {type: 'mousePressed', ...rect, button: 'left', clickCount: 1});
    await call('Input.dispatchMouseEvent', {type: 'mouseReleased', ...rect, button: 'left', clickCount: 1});
    await sleep(200);
    const metrics = await evaluate('window.grazerMetrics()');
    assert.equal(metrics.audio, 'running');
    assert.equal(metrics.error, null);
    assert.ok(metrics.adapter.includes(backend === 'webgl' ? '/ Gl' : '/ BrowserWebGpu'), metrics.adapter);
    await evaluate('window.grazerSetPaused(true)');
    const tick = (await evaluate('window.grazerMetrics()')).ticks;
    await sleep(100);
    assert.equal((await evaluate('window.grazerMetrics()')).ticks, tick, 'pause must stop ticks');
    const native = readFileSync('target/native-trace.txt', 'utf8');
    const expected = createHash('sha256').update(native).digest('hex');
    const actual = await evaluate(`(async () => {
        const trace = window.grazerTrace(100000).join('\\n') + '\\n';
        const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(trace));
        return Array.from(new Uint8Array(digest), b=>b.toString(16).padStart(2,'0')).join('');
    })()`);
    assert.equal(actual, expected, 'all 100,000 browser tick hashes must match native');
    const screenshot = await call('Page.captureScreenshot', {format:'png'});
    writeFileSync(`target/${backend}.png`, Buffer.from(screenshot.data, 'base64'));
    writeFileSync(`target/${backend}-metrics.json`, JSON.stringify({...metrics, traceSha256:actual, conformanceTicks:100000}, null, 2));
    console.log(JSON.stringify({...metrics, traceSha256: actual}));
    console.log(`PASS ${backend}: rendered, audio unlocked, paused, and 100,000 native/browser tick hashes agree`);
} finally {
    await call('Page.close').catch(() => {});
    ws.close();
}
