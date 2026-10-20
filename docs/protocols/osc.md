# OSC remote control

MazzaLab accepts OSC using the address space of the public QLab 5 OSC
dictionary, so existing show-control setups can switch without
reprogramming. Parsing lives in `crates/mazzalab-osc`, execution in
`crates/mazzalab-show` and the network listeners in `apps/mazzalab-server`.

## Transport

| | |
| --- | --- |
| Commands | UDP and TCP, port **53000** |
| UDP replies | port **53001** |
| TCP framing | SLIP, double END (OSC 1.1) |

Prefix any message with `/workspace/{id}` to address one workspace.

## Supported today (parsed)

| Address | Meaning |
| --- | --- |
| `/go`, `/go/{number}` | GO, or set the playhead to a cue and GO |
| `/stop`, `/hardStop`, `/pause`, `/resume`, `/panic`, `/reset` | Workspace transport |
| `/playhead/next`, `/playhead/previous`, `/playhead/nextSequence`, `/playhead/previousSequence` | Move the playhead (`/playbackPosition/...` also accepted) |
| `/playhead/{number}`, `/playheadID/{id}` | Set the playhead |
| `/select/next`, `/select/previous`, `/select/{number}`, `/select_id/{id}` | Selection |
| `/cue/{number\|selected\|active\|playhead}/{action}` | `start`, `stop`, `hardStop`, `pause`, `resume`, `load`, `preview`, `panic`, `reset` |
| `/cue_id/{id}/{action}` | Same, by unique id |
| `/cue/{n}/{property}` | Read a property; with an argument, write it |
| `/cue/{n}/{property}/+ {delta}`, `/-`, `/+/{delta}` | Increment or decrement |
| `.../live` | Change the running value without editing the show |
| `/updates {bool}`, `/alwaysReply {bool}` | Subscriptions |
| `/connect {passcode}`, `/disconnect`, `/version`, `/workspaces`, `/cueLists`, `/thump` | Session |

Boolean arguments are lenient. Non-zero numbers and strings starting with
`y`, `t` or `1`–`9` are true. Zero and strings starting with `n`, `f` or
`0` are false.

## Replies

Replies go to `/reply{original address}` with one JSON string argument:

```json
{ "workspace_id": "…", "address": "/cue/1/name", "status": "ok", "data": "Overture" }
```

`status` is `ok`, `error` or `denied`.

## Updates

After `/updates 1` a client receives:

- `/update/workspace/{id}` when the show structure changes;
- `/update/workspace/{id}/cue_id/{cue}` when a cue starts, stops, pauses or
  completes;
- `/update/workspace/{id}/cueList/{list}/playbackPosition {cue}` when a
  playhead moves.

## Readable and writable cue properties

`uniqueID`, `type`, `listName`/`displayName`, `isRunning`, `isPaused`,
`isBroken`, `preWaitElapsed` and `actionElapsed` are read-only. `number`,
`name`, `notes`, `colorName`, `preWait`, `postWait`, `duration` (Wait,
Fade, Light and Network cues), `continueMode` (0/1/2, wraps around with
`+`/`-`), `armed` and `flagged` can be read and written. Writes are
undoable like any other edit.

## Not yet supported

`load` and `preview` on cues, passcodes, the plain-text port 53535,
`/udpReplyPort`, and `/live` values that differ from the saved show (a
`/live` write currently edits the show).
