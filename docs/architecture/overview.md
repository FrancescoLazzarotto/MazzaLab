# Architecture overview

MazzaLab is a cross-platform show control system: cue-based playback of audio,
video and lighting, plus control of external devices, all driven by an
operator pressing GO.

## Layers

```
 apps/        mazzalab-desktop   mazzalab-server   mazzalab-web   mazzalab-mobile   mazzalab-cli
                    \             |              |            |           /
 control           mazzalab-osc   (web API)   (collaboration)   ...
                         \         |         /
 playback                   mazzalab-engine   <- deterministic, no I/O, injected time
                         /        |         \
 model           mazzalab-core   mazzalab-project (.mazzalab files)
                         \        |         /
 outputs         mazzalab-audio  mazzalab-video  mazzalab-lighting  mazzalab-midi  mazzalab-network
```

- **`mazzalab-core`**: the show data model (workspace, cue lists, cues, cue
  kinds, timing) and validation. No I/O.
- **`mazzalab-engine`**: decides *when* things happen. Callers advance time
  and receive events (`Triggered`, `ActionStarted`, `ActionCompleted`,
  `Stopped`...). Output backends subscribe to these events and report the
  end of open-ended media.
- **`mazzalab-project`**: the `.mazzalab` file format, versioned pretty-printed
  JSON, saved atomically.
- **`mazzalab-osc`**: OSC remote control, address-compatible with the public
  QLab 5 OSC dictionary.
- **Output crates**: turn engine events into sound, pixels, DMX and
  messages. Each one runs its own real-time thread and never blocks the
  engine.

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
2. **Server and remote**: `mazzalab-server` with an OSC UDP/TCP listener wired
   to the engine, `/updates` push, and a web remote served from the same
   process.
3. **Audio**: `mazzalab-audio` on `cpal` + `symphonia`; output patches,
   per-cue levels, fades, sample-accurate start, the audio clock driving the
   engine.
4. **Desktop UI**: cue list, inspector, active cues, GO and panic; editing
   through undoable commands.
5. **Video**: `wgpu` renderer, stages and outputs, layers and opacity, edge
   blending and warping, NDI/Syphon/Spout.
6. **Lighting and devices**: Art-Net/sACN/USB DMX, MIDI/MSC/timecode,
   open network device description schema and library.
7. **Collaboration and redundancy**: multi-user editing, a hot backup that
   follows the main engine.
8. **Scripting and plugins**: sandboxed scripts and a plugin API for new
   cue types and devices.
