// Shaayi M0 spike: trackpad ink, latency and smoothness meter.
// Plain web code, so the same file runs in a browser, Electron or Tauri.

const canvas = document.getElementById('board');
const ctx = canvas.getContext('2d');
const el = (id) => document.getElementById(id);
// Every drawn item, in order: { kind:'segment', a, b, width, session } or
// { kind:'dot', x, y, radius }. Only applyInkOp adds to or removes from it.
const ink = [];
let inkSession = 0; // bumped on every edit-mode change; rollback only touches the current session

function redrawInk() {
  ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);
  for (const item of ink) {
    if (item.kind === 'segment') renderSegment(item.a, item.b, item.width);
    else renderDot(item.x, item.y, item.radius);
  }
}

let dpr = window.devicePixelRatio || 1;
function resize() {
  dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(window.innerWidth * dpr);
  canvas.height = Math.round(window.innerHeight * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  redrawInk();
}
window.addEventListener('resize', resize);

// ---- Latency tracking -------------------------------------------------
// Latency = time from the input event's timestamp to the next frame that
// paints the stroke segment. Measured per pointermove that draws ink.
const samples = [];
let pendingInputTimes = [];
let strokeCount = 0;
let editMode = false;
let mouseFallback = false; // true only when native status says no Precision Touchpad
let pointerPosition = { x: window.innerWidth / 2, y: window.innerHeight / 2 };
const bridgeSamples = [];
let pendingBridgeTimes = [];
window.addEventListener('pointermove', (event) => {
  pointerPosition = { x: event.clientX, y: event.clientY };
});

function percentile(arr, p) {
  if (!arr.length) return null;
  const s = [...arr].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))];
}

function updateStats() {
  el('n').textContent = samples.length;
  if (!samples.length) return;
  const last = samples[samples.length - 1];
  const avg = samples.reduce((a, b) => a + b, 0) / samples.length;
  el('lat').textContent = last.toFixed(1);
  el('avg').textContent = avg.toFixed(1);
  el('p95').textContent = percentile(samples, 95).toFixed(1);
}

// ---- Frame rate --------------------------------------------------------
let frames = 0, lastFpsTime = performance.now();
function frameLoop(now) {
  if (pendingBridgeTimes.length) {
    for (const t of pendingBridgeTimes) bridgeSamples.push(now - t);
    if (bridgeSamples.length > 5000) bridgeSamples.splice(0, bridgeSamples.length - 5000);
    pendingBridgeTimes = [];
    const avg = bridgeSamples.reduce((a, b) => a + b, 0) / bridgeSamples.length;
    el('bridge-lat').textContent = bridgeSamples[bridgeSamples.length - 1].toFixed(1);
    el('bridge-avg').textContent = avg.toFixed(1);
    el('bridge-p95').textContent = percentile(bridgeSamples, 95).toFixed(1);
  }
  // Measure latency for input handled since the last frame.
  if (pendingInputTimes.length) {
    for (const t of pendingInputTimes) {
      const ms = now - t;
      if (ms >= 0 && ms < 1000) samples.push(ms);
    }
    if (samples.length > 5000) samples.splice(0, samples.length - 5000);
    pendingInputTimes = [];
    updateStats();
  }
  frames++;
  if (now - lastFpsTime >= 1000) {
    el('fps').textContent = Math.round((frames * 1000) / (now - lastFpsTime));
    frames = 0; lastFpsTime = now;
  }
  requestAnimationFrame(frameLoop);
}
requestAnimationFrame(frameLoop);


// ---- Drawing: one entry point -------------------------------------------
const INK = () => getComputedStyle(document.documentElement).getPropertyValue('--ink').trim() || '#111';
const SEGMENT_WIDTH = 2.5;
resize();

function renderSegment(a, b, width) {
  ctx.strokeStyle = INK();
  ctx.lineWidth = width;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  ctx.beginPath();
  // Smooth with a midpoint quadratic curve.
  const mx = (a.x + b.x) / 2, my = (a.y + b.y) / 2;
  ctx.moveTo(a.x, a.y);
  ctx.quadraticCurveTo(a.x, a.y, mx, my);
  ctx.stroke();
}

function renderDot(x, y, radius) {
  ctx.fillStyle = INK();
  ctx.beginPath();
  ctx.arc(x, y, radius, 0, Math.PI * 2);
  ctx.fill();
}

function isPoint(p) {
  return Array.isArray(p) && p.length === 2 && Number.isFinite(p[0]) && Number.isFinite(p[1]);
}

// The only place canvas ink is drawn or removed. Ops arrive from the Rust side
// as { op: 'segment', from: [x,y], to: [x,y] }, { op: 'dot', at: [x,y] } or
// { op: 'rollback', segments: n }, with coordinates already in canvas pixels.
function applyInkOp(op) {
  if (!op || !editMode) return; // second safety net behind the Rust-side gating
  if (op.op === 'segment') {
    if (!isPoint(op.from) || !isPoint(op.to)) return;
    const a = { x: op.from[0], y: op.from[1] }, b = { x: op.to[0], y: op.to[1] };
    ink.push({ kind: 'segment', a, b, width: SEGMENT_WIDTH, session: inkSession });
    renderSegment(a, b, SEGMENT_WIDTH);
  } else if (op.op === 'dot') {
    if (!isPoint(op.at)) return;
    ink.push({ kind: 'dot', x: op.at[0], y: op.at[1], radius: 3 });
    renderDot(op.at[0], op.at[1], 3);
  } else if (op.op === 'rollback') {
    let remaining = Math.max(0, Math.floor(Number(op.segments) || 0));
    for (let i = ink.length - 1; i >= 0 && remaining > 0; i--) {
      if (ink[i].kind === 'segment' && ink[i].session === inkSession) {
        ink.splice(i, 1);
        remaining--;
      }
    }
    redrawInk();
  } else {
    return;
  }
  strokeCount = ink.length;
  el('strokes').textContent = strokeCount;
}

function applyInkOps(payload) {
  const receivedAt = performance.now();
  const ops = Array.isArray(payload) ? payload : [payload];
  for (const op of ops) applyInkOp(op);
  pendingBridgeTimes.push(receivedAt);
}

// ---- Mouse fallback ------------------------------------------------------
// Only active in edit mode and only when there is no Precision Touchpad.
// It feeds the same applyInkOp as every other source.
let mouseLast = null;
let mouseMoved = false;
function mousePoint(e) {
  const rect = canvas.getBoundingClientRect();
  return [e.clientX - rect.left, e.clientY - rect.top];
}
canvas.addEventListener('pointerdown', (e) => {
  if (!mouseFallback || !editMode || e.button !== 0) return;
  canvas.setPointerCapture(e.pointerId);
  mouseLast = mousePoint(e);
  mouseMoved = false;
});
canvas.addEventListener('pointermove', (e) => {
  if (!mouseLast) return;
  const events = e.getCoalescedEvents ? e.getCoalescedEvents() : [e];
  for (const ev of (events.length ? events : [e])) {
    const next = mousePoint(ev);
    applyInkOp({ op: 'segment', from: mouseLast, to: next });
    mouseLast = next;
    mouseMoved = true;
    pendingInputTimes.push(ev.timeStamp);
  }
});
function endMouse() {
  if (mouseLast && !mouseMoved) applyInkOp({ op: 'dot', at: mouseLast });
  mouseLast = null;
}
canvas.addEventListener('pointerup', endMouse);
canvas.addEventListener('pointercancel', endMouse);

el('clear').addEventListener('click', () => {
  ink.length = 0;
  redrawInk();
  samples.length = 0;
  strokeCount = 0;
  el('strokes').textContent = 0;
  updateStats();
});

function setEditMode(enabled) {
  mouseLast = null;
  inkSession++; // a new session: rollback never reaches into an earlier one
  editMode = enabled;
  document.body.classList.toggle('editing', enabled);
  el('mode').textContent = enabled ? 'EDIT (pointer clipped, Ctrl+E to exit)' : 'VIEW (Ctrl+E to enter edit)';
  positionEditBox();
}

function setNativeStatus(text) {
  el('native-status').textContent = text;
  mouseFallback = String(text).includes('No Precision Touchpad found');
  el('mouse-note').textContent = mouseFallback ? ' (mouse fallback on)' : '';
}

function positionEditBox() {
  const box = el('edit-box');
  const width = Math.min(320, window.innerWidth * 0.6);
  const height = width / 1.6;
  const x = Math.max(width / 2 + 8, Math.min(window.innerWidth - width / 2 - 8, pointerPosition.x));
  const y = Math.max(height / 2 + 8, Math.min(window.innerHeight - height / 2 - 8, pointerPosition.y));
  box.style.left = `${x}px`;
  box.style.top = `${y}px`;
}
window.addEventListener('resize', positionEditBox);
positionEditBox();

document.addEventListener('keydown', async (event) => {
  if (event.ctrlKey && event.key.toLowerCase() === 'e') {
    event.preventDefault();
    try {
      await window.__TAURI__.core.invoke('set_edit_mode', { enabled: !editMode });
    } catch (error) {
      el('native-status').textContent = String(error);
    }
    return;
  }
  if (editMode && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) {
    event.preventDefault();
    event.stopPropagation();
  }
}, true);


if (window.__TAURI__ && window.__TAURI__.event) {
  window.__TAURI__.event.listen('touchpad-report', (event) => {
    if (event.payload && event.payload.status) setNativeStatus(event.payload.status);
  });
  window.__TAURI__.event.listen('edit-mode-changed', (event) => setEditMode(Boolean(event.payload)));
  window.__TAURI__.event.listen('ink-ops', (event) => applyInkOps(event.payload));
  window.__TAURI__.event.listen('phone-status', (event) => {
    el('phone-status').textContent = event.payload.message;
  });
  window.__TAURI__.core.invoke('get_input_status').then(setNativeStatus).catch((error) => {
    setNativeStatus(String(error));
  });
  window.__TAURI__.core.invoke('get_phone_pairing').then((pairing) => {
    el('phone-status').textContent = pairing.status;
    el('phone-qr').src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(pairing.qrSvg)}`;
    el('phone-qr').hidden = pairing.stopped;
    el('phone-url').href = pairing.url;
    el('phone-url').textContent = pairing.url;
    el('phone-url').hidden = pairing.stopped;
    el('stop-phone').disabled = pairing.stopped;
  }).catch((error) => {
    el('phone-status').textContent = String(error);
  });
} else {
  setNativeStatus('Open this page in the Tauri shell');
}

el('stop-phone').addEventListener('click', async () => {
  try {
    await window.__TAURI__.core.invoke('stop_phone_sharing');
    el('phone-status').textContent = 'Sharing stopped';
    el('phone-qr').hidden = true;
    el('phone-url').hidden = true;
    el('stop-phone').disabled = true;
  } catch (error) {
    el('phone-status').textContent = String(error);
  }
});
