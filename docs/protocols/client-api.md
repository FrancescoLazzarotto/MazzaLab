# Client API (HTTP + WebSocket)

`mazzalab-server` (and the desktop app, which embeds it) serves this API on
the HTTP port, 53080 by default. The web UI is an ordinary client of it.
Times are in seconds, levels in dB. A show without passcodes is open to
anyone who can reach the port; see [Passcodes](#passcodes).

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
| `stage` | what the stages show: `now` (the server's clock, seconds), `stages` (the names the show has), `items` (the Video and Text cues that are up, lowest layer first) | on connect and whenever it changes |
| `timecode` | `timecode` (`HH:MM:SS:FF`) and `format` of the timecode coming in | as it arrives, every two frames for MTC |
| `midi_in` | `event`: a channel message that arrived on a MIDI input, as `kind` (`note_on`, `note_off`, `control_change`, `program_change`), `channel`, `number`, `value` | as it arrives; lets a client learn a trigger from the controller |
| `result` | `id` of the request, `error` when it failed | in answer to each client message |

### Client to server

```json
{ "type": "command", "id": 7, "command": { "command": "go" } }
{ "type": "save", "id": 8 }
{ "type": "audition", "id": 9, "cue": "…" }
{ "type": "stage_ended", "id": 10, "cue": "…" }
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
| `GET /api/cue-media?cue=` | the media file of a cue, with range requests, for stages to play |
| `GET /api/screens` | the displays of the machine (`name`, `width`, `height`, `x`, `y` in physical pixels, `main` for the one the operator's window is on) and whether stages are `shown` on them; `available` is false on a server without displays (headless) |
| `POST /api/screens` | `{ "shown": true }` puts stages on the displays they name, full screen, `false` takes them down; 404 without displays |
| `POST /api/timecode/in` | feeds the show `timecode` (`HH:MM:SS:FF`) at a `format` (`fps25`...) as if it had come in, to try timecode triggers without a source |
| `POST /api/midi/in` | feeds the show a MIDI message (`kind`, `channel`, `number`, `value`) as if it had arrived on an input, to try triggers without the controller |
| `GET /api/audio` | `status` (as in the `audio` message, or `null` without an audio backend) and `devices`: every output device with `id`, `name`, `host`, `is_default`, `max_channels`, `sample_rates`, `default_sample_rate`; and `inputs`, every input device with `id`, `name`, `host`, `is_default`, `max_channels` |
| `GET /api/midi` | the MIDI ports of the server's machine: `outputs` and `inputs`, by name |
| `GET /api/waveform?cue=ID&buckets=N` | the outline of an audio cue's file: `duration`, `channels`, `sample_rate`, and `peaks`, a minimum and a maximum (-127…127) for each of N slices. Cached per file |
| `GET /api/media` | the files in the show's `media` folder |
| `POST /api/media?name=FILE` | stores the request body as a file in the `media` folder next to the show file and answers `{ "file": "media/FILE" }`, ready to be used as a cue's `file`. Uploading the same bytes again reuses the file; different bytes under the same name get `FILE (2)` |

Errors come back as `{ "error": "…" }` with a 4xx or 5xx status. Media
endpoints answer 409 for a show that has no file yet, since media lives
next to the show file.

## Passcodes

The show's `settings.passcodes` is a list of `{ "code", "name"?, "view",
"control", "edit" }`. While it is empty everyone may do everything. Once it
has entries, a client on another machine has to give one: as
`Authorization: Bearer <code>` (or `X-MazzaLab-Passcode`) on HTTP requests,
and as `?passcode=<code>` on the `/ws` URL (and on URLs a browser cannot add
a header to, such as `cue-media`). Clients on the server's own machine
always have full access.

- `view`: read the show, its status and the WebSocket. `control` and `edit`
  include it.
- `control`: run the show (GO, start, stop, pause, playheads, panic, MIDI and
  timecode in).
- `edit`: change the show and its settings, save, open and create shows. Only
  clients with `edit` see `passcodes` in snapshots.

`GET /api/access` never needs a passcode. It answers `{ "required", "view",
"control", "edit" }` for the passcode that came with the request, so a client
can tell whether to ask the operator for one. A missing or wrong passcode gets
401, a passcode that does not allow the request 403. On the WebSocket, a
command the passcode does not allow comes back as a `result` with the error
`not allowed with this passcode`; the upgrade itself fails with 401 without
`view`.

The web UI asks for a passcode when `required` is true and `view` is false,
keeps it per server in the browser, and accepts `&passcode=` on its own URL
(stage and show-only pages included), taking it off the address bar.

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
framing, `plain_udp`, `plain_tcp`, `http_get`, `http_post`, `websocket`). For
the last three `host` is the URL (`http://10.0.0.5:8080/api`, `ws://...`; a
URL without a scheme is taken as `http://` or `ws://`) and `port` is not
used. A Network cue names a patch, or uses the first one. Each non-empty
line of its `message` is one message:

- OSC: `/address arg arg …` — whole numbers are integers, numbers with a
  decimal point floats, `true` / `false` booleans, anything else (or
  anything in double quotes) a string;
- plain: the text as bytes, with `\n`, `\r`, `\t`, `\\` and `\xHH` escapes;
- `http_get`: a path (and query) added to the URL: `/scene/2?fade=1`;
- `http_post`: a path, then the body: `/scene {"id": 2}` (a body starting with
  `{` or `[` goes as `application/json`, else `text/plain`);
- `websocket`: the text of one message, sent on a connection that is kept
  open.

HTTP is plain `http://` only: there is no TLS. A status of 400 or more is
reported as a notice.

**Fading values.** In an OSC message, an argument written `from>to`
(`/fader/1 0.0>1.0`, `/level 0>100`) moves between the two numbers over the
cue's `duration`, linearly, at about thirty messages a second, ending exactly on
`to`; whole numbers on both ends stay whole. With no duration the arguments
send `to` at once. Stopping the cue stops the fade where it got to.

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

A cue's `triggers.osc` is an OSC address that starts it when a message to
it arrives on the show's OSC port (UDP or TCP); `*` and `?` in it match
any characters, so `/scene/*/go` answers `/scene/12/go`. A message that a
cue takes is not refused even when it is not one of the show's own
addresses.

A cue's `triggers.wall_clock` starts it at a time of day on the show
machine's clock: `time` (`HH:MM` or `HH:MM:SS`) and `days` (`monday` …
`sunday`; none means every day). It fires as the clock goes past the
time; a machine that was asleep then, or a clock set back, starts
nothing.

## Stages

Video and Text cues go up on stages. `workspace.settings.stages` names
them (`name`, and `width` and `height` in pixels, 1920 by 1080 when
absent); without any there is one called `Main`, and a cue that names no
`stage` goes to the first. A stage is any client that renders the
`stage` message: the web UI does so at `/?stage=Name` (add `&monitor`
for a silent copy), so a projector can be a browser on any machine of
the network. The message's `stages` lists them with their sizes. In the
desktop app a stage with a `screen` (a display's `name` from
`/api/screens`) also goes full screen on that display while stages are
shown, from the Video menu (Ctrl+Shift+D) or `POST /api/screens`.

A stage is drawn in its own pixels and scaled to the screen that shows
it. A cue's `geometry` places it there: `fit` (`fit`, `fill`, `stretch`
or `native`: how a picture fills the stage before it is moved), `x` and
`y` (from the stage's centre to the picture's, right and down positive),
`scale_x` and `scale_y`, `rotation` (degrees, clockwise) and
`crop_top`, `crop_right`, `crop_bottom`, `crop_left` (fractions of the
picture). Absent values are at their defaults. A Text cue also has an
`opacity` and a `style`: `font`, `size` (stage pixels), `color`
(`#rrggbb`), `align` (`left`, `center`, `right`), `bold`, `italic`,
`line_height`, `width` (where lines wrap) and `background` (a box behind
the text, `#rrggbb` or `#rrggbbaa`).

Each item has the `cue`, its `stage`, `layer`, `opacity` and `geometry`
(the values it has or is going to, with `fade`: `from`, `from_geometry`,
`started`, `duration` and `curve` while it fades), `started` and
`offset` (it was `offset` seconds into its action at `started`, on the
server's clock), `paused`, and its `kind`: `video` (`start_time`,
`end_time`, `looped`, `hold`, `rate`) or `text` (`text`, `style`). A
stage lines its clock up with `now`, plays the cue's file from
`/api/cue-media`, and sends `stage_ended` when a file it plays reaches its
end: a cue whose length the show did not know then completes. A Fade cue
with an `opacity` or a `geometry` fades the Video or Text cue it targets
there along its `curve`, from wherever it is; a fade does not change the
target's `fit`.

## Timecode

A Timecode cue sends timecode from its `start` (`HH:MM:SS:FF`) at its
`format` (`fps24`, `fps25`, `fps30_non_drop`, `fps30_drop`, `fps23_976`,
`fps29_97_non_drop`, `fps29_97_drop`) for its `duration`, or until it is
stopped. With `signal: "mtc"` it goes to the MIDI `patch` (the first when
absent) as a full frame and then quarter frames; with `signal: "ltc"` it
comes out of the audio device's `output` (from 1) at `level_db`. Pausing
holds it, resuming locates the receiver again and runs on. A cue loaded
to a time starts that much further on.

MTC arriving on any MIDI input is followed, and so is LTC on the audio
input `workspace.settings.ltc_input` names (`device`, an input's `id`
from `/api/audio`, absent for the default input, and `channel` from 1);
its rate is read from the signal itself. A cue's `triggers.timecode`
starts it when incoming timecode, running forward, passes that frame; a
jump (a locate, a source looping back) starts nothing, so a show can be
rewound over its triggers.

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

## Hot backup

A second server can follow the main one and take over if it goes silent.
`mazzalab-server --follow http://main:53080` (with `?passcode=` on the URL if
the main server asks for one) runs a show file as the backup.

The main server streams its input record on `GET /api/follow`, a WebSocket
with one JSON line in each text frame (it needs `view` access, like `/ws`):

1. the record's header, then every entry recorded so far (the format of
   `logs/` files, see `mazzalab replay`), then each new entry as it is
   written;
2. a tick, `{"tick": <engine time in ns>, "hash": <u64>}`, every 250 ms, with a
   hash of the engine's state.

The engine is deterministic: a backup that applies the same inputs at the same
instants is in the same state, cue for cue. The backup compares its own hash
with each tick's, and takes over after 1 s without a tick: it starts
answering its own triggers and the operators' commands, and sends a warning
notice (`… has taken over`) over its WebSocket.

While it follows, `GET /api/access` answers `"following": true` and every
client has view access only; clients re-ask on every notice and on
reconnecting, so that a screen watching the backup gains control once it
takes over.

Limits:

- cues already running when the backup joins are not heard there until the
  next ones start;
- the backup records nothing until it takes over;
- the media is not copied: both machines need the same files at the same
  paths (the show's `media` folder, and any absolute paths).

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
number on the wire (Art-Net counts from 0, sACN from 1). With `sync: true`
an output's frames are released together: an ArtSync packet follows the
frames (Art-Net), or the sACN data packets name sync universe 63999 and an
E1.31 sync packet for it follows once every output's frame is out.

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
