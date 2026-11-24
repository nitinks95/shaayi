//! macOS `PointerBackend`: detaches the pointer from the mouse with
//! `CGAssociateMouseAndMouseCursorPosition` while editing.

use shaayi_input::pointer_lock::PointerBackend;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGAssociateMouseAndMouseCursorPosition(connected: u32) -> i32;
}

pub struct AssociateBackend;

fn associate(connected: bool) -> Result<(), String> {
    let err = unsafe { CGAssociateMouseAndMouseCursorPosition(connected as u32) };
    if err == 0 {
        Ok(())
    } else {
        Err(format!("CGAssociateMouseAndMouseCursorPosition({}) failed: CGError {err}", connected as u32))
    }
}

impl PointerBackend for AssociateBackend {
    fn detach(&mut self) -> Result<(), String> {
        associate(false)
    }

    fn reattach(&mut self) -> Result<(), String> {
        associate(true)
    }
}
