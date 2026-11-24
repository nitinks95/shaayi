# Shaayi Input Platforms Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build one input pipeline for Shaayi on macOS and Windows: trackpad (native) and phone sources feed one shared contact-event format, one gesture and ink rule set, and one edit-mode controller, so the canvas behaves the same whichever source or OS produced the input.

**Architecture:** A platform-neutral Rust crate (`shaayi-input`) holds the contact-event type, the edit-box mapping, the gesture state machine and the pointer-lock state machine, all pure logic and fully unit-tested. Each source is an adapter that emits contact events: a Windows HID adapter (descriptor-driven, already spiked), a macOS NSTouch adapter (to be ported from the Swift spike), and the phone WebSocket adapter (already working). The Tauri shell hosts the canvas and forwards only the shared events to it. The macOS and Windows pointer-lock implementations sit behind one trait.

**Tech Stack:** Rust (stable), Tauri 2, `windows-sys` (Windows HID and raw input), `objc2` / `objc2-app-kit` (macOS NSTouch and CoreGraphics), axum and tokio (phone bridge, existing), plain JavaScript for the canvas (existing `spike/web`).

**Spec:** `PRODUCT.md` section 6.2 and section 17 (items 1–17), and `docs/superpowers/specs/2026-11-23-windows-precision-touchpad-design.md`. The Swift spike in `spike/macos/Sources/ShaayiTouchSpike/main.swift` is the behavioural reference for macOS.

## Global Constraints

- Edit box aspect ratio: 1.6 (width : height), trackpad-shaped; the box is anchored where edit mode starts and clamped inside the canvas.
- Two or more contacts are a gesture, never ink. A second contact within 150 ms of the first removes that session's ink; after 150 ms the accepted ink stays and drawing stops for that session.
- Single unmodified keys are ignored in edit mode; any key with Ctrl/Cmd, Alt/Option or Shift passes through.
- The pointer is restored on every exit path: toggle off, focus loss, window close, app quit, and on startup (clear any stale lock).
- Phone heartbeat: client ping every 1.5 s; server frees a silent link after 4 s; a "busy" response carries `retry: true`.
- Autosave is out of scope here; this plan only produces contact events, ink segments and dots, and hands them to the existing canvas.
- No Windows-only or macOS-only code may leak into the shared crate. Platform code lives in adapter modules behind `#[cfg(target_os = ...)]`.
- Gesture remapping (round-2 item 2) is out of scope; it is a separate plan.

## Review Focus

1. A finger lifts or the trackpad disconnects mid-stroke → ink stops cleanly, no phantom segment joins to the next stroke. (Task 1, Task 4, Task 5)
2. The second finger lands 151–300 ms after the first → the first stroke's early ink stays, and drawing is suppressed for the rest of that session. (Task 1)
3. Wi-Fi drops mid-stroke on the phone → contacts clear and the stroke ends; the phone reconnects with its existing token without a new QR code. (Task 7)
4. The app crashes or is killed while the pointer is frozen → the next launch restores the pointer before any input is handled. (Task 6)
5. The window moves, resizes, or moves to another display while in edit mode → the box stays inside the canvas and the mapping updates; the pointer lock is not reapplied to a stale rect. (Task 2, Task 6)
6. No precision touchpad present (HP laptop) → the native source reports "not available", edit mode still works for phone input, and nothing crashes. (Task 4)

Each line above has its test in the owning task.

---

### Task 1: Shared contact event and gesture state machine

**Files:**
- Create: `shaayi-input/Cargo.toml`
- Create: `shaayi-input/src/lib.rs`
- Create: `shaayi-input/src/event.rs`
- Create: `shaayi-input/src/gesture.rs`
- Test: `shaayi-input/tests/gesture.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub struct ContactEvent { pub source: Source, pub id: u64, pub phase: Phase, pub x: f64, pub y: f64, pub at_ms: f64 }` where `x`, `y` are normalised 0.0–1.0 across the source surface, origin top-left.
  - `pub enum Source { Trackpad, Phone }`, `pub enum Phase { Down, Move, Up, Cancel }`.
  - `pub enum InkOp { Segment { from: (f64, f64), to: (f64, f64) }, Dot { at: (f64, f64) }, Rollback { segments: usize } }` — coordinates are normalised to the edit box, not the surface.
  - `pub struct GestureMachine` with `pub fn new() -> Self`, `pub fn handle(&mut self, e: ContactEvent) -> Vec<InkOp>`, `pub fn reset(&mut self) -> Vec<InkOp>` (ends the session on focus loss or edit-mode exit).

- [ ] **Step 1: Write the failing tests**

Create `shaayi-input/tests/gesture.rs`:

```rust
use shaayi_input::{ContactEvent, GestureMachine, InkOp, Phase, Source};

fn ev(id: u64, phase: Phase, x: f64, y: f64, t: f64) -> ContactEvent {
    ContactEvent { source: Source::Trackpad, id, phase, x, y, at_ms: t }
}

#[test]
fn single_finger_move_emits_segment() {
    let mut g = GestureMachine::new();
    assert!(g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0)).is_empty());
    let ops = g.handle(ev(1, Phase::Move, 0.3, 0.2, 10.0));
    assert!(matches!(ops.as_slice(), [InkOp::Segment { .. }]));
}

#[test]
fn tap_emits_dot() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.5, 0.5, 0.0));
    let ops = g.handle(ev(1, Phase::Up, 0.5, 0.5, 80.0));
    assert!(matches!(ops.as_slice(), [InkOp::Dot { .. }]));
}

#[test]
fn second_finger_within_150ms_rolls_back_session() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0));
    g.handle(ev(1, Phase::Move, 0.3, 0.2, 40.0));
    let ops = g.handle(ev(2, Phase::Down, 0.6, 0.6, 100.0));
    assert!(matches!(ops.as_slice(), [InkOp::Rollback { segments: 1 }]));
}

#[test]
fn second_finger_after_150ms_keeps_ink_and_suppresses_drawing() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0));
    g.handle(ev(1, Phase::Move, 0.3, 0.2, 40.0));
    let ops = g.handle(ev(2, Phase::Down, 0.6, 0.6, 200.0));
    assert!(ops.is_empty());
    assert!(g.handle(ev(1, Phase::Move, 0.4, 0.2, 220.0)).is_empty());
}

#[test]
fn lift_mid_stroke_ends_cleanly_without_joining_next_stroke() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0));
    g.handle(ev(1, Phase::Move, 0.3, 0.2, 10.0));
    g.handle(ev(1, Phase::Cancel, 0.3, 0.2, 12.0));
    let ops = g.handle(ev(3, Phase::Down, 0.9, 0.9, 500.0));
    assert!(ops.is_empty(), "a new Down must not connect to the old stroke");
}

#[test]
fn reset_ends_session_and_clears_state() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0));
    g.reset();
    assert!(g.handle(ev(1, Phase::Move, 0.3, 0.2, 10.0)).is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd shaayi-input && cargo test`
Expected: FAIL — the crate does not compile yet (types and `GestureMachine` missing).

- [ ] **Step 3: Implement `event.rs` and `gesture.rs`**

Define the types listed under Produces, exactly. Implement `GestureMachine` with these rules, in this order:
- A session starts on the first `Down`. Record the session start time and the segment count.
- A `Move` from a single active contact emits one `Segment` from that contact's last point. Ignore it if the session is suppressed.
- An `Up` with no movement since `Down` emits one `Dot` at that point. An `Up` after movement emits nothing more.
- A second active contact: if `at_ms - session_start <= 150`, emit `Rollback { segments: n }` where `n` is the number of segments this session emitted, and suppress the session. Otherwise suppress without rollback.
- `Cancel` ends that contact's stroke without a dot.
- When the last contact lifts, the session closes.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shaayi-input && cargo test`
Expected: all 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add shaayi-input
git commit -m "feat(input): shared contact event and gesture rules"
```

---

### Task 2: Edit-box mapping

**Files:**
- Create: `shaayi-input/src/edit_box.rs`
- Modify: `shaayi-input/src/lib.rs` (add `pub mod edit_box;` and re-exports)
- Test: `shaayi-input/tests/edit_box.rs`

**Interfaces:**
- Consumes: `ContactEvent` (normalised 0–1 surface coordinates) from Task 1.
- Produces:
  - `pub struct EditBox { pub anchor: (f64, f64), pub width: f64 }` in canvas pixels, with `pub const ASPECT: f64 = 1.6;`
  - `impl EditBox { pub fn height(&self) -> f64; pub fn clamp_to(&mut self, canvas_w: f64, canvas_h: f64); pub fn map(&self, x: f64, y: f64) -> (f64, f64); pub fn contains(&self, p: (f64, f64)) -> bool; }`

- [ ] **Step 1: Write the failing tests**

Create `shaayi-input/tests/edit_box.rs`:

```rust
use shaayi_input::edit_box::{EditBox, ASPECT};

#[test]
fn height_follows_aspect() {
    let b = EditBox { anchor: (500.0, 300.0), width: 320.0 };
    assert!((b.height() - 320.0 / ASPECT).abs() < 1e-9);
}

#[test]
fn map_corners_hit_box_corners() {
    let b = EditBox { anchor: (500.0, 300.0), width: 320.0 };
    let (x0, y0) = (b.anchor.0 - 160.0, b.anchor.1 - b.height() / 2.0);
    assert_eq!(b.map(0.0, 0.0), (x0, y0));
    assert_eq!(b.map(1.0, 1.0), (x0 + 320.0, y0 + b.height()));
}

#[test]
fn clamp_keeps_box_on_canvas() {
    let mut b = EditBox { anchor: (5.0, 5.0), width: 320.0 };
    b.clamp_to(1000.0, 700.0);
    assert!(b.anchor.0 - 160.0 >= 0.0 && b.anchor.1 - b.height() / 2.0 >= 0.0);
}

#[test]
fn clamp_after_window_shrinks() {
    let mut b = EditBox { anchor: (900.0, 600.0), width: 320.0 };
    b.clamp_to(400.0, 300.0);
    assert!(b.anchor.0 + 160.0 <= 400.0 && b.anchor.1 + b.height() / 2.0 <= 300.0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd shaayi-input && cargo test --test edit_box`
Expected: FAIL — module `edit_box` not found.

- [ ] **Step 3: Implement `edit_box.rs`**

Use the `ASPECT` constant. `map` is a linear map from 0–1 to the box rectangle, with y measured downward from the box top. `clamp_to` moves the anchor so the whole box fits inside `[0, canvas_w] × [0, canvas_h]`; if the box is larger than the canvas, shrink `width` first.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shaayi-input && cargo test --test edit_box`
Expected: all 4 PASS.

- [ ] **Step 5: Commit**

```bash
git add shaayi-input
git commit -m "feat(input): edit-box mapping and clamping"
```

---

### Task 3: Pointer-lock state machine

**Files:**
- Create: `shaayi-input/src/pointer_lock.rs`
- Modify: `shaayi-input/src/lib.rs`
- Test: `shaayi-input/tests/pointer_lock.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub trait PointerBackend { fn detach(&mut self) -> Result<(), String>; fn reattach(&mut self) -> Result<(), String>; }` — implemented per platform in Tasks 4 and 5.
  - `pub struct PointerLock<B: PointerBackend> { ... }` with `pub fn new(backend: B) -> Self`, `pub fn enter_edit(&mut self) -> Result<(), String>`, `pub fn exit_edit(&mut self) -> Result<(), String>`, `pub fn is_detached(&self) -> bool`, `pub fn startup_recover(&mut self) -> Result<(), String>`.

- [ ] **Step 1: Write the failing tests**

Create `shaayi-input/tests/pointer_lock.rs` with a `RecordingBackend` that counts `detach`/`reattach` calls and can be told to fail `detach`:

```rust
use shaayi_input::pointer_lock::{PointerBackend, PointerLock};

#[derive(Default)]
struct Fake { detaches: u32, reattaches: u32, fail_detach: bool }
impl PointerBackend for Fake {
    fn detach(&mut self) -> Result<(), String> {
        if self.fail_detach { return Err("denied".into()); }
        self.detaches += 1; Ok(())
    }
    fn reattach(&mut self) -> Result<(), String> { self.reattaches += 1; Ok(()) }
}

#[test]
fn enter_then_exit_detaches_once_and_restores_once() {
    let mut p = PointerLock::new(Fake::default());
    p.enter_edit().unwrap();
    assert!(p.is_detached());
    p.enter_edit().unwrap(); // idempotent
    p.exit_edit().unwrap();
    assert!(!p.is_detached());
}

#[test]
fn failed_detach_leaves_pointer_attached_and_reports_error() {
    let mut p = PointerLock::new(Fake { fail_detach: true, ..Default::default() });
    assert!(p.enter_edit().is_err());
    assert!(!p.is_detached());
}

#[test]
fn startup_recover_always_reattaches() {
    let mut p = PointerLock::new(Fake::default());
    p.startup_recover().unwrap();
    assert!(!p.is_detached());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd shaayi-input && cargo test --test pointer_lock`
Expected: FAIL — module `pointer_lock` not found.

- [ ] **Step 3: Implement `pointer_lock.rs`**

`enter_edit` calls `detach` only when not already detached. `exit_edit` calls `reattach` only when detached. `startup_recover` always calls `reattach`, ignoring a "not detached" state. Track detached state only after a successful call.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shaayi-input && cargo test --test pointer_lock`
Expected: all 3 PASS.

- [ ] **Step 5: Commit**

```bash
git add shaayi-input
git commit -m "feat(input): pointer-lock state machine behind a backend trait"
```

---

### Task 4: Windows adapter (native trackpad)

**Files:**
- Modify: `spike/windows/src/windows_input.rs` (emit `ContactEvent` instead of the spike payload; keep the descriptor decoder)
- Create: `spike/windows/src/pointer_win.rs` (implements `PointerBackend` with `ClipCursor`)
- Modify: `spike/windows/src/main.rs` (wire `GestureMachine`, `EditBox`, `PointerLock`)
- Modify: `spike/windows/Cargo.toml` (path dependency on `shaayi-input`)

**Interfaces:**
- Consumes: `ContactEvent`, `Source::Trackpad`, `GestureMachine`, `EditBox`, `PointerBackend` from Tasks 1–3.
- Produces: Tauri event `ink-ops` carrying `Vec<InkOp>` as JSON; `#[tauri::command] fn native_status() -> String`, returning "Precision Touchpad active" or "No Precision Touchpad found; native trackpad input is off".

- [ ] **Step 1: Write the failing test (pure part)**

Add a unit test in `windows_input.rs` for the decoder's coordinate normalisation, which must produce `x`, `y` in 0–1 with origin top-left (the Windows spike already flips Y; the test fixes the rule):

```rust
#[test]
fn logical_range_normalises_to_unit_square() {
    assert_eq!(normalise(0, 0, 4095, 2499), (0.0, 0.0));
    assert_eq!(normalise(4095, 2499, 4095, 2499), (1.0, 1.0));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd spike/windows && cargo test normalise`
Expected: FAIL — `normalise` not defined.

- [ ] **Step 3: Implement the adapter**

- `fn normalise(x: i32, y: i32, max_x: i32, max_y: i32) -> (f64, f64)` returns `x / max_x` and `(max_y - y) / max_y` (top-left origin, matching the ASUS fix recorded in the docs).
- Map each decoded contact (tip up/down, contact ID, X/Y) to a `ContactEvent` with `source: Trackpad`. Send only through `GestureMachine::handle`, then emit the returned `InkOp`s.
- `PointerBackend` for Windows: `detach` calls `ClipCursor` with a 1-pixel rectangle at the current cursor position; `reattach` calls `ClipCursor(null)`.
- `native_status`: "No Precision Touchpad found; native trackpad input is off" when no HID device with usage page 0x0D, usage 0x05 exists (the check in `has_precision_touchpad`, already added).

- [ ] **Step 4: Run tests to verify they pass, and build on Windows**

Run: `cd spike/windows && cargo test` then `cargo build --locked`
Expected: test PASS; build succeeds on a Windows machine. (Cannot be built on macOS: the Windows resource compiler is needed.)

- [ ] **Step 5: Manual check on a Precision Touchpad laptop**

Run `cargo run`. Expected: status reads "Precision Touchpad active"; one-finger draws in edit mode; Ctrl+E freezes the pointer. Record results in `WINDOWS_TODO.md`.

- [ ] **Step 6: Commit**

```bash
git add spike/windows
git commit -m "feat(windows): native trackpad adapter emits shared contact events"
```

---

### Task 5: macOS adapter (native trackpad), with a feasibility spike first

**Files:**
- Create: `spike/macos-tauri/` (feasibility spike; a Tauri window with an overlay `NSView` that receives `NSTouch` events)
- Create (after the spike passes): `spike/windows/src/...` is not touched; new files under the Tauri app: `src/macos_input.rs`, `src/pointer_mac.rs`

**Interfaces:**
- Consumes: `ContactEvent`, `Source::Trackpad`, `GestureMachine`, `EditBox`, `PointerBackend` from Tasks 1–3.
- Produces: the same Tauri `ink-ops` event and `native_status` command as Task 4; the macOS `PointerBackend` uses `CGAssociateMouseAndMouseCursorPosition(0/1)`.

- [ ] **Step 1: Feasibility spike — can an overlay view receive NSTouch inside a Tauri webview window?**

Create a minimal Tauri window whose content view has a transparent `NSView` subclass added on top of the webview, with `allowedTouchTypes = [.indirect]`. Log `touchesBegan/Moved/Ended` counts and normalised positions.
Expected: touches arrive while the window is key. **If they don't arrive, stop here** and record the failure in `PRODUCT.md`. The fallback is a Swift helper process that reads NSTouch and sends JSON over a local socket; that choice needs a separate decision.

- [ ] **Step 2: Write the failing test (pure part)**

Unit-test the macOS normalisation: `normalizedPosition` is already 0–1 with origin bottom-left, so the adapter flips Y:

```rust
#[test]
fn normalised_touch_flips_to_top_left() {
    assert_eq!(to_top_left(0.25, 0.1), (0.25, 0.9));
}
```

- [ ] **Step 3: Run test to verify it fails, implement, and run again**

Run: `cargo test to_top_left` → FAIL, then implement `fn to_top_left(x: f64, y: f64) -> (f64, f64) { (x, 1.0 - y) }`, then re-run → PASS.

- [ ] **Step 4: Wire the adapter**

Feed `NSTouch` contacts (Down/Move/Up/Cancel per `identity`) through `GestureMachine`, emit `ink-ops`. `PointerBackend` for macOS calls `CGAssociateMouseAndMouseCursorPosition(0)` on detach and `(1)` on reattach; `startup_recover` runs `(1)` before the first input.

- [ ] **Step 5: Manual check**

Run the app on the Mac. Expected: one-finger draws in the orange box in edit mode; Cmd+E freezes the pointer; force-quit leaves the pointer usable (matches the Swift spike result in `PRODUCT.md` item 11).

- [ ] **Step 6: Commit**

```bash
git add spike/windows
git commit -m "feat(macos): native trackpad adapter via NSTouch"
```

---

### Task 6: Canvas consumes only shared events, on both OSes

**Files:**
- Modify: `spike/web/spike.js` (one entry point `applyInkOp(op)`; remove separate code paths for raw reports, phone contacts, and native contacts)
- Modify: `spike/web/index.html` (HUD line "Source: Trackpad | Phone")
- Test: manual; the JavaScript has no test harness in this spike.

**Interfaces:**
- Consumes: Tauri events `ink-ops` (from Tasks 4–5) and phone-derived `ink-ops` (from Task 7). Each carries `InkOp` objects: `{type:"segment",from:[x,y],to:[x,y]}`, `{type:"dot",at:[x,y]}`, `{type:"rollback",segments:n}`.
- Produces: `applyInkOp(op)`, which draws into the canvas and keeps a segment list so `rollback` can remove the last `n` segments of the current session.

- [ ] **Step 1: Confirm the current paths (no code change yet)**

List every function in `spike/web/spike.js` that draws from contacts. Expected: `phoneInput`, the native contact listener and the pointer fallback.

- [ ] **Step 2: Replace them with `applyInkOp`**

Route every source through `applyInkOp`. Keep the mouse fallback only when the native source reports "No Precision Touchpad found".

- [ ] **Step 3: Manual check on both OSes**

Expected: the same stroke rendered from trackpad and phone looks identical; a two-finger rollback removes the same ink on both OSes.

- [ ] **Step 4: Commit**

```bash
git add spike/web
git commit -m "refactor(canvas): single ink-op entry point for all sources"
```

---

### Task 7: Phone adapter emits shared events, on either desktop

**Files:**
- Modify: `spike/windows/src/phone_trackpad.rs` (emit `ContactEvent { source: Phone, … }` through `GestureMachine`; the heartbeat and timeouts stay as they are)
- Test: `spike/windows/tests/phone_timeouts.rs` (new), testing a pure function `link_state(last_seen_ms, now_ms) -> LinkState`.

**Interfaces:**
- Consumes: `ContactEvent`, `Source::Phone`, `GestureMachine` (Task 1).
- Produces: `pub enum LinkState { Alive, Dead }` and `pub fn link_state(last_seen_ms: f64, now_ms: f64) -> LinkState` returning `Dead` when `now_ms - last_seen_ms > 4000.0`; `ink-ops` events as in Task 6.

- [ ] **Step 1: Write the failing test**

```rust
use shaayi_input::phone::{link_state, LinkState};

#[test]
fn silent_for_four_seconds_is_dead() {
    assert_eq!(link_state(0.0, 4001.0), LinkState::Dead);
    assert_eq!(link_state(0.0, 3999.0), LinkState::Alive);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test phone_timeouts` → FAIL (module missing).

- [ ] **Step 3: Implement and wire**

Add `shaayi-input/src/phone.rs` with `link_state`. In `phone_trackpad.rs`, replace the inline 4-second timeout with `link_state`, and route phone contacts through `GestureMachine` with `source: Source::Phone`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test` → PASS. Then manual: scan QR from a phone on both a Mac and a Windows desktop; reconnect after a Wi-Fi drop.

- [ ] **Step 5: Commit**

```bash
git add shaayi-input spike/windows
git commit -m "feat(phone): phone adapter emits shared contact events"
```

---

### Task 8: First launch and permissions

**Files:**
- Create: `spike/web/first-launch.html` (terms and conditions; checklist)
- Create: `spike/web/first-launch.js`
- Modify: `spike/windows/src/main.rs` (show the first-launch page once; store acceptance in the app config directory)

**Interfaces:**
- Consumes: nothing from the input crates.
- Produces: `#[tauri::command] fn accept_terms()` and `#[tauri::command] fn has_accepted_terms() -> bool`.

- [ ] **Step 1: Write the failing test**

Unit-test that acceptance persists: `accept_terms` writes a marker file; `has_accepted_terms` reads it. Use a temp directory.

- [ ] **Step 2: Run test to verify it fails**, then implement, then run to verify it passes.

- [ ] **Step 3: Permissions checklist**

macOS: none required for the current design (NSTouch and CGAssociate need no permission in the spike; verify on a clean user account). Windows: none required for Raw Input. The page explains this and links to the input settings only if a future source needs it.

- [ ] **Step 4: Commit**

```bash
git add spike/web spike/windows
git commit -m "feat(app): first-launch terms and permissions page"
```

---

### Task 9: Cross-platform verification

**Files:**
- Modify: `docs/superpowers/plans/2026-11-24-input-platforms.md` (tick results)
- Modify: `PRODUCT.md` section 17 (record the matrix)

**Interfaces:** none.

- [ ] **Step 1: Run the matrix**

| Check | macOS | Windows (Precision Touchpad) | Windows (no touchpad, e.g. HP) |
|---|---|---|---|
| One-finger ink in edit mode | | | n/a |
| Two-finger rollback within 150 ms | | | n/a |
| Pointer freezes in edit mode | | | n/a |
| Force-quit leaves pointer usable | | | n/a |
| Phone draws and taps | | | |
| Phone reconnects after Wi-Fi drop | | | |
| Box stays on canvas after window move | | | |
| "No Precision Touchpad" status shown | n/a | n/a | |

- [ ] **Step 2: Record results and commit**

```bash
git add PRODUCT.md docs/superpowers/plans
git commit -m "docs: record cross-platform input verification"
```

---

## Out of scope for this plan

- Gesture remapping and per-OS defaults (round-2 item 2): separate plan.
- Autosave and note storage (M1).
- Ink-to-text (v3) and cloud cleanup (v2).
- A second Windows precision touchpad: to be run when one is available.
