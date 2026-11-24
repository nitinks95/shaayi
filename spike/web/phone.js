const pad = document.getElementById('pad');
const statusText = document.getElementById('status');
const token = new URLSearchParams(window.location.hash.slice(1)).get('token');
const activePointers = new Map();
let socket = null;
let reconnectTimer = null;
let heartbeatTimer = null;
let intentionalClose = false;
let terminalFailure = false;

function setStatus(message, connected = false) {
  statusText.textContent = message;
  document.body.classList.toggle('connected', connected);
}

function send(message) {
  if (socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
}

function pointInPad(event) {
  const rect = pad.getBoundingClientRect();
  const x = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
  const y = Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height));
  return { type: 'pointer', id: event.pointerId, x, y };
}

function endAllPointers(phase = 'cancel') {
  for (const [id, last] of activePointers) send({ type: 'pointer', phase, id, x: last.x, y: last.y });
  activePointers.clear();
}

function connect() {
  if (!token) {
    setStatus('This QR code is missing its pairing token. Scan the current code again.');
    return;
  }
  if (intentionalClose) return;
  terminalFailure = false;
  const url = `ws://${window.location.host}/ws`;
  socket = new WebSocket(url);
  setStatus('Connecting to your PC…');

  socket.addEventListener('open', () => {
    send({ type: 'auth', token });
    // Heartbeat so the desktop can tell a dead Wi-Fi link from an idle phone.
    clearInterval(heartbeatTimer);
    heartbeatTimer = window.setInterval(() => send({ type: 'ping' }), 1500);
  });
  socket.addEventListener('message', (event) => {
    let message;
    try { message = JSON.parse(event.data); } catch { return; }
    if (message.type === 'connected') setStatus('Connected · move one finger to write', true);
    else if (message.type === 'error') {
      terminalFailure = message.retry !== true;
      setStatus(message.message || 'Pairing failed');
    }
  });
  socket.addEventListener('close', () => {
    clearInterval(heartbeatTimer);
    endAllPointers('cancel');
    if (terminalFailure) return;
    setStatus('Disconnected · reconnecting…');
    if (!intentionalClose) reconnectTimer = window.setTimeout(connect, 1000);
  });
  socket.addEventListener('error', () => setStatus('Connection failed · check that both devices share Wi-Fi'));
}

pad.addEventListener('pointerdown', (event) => {
  event.preventDefault();
  pad.setPointerCapture(event.pointerId);
  const point = pointInPad(event);
  point.phase = 'down';
  activePointers.set(event.pointerId, point);
  send(point);
});

pad.addEventListener('pointermove', (event) => {
  if (!activePointers.has(event.pointerId)) return;
  event.preventDefault();
  const point = pointInPad(event);
  point.phase = 'move';
  activePointers.set(event.pointerId, point);
  send(point);
});

function releasePointer(event, phase) {
  if (!activePointers.has(event.pointerId)) return;
  event.preventDefault();
  const point = pointInPad(event);
  point.phase = phase;
  activePointers.delete(event.pointerId);
  send(point);
}
pad.addEventListener('pointerup', (event) => releasePointer(event, 'up'));
pad.addEventListener('pointercancel', (event) => releasePointer(event, 'cancel'));
pad.addEventListener('lostpointercapture', (event) => releasePointer(event, 'cancel'));

window.addEventListener('pagehide', () => {
  endAllPointers('cancel');
  intentionalClose = true;
  if (reconnectTimer) clearTimeout(reconnectTimer);
  if (socket) socket.close();
});

connect();
