//! Windows Precision Touchpad adapter. Captures the Precision Touchpad HID TLC
//! as raw reports, decodes them with the device descriptor, and feeds shared
//! `ContactEvent`s (source: Trackpad) through `GestureMachine`. The resulting
//! `InkOp`s are mapped into the edit box and emitted as the `ink-ops` event.

use std::{
    collections::HashMap,
    ffi::c_void,
    mem::{size_of, zeroed},
    ptr,
    sync::{mpsc, Mutex, OnceLock},
    thread,
    time::Instant,
};

use shaayi_input::{
    pointer_lock::PointerLock, ContactEvent, EditBox, GestureMachine, InkOp, Phase, Source,
};

use crate::normalise::normalise;
use crate::pointer_win::ClipBackend;
use tauri::{AppHandle, Emitter};
use windows_sys::Win32::{
    Devices::HumanInterfaceDevice::{
        HidP_GetCaps, HidP_GetLinkCollectionNodes, HidP_GetSpecificValueCaps,
        HidP_GetUsageValue, HidP_GetUsages, HIDP_CAPS, HIDP_LINK_COLLECTION_NODE,
        HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, HidP_Input,
    },
    UI::Input::{GetRawInputDeviceInfoW, RIDI_PREPARSEDDATA},
};

type Handle = *mut c_void;
type WndProc = unsafe extern "system" fn(Handle, u32, usize, isize) -> isize;

const WM_INPUT: u32 = 0x00FF;
const RID_INPUT: u32 = 0x10000003;
const RIM_TYPEHID: u32 = 2;
const HID_USAGE_PAGE_DIGITIZER: u16 = 0x0D;
const HID_USAGE_DIGITIZER_TOUCHPAD: u16 = 0x05;
const CLASS_NAME: &[u16] = &[83, 104, 97, 97, 121, 105, 82, 97, 119, 73, 110, 112, 117, 116, 0];

static REPORTS: OnceLock<mpsc::Sender<serde_json::Value>> = OnceLock::new();
static POINTER: OnceLock<Mutex<PointerLock<ClipBackend>>> = OnceLock::new();
static GESTURES: OnceLock<Mutex<GestureMachine>> = OnceLock::new();
static EDIT_BOX: OnceLock<Mutex<EditBox>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();
static STATUS: OnceLock<std::sync::Mutex<String>> = OnceLock::new();
static ACTIVE_CONTACTS: OnceLock<Mutex<HashMap<usize, HashMap<u32, (f64, f64)>>>> = OnceLock::new();

#[repr(C)]
struct RawInputHeader {
    dw_type: u32,
    dw_size: u32,
    device: Handle,
    w_param: usize,
}

#[repr(C)]
struct RawInputDevice {
    usage_page: u16,
    usage: u16,
    flags: u32,
    target: Handle,
}

#[repr(C)]
struct WindowClass {
    style: u32,
    wnd_proc: Option<WndProc>,
    class_extra: i32,
    window_extra: i32,
    instance: Handle,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu_name: *const u16,
    class_name: *const u16,
}

#[repr(C)]
struct Message {
    window: Handle,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    point_x: i32,
    point_y: i32,
    private: u32,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(class: *const WindowClass) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        parameter: *mut c_void,
    ) -> Handle;
    fn DefWindowProcW(window: Handle, message: u32, w_param: usize, l_param: isize) -> isize;
    fn RegisterRawInputDevices(devices: *const RawInputDevice, count: u32, size: u32) -> i32;
    fn GetRawInputData(input: Handle, command: u32, data: *mut c_void, size: *mut u32, header_size: u32) -> u32;
    fn GetMessageW(message: *mut Message, window: Handle, min: u32, max: u32) -> i32;
    fn TranslateMessage(message: *const Message) -> i32;
    fn DispatchMessageW(message: *const Message) -> isize;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn GetLastError() -> u32;
}

pub fn start(app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel::<serde_json::Value>();
    REPORTS.set(tx).map_err(|_| std::io::Error::new(std::io::ErrorKind::AlreadyExists, "Raw input channel already initialized"))?;
    START.get_or_init(Instant::now);
    STATUS.set(std::sync::Mutex::new("Initializing raw input…".into())).ok();

    thread::Builder::new()
        .name("shaayi-raw-input".into())
        .spawn(input_window_thread)?;

    thread::Builder::new()
        .name("shaayi-input-events".into())
        .spawn(move || {
            for payload in rx {
                if let Some(ops) = payload.get("ink-ops") {
                    let _ = app.emit("ink-ops", ops.clone());
                } else {
                    let _ = app.emit("touchpad-report", payload);
                }
            }
        })?;

    // Clear a stale cursor clip from a previous force-terminated run.
    pointer().lock().map_err(|_| "Pointer lock is unavailable")?.startup_recover()?;
    Ok(())
}

fn pointer() -> &'static Mutex<PointerLock<ClipBackend>> {
    POINTER.get_or_init(|| Mutex::new(PointerLock::new(ClipBackend)))
}

fn gestures() -> &'static Mutex<GestureMachine> {
    GESTURES.get_or_init(|| Mutex::new(GestureMachine::new()))
}

fn edit_box() -> &'static Mutex<EditBox> {
    EDIT_BOX.get_or_init(|| Mutex::new(EditBox { anchor: (0.0, 0.0), width: 0.0 }))
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

pub fn native_status() -> String {
    if has_precision_touchpad() {
        "Precision Touchpad active".into()
    } else {
        "No Precision Touchpad found; native trackpad input is off".into()
    }
}


pub fn status() -> String {
    STATUS.get_or_init(|| std::sync::Mutex::new("Initializing raw input…".into()))
        .lock().map(|status| status.clone())
        .unwrap_or_else(|_| "Native input status unavailable".into())
}

pub fn set_edit_mode(enabled: bool) -> Result<(), String> {
    let mut lock = pointer().lock().map_err(|_| "Pointer lock is unavailable")?;
    if enabled {
        lock.enter_edit()
    } else {
        let result = lock.exit_edit();
        if let Ok(mut g) = gestures().lock() {
            g.reset();
        }
        result
    }
}

fn input_window_thread() {
    unsafe {
        let instance = GetModuleHandleW(ptr::null());
        let class = WindowClass {
            style: 0,
            wnd_proc: Some(window_proc),
            class_extra: 0,
            window_extra: 0,
            instance,
            icon: ptr::null_mut(),
            cursor: ptr::null_mut(),
            background: ptr::null_mut(),
            menu_name: ptr::null(),
            class_name: CLASS_NAME.as_ptr(),
        };
        if RegisterClassW(&class) == 0 {
            send_status(format!("RegisterClassW failed: {}", GetLastError()));
            return;
        }

        let window = CreateWindowExW(
            0, CLASS_NAME.as_ptr(), ptr::null(), 0, 0, 0, 0, 0,
            ptr::null_mut(), ptr::null_mut(), instance, ptr::null_mut(),
        );
        if window.is_null() {
            send_status(format!("CreateWindowExW failed: {}", GetLastError()));
            return;
        }

        let device = RawInputDevice {
            usage_page: HID_USAGE_PAGE_DIGITIZER,
            usage: HID_USAGE_DIGITIZER_TOUCHPAD,
            flags: 0,
            target: window,
        };
        if RegisterRawInputDevices(&device, 1, size_of::<RawInputDevice>() as u32) == 0 {
            send_status(format!("RegisterRawInputDevices failed: {}", GetLastError()));
            return;
        }
        if has_precision_touchpad() {
            send_status("Listening for Precision Touchpad HID reports (page 0x0D, usage 0x05).".into());
        } else {
            send_status("No Precision Touchpad found. Native trackpad input is off; use the phone pad or the mouse.".into());
        }

        let mut message: Message = zeroed();
        loop {
            let result = GetMessageW(&mut message, ptr::null_mut(), 0, 0);
            if result <= 0 { break; }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn window_proc(window: Handle, message: u32, w_param: usize, l_param: isize) -> isize {
    if message == WM_INPUT {
        capture_raw_input(l_param as Handle);
    }
    // WM_INPUT cleanup is required for foreground input messages.
    unsafe { DefWindowProcW(window, message, w_param, l_param) }
}

unsafe fn capture_raw_input(input: Handle) {
    let header_size = size_of::<RawInputHeader>() as u32;
    let mut size = 0u32;
    let result = unsafe { GetRawInputData(input, RID_INPUT, ptr::null_mut(), &mut size, header_size) };
    if result == u32::MAX || size < header_size + 8 { return; }

    // Vec<usize> gives the buffer the alignment required by RAWINPUT.
    let mut storage = vec![0usize; (size as usize + size_of::<usize>() - 1) / size_of::<usize>()];
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(storage.as_mut_ptr().cast::<u8>(), storage.len() * size_of::<usize>())
    };
    let read = unsafe {
        GetRawInputData(input, RID_INPUT, bytes.as_mut_ptr().cast(), &mut size, header_size)
    };
    if read == u32::MAX || read < header_size + 8 { return; }

    let header = unsafe { &*bytes.as_ptr().cast::<RawInputHeader>() };
    if header.dw_type != RIM_TYPEHID { return; }
    let hid_offset = header_size as usize;
    let report_size = u32::from_ne_bytes(bytes[hid_offset..hid_offset + 4].try_into().unwrap()) as usize;
    let report_count = u32::from_ne_bytes(bytes[hid_offset + 4..hid_offset + 8].try_into().unwrap()) as usize;
    let report_offset = hid_offset + 8;
    if report_size == 0 || report_count == 0 || report_count > 64
        || report_size.saturating_mul(report_count) > bytes.len().saturating_sub(report_offset) { return; }

    let report_slices: Vec<&[u8]> = (0..report_count)
        .map(|i| &bytes[report_offset + i * report_size..report_offset + (i + 1) * report_size])
        .collect();
    let reports: Vec<String> = report_slices.iter()
        .map(|report| report.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "))
        .collect();
    if let Some(tx) = REPORTS.get() {
        let _ = tx.send(serde_json::json!({
            "device": header.device as usize,
            "reportSize": report_size,
            "reports": reports,
        }));
    }
    for report in report_slices {
        if let Some(contacts) = decode_touchpad_report(header.device, report) {
            emit_contact_changes(header.device as usize, contacts);
        }
    }
}

#[derive(Clone, Copy)]
struct DecodedContact {
    id: u32,
    x: f64,
    y: f64,
}

/// Raw logical value (offset to zero) and logical span for one usage.
type Raw = (i32, i32);

fn decode_touchpad_report(device: Handle, report: &[u8]) -> Option<Vec<DecodedContact>> {
    unsafe {
        let mut preparsed_size = 0u32;
        let result = GetRawInputDeviceInfoW(device, RIDI_PREPARSEDDATA, ptr::null_mut(), &mut preparsed_size);
        if result == u32::MAX || preparsed_size == 0 {
            return None;
        }
        let word_count = (preparsed_size as usize + size_of::<usize>() - 1) / size_of::<usize>();
        let mut preparsed = vec![0usize; word_count];
        if GetRawInputDeviceInfoW(device, RIDI_PREPARSEDDATA, preparsed.as_mut_ptr().cast(), &mut preparsed_size) == u32::MAX {
            return None;
        }
        let preparsed_data = preparsed.as_mut_ptr() as isize;
        let mut caps = HIDP_CAPS::default();
        if HidP_GetCaps(preparsed_data, &mut caps) != HIDP_STATUS_SUCCESS || caps.NumberLinkCollectionNodes == 0 {
            return None;
        }

        let mut nodes = vec![HIDP_LINK_COLLECTION_NODE::default(); caps.NumberLinkCollectionNodes as usize];
        let mut node_count = nodes.len() as u32;
        if HidP_GetLinkCollectionNodes(nodes.as_mut_ptr(), &mut node_count, preparsed_data) != HIDP_STATUS_SUCCESS {
            return None;
        }

        let mut contacts = Vec::new();
        for (index, node) in nodes.iter().take(node_count as usize).enumerate().skip(1) {
            let page = ptr::addr_of!(node.LinkUsagePage).read_unaligned();
            let usage = ptr::addr_of!(node.LinkUsage).read_unaligned();
            if page != 0x0D || usage != 0x22 { continue; }
            let link = index as u16;
            let mut tip_usages = [0u16; 16];
            let mut tip_count = tip_usages.len() as u32;
            let tip_status = HidP_GetUsages(
                HidP_Input, 0x0D, link, tip_usages.as_mut_ptr(), &mut tip_count,
                preparsed_data, report.as_ptr().cast_mut(), report.len() as u32,
            );
            if tip_status != HIDP_STATUS_SUCCESS || !tip_usages[..tip_count as usize].contains(&0x42) {
                continue;
            }

            let Some(id) = usage_value(preparsed_data, report, link, 0x0D, 0x51) else { continue; };
            let Some((rx, max_x)) = raw_usage(preparsed_data, report, link, 0x01, 0x30) else { continue; };
            let Some((ry, max_y)) = raw_usage(preparsed_data, report, link, 0x01, 0x31) else { continue; };
            let (x, y) = normalise(rx, ry, max_x, max_y);
            contacts.push(DecodedContact { id, x: x.clamp(0.0, 1.0), y: y.clamp(0.0, 1.0) });
        }
        Some(contacts)
    }
}

unsafe fn usage_value(preparsed: isize, report: &[u8], link: u16, page: u16, usage: u16) -> Option<u32> {
    let mut value = 0u32;
    let status = unsafe { HidP_GetUsageValue(HidP_Input, page, link, usage, &mut value, preparsed, report.as_ptr(), report.len() as u32) };
    (status == HIDP_STATUS_SUCCESS).then_some(value)
}

unsafe fn raw_usage(preparsed: isize, report: &[u8], link: u16, page: u16, usage: u16) -> Option<Raw> {
    let mut caps = HIDP_VALUE_CAPS::default();
    let mut cap_count = 1u16;
    let status = unsafe { HidP_GetSpecificValueCaps(HidP_Input, page, link, usage, &mut caps, &mut cap_count, preparsed) };
    if status != HIDP_STATUS_SUCCESS || cap_count == 0 || caps.LogicalMax <= caps.LogicalMin { return None; }
    let value = unsafe { usage_value(preparsed, report, link, page, usage)? } as i64;
    let span = (caps.LogicalMax as i64) - (caps.LogicalMin as i64);
    Some(((value - caps.LogicalMin as i64) as i32, span as i32))
}

fn emit_contact_changes(device: usize, current: Vec<DecodedContact>) {
    let contacts = ACTIVE_CONTACTS.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut devices) = contacts.lock() else { return; };
    let previous = devices.entry(device).or_default();
    let mut next = HashMap::new();
    for contact in current {
        let phase = if previous.contains_key(&contact.id) { Phase::Move } else { Phase::Down };
        next.insert(contact.id, (contact.x, contact.y));
        handle_contact(contact.id, phase, contact.x, contact.y);
    }
    for (id, (x, y)) in previous.iter() {
        if !next.contains_key(id) { handle_contact(*id, Phase::Up, *x, *y); }
    }
    *previous = next;
}

/// The only path from a touchpad contact to ink: through `GestureMachine`.
fn handle_contact(id: u32, phase: Phase, x: f64, y: f64) {
    // Contacts only draw while edit mode holds the pointer.
    if !pointer().lock().map(|p| p.is_detached()).unwrap_or(false) { return; }
    let at_ms = START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0;
    let event = ContactEvent { source: Source::Trackpad, id: id as u64, phase, x, y, at_ms };
    let ops = match gestures().lock() {
        Ok(mut g) => g.handle(event),
        Err(_) => return,
    };
    if ops.is_empty() { return; }
    let Ok(edit) = edit_box().lock().map(|b| *b) else { return; };
    let json: Vec<serde_json::Value> = ops.iter().map(|op| ink_op_json(op, &edit)).collect();
    if let Some(tx) = REPORTS.get() {
        let _ = tx.send(serde_json::json!({ "ink-ops": json }));
    }
}

pub fn ink_op_json(op: &InkOp, edit: &EditBox) -> serde_json::Value {
    match op {
        InkOp::Segment { from, to } => {
            let (f, t) = (edit.map(from.0, from.1), edit.map(to.0, to.1));
            serde_json::json!({ "op": "segment", "from": [f.0, f.1], "to": [t.0, t.1] })
        }
        InkOp::Dot { at } => {
            let a = edit.map(at.0, at.1);
            serde_json::json!({ "op": "dot", "at": [a.0, a.1] })
        }
        InkOp::Rollback { segments } => serde_json::json!({ "op": "rollback", "segments": segments }),
    }
}

#[repr(C)]
struct RawInputDeviceList {
    device: Handle,
    device_type: u32,
}

// RID_DEVICE_INFO: cbSize, dwType, then the union. The HID member starts at offset 8.
#[repr(C)]
struct RawInputDeviceInfo {
    size: u32,
    device_type: u32,
    vendor_id: u32,
    product_id: u32,
    version: u32,
    usage_page: u16,
    usage: u16,
    _union_tail: [u32; 2],
}

extern "system" {
    fn GetRawInputDeviceList(devices: *mut RawInputDeviceList, count: *mut u32, size: u32) -> u32;
    fn GetRawInputDeviceInfoW(device: Handle, command: u32, data: *mut c_void, size: *mut u32) -> u32;
}

const RIDI_DEVICEINFO: u32 = 0x2000_000B;

/// True when an attached HID device exposes the Digitizer / Touch Pad usage.
fn has_precision_touchpad() -> bool {
    unsafe {
        let item_size = size_of::<RawInputDeviceList>() as u32;
        let mut count: u32 = 0;
        if GetRawInputDeviceList(ptr::null_mut(), &mut count, item_size) == u32::MAX {
            return false;
        }
        let mut list: Vec<RawInputDeviceList> = (0..count)
            .map(|_| RawInputDeviceList { device: ptr::null_mut(), device_type: 0 })
            .collect();
        let filled = GetRawInputDeviceList(list.as_mut_ptr(), &mut count, item_size);
        if filled == u32::MAX {
            return false;
        }
        list.truncate(filled as usize);
        list.iter()
            .filter(|device| device.device_type == RIM_TYPEHID)
            .any(|device| {
                let mut info: RawInputDeviceInfo = zeroed();
                info.size = size_of::<RawInputDeviceInfo>() as u32;
                let mut size = info.size;
                let read = GetRawInputDeviceInfoW(
                    device.device,
                    RIDI_DEVICEINFO,
                    &mut info as *mut RawInputDeviceInfo as *mut c_void,
                    &mut size,
                );
                read != u32::MAX
                    && info.usage_page == HID_USAGE_PAGE_DIGITIZER
                    && info.usage == HID_USAGE_DIGITIZER_TOUCHPAD
            })
    }
}

fn send_status(status: String) {
    if let Ok(mut current) = STATUS.get_or_init(|| std::sync::Mutex::new(String::new())).lock() {
        *current = status.clone();
    }
    if let Some(tx) = REPORTS.get() {
        let _ = tx.send(serde_json::json!({ "status": status }));
    }
}
