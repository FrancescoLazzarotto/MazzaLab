# Architecture overview

MazzaLab is a cross-platform show control system: cue-based playback of audio,
video and lighting, plus control of external devices, all driven by an
operator pressing GO.

## Layers

```
 front ends     mazzalab-desktop (Tauri window)   browsers / tablets   OSC controllers   mazzalab-cli
                          \                              |                   |
 server              mazzalab-server: show actor + HTTP/WebSocket API + OSC UDP/TCP + outputs
                                                         |
 control                                       mazzalab-show (commands, undo, status, OSC mapping)
                                                         |
 playback                                      mazzalab-engine   <- deterministic, no I/O, injected time
                                                         |
 model                              mazzalab-core   mazzalab-project (.mazzalab files)

 outputs        mazzalab-audio   (planned: video, lighting, midi, network)
```

- **`mazzalab-core`**: the show data model (workspace, cue lists, cues, cue
  kinds, timing), validation and cue tree edits. No I/O.
- **`mazzalab-engine`**: decides *when* things happen. Callers advance time
  and receive events (`Triggered`, `ActionStarted`, `ActionCompleted`,
  `Stopped`, `Panicking`...). It also handles panic fades, reset, playhead
  navigation, run progress and edits made while the show is running.
- **`mazzalab-show`**: the controller that front ends talk to. It runs
  commands (GO, transport, playheads, edits with undo/redo), produces JSON
  snapshots and status, and executes QLab-style OSC requests. It keeps
  selection per client, so several operators can work at once.
- **`mazzalab-project`**: the `.mazzalab` file format, versioned pretty-printed
  JSON, saved atomically.
- **`mazzalab-osc`**: OSC parsing, replies and SLIP framing,
  address-compatible with the public QLab 5 OSC dictionary.
- **`mazzalab-server`**: one task (the *show actor*) owns the show and drives
  it with the clock. Every request is serialized through it. The actor
  broadcasts snapshots, status and events to WebSocket clients and
  `/update` messages to subscribed OSC clients, and feeds pluggable
  **outputs**.
- **`mazzalab-audio`**: one reader thread per playing cue decodes ahead into
  a lock-free ring buffer. The mixer runs inside the device callback, never
  allocates or locks, and resamples, applies dB levels and fades, and sums
  the voices. Natural ends are reported back to the engine.
- **`mazzalab-web`**: the operator UI (React + TypeScript), served by the
  server. It is the same UI on the desktop, in a browser and on a tablet.
- **`mazzalab-desktop`**: a Tauri app that runs the server in-process and
  opens the UI in a native window. Its File menu creates, opens and saves
  shows; opening another show restarts the server on it and points the
  window at the new server.

## Why a deterministic engine

The engine never reads the clock. A runtime loop calls
`advance_to(now)` with the audio clock (or a monotonic clock) and sleeps
until `next_deadline()` or the next external input. This gives us:

- **Simulation**: `mazzalab simulate show.mazzalab` runs a whole act in
  milliseconds and prints the timeline.
- **Testing**: every timing rule is unit tested (`crates/mazzalab-engine/tests`).
- **Redundancy**: a backup machine fed the same inputs reaches the same
  state, so main/backup failover is exact.
- **Replay**: a log of operator inputs reproduces a performance for
  debugging.

## Roadmap

The goal is a show-control system better than QLab that runs everywhere.
The detailed plan, with every missing piece and what "better" means in
measurable terms, is in [PIANO.md](../PIANO.md) (Italian). In short:

0. **Shared foundations**: audio clock driving the engine, show switching
   without restarting the server, extended show settings (format v2),
   `/live` values, operator input log and replay, media management,
   autosave and crash recovery.
1. **Audio**: device selection and low-latency drivers, output patches and
   per-cue matrices, pre-loading, sample-aligned starts, slices, pitch-
   preserving rate, full fades, ducking, LUFS, meters, CLAP/VST3 effects,
   object-based spatialisation, Mic cues.
2. **Operator UI**: drag and drop, copy/paste, renumbering, templates,
   waveform editor, timeline view, carts, show/edit modes, accessibility.
3. **Triggers, OSC and API**: hotkey/MIDI/timecode/wall-clock triggers,
   full QLab OSC coverage, permissions, a documented public API, mDNS.
4. **MIDI, timecode and network devices**: MIDI/MSC/MTC/LTC in and out,
   network cues that send, an open device schema and library.
5. **Lighting**: Art-Net, sACN, USB DMX, RDM, GDTF and Open Fixture Library
   patching, console-style light cues, live dashboard.
6. **Video**: `wgpu` renderer, hardware decoding, stages and layers, edge
   blending and warping, NDI/Syphon/Spout, remote render nodes.
7. **Collaboration and redundancy**: multi-user editing with permissions and
   a hot backup replaying the same input log.
8. **Scripting and plugins**: sandboxed script cues and a plugin API.
9. **Interoperability**: pre-show check, printable cue sheets, CSV import.
10. **Product and quality** (ongoing): signed installers, auto-update,
    headless ARM server, hardware test suite, user guide.
