//! Temporary phone-as-trackpad bridge for the Windows interaction spike.

use std::{
    net::{IpAddr, SocketAddr, TcpListener},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use qrcode::{render::svg, QrCode};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use shaayi_input::{
    phone::{link_state, LinkState},
    ContactEvent, EditBox, GestureMachine, InkOp, Phase, Source,
};

#[cfg(target_os = "macos")]
use crate::macos_input as native;
#[cfg(target_os = "windows")]
use crate::windows_input as native;

static PHONE_GESTURES: std::sync::OnceLock<std::sync::Mutex<GestureMachine>> = std::sync::OnceLock::new();

fn phone_gestures() -> &'static std::sync::Mutex<GestureMachine> {
    PHONE_GESTURES.get_or_init(|| std::sync::Mutex::new(GestureMachine::new()))
}

/// Ends any open phone gesture session. Called when edit mode turns off or the phone disconnects.
pub fn reset_phone_gestures() {
    if let Ok(mut g) = phone_gestures().lock() { let _ = g.reset(); }
}

const PHONE_PAGE: &str = include_str!("../../web/phone.html");
const PHONE_SCRIPT: &str = include_str!("../../web/phone.js");
const MAX_SOCKET_MESSAGE_BYTES: usize = 1024;

#[derive(Clone)]
pub struct PhoneTrackpadServer {
    inner: Arc<ServerState>,
}

struct ServerState {
    app: AppHandle,
    address: IpAddr,
    port: u16,
    token: String,
    status: std::sync::Mutex<String>,
    stopped: AtomicBool,
    connected: AtomicBool,
    shutdown: watch::Sender<bool>,
    disconnect: broadcast::Sender<()>,
    started: Instant,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingInfo {
    pub url: String,
    pub qr_svg: String,
    pub status: String,
    pub connected: bool,
    pub stopped: bool,
}

#[derive(Debug, Deserialize)]
struct ClientMessage {
    #[serde(rename = "type")]
    message_type: String,
    token: Option<String>,
    phase: Option<String>,
    id: Option<u64>,
    x: Option<f64>,
    y: Option<f64>,
}

impl PhoneTrackpadServer {
    pub fn start(app: AppHandle) -> Result<Self, std::io::Error> {
        let address = local_ip_address::local_ip()
            .map_err(std::io::Error::other)?;
        if !address.is_ipv4() {
            return Err(std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "Phone pairing needs an IPv4 address on the local Wi-Fi."));
        }

        let listener = TcpListener::bind(SocketAddr::new(address, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();

        let (shutdown, shutdown_rx) = watch::channel(false);
        let (disconnect, _) = broadcast::channel(2);
        let inner = Arc::new(ServerState {
            app,
            address,
            port,
            token: Uuid::new_v4().to_string(),
            status: std::sync::Mutex::new("Starting local pairing".into()),
            stopped: AtomicBool::new(false),
            connected: AtomicBool::new(false),
            shutdown,
            disconnect,
            started: Instant::now(),
        });

        let routes = Router::new()
            .route("/touchpad", get(phone_page))
            .route("/phone.js", get(phone_script))
            .route("/ws", get(websocket_upgrade))
            .with_state(Arc::clone(&inner));
        let mut shutdown_rx = shutdown_rx;
        let task_state = Arc::clone(&inner);
        tauri::async_runtime::spawn(async move {
            let listener = match tokio::net::TcpListener::from_std(listener) {
                Ok(listener) => listener,
                Err(error) => {
                    emit_status(&task_state, false, &format!("Could not start phone listener: {error}"));
                    return;
                }
            };
            emit_status(&task_state, false, "Ready to pair");
            let _ = axum::serve(listener, routes)
                .with_graceful_shutdown(async move {
                    while !*shutdown_rx.borrow_and_update() {
                        if shutdown_rx.changed().await.is_err() { break; }
                    }
                })
                .await;
        });

        Ok(Self { inner })
    }

    pub fn pairing_info(&self) -> Result<PairingInfo, String> {
        let url = format!("http://{}:{}/touchpad#token={}", self.inner.address, self.inner.port, self.inner.token);
        let code = QrCode::new(url.as_bytes()).map_err(|error| format!("QR encoding failed: {error}"))?;
        let qr_svg = code.render::<svg::Color>()
            .min_dimensions(240, 240)
            .dark_color(svg::Color("#161616"))
            .light_color(svg::Color("#ffffff"))
            .build();

        Ok(PairingInfo {
            url,
            qr_svg,
            status: self.inner.status.lock().map(|status| status.clone()).unwrap_or_else(|_| "Pairing status unavailable".into()),
            connected: self.inner.connected.load(Ordering::SeqCst),
            stopped: self.inner.stopped.load(Ordering::SeqCst),
        })
    }

    pub fn disconnect_phone(&self) {
        let _ = self.inner.disconnect.send(());
    }

    pub fn stop(&self) {
        if self.inner.stopped.swap(true, Ordering::SeqCst) { return; }
        let _ = self.inner.disconnect.send(());
        let _ = self.inner.shutdown.send(true);
        emit_status(&self.inner, false, "Sharing stopped");
    }
}

async fn phone_page(State(state): State<Arc<ServerState>>) -> Response {
    if state.stopped.load(Ordering::SeqCst) {
        return (StatusCode::GONE, "Phone sharing has stopped. Restart the Windows spike to pair again.").into_response();
    }
    Html(PHONE_PAGE).into_response()
}

async fn phone_script(State(state): State<Arc<ServerState>>) -> Response {
    if state.stopped.load(Ordering::SeqCst) {
        return StatusCode::GONE.into_response();
    }
    ([ (axum::http::header::CONTENT_TYPE, "text/javascript; charset=utf-8") ], PHONE_SCRIPT).into_response()
}

async fn websocket_upgrade(
    State(state): State<Arc<ServerState>>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if state.stopped.load(Ordering::SeqCst) {
        return StatusCode::GONE.into_response();
    }
    upgrade
        .max_message_size(MAX_SOCKET_MESSAGE_BYTES)
        .max_frame_size(MAX_SOCKET_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<ServerState>) {
    let mut disconnect = state.disconnect.subscribe();
    let first = tokio::select! {
        message = tokio::time::timeout(Duration::from_secs(6), socket.recv()) => message,
        _ = disconnect.recv() => {
            if state.stopped.load(Ordering::SeqCst) {
                let _ = socket.send(Message::Text(r#"{"type":"error","message":"Phone sharing stopped"}"#.into())).await;
            }
            let _ = socket.send(Message::Close(None)).await;
            return;
        },
    };
    let Ok(Some(Ok(Message::Text(text)))) = first else {
        let _ = socket.send(Message::Close(None)).await;
        return;
    };
    let Ok(auth) = serde_json::from_str::<ClientMessage>(text.as_str()) else {
        let _ = socket.send(Message::Text(r#"{"type":"error","message":"Invalid pairing message"}"#.into())).await;
        let _ = socket.send(Message::Close(None)).await;
        return;
    };
    if auth.message_type != "auth" || auth.token.as_deref() != Some(state.token.as_str()) {
        let _ = socket.send(Message::Text(r#"{"type":"error","message":"Pairing token is invalid"}"#.into())).await;
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    if state.stopped.load(Ordering::SeqCst) {
        let _ = socket.send(Message::Text(r#"{"type":"error","message":"Phone sharing stopped"}"#.into())).await;
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    if state.connected.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        // Often a stale slot from a dropped Wi-Fi link that has not timed out yet. Retry, don't stop.
        let _ = socket.send(Message::Text(r#"{"type":"error","message":"Desktop is still releasing the last phone; retrying","retry":true}"#.into())).await;
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    let _connection = PhoneConnection::new(Arc::clone(&state));
    if socket.send(Message::Text(r#"{"type":"connected"}"#.into())).await.is_err() {
        return;
    }
    emit_status(&state, true, "Phone connected");

    let mut last_seen_ms = state.now_ms();
    loop {
        tokio::select! {
            incoming = tokio::time::timeout(Duration::from_millis(500), socket.recv()) => {
                // No frame for 4 s (phone heartbeat is every 1.5 s): treat the link as dead and free the slot.
                let Ok(incoming) = incoming else {
                    if link_state(last_seen_ms, state.now_ms()) == LinkState::Dead { break; }
                    continue;
                };
                last_seen_ms = state.now_ms();
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if text.as_str() == r#"{"type":"ping"}"# { continue; }
                        let Ok(message) = serde_json::from_str::<ClientMessage>(text.as_str()) else {
                            let _ = socket.send(Message::Close(None)).await;
                            break;
                        };
                        if !forward_pointer(&state, message) {
                            let _ = socket.send(Message::Close(None)).await;
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() { break; }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Binary(_))) => {
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
            _ = disconnect.recv() => {
                if state.stopped.load(Ordering::SeqCst) {
                    let _ = socket.send(Message::Text(r#"{"type":"error","message":"Phone sharing stopped"}"#.into())).await;
                } else {
                    let _ = socket.send(Message::Text(r#"{"type":"error","message":"Desktop lost focus; reconnecting","retry":true}"#.into())).await;
                }
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
        }
    }
}

fn forward_pointer(state: &ServerState, message: ClientMessage) -> bool {
    if message.message_type != "pointer" { return false; }
    let (Some(phase), Some(id), Some(x), Some(y)) = (message.phase, message.id, message.x, message.y) else {
        return false;
    };
    if !x.is_finite() || !y.is_finite() || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return false;
    }
    if !matches!(phase.as_str(), "down" | "move" | "up" | "cancel") { return false; }
    let _ = state.app.emit("phone-input", serde_json::json!({ "phase": phase, "id": id, "x": x, "y": y }));
    route_through_gestures(state, &phase, id, x, y);
    true
}

impl ServerState {
    fn now_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }
}

/// Phone contacts become ink the same way trackpad contacts do: through `GestureMachine`.
fn route_through_gestures(state: &ServerState, phase: &str, id: u64, x: f64, y: f64) {
    // Ink only draws in edit mode, for every source.
    if !edit_mode_on() {
        reset_phone_gestures();
        return;
    }
    let phase = match phase {
        "down" => Phase::Down,
        "move" => Phase::Move,
        "up" => Phase::Up,
        _ => Phase::Cancel,
    };
    let event = ContactEvent { source: Source::Phone, id, phase, x, y, at_ms: state.now_ms() };
    let ops = match phone_gestures().lock() {
        Ok(mut g) => g.handle(event),
        Err(_) => return,
    };
    emit_ink_ops(state, &ops);
}

fn emit_ink_ops(state: &ServerState, ops: &[InkOp]) {
    if ops.is_empty() { return; }
    let Some(edit) = current_edit_box() else { return; };
    let json: Vec<serde_json::Value> = ops.iter().map(|op| native_ink_op_json(op, &edit)).collect();
    let _ = state.app.emit("ink-ops", json);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn edit_mode_on() -> bool { native::edit_mode_on() }
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn edit_mode_on() -> bool { false }

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn current_edit_box() -> Option<EditBox> { native::current_edit_box() }
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn current_edit_box() -> Option<EditBox> { None }

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn native_ink_op_json(op: &InkOp, edit: &EditBox) -> serde_json::Value { native::ink_op_json(op, edit) }
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_ink_op_json(_op: &InkOp, _edit: &EditBox) -> serde_json::Value { serde_json::Value::Null }

struct PhoneConnection(Arc<ServerState>);

impl PhoneConnection {
    fn new(state: Arc<ServerState>) -> Self { Self(state) }
}

impl Drop for PhoneConnection {
    fn drop(&mut self) {
        self.0.connected.store(false, Ordering::SeqCst);
        reset_phone_gestures();
        let message = if self.0.stopped.load(Ordering::SeqCst) { "Sharing stopped" } else { "Phone disconnected" };
        emit_status(&self.0, false, message);
        let _ = self.0.app.emit("phone-input-cleared", ());
    }
}

fn emit_status(state: &ServerState, connected: bool, message: &str) {
    if let Ok(mut current) = state.status.lock() {
        *current = message.to_owned();
    }
    let _ = state.app.emit("phone-status", serde_json::json!({ "connected": connected, "message": message }));
}

