//! macOS trackpad adapter. A transparent `NSView` overlay on top of the webview
//! receives indirect `NSTouch` events (no permission needed while the window is
//! key), converts them to shared `ContactEvent`s (source: Trackpad), runs them
//! through `GestureMachine`, and emits the resulting `InkOp`s as `ink-ops`.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use objc2::rc::Retained;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSEvent, NSResponder, NSTouchPhase, NSTouchTypeMask, NSView, NSWindow,
};
use objc2_foundation::MainThreadMarker;
use shaayi_input::normalise::to_top_left;
use shaayi_input::{
    pointer_lock::PointerLock, ContactEvent, EditBox, GestureMachine, InkOp, Phase, Source,
};
use tauri::{AppHandle, Emitter};

use crate::pointer_mac::AssociateBackend;

static APP: OnceLock<AppHandle> = OnceLock::new();
static POINTER: OnceLock<Mutex<PointerLock<AssociateBackend>>> = OnceLock::new();
static GESTURES: OnceLock<Mutex<GestureMachine>> = OnceLock::new();
static EDIT_BOX: OnceLock<Mutex<EditBox>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();
static OVERLAY: Mutex<Option<usize>> = Mutex::new(None); // raw pointer to the overlay, main thread only

fn pointer() -> &'static Mutex<PointerLock<AssociateBackend>> {
    POINTER.get_or_init(|| Mutex::new(PointerLock::new(AssociateBackend)))
}
fn gestures() -> &'static Mutex<GestureMachine> {
    GESTURES.get_or_init(|| Mutex::new(GestureMachine::new()))
}
fn edit_box() -> &'static Mutex<EditBox> {
    EDIT_BOX.get_or_init(|| Mutex::new(EditBox { anchor: (0.0, 0.0), width: 0.0 }))
}

pub struct Ivars {
    /// Responder that had focus before edit mode; keys are forwarded to it.
    previous: Mutex<Option<Retained<NSResponder>>>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "ShaayiTouchOverlay"]
    #[ivars = Ivars]
    pub struct TouchOverlay;

    impl TouchOverlay {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool { true }

        // Claim hits only while editing, so the webview keeps normal mouse input otherwise.
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: objc2_foundation::NSPoint) -> Option<Retained<NSView>> {
            if pointer().lock().map(|p| p.is_detached()).unwrap_or(false) {
                let _ = point;
                Some(unsafe { Retained::retain(self as *const Self as *mut NSView).unwrap() })
            } else {
                None
            }
        }

        #[unsafe(method(touchesBeganWithEvent:))]
        fn began(&self, e: &NSEvent) { self.forward(e, NSTouchPhase::Began, Phase::Down); }
        #[unsafe(method(touchesMovedWithEvent:))]
        fn moved(&self, e: &NSEvent) { self.forward(e, NSTouchPhase::Moved, Phase::Move); }
        #[unsafe(method(touchesEndedWithEvent:))]
        fn ended(&self, e: &NSEvent) { self.forward(e, NSTouchPhase::Ended, Phase::Up); }
        #[unsafe(method(touchesCancelledWithEvent:))]
        fn cancelled(&self, e: &NSEvent) { self.forward(e, NSTouchPhase::Cancelled, Phase::Cancel); }

        // Keep keyboard shortcuts (Cmd+E) reaching the webview while this view is first responder.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, e: &NSEvent) { self.pass_key(e, true); }
        #[unsafe(method(keyUp:))]
        fn key_up(&self, e: &NSEvent) { self.pass_key(e, false); }
        #[unsafe(method(performKeyEquivalent:))]
        fn key_equiv(&self, _e: &NSEvent) -> bool { false }
    }
);

impl TouchOverlay {
    fn forward(&self, e: &NSEvent, ns_phase: NSTouchPhase, phase: Phase) {
        // Contacts only draw while edit mode holds the pointer.
        if !pointer().lock().map(|p| p.is_detached()).unwrap_or(false) {
            return;
        }
        let touches = e.touchesMatchingPhase_inView(ns_phase, Some(self));
        for t in touches.iter() {
            // Skip resting touches for Down/Move, but always forward Ended and
            // Cancelled so a touch that became resting still closes its contact.
            if t.isResting() && matches!(phase, Phase::Down | Phase::Move) { continue; }
            let ident = t.identity();
            let id: usize = unsafe { msg_send![&*ident, hash] };
            let id = id as u64;
            let p = t.normalizedPosition();
            let (x, y) = to_top_left(p.x, p.y);
            handle_contact(id, phase, x, y);
        }
    }

    fn pass_key(&self, e: &NSEvent, down: bool) {
        let prev = self.ivars().previous.lock().ok().and_then(|g| g.clone());
        if let Some(r) = prev {
            if down { r.keyDown(e) } else { r.keyUp(e) }
        }
    }
}

/// The only path from a trackpad contact to ink: through `GestureMachine`.
fn handle_contact(id: u64, phase: Phase, x: f64, y: f64) {
    let at_ms = START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0;
    let event = ContactEvent { source: Source::Trackpad, id, phase, x, y, at_ms };
    let ops = match gestures().lock() {
        Ok(mut g) => g.handle(event),
        Err(_) => return,
    };
    if ops.is_empty() { return; }
    let Ok(edit) = edit_box().lock().map(|b| *b) else { return; };
    // Contacts are dropped until JS has placed the edit box (width 0 = unset).
    let json: Vec<serde_json::Value> = ops.iter().filter_map(|op| ink_op_json(op, &edit)).collect();
    if json.is_empty() { return; }
    if let Some(app) = APP.get() {
        let _ = app.emit("ink-ops", json);
    }
}

/// Map an op into the edit box; `None` while the box is not set (width 0).
pub fn ink_op_json(op: &InkOp, edit: &EditBox) -> Option<serde_json::Value> {
    Some(match edit.map_op(op)? {
        InkOp::Segment { from: f, to: t } => {
            serde_json::json!({ "op": "segment", "from": [f.0, f.1], "to": [t.0, t.1] })
        }
        InkOp::Dot { at: a } => serde_json::json!({ "op": "dot", "at": [a.0, a.1] }),
        InkOp::Rollback { segments } => serde_json::json!({ "op": "rollback", "segments": segments }),
    })
}

/// Install the overlay on the main window and clear any stale pointer detach
/// left by a force-terminated previous run. Must run on the main thread.
pub fn start(app: AppHandle, window: &NSWindow) -> Result<(), String> {
    APP.set(app).map_err(|_| "macOS input already started".to_string())?;
    START.get_or_init(Instant::now);
    pointer().lock().map_err(|_| "Pointer lock is unavailable")?.startup_recover()?;

    let mtm = MainThreadMarker::new().ok_or("macOS input must start on the main thread")?;
    let content = window.contentView().ok_or("window has no content view")?;
    let this = mtm.alloc::<TouchOverlay>().set_ivars(Ivars { previous: Mutex::new(None) });
    let view: Retained<TouchOverlay> = unsafe { msg_send![super(this), initWithFrame: content.bounds()] };
    view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
    view.setAllowedTouchTypes(NSTouchTypeMask::Indirect);
    view.setWantsRestingTouches(true);
    content.addSubview(&view);
    if let Ok(mut o) = OVERLAY.lock() {
        *o = Some(Retained::as_ptr(&view) as usize);
    }
    std::mem::forget(view); // owned by the content view; pointer kept for focus handoff
    Ok(())
}

fn overlay_and_window() -> Option<(&'static TouchOverlay, Retained<NSWindow>)> {
    let ptr = (*OVERLAY.lock().ok()?)? as *const TouchOverlay;
    let view: &'static TouchOverlay = unsafe { ptr.as_ref()? };
    let window = view.window()?;
    Some((view, window))
}

pub fn native_status() -> String {
    "Trackpad (NSTouch) active".into()
}

/// True while edit mode holds the pointer (ink draws only then).
pub fn edit_mode_on() -> bool {
    pointer().lock().map(|p| p.is_detached()).unwrap_or(false)
}

/// Snapshot of the current edit box, for the phone adapter.
pub fn current_edit_box() -> Option<EditBox> {
    edit_box().lock().ok().map(|b| *b)
}

/// Place the edit box (canvas pixels) and clamp it inside the canvas.
pub fn set_edit_box(anchor_x: f64, anchor_y: f64, width: f64, canvas_w: f64, canvas_h: f64) {
    if let Ok(mut b) = edit_box().lock() {
        *b = EditBox { anchor: (anchor_x, anchor_y), width };
        b.clamp_to(canvas_w, canvas_h);
    }
}

/// Enter or leave edit mode. Entering detaches the pointer and gives the overlay
/// focus; leaving reattaches it and hands focus back. Call on the main thread.
pub fn set_edit_mode(enabled: bool) -> Result<(), String> {
    let mut lock = pointer().lock().map_err(|_| "Pointer lock is unavailable")?;
    if enabled {
        lock.enter_edit()?;
        drop(lock);
        // Start every edit session with clean gesture state.
        if let Ok(mut g) = gestures().lock() {
            g.reset();
        }
        crate::phone_trackpad::reset_phone_gestures();
        if let Some((view, window)) = overlay_and_window() {
            *view.ivars().previous.lock().map_err(|_| "poisoned")? = window.firstResponder();
            window.makeFirstResponder(Some(view));
        }
        Ok(())
    } else {
        let result = lock.exit_edit();
        drop(lock);
        if let Ok(mut g) = gestures().lock() {
            g.reset();
        }
        crate::phone_trackpad::reset_phone_gestures();
        if let Some((view, window)) = overlay_and_window() {
            let prev = view.ivars().previous.lock().ok().and_then(|mut g| g.take());
            window.makeFirstResponder(prev.as_deref());
        }
        result
    }
}

/// Always reattach the pointer (used on exit).
pub fn release_pointer() {
    if let Ok(mut p) = pointer().lock() {
        let _ = p.startup_recover();
    }
}
