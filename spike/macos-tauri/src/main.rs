//! Throwaway spike: transparent NSView overlay on top of the WKWebView that logs NSTouch.
#[cfg(target_os = "macos")]
mod overlay {
    use objc2::rc::Retained;
    use objc2::{define_class, msg_send, MainThreadOnly, DefinedClass};
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSEvent, NSTouchPhase, NSTouchTypeMask, NSView, NSWindow};
    use objc2_foundation::{MainThreadMarker, NSRect};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::cell::Cell;

    static COUNT: AtomicU64 = AtomicU64::new(0);

    pub struct Ivars { first: Cell<Option<std::time::Instant>> }

    define_class!(
        #[unsafe(super(NSView))]
        #[thread_kind = MainThreadOnly]
        #[name = "ShaayiTouchOverlay"]
        #[ivars = Ivars]
        pub struct TouchOverlay;

        impl TouchOverlay {
            #[unsafe(method(acceptsFirstResponder))]
            fn accepts_first_responder(&self) -> bool { true }
            #[unsafe(method(touchesBeganWithEvent:))]
            fn began(&self, e: &NSEvent) { self.log("began", e); }
            #[unsafe(method(touchesMovedWithEvent:))]
            fn moved(&self, e: &NSEvent) { self.log("moved", e); }
            #[unsafe(method(touchesEndedWithEvent:))]
            fn ended(&self, e: &NSEvent) { self.log("ended", e); }
            #[unsafe(method(touchesCancelledWithEvent:))]
            fn cancelled(&self, e: &NSEvent) { self.log("cancelled", e); }
        }
    );

    impl TouchOverlay {
        fn log(&self, what: &str, e: &NSEvent) {
            let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            let touches = e.touchesMatchingPhase_inView(NSTouchPhase::Any, Some(self));
            let mut s = String::new();
            for t in touches.iter() {
                let p = t.normalizedPosition();
                s.push_str(&format!(" [{:.3},{:.3}]", p.x, p.y));
            }
            println!("TOUCH #{n} {what} n={}{s}", touches.len());
        }
    }

    pub fn install(window: &NSWindow) {
        let mtm = MainThreadMarker::new().expect("main thread");
        let content = window.contentView().expect("content view");
        let frame: NSRect = content.bounds();
        let this = mtm.alloc::<TouchOverlay>().set_ivars(Ivars { first: Cell::new(None) });
        let view: Retained<TouchOverlay> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        view.setAllowedTouchTypes(NSTouchTypeMask::Indirect);
        view.setWantsRestingTouches(true);
        content.addSubview(&view);
        window.makeFirstResponder(Some(&view));
        println!("overlay installed, frame={:?}, firstResponder set", frame);
        let _ = view.ivars();
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                use tauri::Manager;
                let w = app.get_webview_window("main").unwrap();
                let ptr = w.ns_window().unwrap() as *mut objc2_app_kit::NSWindow;
                overlay::install(unsafe { &*ptr });
            }
            let _ = app;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run");
}
