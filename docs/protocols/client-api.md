# Client API (HTTP + WebSocket)

`mazzalab-server` (and the desktop app, which embeds it) serves this API on
the HTTP port, 53080 by default. The web UI is an ordinary client of it.
Times are in seconds, levels in dB. There is no authentication yet: anyone
who can reach the port can drive the show, as with OSC.

## WebSocket: `/ws`

JSON text frames in both directions.

### Server to client

| `type` | Payload | Sent |
| --- | --- | --- |
| `snapshot` | `snapshot`: revision, `dirty`, `recovered` (the unsaved changes come from an autosave), `can_undo`, `can_redo`, the whole `workspace` | on connect and after every change to the show |
| `status` | `status`: `now`, `panicking`, `playheads`, `running` (cue, phase, elapsed, length, paused) | on connect, 20 times a second while cues run, once a second otherwise |
| `events` | `events`: engine events in order (`triggered`, `action_started`, `action_completed`, `stopped`, `paused`, `resumed`, `panicking`, `playhead_moved`, `skipped`) | as they happen |
| `meters` | `peaks`: peak level in dBFS of each audio output since the previous message | with `status`, while there is signal (and once more when it ends) |
| `audio` | `audio`: `running`, `device_id`, `device_name`, `sample_rate`, `channels`, `buffer_size`, `fallback`, `error` | on connect and when the audio output changes |
| `notice` | `notice`: `level` (`info`, `warning`, `error`) and `message` | when the operator should know something: unreadable file, missing network patch, device lost |
| `dmx` | `levels`: the DMX slots that are lit, each as `[universe, address, value]` with values from 1 to 255 | on connect and whenever they change, when the server drives lights |
| `midi_in` | `event`: a channel message that arrived on a MIDI input, as `kind` (`note_on`, `note_off`, `control_change`, `program_change`), `channel`, `number`, `value` | as it arrives; lets a client learn a trigger from the controller |
| `result` | `id` of the request, `error` when it failed | in answer to each client message |

### Client to server

```json
{ "type": "command", "id": 7, "command": { "command": "go" } }
{ "type": "save", "id": 8 }
{ "type": "audition", "id": 9, "cue": "…" }
```

`audition` plays an audio cue by itself on the audition output, without
running it in the show: no cue starts, the playhead stays, nothing is
edited. Without a `cue` it stops what is being auditioned. The `audio`
message carries the cue being auditioned in `audition` and is sent again
when the audition ends.

`id` is chosen by the client and comes back in the `result`.

## Commands

The same JSON is accepted by `POST /api/command`.

Transport: `go` (`list?`), `start` / `stop` / `pause` / `resume` (`cue`),
`load` (`cue`, `time`), `stop_all`, `pause_all`, `resume_all`, `panic`,
`reset`, `set_playhead` (`cue`, `list?`), `playhead_next`,
`playhead_previous`.

`load` makes a cue's next start pick up `time` seconds into it, counted
from the beginning of its pre-wait; the status lists loaded cues in
`loaded` until they start. A Load cue does the same to its target. Events
gain `loaded` (`cue`, `time`), `unloaded` and `devamped`.

Editing (each is one undo step): `update_cue` (`cue`), `insert_cue`
(`container`, `index?`, `cue`), `delete_cue` (`cue`), `move_cue` (`cue`,
`container`, `index?`), `add_cue_list` (`name`), `rename_cue_list`,
`delete_cue_list`, `rename_workspace` (`name`), `update_settings`
(`settings`), `undo`, `redo`, and:

- `set_numbers` — `numbers`: a list of `[cue id, number or null]`. All the
  numbers are cleared first and then set, so cues can swap or shift numbers
  in one go.
- `batch` — `commands`: a list of editing commands applied as one. Either
  all succeed, as a single undo step, or the show is left untouched.

A `container` is `{ "kind": "list", "id": … }` or
`{ "kind": "group", "id": … }`. For `move_cue`, `index` is the position in
the target container once the cue has been taken out of its old place.

## HTTP

| Method and path | Purpose |
| --- | --- |
| `GET /api/snapshot` | the current snapshot |
| `GET /api/status` | the current status |
| `POST /api/command` | run a command (JSON body) |
| `POST /api/save` | save the show to its file |
| `POST /api/midi/in` | feeds the show a MIDI message (`kind`, `channel`, `number`, `value`) as if it had arrived on an input, to try triggers without the controller |
| `GET /api/audio` | `status` (as in the `audio` message, or `null` without an audio backend) and `devices`: every output device with `id`, `name`, `host`, `is_default`, `max_channels`, `sample_rates`, `default_sample_rate` |
| `GET /api/midi` | the MIDI ports of the server's machine: `outputs` and `inputs`, by name |
| `GET /api/waveform?cue=ID&buckets=N` | the outline of an audio cue's file: `duration`, `channels`, `sample_rate`, and `peaks`, a minimum and a maximum (-127…127) for each of N slices. Cached per file |
| `GET /api/media` | the files in the show's `media` folder |
| `POST /api/media?name=FILE` | stores the request body as a file in the `media` folder next to the show file and answers `{ "file": "media/FILE" }`, ready to be used as a cue's `file`. Uploading the same bytes again reuses the file; different bytes under the same name get `FILE (2)` |

Errors come back as `{ "error": "…" }` with a 4xx or 5xx status. Media
endpoints answer 409 for a show that has no file yet, since media lives
next to the show file.

## Audio settings and routing

`workspace.settings.audio` chooses the output: `device` (an id from
`/api/audio`, absent for the system default), `sample_rate`, `buffer_size`
(frames) and `master_db`. Changing device, rate or buffer reopens the
output while the server keeps running; a missing device falls back to the
default one and says so (`fallback`).

An audio cue's `matrix` is `matrix[file channel][output]` in dB, with
`null` for silence. Without a matrix, mono files go to outputs 1 and 2 and
other files go channel for channel. `fade_in` and `fade_out` are built into
the cue; `level_db` and `matrix` can be changed while the cue plays.

## Network patches

`workspace.settings.network_patches` lists the devices Network cues send
to: `name`, `host`, `port` and `protocol` (`osc_udp`, `osc_tcp` with SLIP
framing, `plain_udp`, `plain_tcp`). A Network cue names a patch, or uses
the first one. Each non-empty line of its `message` is one message:

- OSC: `/address arg arg …` — whole numbers are integers, numbers with a
  decimal point floats, `true` / `false` booleans, anything else (or
  anything in double quotes) a string;
- plain: the text as bytes, with `\n`, `\r`, `\t`, `\\` and `\xHH` escapes.

## MIDI patches

`workspace.settings.midi_patches` lists the outputs MIDI cues send to:
`name` and `port` (a name from `/api/midi`). A MIDI cue names a patch, or
uses the first one. Its `message` is one of:

| `kind` | Fields |
| --- | --- |
| `note_on`, `note_off` | `channel` (1–16), `note`, `velocity` |
| `control_change` | `channel`, `controller`, `value` |
| `program_change` | `channel`, `program` |
| `msc` | `device_id`, `command_format`, `command`, optional `q_number` and `q_list` (digits and dots) |
| `sys_ex` | `data`: the bytes between `F0` and `F7`, each below `0x80` |

## Triggers

A cue's `triggers.hotkey` names a key or combination that starts it from
any operator screen: `Q`, `F5`, `Num3`, `Ctrl+Shift+1`. Modifiers come in
the order Ctrl, Alt, Shift; letters and digits are named after the physical
key. The keys the operator screen uses itself (Space, Esc, arrows, Enter,
Delete, V and the Ctrl shortcuts) cannot be assigned.

A cue's `triggers.midi` is a message that starts it when it arrives on any
MIDI input: `kind`, `number` (note, controller or program), and optionally
the `channel` (1–16) and the `value` it must carry. A note on with
velocity 0 counts as a note off. `workspace.settings.midi_controls` ties
`go`, `stop_all`, `panic`, `playhead_next` and `playhead_previous` to
messages in the same form.

## Slices

An audio cue's `slices` are markers, each a `time` in the file and the
`play_count` of the slice that ends there (0 loops it until devamped);
`last_slice_play_count` is the count of the slice after the last marker.
Markers outside the region between `start_time` and `end_time` are
ignored. While a cue has slices its own `play_count` and `infinite_loop`
do not apply, and a cue with a slice that loops has no fixed length.

A Devamp cue aimed at a sliced cue lets the pass in progress finish and
moves on to the next slice, on the sample; in the last slice, or on a
plain loop, it ends the cue at the end of that pass. A devamp right at
the top of a pass plays that whole pass.

## Carts

A cue list with `cart: true` is a cart: its cues are called up in no
particular order and clients show them as a grid of buttons. `add_cue_list`
takes `cart` to make one.

## Audition

`workspace.settings.audio.audition_device` names the device cues are
auditioned on (headphones, a desk monitor); absent, it is the system
default output. An auditioned cue is heard with its trims, level and fades
on the first outputs of that device, once through, whatever its routing
and loops. A panic stops it too.

## Safety copies

Next to a show file, the `backups` folder holds:

- `NAME.autosave.mazzalab` — unsaved changes, written a few seconds after
  they are made and removed on save. If it is newer than the show file when
  the show is opened, the desktop app opens it instead and marks the show
  as recovered; the headless server does so with `--recover`.
- `NAME.1.mazzalab` … `NAME.10.mazzalab` — the versions replaced by the last
  saves, 1 being the most recent.

## Lights

`workspace.settings.dmx_outputs` says where each universe of the show
goes: `universe` (counted from 1, as cues name it), `protocol` (`art_net`
or `sacn`), `host` (an address, optionally with a port; empty broadcasts
Art-Net and uses the multicast group for sACN) and `net_universe`, the
number on the wire (Art-Net counts from 0, sACN from 1).

`workspace.settings.fixtures` is the rig: each fixture has a `name`, a
`universe` (1 if absent), the first `address` it takes and `parameters`,
what its channels do in order from that address (`["intensity"]`, a
dimmer, if absent; `["red", "green", "blue"]` for an RGB LED).
`workspace.settings.fixture_groups` gives `members` (names of fixtures or
of other groups) a single `name`. Names are matched ignoring case and
extra spaces.

A Light cue holds `commands`, one per line, and a `duration` for the fade.
A command is `selection @ value`:

- the selection joins terms with `+` (or `and`); a term is an address from
  1 to 512, `2/7` for universe 2, a range with `thru` (`1 thru 10`), `all`
  for a whole universe, or the name of a fixture or a group;
- a fixture or group on its own sets its `intensity` (or, if it has none,
  every channel); `name.parameter` sets one parameter, on those fixtures of
  a group that have it (`front wash.intensity`, `spot.pan`);
- the value is a percentage, `full` or `out`, or a colour as `#rrggbb` for
  fixtures and groups: red, green and blue channels take it as it is,
  cyan, magenta and yellow as what they take away.

A cue takes over the slots it names and leaves the others where they are,
so looks build on each other. Lines that cannot be read are reported as a
notice and the rest of the cue still runs. Stopping a cue, or a panic, does
not change the lights: a cue with `all @ out` is a blackout.
