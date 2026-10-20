# QLab 5 — external analysis

What we learned about QLab 5 (Figure 53) to inform CuLab's design, how we
learned it, and where CuLab can do better.

- **Target:** QLab 5.6.3 (build 5603, bundle id `com.figure53.QLab.5`,
  requires macOS 11+), downloaded from the official site on 2026-10-20.
- **Tools:** [REA](https://github.com/morluto/rea) `rea-agents` 6.7.0 CLI,
  7-Zip (DMG extraction), Python scripts over bundle resources, plus QLab's
  public OSC documentation.
- **Host:** Windows 11. REA's Mach-O, plist and native providers require
  macOS, so only REA's provider-neutral tools were usable.

## Ground rules (clean room)

QLab's EULA (§4.2) forbids reverse engineering, decompiling and
disassembling the software. We therefore:

- **did not** disassemble, decompile or trace the `QLab` executable, and did
  not extract Objective-C/Swift class metadata from it;
- **did** read what the application openly ships and documents: the bundle
  layout, `Info.plist`, the AppleScript dictionary (`QLab.sdef`), the
  credits and changelog pages, JSON device descriptions, Interface Builder
  layouts (which only describe the UI), and the public OSC documentation;
- **do not** copy any QLab file, text, artwork or code into this repository.
  This document records facts and observations in our own words. CuLab's
  formats and code are designed independently.

Compatibility with QLab's *publicly documented* OSC protocol is a deliberate
interoperability goal (see [OSC protocol](../protocols/osc.md)).

## REA runs

| Command | Result |
| --- | --- |
| `inspect-artifact QLab.app` | Inventory graph: 589 nodes, 646 occurrences, 645 edges. Next probes: `inspect_plist`, `open_binary`. |
| `inspect-plist`, `inspect-macho` | Not available: "Native macOS utilities require macOS." |
| `decode-interface-builder` | All 95 QLab nibs decoded (in batches, the tool caps at 64 archives per run): UI text, controls, target/action names. |
| `analyze`, `decompile`, `function`, `inspect-native-dispatch-metadata` | Deliberately not run (EULA §4.2). |

## Technology stack

Everything in the bundle points at a native macOS application with no
portable layer:

| Area | What ships in the bundle | Portable equivalent for CuLab |
| --- | --- | --- |
| UI | AppKit, 95 compiled nibs | Rust UI toolkit (egui/iced/Slint) or web UI |
| Video effects | Metal shader libraries plus 57 effect presets (blur, keystone, kaleidoscope, colour...) | `wgpu` with WGSL shaders |
| Video sharing | Syphon (macOS only), NDI (`libndi`) | Syphon / Spout / PipeWire + NDI SDK |
| MIDI | SnoizeMIDI (CoreMIDI) | `midir` |
| Serial / USB DMX | ORSSerialPort (credited) | `serialport` |
| iOS remote over USB | PeerTalk (credited) | Web remote over the LAN |
| Codec | Hap (with Snappy) | Hap decoder + FFmpeg |
| Updates | Sparkle | Platform installers / updater |
| Scripting | AppleScript (`NSAppleScriptEnabled`, `QLab.sdef`) | Embedded cross-platform scripting (Lua or WASM) |

Declared privacy usages: camera (Camera cue), microphone (Mic cue and LTC
input), local network (OSC, NDI, collaboration), Apple Events (Script cue).

## Documents and formats

| Extension | Purpose | Notes |
| --- | --- | --- |
| `.qlab5` | Workspace | Format not inspected (no sample workspace available). |
| `.qlab5template`, `.qlab5backup`, `.qlab5tutorial` | Templates, backups, tutorial | |
| `.qlabsettings` | Workspace settings export | `application/qlab+json` |
| `.qlablight` | Light instrument definition | `application/qlab+json` |
| `.qlabnetwork` | Network device definition | JSON, see below |
| `.cues`, `.qlab4`, `.qlab4template` | QLab 3/4 documents (import) | |
| `.gobutton`, `.gobuttonarchive`, `.gobundle` | Go Button shows (import) | XML / ZIP |
| `.qlab5key`, `.qlabactivation` | Licensing | |

URL schemes `qlab`, `qlab4` and `qlab5` are registered.

## Show model (from the public AppleScript dictionary)

**Workspace** → **cue lists** (a cue list is a special group) → **cues**,
with nested **groups**.

Common cue properties: unique id, number (unique when present), name, notes,
colour (plus an optional second colour once started), target cue / target
file, pre-wait, duration (and a temporary "live" duration), post-wait,
continue mode, flagged, auto-load, armed, skip-if-disarmed, and runtime
state (loaded, running, paused, broken, elapsed and percentage per phase).

Triggers per cue: hotkey, MIDI (with channel), timecode, wall clock, and a
"second trigger" action (panic, stop, hard stop, restart, devamp...).
"Fade and stop others" (peers / list / all) and "duck others" are cue-level
options.

| Enumeration | Values |
| --- | --- |
| Continue modes | do not continue, auto-continue, auto-follow |
| Group modes | cue list, timeline, playlist, start first and enter, start first, start random |
| Timecode formats | 24, 25, 30 non-drop, 30 drop, 23.976, 24.975, 29.97 non-drop, 29.97 drop |
| Timecode start behaviour | skip, recent minute, recent hour, lookback time, start all |
| Timecode stop behaviour | none, hard pause, hard stop |
| MIDI message types | voice, MSC, SysEx |

Cue types with their own properties: Audio, Mic, Video, Camera, Text, Light,
Fade, Network, MIDI, MIDI File, Timecode, Group, Devamp, Load, Reset, Target,
Script. The remaining control cues (Start, Stop, Pause, Go To, Arm, Disarm,
Memo, Wait) only use the common properties. Playlist groups add shuffle,
loop and crossfade. Video-type cues share stage, layer, opacity,
translation, scale, anchor, blend mode and fill style. Audio and video share
start/end time, play count, infinite loop, rate, preserve pitch, integrated
fades and slice markers.

## Remote control (from the public OSC documentation)

- OSC on **UDP and TCP port 53000**. UDP replies go to **53001**, which is
  configurable. TCP uses double-END **SLIP** framing (OSC 1.1). Plain-text
  OSC is accepted on **UDP 53535**.
- Messages can be prefixed with `/workspace/{id or name}` to address one
  workspace. Without the prefix, every workspace on that port responds.
- Replies go to `/reply{address}` with a JSON string
  `{workspace_id, address, status: ok|error|denied, data}`. Clients can
  subscribe to push updates with `/updates 1`.
- Access control: view, edit and control permissions, with passcodes.
- Property syntax: `/cue/{n}/{property}` reads it and adds an argument to
  write it. `/+` or `/-` increment and decrement. A trailing `/live` changes
  the running value without editing the show. Booleans are lenient (numbers,
  `Yes`/`no` strings and so on).
- Roughly 1,100 documented methods, most of them per-cue properties and
  audio/video settings.

## Network device descriptions

The bundle ships 75 declarative device descriptions. Each one is a JSON
document with an identifier, display name, category, message type
(`OSC`, `TEXT` or `HEX`), transports with default ports, and parameters
(menu, string, scalar, boolean, point). It also lists *conditions*: a
predicate over the parameters selects an address template such as
`/obj/{index}/azim {value}`. Categories: 34 audio, 17 video, 12 lighting,
4 communications, 3 automation, 3 general and 2 uncategorised. Five of the
files contain trailing commas, so QLab's parser is more permissive than
strict JSON.

This is one of QLab's strongest ideas: devices are supported by data, not
code. CuLab should have the same capability with its own open, versioned
schema and a community-maintained library.

## UI structure (from Interface Builder layouts)

- The workspace window combines the cue list (rows in three sizes, plus cue
  carts), the active cues and an inspector with type-specific tabs.
- Settings panes: General, Audio (output patches, maps, metering), Video
  (stages, routes, regions), Light (patch, library, Art-Net/USB), MIDI,
  Network, OSC access, Collaboration, Key map, Cue templates and Mic/Camera.
- General settings include double-GO protection, auto-load, auto-numbering
  increment, backup rotation, a start cue on open and close, and sorting of
  the active cues list.
- Collaboration lets remote machines connect with view, edit or control
  rights. Show mode can force collaborators to view-only, and the playhead
  can be locked to the selection.
- The video stage editor offers regions, edge blending with gamma,
  perspective/linear/Bézier warping, mesh subdivisions, masks and alignment
  grids.
- The fade inspector offers S-curve, linear, parametric and custom curves,
  absolute or relative mode, and 2D path fades with loop and smoothing.

## Release history

The bundled changelog lists **60 releases** from 5.0 (30 August 2022) to
5.6.3 (21 July 2026), with **291 additions, 94 changes and 507 fixes**.
"Crash" appears 56 times. Video, audio, network and collaboration are the
most frequently mentioned areas.

## Where CuLab can be better

| QLab today | CuLab direction |
| --- | --- |
| macOS only | Windows, macOS and Linux, plus headless server and browser/mobile remotes |
| Opaque workspace file | Plain-text JSON show (`.culab`), reviewable and mergeable in git |
| Playback logic only testable by running the show | Deterministic, I/O-free engine (`culab-engine`): simulate a show in milliseconds, test it in CI, replay a log |
| Main/backup redundancy built from OSC mirroring | Deterministic engine state that can be replicated exactly to a hot backup |
| AppleScript scripting | Cross-platform sandboxed scripting (Lua or WASM) |
| Device descriptions bundled with the app | Open schema plus a community device library and plugin SDK |
| Existing control surfaces speak QLab OSC | QLab-compatible OSC layer (`culab-osc`) so they work unchanged |
| Paid tiers for audio, video and lighting | To be decided (licensing not chosen yet) |

## Not examined / open questions

- The `.qlab5` workspace format: importing QLab shows would need a sample
  workspace. Any interoperability work beyond the public documentation needs
  a legal check first (EU Software Directive art. 6).
- Audio engine internals, latency and sync accuracy: these can only be
  measured on a Mac running QLab (black-box tests through OSC).
- Undocumented OSC behaviour (error cases, timing of `/update` messages).
