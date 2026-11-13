import { describe, expect, it } from "vitest";
import type { AudioCue } from "./api/types";
import { cueTime, playbackPosition } from "./components/Waveform";
import { actionLength } from "./cues";
import { addMarker, fileLength, firstReach, locate, loopsUntilDevamped, markers, newMarkerTime, segments } from "./slices";

/** Ten seconds of file: an intro to 2 s, a vamp to 6 s that loops, an ending. */
const vamp: AudioCue = {
  id: "a",
  type: "audio",
  media_duration: 10,
  // Out of order, as they may be stored: the marker at 6 s ends the vamp.
  slices: [
    { time: 6, play_count: 0 },
    { time: 2, play_count: 1 },
  ],
};

describe("slices", () => {
  it("cut the region the way the server does", () => {
    // A marker past the end of the file does nothing.
    const cue = { ...vamp, slices: [...(vamp.slices ?? []), { time: 12 }] };
    expect(markers(cue, 10).map((marker) => marker.time)).toEqual([2, 6]);
    expect(segments(cue, 10)).toEqual([
      { from: 0, to: 2, passes: 1 },
      { from: 2, to: 6, passes: null },
      { from: 6, to: 10, passes: 1 },
    ]);
    expect(loopsUntilDevamped(cue, 10)).toBe(true);
    expect(fileLength(cue, 10)).toBeNull();
    // Without markers, the cue's own loop settings.
    expect(segments({ start_time: 1, play_count: 3 }, 10)).toEqual([{ from: 1, to: 10, passes: 3 }]);
  });

  it("follow playback through the slices", () => {
    const cue = { ...vamp, slices: [{ time: 2, play_count: 2 }, { time: 6, play_count: 1 }] };
    expect(fileLength(cue, 10)).toBe(12);
    expect(actionLength({ ...cue, rate: 2 })).toBe(6);
    expect(locate(cue, 3, 10)).toEqual({ segment: 0, pass: 1, within: 1 });
    expect(locate(cue, 13, 10)).toBeNull();
    expect(playbackPosition(cue, 3, 10)).toBe(1);
    expect(playbackPosition(cue, 4.5, 10)).toBe(2.5);
    expect(playbackPosition(cue, 13, 10)).toBe(10);
    // A point of the ending is reached after both passes of the intro and the middle.
    expect(firstReach(cue, 7, 10)).toBe(4 + 4 + 1);
    expect(cueTime({ ...cue, pre_wait: 1 }, 7, 10)).toBe(10);
    // Behind a vamp, a point is only reached by devamping.
    const looped = { ...cue, slices: [{ time: 2, play_count: 1 }, { time: 6, play_count: 0 }] };
    expect(cueTime(looped, 3, 10)).toBe(3);
    expect(cueTime(looped, 7, 10)).toBeNull();
  });

  it("are added where they make sense", () => {
    expect(addMarker(vamp, 4.0004).map((marker) => marker.time)).toEqual([2, 4, 6]);
    expect(addMarker(vamp, 2)).toBe(vamp.slices);
    // Where the cue is loaded to, or else the middle of the longest slice.
    expect(newMarkerTime(vamp, 10, 3)).toBe(3);
    expect(newMarkerTime(vamp, 10, 2)).toBe(4);
    expect(newMarkerTime({}, 10)).toBe(5);
    expect(newMarkerTime({}, 0)).toBeNull();
  });
});
