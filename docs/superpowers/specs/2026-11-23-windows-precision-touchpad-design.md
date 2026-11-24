# Windows Precision Touchpad Scribbling Design

## Intent

Enable native one-finger scribbling on Windows Precision Touchpads in the existing Tauri spike, matching the behavior already demonstrated by the macOS `NSTouch` spike. The ASUS VivoBook with an ELAN Precision Touchpad is the first validation device. The decoder design should use HID report descriptors so another compatible device can be supported without hard-coded report offsets.

## Goals

- Read the selected Precision Touchpad TLC's HID report descriptor and decode report IDs, contact count, contact IDs, X/Y logical ranges, and contact tip/in-range state.
- Emit normalized native contact events from Windows into the shared web canvas.
- In edit mode, map one active finger to the anchored edit box, draw continuous ink, and place a dot on click.
- Treat two or more contacts as a gesture, suppressing ink and rolling back the first contact's ink when a second contact lands within 150 ms.
- Preserve raw report diagnostics and phone input as separate paths.
- Restore cursor state and clear active contacts when edit mode ends, focus is lost, or the app closes.
- Validate first on the ASUS ELAN pad, then record which devices and descriptor layouts have been verified.

## Non-goals

- Changing the macOS spike or its native `NSTouch` implementation.
- Replacing the phone input bridge.
- Claiming universal Windows device support from one ELAN validation. Unrecognized descriptor layouts must report a clear status and remain diagnosable.
- Decoding touchpad packets by guessing fixed offsets from one sample report.

## Constraints

- Keep the existing Tauri, Rust, Win32 raw-input, and web-canvas stack.
- Avoid a HID decoding crate unless built-in Windows HID preparsed-data APIs prove insufficient.
- Use the existing edit-mode and 150 ms gesture semantics from the macOS spike.
- Preserve user-visible raw report count and sample diagnostics.
- Keep work in the Windows spike and shared web canvas. Do not edit macOS source.

## Approaches

1. **Descriptor-driven Raw Input decoder (chosen).** Query the HID TLC descriptor with Windows HID APIs, parse report fields using preparsed data, then emit normalized contact events. This builds on the already observed raw report stream and avoids hard-coded ELAN byte offsets. It requires careful handling of report IDs, contact slots, logical ranges, and devices whose contacts span multiple reports.
2. **Windows pointer messages.** Investigate `WM_POINTER` or related APIs and use them if they expose precision touchpad contacts. This may simplify decoding, but current evidence does not show that this device exposes the required raw finger coordinates through those APIs.
3. **ELAN-specific byte offsets.** Decode only the screenshot's 14-byte sample layout. This would be fast to prototype but is fragile across firmware and manufacturers, so it is unsuitable for the stated general plan.

## Proposed architecture

Keep raw input registration in `spike/tauri/src/windows_input.rs`. Add a descriptor-based decoder layer that obtains and caches HID preparsed data and capabilities per raw device, decodes each input report, and maintains per-device contact state when one logical touch frame spans reports. Emit normalized contact events through a dedicated Tauri event, retaining the current raw-report event for diagnostics. In `spike/web/spike.js`, route native events through the same contact-session and stroke rules used by the phone bridge, with a normalized-position mapping suitable for the edit box. Keep pointer clipping, edit-mode anchoring, focus-loss cleanup, and close cleanup under the existing Windows input lifecycle.

## Validation

On the ASUS ELAN laptop, confirm descriptor discovery, report-ID and field decoding, one-finger down/move/up lifecycle, normalized coordinate orientation and bounds, click-to-dot behavior, second-finger rollback within 150 ms, later multi-finger suppression, and cleanup on focus loss and edit-mode exit. Compare raw report activity with decoded contact counts so report arrival is never mistaken for contact decoding. Measure event-to-next-frame latency after drawing works. Then repeat descriptor discovery and basic decoding on at least one other Precision Touchpad before describing the implementation as cross-device validated.

## Builder decisions

- Use Windows HID preparsed-data APIs and capability queries for descriptor-driven field access unless the existing Rust dependencies already provide a reliable equivalent.
- Normalize coordinates from each device's logical min/max ranges, then flip the vertical axis where needed to match the macOS bottom-left normalized origin.
- Keep one-finger drawing limited to edit mode. On a second contact, stop drawing and roll back the session's ink if it began within 150 ms.
- Treat malformed or unsupported descriptors as a status error while leaving raw report diagnostics available.
