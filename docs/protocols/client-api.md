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
| `result` | `id` of the request, `error` when it failed | in answer to each client message |

### Client to server

```json
{ "type": "command", "id": 7, "command": { "command": "go" } }
{ "type": "save", "id": 8 }
```

`id` is chosen by the client and comes back in the `result`.

## Commands

The same JSON is accepted by `POST /api/command`.

Transport: `go` (`list?`), `start` / `stop` / `pause` / `resume` (`cue`),
`stop_all`, `pause_all`, `resume_all`, `panic`, `reset`, `set_playhead`
(`cue`, `list?`), `playhead_next`, `playhead_previous`.

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

## Safety copies

Next to a show file, the `backups` folder holds:

- `NAME.autosave.mazzalab` — unsaved changes, written a few seconds after
  they are made and removed on save. If it is newer than the show file when
  the show is opened, the desktop app opens it instead and marks the show
  as recovered; the headless server does so with `--recover`.
- `NAME.1.mazzalab` … `NAME.10.mazzalab` — the versions replaced by the last
  saves, 1 being the most recent.
