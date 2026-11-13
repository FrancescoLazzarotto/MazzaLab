import { describe, expect, it } from "vitest";
import { problemOf } from "./cues";
import { normalizeTimecode } from "./timecode";

describe("timecode", () => {
  it("is read the way the server reads it", () => {
    expect(normalizeTimecode("1:0:5:9")).toBe("01:00:05:09");
    expect(normalizeTimecode(" 10:00:00;12 ")).toBe("10:00:00:12");
    expect(normalizeTimecode("5:10")).toBe("00:00:05:10");
    expect(normalizeTimecode("24:00:00:00")).toBeNull();
    expect(normalizeTimecode("01:00:00:30")).toBeNull();
    expect(normalizeTimecode("soon")).toBeNull();
    expect(normalizeTimecode("1:2:3:4:5")).toBeNull();
  });

  it("flags a timecode cue that does not start on a timecode", () => {
    const cue = { id: "t", type: "timecode" as const, start: "01:00:00:00" };
    expect(problemOf(cue, () => true)).toBeNull();
    expect(problemOf({ ...cue, start: "later" }, () => true)).toBe("bad_timecode");
  });
});
