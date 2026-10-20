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
  opens the UI in a native window.

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

1. **Foundation** (done): model, engine, show file, OSC parsing, CLI, CI.
2. **Server and remote** (done): show actor, OSC UDP/TCP with replies and
   `/updates`, HTTP/WebSocket API, web UI served to any device.
3. **Audio** (first version done): streaming decode, real-time mixer,
   levels, fades, panic, natural-end reporting. Next: output patches and
   channel routing, device selection, pre-loading for zero-latency starts,
   the audio clock driving the engine, and waveform display.
4. **Operator UI** (first version done): GO/standby, cue list, active cues,
   inspector, add/delete, undo/redo, save, IT/EN. Next: drag and drop,
   copy/paste, cue templates, file picker with media browser, multiple
   cue lists, cue carts, waveform trims.
5. **Desktop app** (first version done): Tauri shell with the embedded
   server. Next: open/save dialogs, unsaved-changes prompt, recent shows,
   installers for every platform, auto-update.
6. **Lighting**: Art-Net and sACN output, fixture patch, light cues with
   fades, USB DMX.
7. **Devices**: MIDI/MSC/timecode input and output, network cues with an
   open device description schema and library.
8. **Video**: `wgpu` renderer, stages and outputs, layers and opacity, edge
   blending and warping, NDI/Syphon/Spout.
9. **Collaboration and redundancy**: multi-user editing with permissions,
   a hot backup that follows the main engine.
10. **Scripting and plugins**: sandboxed scripts and a plugin API for new
    cue types and devices.
