import { describe, expect, it } from "vitest";
import type { Cue } from "./api/types";
import { serverUrl } from "./api/connection";
import { actionLength, findCue, flatten, newCue, nextNumber } from "./cues";
import { formatTime, parseTime } from "./format";
import { DICTIONARIES } from "./i18n";

describe("time formatting", () => {
  it("formats seconds", () => {
    expect(formatTime(0)).toBe("0:00.00");
    expect(formatTime(65.5)).toBe("1:05.50");
    expect(formatTime(3723.25)).toBe("1:02:03.25");
    expect(formatTime(null)).toBe("—");
  });

  it("parses operator input", () => {
    expect(parseTime("5")).toBe(5);
    expect(parseTime("2,5")).toBe(2.5);
    expect(parseTime("1:05")).toBe(65);
    expect(parseTime("1:02:03.5")).toBe(3723.5);
    expect(parseTime("")).toBeNull();
    expect(parseTime("abc")).toBeNull();
    expect(parseTime("1::2")).toBeNull();
  });
});

describe("cue helpers", () => {
  const group: Cue = {
    id: "g",
    number: "2",
    type: "group",
    children: [{ id: "c", number: "2.5", type: "memo" }],
  };
  const cues: Cue[] = [{ id: "a", number: "1", type: "wait", duration: 3 }, group];

  it("flattens groups with depth", () => {
    expect(flatten(cues).map((row) => [row.cue.id, row.depth, row.parent])).toEqual([
      ["a", 0, null],
      ["g", 0, null],
      ["c", 1, "g"],
    ]);
    expect(findCue(cues, "c")?.number).toBe("2.5");
  });

  it("numbers new cues after the highest number", () => {
    expect(nextNumber(cues)).toBe("3");
    expect(nextNumber([])).toBe("1");
  });

  it("computes action lengths like the engine", () => {
    expect(actionLength(cues[0])).toBe(3);
    expect(actionLength({ id: "x", type: "audio", media_duration: 10, start_time: 2, play_count: 2, rate: 2 })).toBe(8);
    expect(actionLength({ id: "x", type: "audio" })).toBeNull();
    expect(actionLength(group)).toBeNull();
  });

  it("creates valid default cues", () => {
    const fade = newCue("fade", "4", "id");
    expect(fade).toMatchObject({ id: "id", number: "4", type: "fade", duration: 5 });
  });
});

describe("i18n", () => {
  it("translates every key in every language", () => {
    const english = Object.keys(DICTIONARIES.en).sort();
    expect(Object.keys(DICTIONARIES.it).sort()).toEqual(english);
    for (const value of Object.values(DICTIONARIES.it)) expect(value.length).toBeGreaterThan(0);
  });
});

describe("server url", () => {
  it("uses the page origin or the ?server override", () => {
    expect(serverUrl({ protocol: "http:", host: "localhost:53080", search: "" })).toBe("ws://localhost:53080/ws");
    expect(serverUrl({ protocol: "https:", host: "x", search: "?server=10.0.0.5:53080" })).toBe(
      "wss://10.0.0.5:53080/ws",
    );
  });
});
