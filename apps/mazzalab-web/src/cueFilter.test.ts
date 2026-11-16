import { describe, expect, it } from "vitest";
import type { Cue } from "./api/types";
import { NO_FILTER, isFiltering, matches, visibleIds } from "./cueFilter";

const memo = (id: string, extra: Partial<Cue> = {}): Cue => ({ id, type: "memo", ...extra }) as Cue;
const group = (id: string, children: Cue[]): Cue => ({ id, type: "group", mode: "timeline", children }) as Cue;

describe("cue filter", () => {
  it("filters nothing by default", () => {
    expect(isFiltering(NO_FILTER)).toBe(false);
    expect(isFiltering({ ...NO_FILTER, text: "  " })).toBe(false);
    expect(visibleIds([memo("a")], NO_FILTER)).toBeNull();
  });

  it("matches text in number, name and notes, ignoring case and accents", () => {
    const cue = memo("a", { number: "12.5", name: "Èntrata", notes: "Dark Blue wash" });
    expect(matches(cue, { ...NO_FILTER, text: "12.5" })).toBe(true);
    expect(matches(cue, { ...NO_FILTER, text: "entrata" })).toBe(true);
    expect(matches(cue, { ...NO_FILTER, text: "blue WASH" })).toBe(true);
    expect(matches(cue, { ...NO_FILTER, text: "entrata red" })).toBe(false);
  });

  it("matches the name the list shows when none is set", () => {
    expect(matches(memo("a"), { ...NO_FILTER, text: "memo" })).toBe(true);
  });

  it("filters by type, colour and flag together", () => {
    const cue = memo("a", { color: "red", flagged: true });
    expect(matches(cue, { ...NO_FILTER, type: "memo", color: "red", flagged: true })).toBe(true);
    expect(matches(cue, { ...NO_FILTER, type: "wait" })).toBe(false);
    expect(matches(cue, { ...NO_FILTER, color: "blue" })).toBe(false);
    expect(matches(memo("b"), { ...NO_FILTER, flagged: true })).toBe(false);
    expect(matches(memo("b"), { ...NO_FILTER, color: "none" })).toBe(true);
    expect(matches(cue, { ...NO_FILTER, color: "none" })).toBe(false);
  });

  it("keeps the groups that hold a match, and only those", () => {
    const cues = [group("g1", [memo("a", { name: "x" }), group("g2", [memo("b", { name: "needle" })])]), group("g3", [memo("c")]), memo("d")];
    const shown = visibleIds(cues, { ...NO_FILTER, text: "needle" });
    expect([...shown!].sort()).toEqual(["b", "g1", "g2"]);
  });
});
