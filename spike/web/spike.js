// Shaayi M0 spike: trackpad ink, latency and smoothness meter.
// Plain web code, so the same file runs in a browser, Electron or Tauri.

const canvas = document.getElementById('board');
const ctx = canvas.getContext('2d');
const el = (id) => document.getElementById(id);
const strokes = [];
const dots = [];

function redrawInk() {
  ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);
  for (const item of strokes) {
    for (const segment of item.segments) renderSegment(segment.a, segment.b, segment.width);
  }
  ctx.fillStyle = INK();
  for (const dot of dots) {
    ctx.beginPath();
    ctx.arc(dot.x, dot.y, dot.radius, 0, Math.PI * 2);
    ctx.fill();
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
let reportCount = 0;
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

// ---- Drawing -----------------------------------------------------------
let drawing = false;
let stroke = null; // { points: [{x,y,p}], color, width }
const phoneTouches = new Map();
const nativeTouches = new Set();
let phoneStroke = null;
let phoneGesture = false;
let phoneStrokeStartedAt = 0;
const INK = () => getComputedStyle(document.documentElement).getPropertyValue('--ink').trim() || '#111';

function widthFor(pressure, pointerType) {
  // Trackpads often report pressure 0.5 or 0 when not force-touch,
  // so fall back to a fixed width in that case.
  if (pointerType === 'pen' || (pressure > 0 && pressure !== 0.5)) {
    return 1.5 + pressure * 4;
  }
  return 2.5;
}
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

function drawSegment(a, b, width, target) {
  target.segments.push({ a: { x: a.x, y: a.y }, b: { x: b.x, y: b.y }, width });
  renderSegment(a, b, width);
}

function addDot(point, radius = 3) {
  dots.push({ x: point.x, y: point.y, radius });
  ctx.fillStyle = INK();
  ctx.beginPath();
  ctx.arc(point.x, point.y, radius, 0, Math.PI * 2);
  ctx.fill();
}

canvas.addEventListener('pointerdown', (e) => {
  if (editMode) {
    // Until contact coordinates are decoded, the click is represented at the
    // anchored centre of the edit box.
    const rect = el('edit-box').getBoundingClientRect();
    const canvasRect = canvas.getBoundingClientRect();
    addDot({ x: rect.left + rect.width / 2 - canvasRect.left, y: rect.top + rect.height / 2 - canvasRect.top });
    return;
  }
  if (e.button !== 0) return;
  canvas.setPointerCapture(e.pointerId);
  drawing = true;
  stroke = { points: [], segments: [], source: 'pointer' };
  strokes.push(stroke);
  strokeCount++;
  el('strokes').textContent = strokeCount;
  addPoint(e);
});

canvas.addEventListener('pointermove', (e) => {
  if (!drawing) return;
  // getCoalescedEvents gives every sample the OS delivered this frame,
  // which is what makes trackpad/pen strokes look smooth.
  const events = e.getCoalescedEvents ? e.getCoalescedEvents() : [e];
  for (const ev of (events.length ? events : [e])) addPoint(ev);
});

function endStroke(e) {
  if (!drawing) return;
  drawing = false;
  stroke = null;
}
canvas.addEventListener('pointerup', endStroke);
canvas.addEventListener('pointercancel', endStroke);

function addPoint(e) {
  const rect = canvas.getBoundingClientRect();
  const p = { x: e.clientX - rect.left, y: e.clientY - rect.top, pr: e.pressure };
  el('pr').textContent = e.pressure.toFixed(2);
  el('pt').textContent = e.pointerType || 'unknown';

  const prev = stroke.points[stroke.points.length - 1];
  stroke.points.push(p);
  if (prev) {
    const w = widthFor(p.pr, e.pointerType);
    drawSegment(prev, p, w, stroke);
  }
  // Record when this input happened so the next frame can compute latency.
  pendingInputTimes.push(e.timeStamp);
}

el('clear').addEventListener('click', () => {
  strokes.length = 0;
  dots.length = 0;
  redrawInk();
  samples.length = 0;
  strokeCount = 0;
  el('strokes').textContent = 0;
  updateStats();
});

function mapPhonePoint(x, y) {
  const box = el('edit-box').getBoundingClientRect();
  const canvasRect = canvas.getBoundingClientRect();
  return { x: box.left + x * box.width - canvasRect.left, y: box.top + y * box.height - canvasRect.top };
}

function updatePhoneContactCount() {
  el('phone-contact-count').textContent = phoneTouches.size;
}

function phoneInput(input) {
  const id = Number(input.id);
  if (!Number.isSafeInteger(id)) return;
  const point = mapPhonePoint(input.x, input.y);
  const phase = input.phase;

  if (phase === 'down') {
    if (!editMode) return;
    phoneTouches.set(id, point);
    updatePhoneContactCount();
    if (phoneTouches.size === 1) {
      phoneGesture = false;
      phoneStrokeStartedAt = performance.now();
      phoneStroke = { points: [point], segments: [], source: 'phone', tentative: true };
      strokes.push(phoneStroke);
      strokeCount++;
      el('strokes').textContent = strokeCount;
    } else {
      phoneGesture = true;
      if (phoneTouches.size === 2) {
        if (performance.now() - phoneStrokeStartedAt <= 150 && phoneStroke) {
          const index = strokes.indexOf(phoneStroke);
          if (index >= 0) strokes.splice(index, 1);
          strokeCount = Math.max(0, strokeCount - 1);
          el('strokes').textContent = strokeCount;
          redrawInk();
        } else if (phoneStroke && phoneStroke.segments.length === 0) {
          const index = strokes.indexOf(phoneStroke);
          if (index >= 0) {
            strokes.splice(index, 1);
            strokeCount = Math.max(0, strokeCount - 1);
            el('strokes').textContent = strokeCount;
            redrawInk();
          }
        }
        phoneStroke = null;
      }
    }
    return;
  }

  if (!phoneTouches.has(id)) return;
  if (phase === 'move') {
    phoneTouches.set(id, point);
    if (phoneGesture || phoneTouches.size !== 1 || !phoneStroke) return;
    const previous = phoneStroke.points[phoneStroke.points.length - 1];
    phoneStroke.points.push(point);
    drawSegment(previous, point, 2.5, phoneStroke);
    pendingBridgeTimes.push(performance.now());
    return;
  }

  if (phase === 'up' || phase === 'cancel') {
    phoneTouches.set(id, point);
    const wasSingleTap = phase === 'up' && !phoneGesture && phoneTouches.size === 1
      && phoneStroke && phoneStroke.segments.length === 0;
    if (wasSingleTap) {
      const index = strokes.indexOf(phoneStroke);
      if (index >= 0) strokes.splice(index, 1);
      strokeCount = Math.max(0, strokeCount - 1);
      el('strokes').textContent = strokeCount;
      addDot(point);
    } else if (phoneStroke && phase === 'cancel' && phoneStroke.segments.length === 0) {
      const index = strokes.indexOf(phoneStroke);
      if (index >= 0) {
        strokes.splice(index, 1);
        strokeCount = Math.max(0, strokeCount - 1);
        el('strokes').textContent = strokeCount;
        redrawInk();
      }
    } else if (phoneStroke) {
      phoneStroke.tentative = false;
    }
    phoneTouches.delete(id);
    updatePhoneContactCount();
    if (phoneTouches.size === 0) {
      phoneStroke = null;
      phoneGesture = false;
    }
  }
}

function clearPhoneContacts() {
  if (phoneStroke && phoneStroke.segments.length === 0) {
    const index = strokes.indexOf(phoneStroke);
    if (index >= 0) {
      strokes.splice(index, 1);
      strokeCount = Math.max(0, strokeCount - 1);
      el('strokes').textContent = strokeCount;
      redrawInk();
    }
  } else if (phoneStroke) {
    phoneStroke.tentative = false;
  }
  phoneTouches.clear();
  updatePhoneContactCount();
  phoneStroke = null;
  phoneGesture = false;
}

function setEditMode(enabled) {
  if (!enabled) {
    clearPhoneContacts();
    nativeTouches.clear();
    el('native-contact-count').textContent = '0';
  }
  editMode = enabled;
  document.body.classList.toggle('editing', enabled);
  el('mode').textContent = enabled ? 'EDIT (pointer clipped, Ctrl+E to exit)' : 'VIEW (Ctrl+E to enter edit)';
  positionEditBox();
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
    const payload = event.payload;
    if (payload.status) {
      el('native-status').textContent = payload.status;
      return;
    }
    reportCount += payload.reports.length;
    el('raw-count').textContent = reportCount;
    el('raw-sample').textContent = `device ${payload.device} · ${payload.reportSize} bytes · ${payload.reports[0]}`;
    pendingBridgeTimes.push(performance.now());
  });
  window.__TAURI__.event.listen('edit-mode-changed', (event) => setEditMode(Boolean(event.payload)));
  window.__TAURI__.event.listen('phone-input', (event) => {
    const receivedAt = performance.now();
    phoneInput(event.payload);
    pendingBridgeTimes.push(receivedAt);
  });
  window.__TAURI__.event.listen('native-touchpad-input', (event) => {
    const receivedAt = performance.now();
    const contact = event.payload;
    if (contact.phase === 'down' || contact.phase === 'move') nativeTouches.add(Number(contact.id));
    if (contact.phase === 'up' || contact.phase === 'cancel') nativeTouches.delete(Number(contact.id));
    el('native-contact-count').textContent = nativeTouches.size;
    phoneInput(contact);
    pendingInputTimes.push(receivedAt);
  });
  window.__TAURI__.event.listen('phone-input-cleared', clearPhoneContacts);
  window.__TAURI__.event.listen('phone-status', (event) => {
    const status = event.payload;
    el('phone-status').textContent = status.message;
    if (!status.connected) clearPhoneContacts();
  });
  window.__TAURI__.core.invoke('get_input_status').then((status) => {
    el('native-status').textContent = status;
  }).catch((error) => {
    el('native-status').textContent = String(error);
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
  el('native-status').textContent = 'Open this page in the Tauri Windows shell';
}

el('stop-phone').addEventListener('click', async () => {
  try {
    await window.__TAURI__.core.invoke('stop_phone_sharing');
    el('phone-status').textContent = 'Sharing stopped';
    el('phone-qr').hidden = true;
    el('phone-url').hidden = true;
    el('stop-phone').disabled = true;
    clearPhoneContacts();
  } catch (error) {
    el('phone-status').textContent = String(error);
  }
});
