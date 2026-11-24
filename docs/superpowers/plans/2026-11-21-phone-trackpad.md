# Phone Trackpad Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let one phone on the same Wi-Fi act as a temporary trackpad for the Windows Tauri spike, paired by a QR code shown on the desktop.

**Architecture:** Add a temporary Axum HTTP/WebSocket service to the Tauri process, bound to the selected local IPv4 interface. A random per-run token in the QR fragment pairs the phone page; the page sends normalized pointer contacts over WebSocket, and Tauri forwards validated events into the existing canvas. Desktop code applies edit-box mapping, ink, tap dots, and the macOS 150 ms multi-finger rollback rule.

**Tech Stack:** Rust 2021, Tauri 2, Axum 0.8.9 (`ws` feature), Tokio, `local-ip-address` 0.6.13, UUID v4, `qrcode` 0.14.1 SVG renderer, existing HTML/JavaScript canvas. Axum 0.8.9 requires Rust 1.80 or newer.

**Spec:** `docs/superpowers/specs/2026-11-21-phone-trackpad-design.md`

## Global Constraints

- The phone and PC use the same local Wi-Fi network.
- The service is temporary and only runs while the spike is open.
- Pairing tokens are cryptographically random and scoped to a single app run; reject missing or invalid tokens.
- Serve only the touch page and the input socket. Do not expose file, shell, or general-purpose RPC endpoints.
- The phone and PC must be able to reach each other on the local network; guest Wi-Fi isolation may prevent this.
- This is an interaction prototype; it does not emulate the Windows Precision Touchpad HID device or validate its native input driver.
- Support one phone drawing at a time.
- Treat two or more simultaneous fingers as gestures; discard first-finger ink if another finger lands within 150 ms.
- The phone-event-to-next-frame timing is separate from physical touch-to-ink latency.

## Review Focus

- Missing, invalid, or stale pairing token: reject the WebSocket before accepting input. Manual check in Task 1.
- A second phone connects while one is paired: reject the second session and retain the first. Manual check in Task 1.
- Coordinates are malformed, non-finite, or outside 0…1: ignore the input and close malformed sessions. Manual check in Task 1.
- A second finger lands within or after 150 ms: remove early ink only in the within-150-ms case; never draw while two contacts are active. Manual checks in Task 3.
- Phone disconnects or desktop loses focus mid-stroke: finish/cancel the active stroke, clear contact state, turn off edit mode, and release cursor clipping. Manual checks in Tasks 3 and 4.

## File Structure

- `spike/tauri/src/phone_trackpad.rs` — LAN interface selection, token/session state, Axum routes, WebSocket validation, pairing info, and service shutdown.
- `spike/tauri/src/main.rs` — manage the phone service, expose pairing/status/stop commands, forward phone events, and stop the service on app exit.
- `spike/tauri/Cargo.toml`, `spike/tauri/Cargo.lock` — Axum, Tokio, QR, IP-address, UUID, and serialization dependencies.
- `spike/web/phone.html`, `spike/web/phone.js` — phone touch surface, pairing handshake, normalized contact messages, connection state.
- `spike/web/index.html` — desktop QR, address, connection state, and stop-sharing controls.
- `spike/web/spike.js` — render the QR, consume phone events, map phone coordinates to the edit box, retain/redraw strokes for rollback, and report bridge-to-frame timing.
- `WINDOWS_TODO.md` — record the phone-pad path and keep native precision-touchpad work clearly outstanding.

## Tasks

### Task 1: Local pairing and WebSocket service

**Files:**
- Create: `spike/tauri/src/phone_trackpad.rs`
- Modify: `spike/tauri/Cargo.toml`
- Modify: `spike/tauri/Cargo.lock`

**Interfaces:**
- Produces `PhoneTrackpadServer::start(AppHandle) -> Result<PhoneTrackpadServer, String>`.
- Produces `PhoneTrackpadServer::pairing_info() -> PairingInfo`, with URL, SVG QR, and connected state.
- Produces `PhoneTrackpadServer::stop()` to disable routes, close active sockets, and stop the listener.
- The phone sends an initial JSON auth message `{ "type": "auth", "token": "…" }`, followed by `{ "type": "pointer", "phase": "down|move|up|cancel", "id": u64, "x": f64, "y": f64 }` where x/y are normalized to 0…1.
- The service emits accepted pointer messages as `phone-input` events and connection changes as `phone-status` events.

- [x] Add the dependency families to `Cargo.toml`: Axum 0.8 with `http1`, `tokio`, and `ws`; Tokio `net`, `sync`, and `time`; `local-ip-address` 0.6; `uuid` with `v4`; `qrcode` 0.14 with `default-features = false` and `svg`; and Serde derive. Keep `serde_json` as the wire format.
- [ ] Regenerate `Cargo.lock` after Cargo becomes available.
- [x] Implement LAN address selection, bind Axum to that local IPv4 on an ephemeral port, and create a per-run UUID v4 token.
- [x] Serve the pairing page/script and `/ws`. Put the token in the QR URL fragment; require it in the first WebSocket message and acknowledge only a valid token.
- [x] Allow one active phone, validate pointer phases/IDs/finite coordinates in 0…1, and forward typed input/status events to Tauri.
- [x] Render the pairing URL as an SVG QR using `qrcode`; expose the URL as readable text for manual entry.
- [x] Implement `stop()` so the HTTP listener and active socket end cleanly.
- [ ] Manual check: the pairing URL uses the selected local IPv4 and the QR decodes to that URL; missing/incorrect tokens fail; a second phone is refused; stop closes the socket and listener.

### Task 2: Phone touch page

**Files:**
- Create: `spike/web/phone.html`
- Create: `spike/web/phone.js`
- Modify: `spike/tauri/src/phone_trackpad.rs`

**Interfaces:**
- Consumes the `/touchpad` page and `/ws` protocol from Task 1.
- Emits `auth`, then pointer `down`, `move`, `up`, and `cancel` messages with a stable browser `pointerId` and coordinates normalized to the 16:10 pad surface.

- [x] Build a full-screen, touch-action-disabled 16:10 pad with a connection indicator and a visible message for failed pairing.
- [x] Read the token from the URL fragment, connect to the same host/port via WebSocket, send auth first, and show “Connected” only after the server acknowledges it.
- [x] Capture touch pointer events with pointer capture and send normalized coordinates; send cancel/end events on pointer cancellation, page hide, socket close, and navigation away.
- [ ] Manual check: a valid QR opens the page and shows connected; rotation/resizing keeps coordinates within 0…1; reload and Wi-Fi loss show a disconnected state and clear active contacts.

### Task 3: Desktop ink mapping and gesture behavior

**Files:**
- Modify: `spike/web/spike.js`

**Interfaces:**
- Consumes `phone-input` events with `phase`, `id`, `x`, and `y` from Task 1.
- Uses the current edit-box rectangle and existing canvas drawing context.
- Emits existing HUD metrics for phone-event-to-next-frame timing.

- [x] Add a retained ink operation list and redraw helper so a tentative stroke can be removed without erasing earlier ink; route normal browser-pointer and phone strokes through that renderer.
- [x] Map normalized phone coordinates into the clamped edit box and draw one-finger movement only while desktop edit mode is active.
- [x] Add tap-dot handling when a contact ends without a movement stroke.
- [x] Track active phone contact IDs. When a second contact begins, suspend ink immediately; remove the tentative first-contact stroke if the second begins within 150 ms; preserve already accepted ink if it begins later.
- [x] On phone cancel/disconnect or edit-mode exit, clear active contacts and finalize/cancel the current tentative stroke so no later move resumes it.
- [ ] Manual check: one-finger line maps corner-to-corner inside the edit box; taps make dots; second-finger rollback is correct on both sides of 150 ms; two-finger movement creates no ink; disconnected contacts never leave a stuck stroke.

### Task 4: Desktop pairing HUD and lifecycle

**Files:**
- Modify: `spike/tauri/src/main.rs`
- Modify: `spike/web/index.html`
- Modify: `spike/web/spike.js`
- Modify: `spike/tauri/src/windows_input.rs`
- Modify: `WINDOWS_TODO.md`

**Interfaces:**
- Tauri commands: `get_phone_pairing() -> PairingInfo` and `stop_phone_sharing() -> Result<(), String>`.
- Desktop HUD consumes pairing info and `phone-status` events; it sends `stop_phone_sharing` when the user stops sharing.

- [x] Start and manage the phone service during Tauri setup; expose pairing and stop commands and shut down the listener on app exit.
- [x] Add a desktop panel with QR image, readable URL, connection state, and a stop-sharing button. Do not display the pairing token outside the QR/URL.
- [x] On desktop focus loss, stop phone input immediately, clear all phone contacts, turn off edit mode, and release `ClipCursor` through the existing Windows input module.
- [x] Update `WINDOWS_TODO.md` to distinguish phone interaction checks from pending Windows Precision Touchpad HID checks.
- [ ] Manual check: launch displays QR without internet; scan/connect on same Wi-Fi; stop sharing ends the session; focus loss releases pointer clipping; app exit closes the port.

## Self-review

- Spec coverage: local page/socket, QR with run token, one-phone normalized contacts, status, ink/taps, 150 ms gesture rollback, stop/disconnect/focus handling, offline behavior, and the HID limitation are assigned to Tasks 1–4.
- Step scan: each implementation step changes one component or behavior, with an explicit manual check at each deliverable boundary.
- Type consistency: Task 1 defines `PairingInfo` and pointer message shape; Tasks 2–4 consume those names and fields.
- Review focus: all five input classes are assigned manual checks in the owning tasks.
- Proportion: four tasks cover the Rust service, mobile page, desktop input, and lifecycle/UI without expanding the approved spike scope.
