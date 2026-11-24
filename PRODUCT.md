# Shaayi (ಶಾಯಿ) — Product Spec (v0.1 draft)

> *ಶಾಯಿ* (shaayi) means "ink" in Kannada. Pronounced "shaa-yi". The logo is a single ink stroke in plain black.

> A personal whiteboard for handwritten notes. Scribble with a trackpad, file notes into folders, and later turn ink into copyable text and reminders. v1 runs fully offline. Cloud features arrive later, each behind explicit consent (see section 12).

---

## 1. Name

**Shaayi** — "ink" for the scribbling, "shelf" for the folder-like structure where notes live.

- Short, easy to say, works as a CLI/folder name (`shaayi/`), and is unlikely to clash with a major product.
- Naming check (trademark / app-store / domain search) is still pending before any public use.

---

## 2. Problem

- Digital note apps are built around typing. Sketching, diagramming, and quick handwritten thinking feel clumsy on them.
- Paper and whiteboards are fast and free-form, but they are not organised, searchable, or saved.
- Trackpad users (MacBook, Windows precision touchpads) have no good, natural way to write notes.
- Existing handwriting-to-text tools are either cloud-first (privacy concern) or buried inside large suites.

**Goal:** a calm, fast, private canvas where handwriting is a first-class input, notes are kept in folders, and ink can be converted to text on demand.

---

## 3. Target user

- Primary: the owner (single user, personal use, desktop).
- Secondary: knowledge workers and students who think by sketching, and who want their notes to stay local.

Out of scope for v1: multi-user, real-time collaboration, mobile phones/tablets.

---

## 4. Core principles

1. **Ink first.** Writing with a trackpad should feel like a pen on paper, not a form.
2. **Local by default.** Notes live on the user's disk, in open formats. No account required.
3. **Folders, not tags.** A simple, visible hierarchy the user can navigate and reason about.
4. **AI is optional and explicit.** Converting ink to text is a user action, never a background upload.
5. **Cross-platform from day one.** Same app on macOS and Windows.

---

## 5. Platform and tech approach

| Option | Pros | Cons |
|---|---|---|
| **Electron + web canvas (recommended)** | One codebase for macOS and Windows; mature pointer/pressure APIs; large ecosystem; easy to build the UI in HTML/Canvas/SVG | Larger app size, higher RAM use |
| Tauri (Rust shell + web UI) | Small binary, lower memory | Smaller ecosystem; more setup for file and native features |
| Native (SwiftUI + WinUI) | Best feel and performance per OS | Two codebases; slower to build solo |
| Python + Qt/PySide | Single codebase, simple to script | Less polished look; packaging is heavier |

**Recommendation:** Electron (or Tauri if size matters) with a web-based canvas. Revisit after a spike on trackpad ink latency.

**Local storage:** plain files on disk, one folder per notebook/folder, one file per note. See section 8.

---

## 6. Feature list

### 6.1 Whiteboard (canvas)

- Infinite, pannable, zoomable canvas.
- Each **note** is a bounded region on the canvas (a "card") that can be moved, resized, and re-ordered.
- Background: plain, dotted, or ruled (toggle).
- Undo / redo, including for ink strokes.
- Keyboard shortcuts for pen, eraser, select, pan, zoom.

### 6.2 Trackpad scribbling (the main input)

- **Trackpad-as-pad mode (revised):** the trackpad is a drawing surface, not a pointer. Finger strokes on the trackpad become ink on the note, with no cursor involved.
- **Mapped edit area:** a small box on the board shows the trackpad's aspect ratio. Only that box receives ink. The user can box-select any region of the board to make it the edit area.
- **Gestures (proposed, to be tested):**
  - Pinch: resize the edit box, which changes how much board the trackpad covers.
  - Two or three finger move (left/right/up/down): slide the edit box across the board.
  - A separate gesture enters and exits edit mode.
- **Pointer behaviour:** in edit mode the mouse cursor is hidden and the trackpad stops moving it. On exit, the cursor is restored.
- **Conflicts to check:** macOS and Windows already use 3-finger and 4-finger gestures (e.g. Mission Control, app switching). The final gesture set must avoid them or override them while in edit mode.
- Pressure (on force-touch trackpads) and tilt, if available, shape stroke width.
- Stroke smoothing (configurable) so handwriting looks clean.
- Palm rejection is not needed for trackpads, but accidental clicks/taps must not draw.
- Eraser: a stroke-level eraser (drag over ink to remove whole strokes) and a pixel eraser.
- Pen colour and thickness selector; a small set of presets (e.g. black, blue, red, highlighter).
- Lasso select to move, recolour, or delete a group of strokes.
- Two-finger gesture = pan; pinch = zoom (standard trackpad gestures).

### 6.3 Notes and folders

- **Notebook → Folder → Note** hierarchy. Folders can nest.
- Create, rename, move, duplicate, and delete notes and folders.
- Sidebar tree view, with search by title and by recognised text (see 6.5).
- Each note stores its own canvas; a note can contain multiple cards if the user wants.
- Deleting moves items to an in-app **Trash** folder first; permanent delete is explicit.

### 6.4 Save

- Autosave on every stroke batch (debounced, e.g. 500 ms) and on page change.
- No "Save" button needed. A small status indicator shows "Saved" / "Saving…".
- Crash safety: write to a temp file, then atomic rename. Keep the last N versions (e.g. 20) per note.

### 6.5 Ink cleanup (v2) and ink to text (v3)

- **v2 — cleanup (cloud, opt-in):** select scribbles and send the ink image to a cloud model to tidy or fix the drawing. This never corrects or rewrites text.
- **v3 — convert to text:** select strokes (or a whole note) → **Convert to text**, producing copyable text.
- The selected ink is rasterised to an image and sent to an AI model for handwriting recognition.
- Output is shown as a **text block** that the user can accept, edit, or discard. Original ink is kept unless the user chooses to replace it.
- The recognised text is stored alongside the note (see schema) so it can be searched.
- Support for multiple languages (English first; others later).
- Show clearly, before sending, that the selected ink will leave the machine (see privacy section 9).

### 6.6 Search

- Search across note titles, folder names, and recognised text.
- Results open the note and jump to the matching card.

### 6.7 Export and portability

- Export a note as PNG, PDF, or SVG (ink preserved).
- Export recognised text as Markdown or plain text.
- The whole library is a folder on disk, so backup, sync (user's own choice, e.g. iCloud Drive/OneDrive folder), and migration are simple.

### 6.8 Settings

- Library location (default: `~/Documents/Shaayi`).
- AI: on/off, provider choice, and API key stored in the OS keychain (not in the library folder).
- Pen defaults, smoothing amount, background style, theme (light/dark).

---

## 7. User flows

**A. Create and scribble a note**
1. Click "New note" in a folder (or press ⌘N / Ctrl+N).
2. Blank canvas opens with the pen selected.
3. Drag on the trackpad to write. Ink appears with the chosen colour.
4. Changes autosave. Title is set from the first line or renamed by the user.

**B. Organise**
1. Create a folder "Work / 2026 / Projects".
2. Drag a note from the canvas list onto the folder in the sidebar.

**C. Convert ink to text**
1. Lasso the handwriting.
2. Click "Convert to text" (or ⌘⇧T).
3. A preview appears with the recognised text. Edit if needed.
4. Accept → text block is added and indexed for search.

**D. Find a note**
1. Press ⌘K / Ctrl+K.
2. Type a word. Results show title, folder, and matching recognised text.

---

## 8. Data model and storage

Library layout on disk (open, human-readable):

```
Shaayi/
  .shaayi/
    index.sqlite          # search index, metadata cache (rebuildable)
    settings.json
    trash/
  Work/
    2026/
      Projects/
        meeting-ideas.note/
          note.json        # metadata: id, title, created, updated, card list
          strokes.json     # or strokes.bin for performance
          text.json        # recognised text blocks (optional)
          preview.png      # thumbnail for the sidebar
          versions/        # last N snapshots
```

- **Folders** are real directories. Renaming in the app renames the directory.
- **Notes** are directories ending in `.note` so they are easy to spot.
- **Strokes** are stored as arrays of points (x, y, pressure, timestamp) with a version number, so the format can evolve.
- The SQLite index is a cache; it can be deleted and rebuilt from the files.

Why this layout: transparent, easy to back up, no lock-in, and the user can read their own notes even if the app disappears.

---

## 9. Privacy and security

- **Local first.** No account, no telemetry in v1, no cloud sync built in.
- **v1 is fully offline:** no network calls at all, no AI, no accounts.
- **v2 onward:** notes leave the machine only when the user explicitly asks for cloud cleanup (v2) or conversion (v3) on a selection. Each request is consent-gated.
- Each AI request contains only the selected ink image and a minimal prompt. No note titles, folder paths, or user identifiers.
- API keys are stored in the OS credential store (macOS Keychain / Windows Credential Manager).
- Clear, per-action notice before any upload, with a "don't ask again for this note" option (optional).
- Fully offline operation is a v1 requirement (decided). Offline recognition for v3 is still to be chosen (see section 10).
- The library folder is not encrypted by default; an optional passphrase-based encryption can come later.

---

## 10. AI handwriting recognition: options

| Option | Privacy | Quality | Cost | Notes |
|---|---|---|---|---|
| **Cloud vision model (e.g. Claude)** | Ink leaves the machine per request | Very good, handles messy handwriting and mixed content | Pay per use | Simplest to build; send only the selected ink image |
| Local OCR model (e.g. Tesseract, or a small on-device handwriting model) | Fully local | Good for print, weaker for messy handwriting | Free | Ships larger; quality varies |
| Hybrid | Local by default, cloud on request | Best of both | Mixed | Recommended path if privacy must be strict |

**Decided:** v1 is fully offline, so no recognition runs in v1. v2 uses a cloud model with consent, only for cleaning up scribbles. For v3 (ink to copyable text), the engine is still open: a local model keeps the offline promise, while a cloud model is more accurate. Build recognition behind a provider interface so either can be used.

---

## 11. Non-functional requirements

- **Latency:** ink appears within ~16 ms of input (feels instant on a trackpad).
- **Startup:** cold start under 2 seconds on a mid-range laptop.
- **Large libraries:** 5,000 notes opens and searches without noticeable lag.
- **Reliability:** no data loss on crash or power cut (atomic writes + versions).
- **Accessibility:** keyboard-only navigation for all non-drawing actions; respects system theme and text size.
- **Size:** installer under ~150 MB (Electron) or ~20 MB (Tauri).

---

## 12. Milestones

### Release roadmap

| Version | Scope | Network use | Exit criteria |
|---|---|---|---|
| **M0 — Spike** | Trackpad ink on canvas; measure latency and smoothing on macOS and Windows | None | Handwriting feels natural; latency acceptable |
| **v1 — Offline notebook** | Canvas, folders (nested), notes, autosave, trash, versions, search by title, export PNG/PDF/SVG, packaging for macOS and Windows | None (fully offline) | Create, scribble, organise, and reopen after restart with no loss |
| **v2 — Ink cleanup** | Lasso scribbles and send to a cloud model to tidy the drawing; per-request consent; text is never edited | Cloud, opt-in per request | Cleanup visibly improves scribbles; nothing sent without consent |
| **v3 — Ink to copyable text** | Lasso handwriting, convert to text, preview, copy, search recognised text; engine choice (local or cloud) to be decided | Depends on engine chosen | Converted text is accurate enough to edit, not retype |
| **v4 — Tasks and reminders** | To-dos from notes, due dates, reminders (local notifications) | None planned | Reminders fire reliably on both OSes |

---

## 13. Success metrics (personal product)

- I reach for Shaayi instead of paper for at least 3 thinking sessions per week within 1 month of M1.
- Converted text needs fewer than ~1 in 10 words corrected on typical notes.
- No note lost in normal use (zero data-loss incidents).

---

## 14. Out of scope (v1)

- Real-time collaboration and sharing links.
- Mobile apps and stylus-tablet support (trackpad only in v1; stylus can be added later).
- Built-in cloud sync (use a synced folder instead).
- Shape recognition, auto-diagramming, and handwriting-to-maths.

---

## 15. Open questions

1. **Recognition engine for v3:** local (keeps the offline promise, weaker on messy handwriting) or cloud with consent (more accurate)? Answered for v1 and v2: v1 is offline, v2 is cloud cleanup only.
2. **Stack:** Electron (fastest to build, larger app) or Tauri (lighter, more setup)? Preference?
3. **Folder model:** strict nested folders only, or also allow notes to sit in several folders via links?
4. **Canvas vs. page:** is an infinite canvas per note the right model, or do you prefer fixed-size pages (like a notebook)?
5. **Name:** Shaayi is locked for now. A trademark, app-store and domain check is still needed before any public use.

---

## 16. Next steps

1. Answer the open questions above (especially Q1 and Q2).
2. Confirm whether v1 should include ink-to-text, or only the offline notebook (current reading: notebook only).
2. Build the M0 spike: a single window with trackpad ink, pen settings, and a latency check.
3. Then move into M1 with the library format from section 8.

---

## 17. Decisions from round 2

1. **Scope of rules:** all edit-mode rules apply only while Shaayi is the frontmost app. When another app is active, the cursor and keyboard behave normally.
2. **Customisable gestures:** settings let users remap every gesture. Separate defaults ship for macOS and Windows. A user's custom mapping replaces the default for that gesture.
3. **Keyboard in edit mode:** unmodified single key presses (including key repeat) are ignored in edit mode. Shortcuts with a modifier (Cmd/Ctrl, Option/Alt, Shift) still work.
4. **Mode indication:** a coloured border around the edit box plus a menu-bar (or system tray) icon that shows the current mode. Confirmed.
5. **Hidden cursor (recommended approach):**
   - Do not globally hide or disconnect the cursor.
   - Show a transparent cursor only inside the edit box. Draw ink from raw touch input, not pointer motion. The cursor works normally everywhere else.
   - Cursor state is tied to app focus. When the app loses focus, or quits, or the window closes, the normal cursor is restored automatically.
   - If a stronger hide is ever needed, it must be released on focus loss, quit, and by a watchdog timer. The spike must confirm that the OS restores cursor state if the process crashes.
6. **First launch:** show the terms and conditions, then a permissions checklist. Each permission explains why it's needed and has a button that opens the matching System Settings pane.
7. **Tabs:** each open note is a tab with its own edit box and mapping. Moving or resizing the window recalculates the mapping, and switching tabs restores that tab's mapping.
8. **Autosave:** save after each completed stroke. A stroke in progress may be lost on tab switch, lock, or shutdown. This is accepted.
9. **Layout:** a left panel lists notes and folders, collapsible on demand. The right panel is the canvas with the edit box.
11. **Round 3 (macOS, confirmed by user test):** in edit mode the pointer is detached from the trackpad, so finger movement only draws ink. This replaces the transparent-cursor-only approach in item 5. The edit box anchors where edit mode starts, and a trackpad click drops a dot at the finger position. The pointer is restored on every exit path. Force-quit during edit mode tested on macOS (2026-11-24): the pointer moved normally after Activity Monitor's Force Quit, with no relaunch or reboot needed. Tested on Windows (2026-11-24, user-run): the pointer moved normally after Task Manager's End task, without relaunching.
17. **Second Windows laptop (2026-11-24):** an HP laptop with no Precision Touchpad. The app started, but native trackpad input didn't draw. Ink came from a held left mouse button (browser pointer fallback), and no "no precision touchpad" status was shown. The phone bridge worked fully. Added a clear HUD status when no precision touchpad is found. Not yet compiled or run on Windows; retest on the HP laptop.
16. **Phone reconnect (2026-11-24, macOS):** after a Wi-Fi drop the phone now reconnects automatically. The fix adds a 1.5 s phone heartbeat, frees a silent link after 4 s, and makes the "busy" response retryable. Windows still needs the same fix retested.
15. **Decision (2026-11-24):** keep the Swift spike in `spike/macos` as the macOS trackpad test. The Tauri build stays Windows-only for native trackpad input, and its macOS build covers the phone bridge only.
13. **Input architecture (observed 2026-11-24):** macOS (NSTouch) and Windows (raw HID) read the trackpad differently, and the decoding may change from one laptop to another. Every input source must therefore emit one shared contact event (id, phase, normalised x/y in 0–1, timestamp), and the canvas consumes only that. Platform adapters produce it; the canvas never sees raw HID or NSTouch data.
14. **Universal phone bridge (observed 2026-11-24):** the phone path must work with both Windows and macOS desktops. It already emits the shared contact event, so it should stay a separate adapter that runs on either OS.
12. **Windows build check (user-run):** `cargo check --locked` in `spike/tauri` passes on Windows. The Tauri spike compiles against the committed lockfile. Native Precision Touchpad input is validated only on the ASUS ELAN pad (see the Windows section below).
10. **Spike findings (macOS, native round 1):**
    - Raw trackpad touch (NSTouch) into a mapped box works with no permission. Measured touch-to-draw latency: avg 3.3 ms, p95 5.6 ms.
    - Edit mode must be explicit. Drawing must not depend on cursor position.
    - Two or more fingers are a gesture and never ink. A first finger's ink that starts before the second lands is removed (within 150 ms).
    - Round 2 spike: Cmd+E toggles edit mode, edit mode turns off on focus loss, and the cursor is transparent only in edit mode.

### Windows phone-bridge spike (2026-11-22)

- User-reported manual result: scanning the desktop QR code on a phone and using it to send input to the Windows spike worked as expected. The phone tap, two-finger rollback, reconnect, Wi-Fi-loss, stop-sharing and focus-loss checks also passed on Windows. The same checks still need running on macOS.
- HUD snapshot: phone event-to-frame last 14.1 ms, average 7.8 ms, p95 15.1 ms at 60 fps. This measures the phone bridge to the next rendered frame; it is not physical touch-to-ink latency and is not directly comparable to the macOS native-touch measurement above.
- The same snapshot showed 0 Windows raw HID reports. No Windows Precision Touchpad is available for the next test, so native contact decoding and native touch-to-ink latency remain unvalidated pending access to compatible hardware.


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

### Windows Precision Touchpad implementation check (2026-11-24) — ASUS VivoBook X430FA/S430FA

- User manually confirmed the isolated `spike/windows` Tauri experiment scribbles from the built-in ASUS ELAN Precision Touchpad and behaves as expected alongside the macOS spike.
- Horizontal mapping was correct. The first Windows build inverted vertical movement. Removing the decoder's Y-axis flip corrected it, and the user confirmed the result.
- Windows now obtains touchpad HID preparsed data, reads contact IDs, tip state, and logical X/Y ranges, emits decoded contact events, and routes those events into the shared canvas. The HUD reports raw HID reports separately from decoded Windows contacts.
- The earlier recording showed 14-byte raw reports. No fresh report count or native touch-to-frame latency measurement was captured during the final confirmation. Validation on another Precision Touchpad remains open.
