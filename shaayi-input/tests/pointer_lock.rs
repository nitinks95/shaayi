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
