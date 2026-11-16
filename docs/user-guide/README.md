# MazzaLab user guide

*[Leggi in italiano](README.it.md)*

For the stage manager, the sound and lighting operator and the technician who
sets the show up. It describes what the program does today; things still on
the roadmap are not described. For the technical side see the
[client API](../protocols/client-api.md), the [OSC dictionary](../protocols/osc.md)
and the [plan](../PIANO.md).

New here? The [tutorial](tutorial.md) builds a first act step by step.

## Contents

1. [Install and run](#1-install-and-run)
2. [The show file and the media folder](#2-the-show-file-and-the-media-folder)
3. [The screen](#3-the-screen)
4. [The cue list and the cue types](#4-the-cue-list-and-the-cue-types)
5. [The inspector](#5-the-inspector)
6. [GO, standby and the transport](#6-go-standby-and-the-transport)
7. [Carts](#7-carts)
8. [Audio](#8-audio)
9. [Lights](#9-lights)
10. [Video and stages](#10-video-and-stages)
11. [Triggers: keys, MIDI, OSC, timecode, time of day](#11-triggers)
12. [Network and MIDI cues, timecode out](#12-network-and-midi-cues-timecode-out)
13. [Tablets: the show view, passcodes](#13-tablets)
14. [Save, open, backups and recovery](#14-save-open-backups-and-recovery)
15. [Keyboard shortcuts](#15-keyboard-shortcuts)
16. [Appearance and language](#16-appearance-and-language)

## 1. Install and run

MazzaLab runs on Windows, macOS and Linux. There are two ways to run it, and
both serve the same operator screen.

- **The desktop app** runs the show server inside a window. Use it on the
  machine that plays the show.
- **The headless server** runs without a screen. You open the operator screen
  from a browser on any machine of the network. Use it for a rack machine or
  a media server.

Both are built from source today (the README has the commands: Rust and
Node.js are needed, and the web screen is built once with `npm run build`).
Installers can be produced with Tauri; see the README.

```
cargo run -p mazzalab-desktop -- my-show.mazzalab
cargo run -p mazzalab-server  -- my-show.mazzalab --ui-dir apps/mazzalab-web/dist
```

If the show file does not exist yet it is created at the first save.

The server listens on port **53080** (browser, tablet, stages) and **53000**
(OSC). Open `http://<machine address>:53080` in a browser on the same network
to get the operator screen. Useful options of the headless server:

| Option | Does |
| --- | --- |
| `--http-port`, `--osc-port` | change the ports |
| `--bind` | the address to listen on (all by default) |
| `--no-audio` | start without opening an audio device |
| `--recover` | open the autosave if it is newer than the file (see [section 14](#14-save-open-backups-and-recovery)) |
| `--passcode-local` | ask for passcodes on the machine itself too |

The screen can also be installed as an app from the browser (it keeps its own
files, so it reloads even if the network drops for a moment; the show itself
needs the server).

## 2. The show file and the media folder

A show is one file with the extension `.mazzalab`. It is text (JSON), so it
can be kept in version control and compared. It holds the cue lists, the cues
and the show's settings: audio device, network and MIDI patches, DMX outputs,
fixtures, stages, passcodes.

Media (sound and video files) live in a folder called `media` **next to the
show file**. Cues name their files relative to the show (`media/Rain.wav`), so
the show folder can be copied to another machine as it is.

To bring files in:

- drop audio files on the cue list (each becomes an Audio cue);
- in the **Media** panel (side panel), press the add button and pick files.
  It works from a tablet too, and several files can be picked at once;
- copy them into the `media` folder yourself.

The Media panel lists what is in the folder, with its size and how many cues
play each file. **Unused** in the filter shows files no cue plays. Drag a file
from the panel onto the cue list to make an Audio or Video cue.

A show that has not been saved yet has no folder, so media cannot be added to
it. Save it first.

## 3. The screen

- **Top bar**: show name, *Edited* when there are unsaved changes, the mode
  switch (**Edit** / **Show**), undo and redo, open, new, save as, save, settings.
- **Left**: the transport (GO, standby, buttons), the toolbox with the cue
  types, the cue list tabs, the cue list and, under it, the inspector.
- **Right**: audio output with meters and master level, lights, the stage
  monitor (when the show has Video or Text cues), the active cues and the media.
- **Bottom line**: number of cues, incoming timecode, the audio device, the
  connection and, when the show asks for a passcode, what yours allows.

**Show mode** locks editing: no stray key can change the show. Switch to it for
the performance. The choice is kept on each device.

The two-columns button at the right of the list tabs shows two cue lists side
by side. Click in a pane to work in it; the tabs then change that pane's list.
GO and the inspector follow the pane you worked in last. The split is kept per
device.

## 4. The cue list and the cue types

A show has one or more cue lists. Each row of a list is a cue: number, name,
target (the file, the message or the cue it aims at), pre-wait, action time,
post-wait, a mark for auto-continue or auto-follow, and a flag or a warning.
A coloured number means the cue has a colour label. A warning mark means the
cue cannot do anything yet (no file, no target, no message...).

Add cues from the toolbox above the list; the new cue goes after the selected
one. Double-click a number or a name to edit it. Drag rows to move them; drag
a cue onto a Group to put it inside. Copy, cut, paste and duplicate work as
usual (see [section 15](#15-keyboard-shortcuts)). **Group** wraps the selected
cues in a group. **Renumber** numbers the selection from a first number with a
step.

**Search**: the field above the list finds text in the number, name and notes;
the selects narrow by cue type and colour and the flag button shows flagged
cues only. **Ctrl+F** goes to the field, **Esc** clears it. The filter only
hides rows; GO and the standby follow the real list.

**Cue lists**: the tabs above the list. The plus adds a list, the cart button
adds a cart ([section 7](#7-carts)), double-click a tab to rename it, the
bin deletes the open one. The buttons at the right export the list as CSV,
import one, and print the cue sheet (the browser's Print dialog can save it
as PDF). Import reads a CSV by its header names, English or Italian; columns it
does not know are ignored and the type defaults to Memo.

### Cue types

| Type | Does |
| --- | --- |
| **Audio** | plays a sound file: trims, loops, slices, fades, level, routing |
| **Video** | plays a video file on a stage |
| **Text** | shows text on a stage |
| **Light** | sets DMX levels from console-style commands, with a fade |
| **Fade** | fades another cue's level (or opacity, or position) over time |
| **Network** | sends OSC or plain messages over UDP or TCP |
| **MIDI** | sends a MIDI message: notes, controllers, program changes, SysEx, MIDI Show Control |
| **Timecode** | sends MTC or LTC |
| **Group** | holds other cues and runs them in one of five ways |
| **Start, Stop, Pause, Load, Reset, Devamp, Go To, Arm, Disarm** | act on the cue named as target |
| **Target** | stands for another cue by its number |
| **Wait** | does nothing for its duration (useful with auto-follow) |
| **Memo** | a note in the list; does nothing |

Mic, Camera, MIDI File and Script cues can be added but do not do anything yet.

**Groups** run their children in one of five modes: *Timeline (start all)*
starts every child together, each delayed by its own pre-wait; *Start first*,
*Start first and enter*, *Start random* and *Playlist* as their names say.

**Timing.** Every cue has a *pre-wait* (a delay before its action) and a
*post-wait*. *Continue* chooses what happens next: **Do not continue** waits for
the next GO; **Auto-continue** starts the next cue once the post-wait has
passed, counted from the moment this cue's action starts; **Auto-follow**
starts the next cue as soon as this cue's action ends. A disarmed cue is
skipped; *Armed* is in the Basics tab.

## 5. The inspector

Select a cue and the inspector shows its tabs. Several selected cues can be
edited together for the fields they share.

- **Basics** (all cues): number, colour, name, target (for cues that aim at
  another), pre-wait, post-wait, continue mode, armed, flagged and notes.
- **Time & loops** (Audio): the file and its waveform; start and end; loops
  and slices. See [section 8](#8-audio).
- **Levels** (Audio): level, fades in and out, routing matrix, output patch.
- **The cue's own tab** (named after its type: Wait, Fade, Network, MIDI, Light,
  Timecode, Group...): what the cue does.
- **Geometry** (Video, Text): where the picture or text sits on its stage.
- **Triggers** (all cues): what starts the cue from outside. See
  [section 11](#11-triggers).

In a Group in timeline mode the tab also draws its children as bars along a
time scale: drag a bar to change that child's pre-wait (with the keyboard, the
left and right arrows move a focused bar by a tenth of a second, a second with
Shift).

Changes apply at once and can be undone.

## 6. GO, standby and the transport

The **standby** cue is the one GO will start. It shows next to the GO button
with its number, name and notes. After a cue starts, the standby moves to the
next cue in the list.

- **GO** (Space) starts the standby cue. **Double GO protection** in
  Settings ignores a second GO that comes too soon after the first.
- Selecting a cue puts it in standby; turn this off in Settings, *This device*,
  on a second screen that must not move it. **Enter** puts the selected cue in
  standby; double-clicking the left margin of a row does too.
- **V** starts the selected cue without moving the standby.
- **Stop**, **Pause** and **Resume** act on every running cue. **Reset** stops
  everything and returns to the top of the list.
- **Panic** (Esc) fades out everything and stops it, over the *Panic fade
  time* of the Settings.
- **Active cues** (side panel) lists what is running, with progress and time
  left, and pause, resume and stop for each.
- **Load**: clicking the waveform of an audio cue loads it to that time; its
  next start picks up from there.

## 7. Carts

A **cart** is a cue list shown as a grid of buttons, for sound effects and
other cues called in no fixed order. Add one with the cart button next to the
list tabs. Clicking a button starts its cue (and stops it if it is running).
Cues go into a cart the same way as into a list: drag or drop files, or add
them from the toolbox.

## 8. Audio

**Device.** Settings, *Audio*: choose the output device, sample rate and buffer
size (smaller buffers give lower latency and need a faster machine), and the
master level. ASIO drivers on Windows are an option of the build. If the chosen
device is missing the show plays on the default output and the status bar says
so. A second device can be chosen for **audition**: in the inspector, the
headphone button plays the cue there, once, with its trims and fades, outside
the show.

**Files.** WAV, AIFF, FLAC, MP3, AAC, ALAC and Vorbis play. Cues standing by at
the playhead are decoded before GO.

**Time & loops.** The waveform shows the file with its built-in fades. Drag the
two flags at the top to set the start and end. Under the ruler, the squares on
the fade line set the fade in and fade out lengths. Zoom with the wheel or a
pinch, scroll with Shift and the wheel (or the bar that appears when zoomed),
or use the zoom buttons. Click inside the played part to load the cue there.
Loops: a play count, or loop forever.

**Slices.** Alt-click the waveform to cut a slice. Each slice has its own play
count (0 loops it until devamped); markers can be dragged. A **Devamp** cue aimed
at the cue lets the pass in progress finish and moves on to the next slice
without a gap; in the last slice it ends the cue.

**Levels and routing.** The level is in dB. The *routing matrix* sets, for
each channel of the file, the level into each output of the device, in dB, or
silence. Without a matrix, a mono file goes to outputs 1 and 2 and any other
file goes channel for channel. A **Fade** cue aimed at an Audio cue fades its
level to a target, over a time, along a linear or S curve, and can stop the
target when done.

## 9. Lights

MazzaLab drives lights over the network as **Art-Net** or **sACN**.

**DMX outputs** (Settings, *Lights*): one row per universe. Choose the
protocol, the node's address (empty broadcasts Art-Net and uses the multicast
group for sACN) and the universe number on the wire.

**Fixtures** (Settings, *Fixtures*): name each fixture, give its universe and
first address, and say what its channels do in order (intensity, red, green,
blue...). **Groups** gather fixtures or other groups under one name.

**Light cues.** One command per line, `selection @ value`:

```
1 thru 10 @ 50
front wash @ full
front wash + back wash @ #ff8000
2/7 @ out
spot.pan @ 25
all @ out
```

A selection joins terms with `+`: an address (1 to 512, `2/7` for universe 2),
a range (`1 thru 10`), `all` for a universe, or the name of a fixture or group.
`name.parameter` sets one parameter. The value is a percentage, `full`, `out`,
or a colour `#rrggbb`. The cue's *duration* is the fade time. A cue changes only
the slots it names, so looks build on each other; stopping it or pressing
Panic leaves the lights as they are. For a blackout use `all @ out`. The DMX
panel at the right shows the lit slots, with the fixtures' names.

## 10. Video and stages

A **stage** is a screen that shows Video and Text cues. Any browser can be a
stage: open `http://<machine address>:53080/?stage=Main` on the projector's
computer (add `&monitor` for a silent copy). Settings, *Video* lists the stages
by name and size (1920 by 1080 by default); without any there is one called
Main.

In the desktop app a stage can also go **full screen on a display**: choose the
display for each stage in Settings, *Video*, then use *Video, Show stages on
screens* (Ctrl+Shift+D). The stage monitor in the side panel shows what the
stages show.

A **Video** cue plays a file on a stage, on a layer, with an opacity, a start
and end, a rate and loops. A **Text** cue shows text with a font, size, colour,
alignment and an optional background box. The **Geometry** tab places a cue on
its stage: fit mode, position, scale, rotation and crop, with a preview. A
**Fade** cue aimed at a Video or Text cue can fade its opacity and move it to a
new geometry.

## 11. Triggers

A cue's **Triggers** tab sets what starts it from outside, besides GO. A cue
can have several.

- **Hotkey**: a key or combination (`Q`, `F5`, `Ctrl+Shift+1`) that starts the
  cue from any operator screen. The keys the screen itself uses cannot be taken.
- **MIDI**: a note, controller or program change arriving on any MIDI input.
  Press *Learn* and play the control to fill it in. A note on with velocity 0
  counts as a note off. In Settings, *MIDI*, the same learning ties GO, Stop all,
  Panic and the playhead buttons to controls.
- **OSC**: an OSC address; a message to it on the show's OSC port (UDP or TCP)
  starts the cue. `*` and `?` match any characters, so `/scene/*/go` answers
  `/scene/12/go`.
- **Timecode**: starts the cue when incoming timecode (MTC on any MIDI input,
  or LTC on an audio input) passes that frame. A jump or locate starts nothing,
  so the show can be rewound over its triggers. LTC is read from the input and
  channel chosen in Settings, *Audio*; its frame rate is detected.
- **Time of day**: a time (`19:30`, or with seconds) and the days of the week;
  none means every day. It fires as the machine's clock goes past the time; a
  machine that was asleep at that moment starts nothing.

**Remote control by OSC.** The server answers on port 53000 the same
addresses as QLab 5's public OSC dictionary (`/go`, `/cue/1/start`,
`/cue/selected/name`...), so control surfaces made for QLab work. The list of
what is supported is in the [OSC document](../protocols/osc.md).

## 12. Network and MIDI cues, timecode out

**Network patches** (Settings, *Network*): name, host, port and protocol: OSC
over UDP, OSC over TCP (SLIP framing), plain UDP or plain TCP. A **Network cue**
names a patch (or uses the first) and holds one message per line:
`/address arg arg` for OSC (whole numbers are integers, numbers with a point are
floats, `true` and `false` booleans, anything else a string), or the text itself
for plain protocols (with `\n`, `\r`, `\t`, `\xHH`).

**MIDI patches** (Settings, *MIDI*): a name and one of the server machine's
output ports. A **MIDI cue** sends a note on or off, a controller, a program
change, SysEx bytes, or MIDI Show Control (device, command format, command, cue
number and list).

**Timecode cues** send timecode from a start value for a duration (or until
stopped) at a frame rate (24, 25, 30 and drop-frame variants). **MTC** goes to a
MIDI patch; **LTC** comes out of a numbered output of the audio device.
Pausing holds it; resuming runs it on.

## 13. Tablets

Any tablet or phone on the network is a remote: open
`http://<machine address>:53080`. Nothing needs to be installed.

**The show view.** `http://<machine address>:53080/?view=show` is a screen for
running the show only: the standby cue with its notes, a very large GO,
previous and next standby, what is running with its progress, Stop and Panic.
There is no editing in it.

**Passcodes.** A show without passcodes is open to anyone on the network. In
Settings, *Passcodes*, add codes and tick what each allows:

- **View**: see the show and its state;
- **Control**: run it (GO, stop, pause, playhead, panic);
- **Edit**: change it, save, open and create shows (Control and Edit include View).

Once the first passcode exists, every other machine has to enter one to get in;
the machine that runs the server never asks. If no passcode can edit, only that
machine can change the show. A device that only has Control stays in show mode
and cannot change the show; one that only has View cannot run it either. The
status bar shows what the device allows and lets you change the passcode. A
link can carry the code: `...?view=show&passcode=1234`.

Media can be added from a tablet in the Media panel (if its passcode may edit).

## 14. Save, open, backups and recovery

- **Save** (Ctrl+S) writes the show file. A copy of the show as it was is kept
  first: next to the show file, the `backups` folder holds `NAME.1.mazzalab`
  to `NAME.10.mazzalab`, the versions replaced by the last ten saves, 1 being
  the most recent.
- **Autosave**: a few seconds after each change an unsaved copy is written to
  `backups/NAME.autosave.mazzalab` and removed on save. If the program stops
  before you save, opening the show again offers that copy: the desktop app opens
  it and marks the show *recovered*; the headless server does so with `--recover`.
  Save to make it the show.
- **Open**, **New** and **Save as** are in the top bar (edit mode) and, in the
  desktop app, in the File menu (Ctrl+O, Ctrl+N, Ctrl+Shift+S). The picker
  browses the folders of the machine that runs the server. If the open show has
  unsaved changes you are asked whether to save them, drop them or cancel.
- **Undo** and **redo** work on every edit.

### Hot backup

A second machine can run the show alongside the first and take over if the
first goes quiet. Both need the same show and the same media (copy the show
folder, `media` included, to the backup; media is not copied for you).

1. On the main machine nothing changes: it runs as usual.
2. On the backup, start the headless server on a copy of the show with
   `--follow` and the main machine's address:

   ```
   mazzalab-server my-show.mazzalab --ui-dir apps/mazzalab-web/dist --follow http://main-machine:53080
   ```

   (add `?passcode=1234` to the address if the main machine asks for a
   passcode).
3. Open the backup's address in a browser. A line under the top bar says it is
   a backup that follows the main server, and the screen is read-only. It
   mirrors what the main machine does, input by input.
4. If the main machine stops answering for a second, the backup takes over:
   it sends a warning (it has taken over), the line goes away, and it can be
   operated. From then on its own triggers and outputs run.

Know the limits: sounds already playing when the backup joined are not heard
there until the next cues; the backup records no input log until it takes over;
and there is no menu for it in the desktop app yet, only the headless server.
Try it before the show.

## 15. Keyboard shortcuts

| Keys | Does |
| --- | --- |
| Space | GO |
| Esc | Panic |
| ↑ ↓ | select the previous or next cue |
| Shift + ↑ ↓ | extend the selection |
| Enter | put the selected cue in standby |
| V | start the selected cue |
| Ctrl+A | select all |
| Ctrl+C, X, V | copy, cut, paste |
| Ctrl+D | duplicate |
| Ctrl+G | group |
| Delete | delete |
| Ctrl+Z, Ctrl+Shift+Z | undo, redo |
| Ctrl+S | save |
| Ctrl+F | search the cue list |

(On macOS read Cmd for Ctrl.) The transport keys can be changed for each device:
Settings, *General*, *This device*, *Transport keys*. Besides GO and Panic, Stop
all, Pause, Resume and standby next and previous can be given keys; a key
another action, a cue or the screen already uses is refused, and *Restore
defaults* puts the originals back. A cue's own hotkey (Triggers tab) comes first.

## 16. Appearance and language

In Settings, *General*, *This device*: **Language** (Automatic follows the
browser, English, Italiano) and **Theme**: Dark, Light, *Blackout* (very dim,
deep reds, for use in the dark backstage) and High contrast. They are kept for
each device, so the stage manager's tablet and the booth screen can differ.
