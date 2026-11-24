//! Windows `PointerBackend`: pins the cursor with `ClipCursor` while editing.

use shaayi_input::pointer_lock::PointerBackend;

#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[link(name = "user32")]
extern "system" {
    fn GetCursorPos(point: *mut Point) -> i32;
    fn ClipCursor(rect: *const Rect) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLastError() -> u32;
}

pub struct ClipBackend;

impl PointerBackend for ClipBackend {
    /// Clip the cursor to a 1-pixel rectangle at its current position.
    fn detach(&mut self) -> Result<(), String> {
        let mut point = Point { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return Err(format!("GetCursorPos failed: {}", unsafe { GetLastError() }));
        }
        let clip = Rect { left: point.x, top: point.y, right: point.x + 1, bottom: point.y + 1 };
        if unsafe { ClipCursor(&clip) } == 0 {
            return Err(format!("ClipCursor failed: {}", unsafe { GetLastError() }));
        }
        Ok(())
    }

    /// Release the clip.
    fn reattach(&mut self) -> Result<(), String> {
        if unsafe { ClipCursor(std::ptr::null()) } == 0 {
            return Err(format!("Releasing ClipCursor failed: {}", unsafe { GetLastError() }));
        }
        Ok(())
    }
}
