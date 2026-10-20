# MazzaLab

*[Leggi in italiano](README.it.md)*

Cross-platform show control for theatre, live events and installations:
cue-based playback of audio, video and lighting, and control of the devices
around them, from one GO button. MazzaLab runs on Windows, macOS and Linux.

> **Status:** early development. Working and tested today: the show model,
> the playback engine, `.mazzalab` show files, audio playback, OSC remote
> control, the show server, the web UI (desktop, browser, tablet) and the
> desktop app. Video, lighting and devices are next. See the
> [roadmap](docs/architecture/overview.md#roadmap).

## Highlights

- **Runs everywhere.** One Rust core; a native desktop app (Tauri) on
  Windows, macOS and Linux; the same UI in any browser on the network, so
  tablets and phones become remotes with nothing to install.
- **Deterministic engine.** Playback logic is pure and time-injected. Shows
  can be simulated, tested in CI, replayed, and mirrored exactly to a backup
  machine.
- **Professional audio path.** Streaming decoders (WAV, AIFF, FLAC, MP3,
  AAC, ALAC, Vorbis...), sample-accurate trims and loops, a lock-free
  real-time mixer with dB fades (linear or S-curve), de-clicked stops and
  panic fades.
- **Drop-in remote control.** OSC over UDP and TCP follows the public QLab 5
  OSC dictionary (port 53000), so existing control surfaces keep working.
- **Text show files.** `.mazzalab` files are pretty-printed JSON, readable and
  diffable in git. Every edit is undoable.
- **Familiar cue model.** Audio, Video, Text, Mic, Camera, Light, Fade,
  Network, MIDI, MIDI File, Timecode, Group (timeline, start first, start
  first and enter, random, playlist), Start/Stop/Pause/Load/Reset/Devamp/Go
  To/Arm/Disarm/Target, Wait, Memo, Script, with pre-wait, post-wait,
  auto-continue and auto-follow.
- **Italian and English** UI, switchable at any time.

## Run it

Requirements: Rust (pinned in `rust-toolchain.toml`) and Node.js 22+ for the
web UI. On Linux also install the ALSA and WebKitGTK development packages
(see `.github/workflows/ci.yml`).

```sh
# Build the web UI once
cd apps/mazzalab-web && npm ci && npm run build && cd ../..

# Desktop app (server + window); the show file is created on first save
cargo run -p mazzalab-desktop -- my-show.mazzalab

# ...or a headless server, e.g. on a show machine without a screen
cargo run -p mazzalab-server -- my-show.mazzalab --ui-dir apps/mazzalab-web/dist
```

Then open `http://<machine-ip>:53080` on any tablet or laptop on the same
network. Send OSC to port 53000 (for example `/go`, `/cue/1/start`,
`/cue/selected/name`).

Keyboard: **Space** GO · **Esc** panic · **↑/↓** select · **Enter** set
playhead · **Ctrl+Z / Ctrl+Shift+Z** undo/redo · **Ctrl+S** save.

### Development

```sh
cargo test --workspace                       # Rust tests
cd apps/mazzalab-web && npm test             # UI tests
npm run dev                                  # UI with hot reload, proxied to a server on :53080
cargo run -p mazzalab-cli -- simulate examples/shows/demo.mazzalab --gos 4 --interval 15
```

Installers: from `apps/mazzalab-desktop`, run
`npx @tauri-apps/cli build --config tauri.bundle.conf.json` after building
the web UI.

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/mazzalab-core` | Show data model, validation, cue tree edits |
| `crates/mazzalab-engine` | Deterministic cue playback engine |
| `crates/mazzalab-show` | Show controller: commands, undo/redo, status, OSC handling |
| `crates/mazzalab-project` | `.mazzalab` show file format |
| `crates/mazzalab-osc` | OSC parsing, replies, SLIP framing |
| `crates/mazzalab-audio` | Decoding, real-time mixer, device output |
| `crates/mazzalab-{video,lighting,midi,network,timeline,ui,plugin}` | Planned subsystems |
| `apps/mazzalab-server` | Real-time show server: OSC, HTTP/WebSocket API, outputs |
| `apps/mazzalab-web` | Operator UI (React + TypeScript) |
| `apps/mazzalab-desktop` | Desktop app (Tauri) embedding the server |
| `apps/mazzalab-cli` | `mazzalab` command line tool |
| `docs/` | Architecture, ADRs, protocols |

## Background

MazzaLab's design was informed by a clean-room study of QLab 5's public
surface. See [QLab analysis](docs/architecture/qlab-analysis.md) and
[ADR 0001](docs/adr/0001-clean-room-qlab-analysis.md). MazzaLab is not
affiliated with Figure 53. QLab is a trademark of Figure 53, LLC.

## License

Proprietary: all rights reserved. See [LICENSE](LICENSE).
