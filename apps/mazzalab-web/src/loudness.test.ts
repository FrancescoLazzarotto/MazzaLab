import { describe, expect, it } from "vitest";
import type { Cue } from "./api/types";
import { formatLufs, formatTrim, lufsOfFile, normalizeGain, parseTarget, targetChoice } from "./loudness";

describe("loudness normalization", () => {
  it("adds target minus loudness, within 20 dB, and nothing without both", () => {
    expect(normalizeGain(-23, -26)).toBe(3);
    expect(normalizeGain(-23, -18)).toBe(-5);
    expect(normalizeGain(-16, -60)).toBe(20);
    expect(normalizeGain(-60, -10)).toBe(-20);
    expect(normalizeGain(undefined, -20)).toBe(0);
    expect(normalizeGain(-23, undefined)).toBe(0);
  });

  it("tells the presets from a custom target", () => {
    expect(targetChoice(undefined)).toBe("off");
    expect(targetChoice(-23)).toBe("-23");
    expect(targetChoice(-16)).toBe("-16");
    expect(targetChoice(-20)).toBe("custom");
  });

  it("writes loudness and trim with a real minus", () => {
    expect(formatLufs(-23.04)).toBe("−23 LUFS");
    expect(formatLufs(-18.26)).toBe("−18.3 LUFS");
    expect(formatTrim(3.14)).toBe("+3.1 dB");
    expect(formatTrim(-2)).toBe("−2 dB");
    expect(formatTrim(0.01)).toBe("0 dB");
  });

  it("reads a typed target", () => {
    expect(parseTarget("-20")).toBe(-20);
    expect(parseTarget(" −19,5 ")).toBe(-19.5);
    for (const text of ["", "abc", "0", "5", "-90"]) expect(parseTarget(text)).toBeUndefined();
  });

  it("finds a file's loudness on the first cue that has it", () => {
    const cue = (media_lufs?: number) => ({ id: "x", type: "audio", media_lufs }) as Cue;
    expect(lufsOfFile([cue(), cue(-20), cue(-30)])).toBe(-20);
    expect(lufsOfFile([cue()])).toBeUndefined();
    expect(lufsOfFile([])).toBeUndefined();
  });
});
