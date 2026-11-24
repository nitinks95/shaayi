// Tauri shell for the M0 spike. Loads the shared web spike from ../web.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use tauri::State;

#[cfg(target_os = "windows")]
mod pointer_win;
#[cfg(target_os = "windows")]
mod windows_input;
mod phone_trackpad;

#[tauri::command]
fn set_edit_mode(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows_input::set_edit_mode(enabled)?;
        use tauri::Emitter;
        let _ = app.emit("edit-mode-changed", enabled);
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        // macOS: no pointer clipping here. Edit mode is still switched on so the
        // phone bridge can be tested. Native trackpad input stays Windows-only.
        use tauri::Emitter;
        let _ = app.emit("edit-mode-changed", enabled);
        Ok(())
    }
}

#[tauri::command]
fn get_input_status() -> String {
    #[cfg(target_os = "windows")]
    { return windows_input::status(); }
    #[cfg(not(target_os = "windows"))]
    { "Native input probe is Windows-only.".into() }
}

/// "Precision Touchpad active" or "No Precision Touchpad found; native trackpad input is off".
#[tauri::command]
fn native_status() -> String {
    #[cfg(target_os = "windows")]
    { return windows_input::native_status(); }
    #[cfg(not(target_os = "windows"))]
    { "No Precision Touchpad found; native trackpad input is off".into() }
}

/// Place the edit box (canvas pixels); it is clamped inside the canvas.
#[tauri::command]
fn set_edit_box(anchor_x: f64, anchor_y: f64, width: f64, canvas_w: f64, canvas_h: f64) {
    #[cfg(target_os = "windows")]
    windows_input::set_edit_box(anchor_x, anchor_y, width, canvas_w, canvas_h);
    #[cfg(not(target_os = "windows"))]
    let _ = (anchor_x, anchor_y, width, canvas_w, canvas_h);
}

#[tauri::command]
fn get_phone_pairing(state: State<'_, phone_trackpad::PhoneTrackpadServer>) -> Result<phone_trackpad::PairingInfo, String> {
    state.pairing_info()
}

#[tauri::command]
fn stop_phone_sharing(state: State<'_, phone_trackpad::PhoneTrackpadServer>) {
    state.stop();
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "windows")]
            windows_input::start(app.handle().clone())?;
            let phone_server = phone_trackpad::PhoneTrackpadServer::start(app.handle().clone())?;
            app.manage(phone_server);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_edit_mode, get_input_status, native_status, set_edit_box, get_phone_pairing, stop_phone_sharing])
        .on_window_event(|_window, event| {
            #[cfg(target_os = "windows")]
            if matches!(event, tauri::WindowEvent::Focused(false) | tauri::WindowEvent::CloseRequested { .. }) {
                let _ = windows_input::set_edit_mode(false);
                use tauri::Emitter;
                let _ = _window.app_handle().emit("edit-mode-changed", false);
            }
            if let Some(phone) = _window.app_handle().try_state::<phone_trackpad::PhoneTrackpadServer>() {
                match event {
                    tauri::WindowEvent::Focused(false) => phone.disconnect_phone(),
                    tauri::WindowEvent::CloseRequested { .. } => phone.stop(),
                    _ => {}
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Shaayi M0 spike");
}
