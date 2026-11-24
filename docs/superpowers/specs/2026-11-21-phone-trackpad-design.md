# Phone trackpad for the Windows spike

## Intent

Use a phone browser as a temporary touch surface so the Windows spike can exercise Shaayi's edit-mode and gesture behavior without a precision touchpad. This is an interaction prototype; it does not emulate the Windows Precision Touchpad HID device or validate its native input driver.

## Goals

- Start a temporary phone page and WebSocket listener from the Tauri spike.
- Show a QR code in the desktop HUD that opens the phone page on the same Wi-Fi network.
- Pair each phone session with a random, short-lived token. Accept input only while the spike is running and the token is valid.
- Map one-finger touch coordinates to the anchored edit box, draw strokes, and show taps as dots.
- Treat two or more simultaneous fingers as gestures; discard first-finger ink if another finger lands within 150 ms.
- Show connection state, active contacts, and phone-event-to-frame timing in the HUD.

## Non-goals

- Reproduce raw Windows Precision Touchpad reports, driver behavior, or OS-level three/four-finger gestures.
- Send input or note data to an internet service.
- Support multiple phones drawing at once.
- Replace the existing native raw-input probe.

## Approach

Run a small HTTP and WebSocket service from the Tauri process, bound to the machine's local network interface. Serve a purpose-built touch page and use WebSocket messages for normalized contact events. Generate the QR code at runtime from the selected LAN address, port, and per-run pairing token. This keeps the path offline and makes the URL usable from the phone; a static QR cannot know the PC's current address. A cloud relay adds an internet dependency and does not help validate local behavior, while USB/Bluetooth HID emulation requires a native phone-side setup and cannot be launched by scanning a QR.

## Flow

1. Launch the Windows spike; it starts the local service and creates a random session token.
2. The desktop HUD displays its LAN URL as text and a QR code. The HUD provides a clear connected/disconnected indicator and a way to rotate the token or stop the listener.
3. Scan the QR while the phone and PC are on the same Wi-Fi. The phone page connects with the token and presents a full-screen 16:10 touch surface.
4. In desktop edit mode, phone contacts map from normalized phone coordinates into the anchored edit box. Single-finger motion draws. A second contact suppresses the gesture and removes first-finger ink started within 150 ms. A tap places a dot.
5. Leaving edit mode, losing desktop focus, closing the window, or stopping the listener ends active strokes and releases the Windows cursor clip.

## Security and network constraints

- The listener is temporary and only runs while the spike is open.
- Pairing tokens are cryptographically random and scoped to a single app run; reject missing or invalid tokens.
- Serve only the touch page and the input socket. Do not expose file, shell, or general-purpose RPC endpoints.
- Display the LAN URL and connection status so firewall or network isolation failures are diagnosable.
- The phone and PC must be able to reach each other on the local network; guest Wi-Fi isolation may prevent this.

## Decisions left to implementation

- Keep HTTP and WebSocket endpoints on one port if the chosen Rust server supports an upgrade route; otherwise document the two ports in the UI.
- Bind to the LAN interface for phone access and rely on the per-run token for pairing; do not silently fall back to a cloud or public relay.
- Use an existing QR encoder dependency if one is already available; otherwise add a small maintained Rust QR crate.

## Verification

- Confirm the QR's encoded URL opens the local touch page on a phone on the same Wi-Fi.
- Verify a valid token connects and an invalid/expired token is rejected.
- Verify one-finger strokes, tap dots, second-finger rollback within 150 ms, and no stroke leakage after two-finger gestures.
- Measure phone-event-to-next-frame latency and label it separately from physical touch-to-ink latency.
- Verify edit-mode exit, focus loss, close, and listener shutdown release cursor clipping and end active strokes.
