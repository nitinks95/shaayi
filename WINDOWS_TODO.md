# Shaayi: Windows to-do

Goal: match the macOS spike on Windows. Trackpad-as-pad ink, edit mode with a detached pointer, box anchored at edit start, click drops a dot, two-finger gestures never ink, and pointer restored on every exit.

Reference: `spike/macos/Sources/ShaayiTouchSpike/main.swift` (macOS results: avg 2.5 ms, p95 6.3 ms touch latency).

---

## Current spike state

- The Windows experiment lives in `spike/windows` as a separate Tauri copy. It registers raw HID input for usage page `0x0D`, usage `0x05`, keeps raw report diagnostics in the HUD, queries the device's HID preparsed data, and emits decoded contact events to the shared canvas.
- Ctrl+E clips the pointer to a 1-pixel rectangle and releases the clip on mode exit, focus loss, and close. Startup also clears any stale clip left by a force-terminated run.
- The Windows decoder uses descriptor capabilities to read contact IDs, tip state, and logical X/Y ranges. The ASUS ELAN Precision Touchpad was manually confirmed to scribble like the macOS spike after correcting vertical orientation. Cross-device validation and native touch-to-frame latency measurement remain open.
- A separate phone-pad path serves a temporary touch page and WebSocket on the local IPv4 address, pairs with a per-run QR token, and feeds normalized contacts to the desktop canvas. It is a browser interaction stand-in and does not validate Windows Precision Touchpad HID input.
- The user has launched the spike and confirmed that phone-based input works as expected. The screenshot recorded phone event-to-frame timing of 7.8 ms average and 15.1 ms p95 at 60 fps; this bridge metric is not physical touch-to-ink latency.
- Build check: `cargo check --locked` in `spike/tauri` passes on Windows (user-run). This confirms the build against the committed lockfile, not input behaviour.
- The phone bridge remains a separate input path. Its tap, reconnect/Wi-Fi-loss, two-finger rollback, and stop-sharing checks passed on Windows; the same checks still need running on macOS. The native Windows path has been manually confirmed on the ASUS ELAN pad.

## 0. Decide before coding

- [x] Stack for the Windows spike: **Tauri (Rust)**. The raw-input probe is native Rust/Win32 and shares the web HUD.
- [x] Use a Windows machine with a **precision touchpad** (not a mouse-only PC, and not a USB mouse). Record the model and driver.

## 1. Environment

- [x] Visual Studio Build Tools (C++ workload) and the Windows 10/11 SDK are available; the spike was built and launched on the user's Windows machine.
- [x] Rust (`rustup`, MSVC toolchain) is installed and working on the user's Windows machine.
- [ ] Install Node.js LTS if the shell needs the web layer.
- [x] Confirm the touchpad reports as a precision touchpad. Windows PnP identifies the built-in ELAN1300 as an ASUS Precision Touchpad.

## 2. Raw trackpad input (the core test)

- [ ] Register for raw input with `RegisterRawInputDevices` on the HID touchpad page (usage page `0x0D`, digitizer touch pad, usage `0x05`).
- [x] Read `WM_INPUT` and decode contact IDs and X/Y using HID preparsed data and value capabilities.
- [x] Map decoded contact X/Y into the edit box with vertical orientation matching finger movement.
- [ ] Confirm raw input works without admin rights and without extra permissions.
- [ ] Measure touch-to-draw latency and report avg and p95 in the HUD (same HUD as macOS).

Alternative if raw input proves hard: `WM_POINTER` messages with `POINTER_TYPE_INFO` for touchpad contacts. Test both and record which works better.

## 3. Detached pointer (edit mode)

Windows has no direct equivalent of `CGAssociateMouseAndMouseCursorPosition`. Test these options and record which works:

- [ ] **Option A: ClipCursor** to a 1-pixel rectangle at the edit start point, so the pointer can't move. Release with `ClipCursor(NULL)`.
- [ ] **Option B:** hide the pointer with a transparent cursor and clip it to the box. Check whether the pointer still moves outside the box.
- [ ] Confirm the pointer is restored on: Cmd-equivalent toggle (Ctrl+E), focus loss (`WM_ACTIVATE`/`WM_KILLFOCUS`), window close, and app exit.
- [ ] Test a force-kill (Task Manager > End task) while in edit mode. Record whether the pointer returns on its own. If not, add a startup check that calls `ClipCursor(NULL)`.

## 4. Gestures and accidental input

- [ ] Two or more fingers never draw. Remove ink from a session if the second finger lands within 150 ms (same rule as macOS).
- [ ] Ignore single unmodified key presses in edit mode, and let Ctrl/Alt/Shift/Win shortcuts pass.
- [ ] Check which Windows touchpad gestures the OS keeps (three-finger swipe for task switching, four-finger desktop switch). Record the conflicts.

## 5. Mode and window behaviour

- [ ] Edit mode on/off with Ctrl+E (or chosen shortcut). Orange box border in edit mode, grey outside.
- [ ] Edit mode turns off when the app loses focus.
- [ ] Box anchors where the pointer was at edit start, clamped inside the view.
- [ ] Click in edit mode drops a dot at the finger position.

## 6. Test checklist (run on Windows, record numbers)

- [x] User-reported: phone QR flow and phone input to the Windows spike work as expected.
- [x] Windows: phone tap makes a dot; two-finger contact before 150 ms rolls back early ink; after 150 ms preserves accepted ink and suspends further drawing.
- [x] Windows: phone reconnects after a temporary disconnect; Wi-Fi loss clears active contacts.
- [x] Windows: stop sharing closes the phone session; desktop focus loss clears contacts, exits edit mode, and restores the pointer.
- [x] macOS: phone pairing and phone input work in the Tauri build (user-confirmed 2026-11-24).
- [x] macOS: phone checks passed (user-confirmed 2026-11-24): two-finger rollback, stop sharing, and focus loss work. Reconnect after Wi-Fi drop failed at first (the desktop held a stale phone slot). It was fixed with a 1.5 s phone heartbeat, a 4 s dead-link timeout, and a retryable "busy" response. It now reconnects automatically.
- [x] Second Windows laptop (HP) has no Precision Touchpad (reported by user 2026-11-24). Native input can't be validated there. Observed on 2026-11-24: the app started; the trackpad did not draw, so ink was made by holding the left mouse button and dragging (browser pointer fallback). No "no precision touchpad" status was reported. The phone bridge worked fully on this laptop. Added (2026-11-24, not yet compiled or run on Windows): the app now lists HID devices and shows "No Precision Touchpad found. Native trackpad input is off; use the phone pad or the mouse." when no digitizer touchpad usage (page 0x0D, usage 0x05) is present. Retest on the HP laptop.
- [x] Windows: phone reconnect after Wi-Fi drop reported working with the same fix (user, 2026-11-24). Not separately re-tested after the fix was copied; the fix sits in the shared `spike/windows` and `spike/web` files.
- [ ] macOS: native trackpad input is not bridged into the Tauri build. It works only in the Swift spike in `spike/macos`. Decide whether to port it.
- [x] Draw one-finger ink across the box from a Windows Precision Touchpad. Native touch-to-draw latency remains unmeasured.
- [x] Two-finger gesture leaves no ink.
- [x] Pointer stays put in edit mode.
- [x] Pointer restored after Ctrl+E and focus loss. Normal-close restoration was included in the user's confirmation that the spike behaves like macOS.
- [ ] Force-kill in edit mode: pointer returns on its own? (yes / no)
- [ ] Single key ignored in edit mode; Ctrl+K passes.
- [ ] Native Windows results compared with macOS in `PRODUCT.md` section 17 when compatible hardware is available.

## 7. After the spike

- [ ] Record Windows results in `PRODUCT.md` section 17.
- [ ] Decide the stack (Electron or Tauri) with both platforms' numbers.
- [ ] Update `PRODUCT.md` milestones if the Windows input layer changes the M0 exit criteria.

---

## Open risks to watch

- Precision touchpad drivers differ by laptop maker. Test on at least one Windows laptop with a precision touchpad.
- `ClipCursor` may not behave like macOS when the pointer is on a second monitor. Test with two monitors.
- Some Windows gestures are handled by the OS before the app sees them. Option: intercept only inside edit mode.


### Windows Precision Touchpad spike test (2026-11-22) — ASUS VivoBook X430FA/S430FA

| Step | Result | Observation |
| --- | --- | --- |
| 1. Identify hardware and OS | PASS | ASUSTeK COMPUTER INC., system model `VivoBook_ASUSLaptop X430FA_S430FA`. Windows 11 Home Single Language 25H2 (build 26200.8524). Windows PnP lists the built-in ELAN1300 (HID collection 02) as a started ASUS Precision Touchpad. Installed ASUSTek driver 16.0.0.41, dated 2025-11-26 (`oem166.inf`, original INF `asusptpfilter.inf`). |
| 2. Build and launch | FAIL | Ran `cargo run` from `spike/tauri`. PowerShell could not resolve `cargo`. Compilation never started and the app did not launch. Exact error below. |
| 3. Raw HID reports | NOT TESTED | No HUD available to observe the idle count, changes during one-finger motion, tap/click, two-finger motion or a three-finger gesture, report size, or sample bytes. |
| 4. Ctrl+E clip and release | NOT TESTED | The app did not launch. |
| 5. Alt+Tab and normal close | NOT TESTED | Edit-mode exit and cursor restoration on focus loss and close could not be checked. |
| 6. Force-close and relaunch | NOT TESTED | No running app to force-close. Cursor restoration after termination and relaunch could not be checked. |

```text
The term 'cargo' is not recognized as a name of a cmdlet, function, script file, or executable program.
Check the spelling of the name, or if a path was included, verify that the path is correct and try again.
```

Raw HID report arrival remains unverified on this laptop. Contact ID and X/Y coordinate decoding are unimplemented in the spike.

### Windows Precision Touchpad validation (2026-11-24) — ASUS VivoBook X430FA/S430FA

- User manually confirmed the Windows experiment now behaves like the macOS spike on the built-in ASUS ELAN Precision Touchpad, including touchpad scribbling and the expected edit-mode and gesture behavior.
- User reported vertical movement was initially inverted while horizontal movement was correct. The decoder's Y-axis inversion was removed, and the user confirmed the spike then worked as expected.
- Raw report bytes and decoded contacts are exposed separately in the HUD. The earlier recording showed 14-byte raw reports; no fresh report count or native latency measurement was captured for this confirmation.
- The successful manual check used the isolated Tauri copy at `spike/windows`. Cross-device validation and native latency measurement remain open.
