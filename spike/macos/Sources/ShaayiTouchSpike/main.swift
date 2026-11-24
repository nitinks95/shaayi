// Shaayi native macOS spike (round 3).
// Tests, without any system permission:
//  1. Explicit edit mode (Cmd+E toggles). Ink only in edit mode.
//  2. Edit mode decouples the trackpad from the cursor, so finger movement never moves the
//     pointer. The cursor is restored on every exit path (toggle, focus loss, quit).
//  3. The edit box (trackpad-sized) is anchored where edit mode starts, not the window centre.
//  4. Absolute mapping: finger position maps to the box like a touchscreen.
//  5. A trackpad click in edit mode drops a dot at the finger position.
//  6. Two or more fingers are a gesture, never ink. Early ink from a first finger is removed.
//  7. Single unmodified keys are ignored in edit mode; modifier shortcuts pass.
// Throwaway code: for measurement only.

import AppKit
import CoreGraphics

final class SpikeView: NSView {
    // Trackpad-shaped edit box (16:10), centred on the anchor point chosen at edit start.
    private var anchor: NSPoint?
    private let boxWidth: CGFloat = 320

    var editBox: NSRect {
        let w = boxWidth
        let h = w / 1.6
        let c = anchor ?? NSPoint(x: bounds.midX, y: bounds.midY)
        // Keep the box inside the view.
        let x = min(max(c.x - w / 2, 8), bounds.width - w - 8)
        let y = min(max(c.y - h / 2, 8), bounds.height - h - 8)
        return NSRect(x: x, y: y, width: w, height: h)
    }

    private(set) var editMode = false
    private var segments: [(NSPoint, NSPoint)] = []
    private var dots: [NSPoint] = []
    private var lastPoint: [ObjectIdentifier: NSPoint] = [:]
    private var lastFingerPoint: NSPoint?
    private var latencies: [Double] = []
    private var activeTouches = 0
    private var ignoredKeys = 0
    private var passedShortcuts = 0
    private var strokes = 0
    private var gestures = 0
    private var clicks = 0
    private var cursorDecoupled = false

    // Current touch session (from first finger down to last finger up).
    private var sessionStart: Double = 0
    private var sessionSegmentStart = 0
    private var sessionIsGesture = false

    private let transparentCursor = NSCursor(
        image: NSImage(size: NSSize(width: 16, height: 16)),
        hotSpot: .zero)

    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        allowedTouchTypes = [.indirect]   // trackpad touches
        wantsRestingTouches = false
        window?.makeFirstResponder(self)
        let nc = NotificationCenter.default
        nc.addObserver(self, selector: #selector(appResignedActive),
                       name: NSApplication.didResignActiveNotification, object: nil)
        nc.addObserver(self, selector: #selector(appWillTerminate),
                       name: NSApplication.willTerminateNotification, object: nil)
    }

    @objc private func appResignedActive() { setEditMode(false) }
    @objc private func appWillTerminate() { restoreCursor() }

    // MARK: Edit mode and cursor

    func setEditMode(_ on: Bool) {
        if on == editMode { return }
        editMode = on
        if on {
            // Anchor the box where the pointer is now, so it appears where the user is working.
            if let w = window {
                let p = convert(w.mouseLocationOutsideOfEventStream, from: nil)
                anchor = p
            }
            decoupleCursor()
        } else {
            restoreCursor()
            lastFingerPoint = nil
        }
        window?.invalidateCursorRects(for: self)
        needsDisplay = true
    }

    // Stop the trackpad moving the pointer while in edit mode.
    private func decoupleCursor() {
        guard !cursorDecoupled else { return }
        CGAssociateMouseAndMouseCursorPosition(0)
        cursorDecoupled = true
    }

    // Always restore the pointer when leaving edit mode, losing focus or quitting.
    private func restoreCursor() {
        guard cursorDecoupled else { return }
        CGAssociateMouseAndMouseCursorPosition(1)
        cursorDecoupled = false
    }

    deinit { restoreCursor() }

    // Cursor is transparent only inside the edit box, and only in edit mode.
    override func resetCursorRects() {
        super.resetCursorRects()
        if editMode {
            addCursorRect(editBox, cursor: transparentCursor)
        }
    }

    // MARK: Touches

    override func touchesBegan(with event: NSEvent) { handleTouches(event) }
    override func touchesMoved(with event: NSEvent) { handleTouches(event) }
    override func touchesEnded(with event: NSEvent) { handleTouches(event) }
    override func touchesCancelled(with event: NSEvent) { handleTouches(event) }

    private func handleTouches(_ event: NSEvent) {
        let now = ProcessInfo.processInfo.systemUptime
        let touches = event.touches(matching: .touching, in: self)
        let count = touches.count
        activeTouches = count
        latencies.append(now - event.timestamp)
        if latencies.count > 5000 { latencies.removeFirst(latencies.count - 5000) }

        // Session boundaries
        if count > 0 && sessionStart == 0 {
            sessionStart = now
            sessionSegmentStart = segments.count
            sessionIsGesture = false
            if count == 1 { strokes += 1 }
        }
        if count == 0 {
            sessionStart = 0
            sessionIsGesture = false
            lastPoint.removeAll()
            needsDisplay = true
            return
        }

        // Two or more fingers: a gesture. Remove ink from this session if the
        // second finger landed quickly (first ~150 ms), otherwise keep what was drawn.
        if count >= 2 {
            if !sessionIsGesture {
                sessionIsGesture = true
                gestures += 1
                if now - sessionStart <= 0.15, sessionSegmentStart <= segments.count {
                    segments.removeSubrange(sessionSegmentStart..<segments.count)
                }
            }
            lastPoint.removeAll()
            needsDisplay = true
            return
        }

        // Single finger: ink only in edit mode, and only when not a gesture.
        let box = editBox
        guard let t = touches.first else { return }
        // normalizedPosition: 0...1 across the trackpad, origin bottom-left.
        let p = NSPoint(x: box.minX + t.normalizedPosition.x * box.width,
                        y: box.minY + t.normalizedPosition.y * box.height)
        lastFingerPoint = p

        guard editMode, !sessionIsGesture else {
            lastPoint.removeAll()
            needsDisplay = true
            return
        }
        let key = ObjectIdentifier(t.identity as AnyObject)
        if let prev = lastPoint[key] {
            segments.append((prev, p))
        }
        lastPoint = [key: p]
        needsDisplay = true
    }

    // A trackpad click in edit mode drops a dot at the finger position (touchscreen-like).
    override func mouseDown(with event: NSEvent) {
        guard editMode else { return }
        clicks += 1
        if let p = lastFingerPoint, editBox.contains(p) {
            dots.append(p)
        }
        needsDisplay = true
    }

    // MARK: Keyboard

    override func keyDown(with event: NSEvent) {
        let mods = event.modifierFlags.intersection([.command, .control, .option, .shift])
        if mods.contains(.command), event.charactersIgnoringModifiers == "e" {
            setEditMode(!editMode)
            return
        }
        if mods.isEmpty {
            if editMode { ignoredKeys += 1 }
        } else {
            passedShortcuts += 1
        }
        needsDisplay = true
    }

    // MARK: Drawing

    override func draw(_ dirtyRect: NSRect) {
        NSColor.windowBackgroundColor.setFill()
        bounds.fill()

        let box = editBox
        NSColor.textBackgroundColor.setFill()
        box.fill()

        // Ink
        NSColor.labelColor.setStroke()
        for (a, b) in segments {
            let path = NSBezierPath()
            path.lineWidth = 2
            path.lineCapStyle = .round
            path.move(to: a)
            path.line(to: b)
            path.stroke()
        }
        NSColor.labelColor.setFill()
        for d in dots {
            NSBezierPath(ovalIn: NSRect(x: d.x - 3, y: d.y - 3, width: 6, height: 6)).fill()
        }

        // Mode border: orange only in edit mode, faint grey otherwise.
        let border = NSBezierPath(rect: box.insetBy(dx: -2, dy: -2))
        border.lineWidth = 4
        (editMode ? NSColor.systemOrange : NSColor.separatorColor).setStroke()
        border.stroke()

        drawHUD()
    }

    private func drawHUD() {
        let sorted = latencies.sorted()
        let avg = latencies.isEmpty ? 0 : latencies.reduce(0, +) / Double(latencies.count)
        let p95 = sorted.isEmpty ? 0 : sorted[min(sorted.count - 1, Int(Double(sorted.count) * 0.95))]
        let last = latencies.last ?? 0
        let lines = [
            "Shaayi native spike (macOS)",
            "Mode: \(editMode ? "EDIT (cursor frozen, Cmd+E to exit)" : "VIEW (Cmd+E to enter edit)")",
            String(format: "Touch latency ms: last %.1f · avg %.1f · p95 %.1f · n=%d",
                   last * 1000, avg * 1000, p95 * 1000, latencies.count),
            "Active touches: \(activeTouches) · strokes: \(strokes) · gestures (2+ fingers): \(gestures) · clicks: \(clicks)",
            "Ignored single keys: \(ignoredKeys) · passed shortcuts: \(passedShortcuts)",
        ]
        let attrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 13),
            .foregroundColor: NSColor.labelColor,
        ]
        var y = bounds.height - 24
        for line in lines {
            (line as NSString).draw(at: NSPoint(x: 16, y: y), withAttributes: attrs)
            y -= 20
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!

    func applicationDidFinishLaunching(_ notification: Notification) {
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 720),
            styleMask: [.titled, .closable, .resizable, .miniaturizable],
            backing: .buffered, defer: false)
        window.title = "Shaayi native spike"
        window.contentView = SpikeView()
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationWillTerminate(_ notification: Notification) {
        // Safety net: the pointer must never stay detached from the trackpad.
        CGAssociateMouseAndMouseCursorPosition(1)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)

// Minimal menu so Cmd+Q works.
let mainMenu = NSMenu()
let appItem = NSMenuItem()
mainMenu.addItem(appItem)
let appMenu = NSMenu()
appMenu.addItem(NSMenuItem(title: "Quit Shaayi spike",
                           action: #selector(NSApplication.terminate(_:)),
                           keyEquivalent: "q"))
appItem.submenu = appMenu
app.mainMenu = mainMenu

let delegate = AppDelegate()
app.delegate = delegate
app.run()
