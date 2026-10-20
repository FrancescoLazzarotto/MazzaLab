# Architecture overview

CuLab is a cross-platform show control system: cue-based playback of audio,
video and lighting, plus control of external devices, all driven by an
operator pressing GO.

## Layers

```
 apps/        culab-desktop   culab-server   culab-web   culab-mobile   culab-cli
                    \             |              |            |           /
 control           culab-osc   (web API)   (collaboration)   ...
                         \         |         /
 playback                   culab-engine   <- deterministic, no I/O, injected time
                         /        |         \
 model           culab-core   culab-project (.culab files)
                         \        |         /
 outputs         culab-audio  culab-video  culab-lighting  culab-midi  culab-network
```

- **`culab-core`**: the show data model (workspace, cue lists, cues, cue
  kinds, timing) and validation. No I/O.
- **`culab-engine`**: decides *when* things happen. Callers advance time
  and receive events (`Triggered`, `ActionStarted`, `ActionCompleted`,
  `Stopped`...). Output backends subscribe to these events and report the
  end of open-ended media.
- **`culab-project`**: the `.culab` file format, versioned pretty-printed
  JSON, saved atomically.
- **`culab-osc`**: OSC remote control, address-compatible with the public
  QLab 5 OSC dictionary.
- **Output crates**: turn engine events into sound, pixels, DMX and
  messages. Each one runs its own real-time thread and never blocks the
  engine.

## Why a deterministic engine

The engine never reads the clock. A runtime loop calls
`advance_to(now)` with the audio clock (or a monotonic clock) and sleeps
until `next_deadline()` or the next external input. This gives us:

- **Simulation**: `culab simulate show.culab` runs a whole act in
  milliseconds and prints the timeline.
- **Testing**: every timing rule is unit tested (`crates/culab-engine/tests`).
- **Redundancy**: a backup machine fed the same inputs reaches the same
  state, so main/backup failover is exact.
- **Replay**: a log of operator inputs reproduces a performance for
  debugging.

## Roadmap

1. **Foundation** (done): model, engine, show file, OSC parsing, CLI, CI.
2. **Server and remote**: `culab-server` with an OSC UDP/TCP listener wired
   to the engine, `/updates` push, and a web remote served from the same
   process.
3. **Audio**: `culab-audio` on `cpal` + `symphonia`; output patches,
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
