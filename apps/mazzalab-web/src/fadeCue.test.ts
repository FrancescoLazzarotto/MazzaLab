import { describe, expect, it } from "vitest";
import type { Cue } from "./api/types";
import { asAudioCue, levelRange, startingMatrix, targetChannels } from "./fadeCue";

const sound = (id: string, channels?: number, matrix?: (number | null)[][]): Cue => ({ id, type: "audio", media_channels: channels, matrix }) as Cue;

describe("fade cue", () => {
  it("takes the channel count from the target, or from the widest sound in a group", () => {
    expect(targetChannels(undefined)).toBe(2);
    expect(targetChannels(sound("a", 1))).toBe(1);
    expect(targetChannels(sound("a", undefined, [[0, null], [null, 0], [null, null]]))).toBe(3);
    const group = { id: "g", type: "group", children: [sound("a", 2), { id: "n", type: "memo" } as Cue, sound("b", 6)] } as Cue;
    expect(targetChannels(group)).toBe(6);
    expect(targetChannels({ id: "m", type: "memo" } as Cue)).toBe(2);
  });

  it("starts a matrix from the target's own routing", () => {
    expect(startingMatrix(sound("a", 2, [[-3, null], [null, -3]]), 2, 2)).toEqual([[-3, null], [null, -3]]);
    expect(startingMatrix(sound("a", 1), 1, 2)).toEqual(startingMatrix(undefined, 1, 2));
    expect(startingMatrix(undefined, 2, 2)).toHaveLength(2);
  });

  it("dresses the fade's matrix as an audio cue of the right size", () => {
    const fade = { id: "f", type: "fade", matrix: [[0, null]] } as Cue;
    const dressed = asAudioCue(fade, sound("a", 4));
    expect(dressed.matrix).toEqual([[0, null]]);
    expect(dressed.media_channels).toBe(4);
  });

  it("lets a relative move go either way", () => {
    expect(levelRange(false)).toEqual({ min: -60, max: 12 });
    expect(levelRange(true).min).toBe(-60);
    expect(levelRange(true).max).toBeGreaterThan(12);
  });
});
