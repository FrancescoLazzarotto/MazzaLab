import { describe, expect, it } from "vitest";
import { NO_SPLIT, normalizeSplit, resolveSplit, showInPane, toggleSplit } from "./panes";

const lists = ["a", "b", "c"];

describe("split panes", () => {
  it("reads what was saved, and nothing else", () => {
    expect(normalizeSplit(undefined)).toEqual(NO_SPLIT);
    expect(normalizeSplit({ on: true, ids: ["a", 3] })).toEqual({ on: true, ids: ["a", null] });
    expect(normalizeSplit({ on: "yes" })).toEqual(NO_SPLIT);
  });

  it("needs two lists and the switch on", () => {
    expect(resolveSplit(NO_SPLIT, lists)).toBeNull();
    expect(resolveSplit({ on: true, ids: ["a", "b"] }, ["a"])).toBeNull();
    expect(resolveSplit({ on: true, ids: ["a", "b"] }, lists)).toEqual(["a", "b"]);
  });

  it("repairs lists that are gone or doubled", () => {
    expect(resolveSplit({ on: true, ids: ["x", "y"] }, lists)).toEqual(["a", "b"]);
    expect(resolveSplit({ on: true, ids: ["b", "b"] }, lists)).toEqual(["b", "a"]);
    expect(resolveSplit({ on: true, ids: ["c", null] }, lists)).toEqual(["c", "a"]);
  });

  it("changes the list of one pane, swapping when the other has it", () => {
    expect(showInPane(["a", "b"], 0, "c")).toEqual(["c", "b"]);
    expect(showInPane(["a", "b"], 1, "a")).toEqual(["b", "a"]);
    expect(showInPane(["a", "b"], 0, "a")).toEqual(["a", "b"]);
  });

  it("turns on with the open list on the left", () => {
    expect(toggleSplit(NO_SPLIT, "b", lists)).toEqual({ on: true, ids: ["b", "a"] });
    expect(toggleSplit({ on: true, ids: ["a", "b"] }, "a", lists).on).toBe(false);
    expect(toggleSplit(NO_SPLIT, "a", ["a"])).toEqual(NO_SPLIT);
  });
});
