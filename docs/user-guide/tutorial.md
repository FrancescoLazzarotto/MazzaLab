# Tutorial: a first act in MazzaLab

*[Leggi in italiano](tutorial.it.md)*

This walks through the theatre example that ships with MazzaLab and then has
you build its main parts yourself. It takes about an hour. The
[user guide](README.md) explains each part in full.

## 1. Open the example

The example is `examples/shows/theatre.mazzalab`, with six tiny sound files in
`examples/shows/media/` (together 153 kB: tones and noise, not music). Start the
program on it:

```
cargo run -p mazzalab-desktop -- examples/shows/theatre.mazzalab
```

(or the headless server, then open `http://localhost:53080`). To write a fresh
copy of it somewhere else, with its sound files:

```
cargo run -p mazzalab-cli -- new my-act.mazzalab --example theatre
```

Press the check button in the top bar (Pre-show check): it should say there is
nothing to report. Then read the list from the top. It is one act with 14 cues,
and a second list, *Effects*, which is a cart.

## 2. What is in it

| No. | Cue | What it shows |
| --- | --- | --- |
| 1 | House to half | a **Light** cue: `wash @ 50` and a blue on the cyc, over 5 s |
| 2 | Preshow music | an **Audio** cue that loops, with a fade in |
| 3 | Welcome text | a **Text** cue on the stage called Main |
| 4 | House out | lights out over 4 s |
| 5 | Scene 1 music | audio that **fades out and stops the others** when it starts |
| 6 | Scene 1 lights | a look built from fixtures and a colour |
| 7 | Storm | a **Group** in timeline mode: thunder, lightning, text, with pre-waits |
| 8 | Waiting music | audio cut into **slices**, one of which loops |
| 9 | Leave the loop | a **Devamp** cue aimed at 8 |
| 10 | Sound desk scene 2 | a **Network** cue (OSC), with auto-continue |
| 11 | Fade the scene music | a **Fade** cue that stops its target |
| 12 | Beat | a **Wait** with auto-follow |
| 13 | House up | lights up |
| 14 | End of act 1 | a **Memo** |

Run it from the keyboard: press **Space** for GO and watch the
standby move down the list. Stop with **Esc** (panic). The hotkey **T** fires the
thunder (cue 7.1) from anywhere.

Lights and the network cue send to Art-Net (broadcast) and to `127.0.0.1:9000`;
if nothing listens there, nothing happens. Audio needs an output device, set in
Settings, *Audio*.

## 3. Build it yourself

Start a new show (top bar, New) and save it, so that it has a folder.

### The rig and the stage

1. **Settings, Fixtures**: add *Front wash*, *Back wash* and *Spot* as dimmers on
   universe 1, addresses 1, 2 and 3, and *Cyc* at 10 with the parameters
   `red, green, blue`. Add a group *Wash* with the first two.
2. **Settings, Lights**: add an output for universe 1 (Art-Net).
3. **Settings, Video**: add a stage called *Main*.
4. **Settings, Network**: add a patch *Sound desk*, host `127.0.0.1`, port 9000,
   OSC over UDP.

### Lights

1. Add a **Light** cue (toolbox). In its tab write one command per line:
   `wash @ 50` and `cyc @ #101040`. Set the *duration* to 5 s: that is the fade.
2. Name it *House to half* and press GO to see the DMX panel at the right light up.

### Sound, and starting one sound over another

1. Drop `preshow.wav` from the Media panel on the list. In its **Time & loops**
   tab tick *Infinite loop*; in **Levels** set a fade in of 2 s.
2. Drop `scene1.wav`. In its **Basics** tab, under *Others, when this starts*,
   choose *Fade out and stop*, *Every list*, fade time 3 s. Now GO on it fades the
   preshow music away by itself. (*Duck them* would lower the others instead,
   and bring them back when this cue ends.)

### A slice that loops, and the Devamp

1. Add `waiting.wav`. In its **Time & loops** tab, Alt-click the waveform at 1 s
   and at 2 s to cut three slices. Give the second slice a play count of 0: it
   loops until devamped.
2. Add a **Devamp** cue and pick the waiting music as its target.
3. GO the music, let it settle in the loop, then GO the Devamp: the pass in
   progress finishes and the last slice plays.

### A storm: a group in timeline mode

1. Add a **Group**, mode *Timeline (start all)*. Put `thunder.wav` and two Light
   cues in it (drag them onto the group).
2. Give the lightning a pre-wait of 0.5 s. In the group's tab the children are
   drawn as bars; drag a bar to change its pre-wait.
3. Add a **Text** cue in the group, `THE STORM`, on the stage *Main*, with a
   larger size, and a pre-wait of 1 s. Open `http://localhost:53080/?stage=Main`
   in a browser window to see the stage.

### Fade, wait, and auto-follow

1. Add a **Fade** cue, target the scene music, level −60 dB over 2 s, and tick
   *Stop target when done*.
2. Add a **Wait** of 2 s and set *Continue* to *Auto-follow*: the next cue starts
   when it ends.

### A cart for effects

Press the cart button next to the list tabs, and drop `bell.wav` and `door.wav`
onto it. Each is a button you can press in any order.

## 4. Make it yours

- Select a cue you will use again and press *Save the selected cue as a template*;
  make new ones from the menu next to the toolbox.
- Give the thunder a hotkey (Triggers tab).
- Run **Pre-show check** before every performance.
- Open `/?view=show` on a tablet, and set passcodes in Settings if the network is
  shared.
- Press Ctrl+S: a safety copy is kept in the `backups` folder next to the show.
