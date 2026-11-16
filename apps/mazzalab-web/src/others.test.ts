import { describe, expect, it } from "vitest";
import type { EngineEvent } from "./api/types";
import { NO_OTHERS, applyOthersEvents, cleanDuck, duckLabel, othersMode, withMode } from "./others";

const at = 0;
const duck = (cue: string, by_db: number): EngineEvent => ({ at, kind: "ducked", cue, by_db, fade: 1 });

describe("fade and stop others", () => {
  it("reads and sets the mode", () => {
    expect(othersMode(undefined)).toBe("off");
    expect(othersMode({ action: "duck", scope: "all", fade: 1, duck_db: 6 })).toBe("duck");
    expect(withMode(undefined, "off")).toBeUndefined();
    expect(withMode({ action: "duck", scope: "all", fade: 1, duck_db: 6 }, "off")).toBeUndefined();
  });

  it("starts from usual values and keeps what was chosen", () => {
    expect(withMode(undefined, "fade_and_stop")).toEqual({ action: "fade_and_stop", scope: "list", fade: 2, duck_db: 10 });
    expect(withMode(undefined, "duck")?.fade).toBe(1);
    expect(withMode({ action: "duck", scope: "all", fade: 3, duck_db: 6 }, "fade_and_stop")).toEqual({ action: "fade_and_stop", scope: "all", fade: 3, duck_db: 6 });
  });

  it("keeps the duck amount a positive number", () => {
    expect(cleanDuck(6.04)).toBe(6);
    expect(cleanDuck(-3)).toBe(10);
    expect(cleanDuck(Number.NaN)).toBe(10);
    expect(duckLabel(10)).toBe("−10 dB");
    expect(duckLabel(7.5)).toBe("−7.5 dB");
  });
});

describe("following ducks from events", () => {
  it("tracks ducked and unducked cues", () => {
    const one = applyOthersEvents(NO_OTHERS, [duck("a", 10), duck("b", 6)]);
    expect([...one.ducked]).toEqual([["a", 10], ["b", 6]]);
    // A later event replaces the amount.
    const two = applyOthersEvents(one, [duck("a", 16), { at, kind: "unducked", cue: "b", fade: 1 }]);
    expect([...two.ducked]).toEqual([["a", 16]]);
  });

  it("forgets a cue that ends while ducked or fading", () => {
    const state = applyOthersEvents(NO_OTHERS, [duck("a", 10), { at, kind: "fading_out", cue: "b", fade: 2 }]);
    expect(state.fading.has("b")).toBe(true);
    const after = applyOthersEvents(state, [{ at, kind: "stopped", cue: "a" }, { at, kind: "stopped", cue: "b" }]);
    expect(after.ducked.size).toBe(0);
    expect(after.fading.size).toBe(0);
  });

  it("hands back the same state when nothing changed", () => {
    const state = applyOthersEvents(NO_OTHERS, [duck("a", 10)]);
    expect(applyOthersEvents(state, [{ at, kind: "triggered", cue: "x" }])).toBe(state);
    expect(applyOthersEvents(state, [{ at, kind: "stopped", cue: "x" }])).toBe(state);
  });
});
