# MazzaLab

Cross-platform show control for theatre, live events and installations:
cue-based playback of audio, video and lighting, and control of the devices
around them, from one GO button. MazzaLab runs on Windows, macOS and Linux.

> **Status:** early development. The show model, playback engine, show file
> format, OSC parsing and a command line tool work and are tested. Audio,
> video, lighting and the desktop UI are next. See the
> [roadmap](docs/architecture/overview.md#roadmap).

## Highlights

- **Deterministic engine.** Playback logic is pure and time-injected. Shows
  can be simulated, tested in CI, replayed, and mirrored exactly to a backup
  machine.
- **Text show files.** `.mazzalab` files are pretty-printed JSON, readable and
  diffable in git.
- **Drop-in remote control.** OSC addressing follows the public QLab 5 OSC
  dictionary (port 53000), so existing control surfaces keep working.
- **Familiar cue model.** Audio, Video, Text, Mic, Camera, Light, Fade,
  Network, MIDI, MIDI File, Timecode, Group (timeline, start first, start
  first and enter, random, playlist), Start/Stop/Pause/Load/Reset/Devamp/Go
  To/Arm/Disarm/Target, Wait, Memo, Script, with pre-wait, post-wait,
  auto-continue and auto-follow.

## Try it

Requires Rust (the toolchain is pinned in `rust-toolchain.toml`).

```sh
cargo test --workspace
cargo run -p mazzalab-cli -- new my-show.mazzalab
cargo run -p mazzalab-cli -- info my-show.mazzalab
cargo run -p mazzalab-cli -- simulate examples/shows/demo.mazzalab --gos 4 --interval 15
```

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/mazzalab-core` | Show data model and validation |
| `crates/mazzalab-engine` | Deterministic cue playback engine |
| `crates/mazzalab-project` | `.mazzalab` show file format |
| `crates/mazzalab-osc` | OSC parsing, replies, SLIP framing |
| `crates/mazzalab-{audio,video,lighting,midi,network,timeline,ui,plugin}` | Planned subsystems |
| `apps/mazzalab-cli` | `mazzalab` command line tool |
| `apps/mazzalab-{desktop,server,web,mobile}` | Planned front ends |
| `docs/` | Architecture, ADRs, protocols |

## Background

MazzaLab's design was informed by a clean-room study of QLab 5's public
surface. See [QLab analysis](docs/architecture/qlab-analysis.md) and
[ADR 0001](docs/adr/0001-clean-room-qlab-analysis.md). MazzaLab is not
affiliated with Figure 53. QLab is a trademark of Figure 53, LLC.
