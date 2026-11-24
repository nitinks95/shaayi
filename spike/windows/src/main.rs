// Tauri shell for the M0 spike. Loads the shared web spike from ../web.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use tauri::State;

#[cfg(target_os = "windows")]
mod pointer_win;
#[cfg(target_os = "macos")]
mod macos_input;
#[cfg(target_os = "macos")]
mod pointer_mac;
#[cfg(target_os = "windows")]
mod windows_input;
mod phone_trackpad;
mod first_launch;
#[allow(dead_code)]
mod normalise;

#[tauri::command]
fn set_edit_mode(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows_input::set_edit_mode(enabled)?;
        use tauri::Emitter;
        let _ = app.emit("edit-mode-changed", enabled);
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        // NSView focus and the pointer detach must happen on the main thread.
        let (tx, rx) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || { let _ = tx.send(macos_input::set_edit_mode(enabled)); })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())??;
        use tauri::Emitter;
        let _ = app.emit("edit-mode-changed", enabled);
        return Ok(());
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        use tauri::Emitter;
        let _ = app.emit("edit-mode-changed", enabled);
        Ok(())
    }
}

#[tauri::command]
fn get_input_status() -> String {
    #[cfg(target_os = "windows")]
    { return windows_input::status(); }
    #[cfg(target_os = "macos")]
    { return macos_input::native_status(); }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    { "Native input probe is Windows-only.".into() }
}

/// "Precision Touchpad active" or "No Precision Touchpad found; native trackpad input is off".
#[tauri::command]
fn native_status() -> String {
    #[cfg(target_os = "windows")]
    { return windows_input::native_status(); }
    #[cfg(target_os = "macos")]
    { return macos_input::native_status(); }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    { "No Precision Touchpad found; native trackpad input is off".into() }
}

/// Place the edit box (canvas pixels); it is clamped inside the canvas.
#[tauri::command]
fn set_edit_box(anchor_x: f64, anchor_y: f64, width: f64, canvas_w: f64, canvas_h: f64) {
    #[cfg(target_os = "windows")]
    windows_input::set_edit_box(anchor_x, anchor_y, width, canvas_w, canvas_h);
    #[cfg(target_os = "macos")]
    macos_input::set_edit_box(anchor_x, anchor_y, width, canvas_w, canvas_h);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
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

/// Records that the user accepted the terms (marker file in the app config directory).
#[tauri::command]
fn accept_terms(app: tauri::AppHandle) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    first_launch::accept_in(&dir)
}

#[tauri::command]
fn has_accepted_terms(app: tauri::AppHandle) -> bool {
    app.path()
        .app_config_dir()
        .map(|dir| first_launch::has_accepted_in(&dir))
        .unwrap_or(false)
}

fn main() {
    let app = tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "windows")]
            windows_input::start(app.handle().clone())?;
            #[cfg(target_os = "macos")]
            {
                let window = app.get_webview_window("main").ok_or("main window missing")?;
                let ns = window.ns_window()? as *mut objc2_app_kit::NSWindow;
                macos_input::start(app.handle().clone(), unsafe { &*ns })?;
            }
            let phone_server = phone_trackpad::PhoneTrackpadServer::start(app.handle().clone())?;
            app.manage(phone_server);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_edit_mode, get_input_status, native_status, set_edit_box, get_phone_pairing, stop_phone_sharing, accept_terms, has_accepted_terms])
        .on_window_event(|_window, event| {
            #[cfg(target_os = "windows")]
            if matches!(event, tauri::WindowEvent::Focused(false) | tauri::WindowEvent::CloseRequested { .. }) {
                use tauri::Emitter;
                match windows_input::set_edit_mode(false) {
                    Ok(()) => { let _ = _window.app_handle().emit("edit-mode-changed", false); }
                    Err(e) => eprintln!("Failed to release pointer: {e}"),
                }
            }
            #[cfg(target_os = "macos")]
            if matches!(event, tauri::WindowEvent::Focused(false) | tauri::WindowEvent::CloseRequested { .. }) {
                use tauri::Emitter;
                match macos_input::set_edit_mode(false) {
                    Ok(()) => { let _ = _window.app_handle().emit("edit-mode-changed", false); }
                    Err(e) => eprintln!("Failed to release pointer: {e}"),
                }
            }
            if let Some(phone) = _window.app_handle().try_state::<phone_trackpad::PhoneTrackpadServer>() {
                match event {
                    tauri::WindowEvent::Focused(false) => phone.disconnect_phone(),
                    tauri::WindowEvent::CloseRequested { .. } => phone.stop(),
                    _ => {}
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Shaayi M0 spike");
    app.run(|_app, event| {
        #[cfg(target_os = "windows")]
        if matches!(event, tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }) {
            let _ = windows_input::set_edit_mode(false);
        }
        #[cfg(target_os = "macos")]
        if matches!(event, tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }) {
            macos_input::release_pointer();
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let _ = event;
    });
}
