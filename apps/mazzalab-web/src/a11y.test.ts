import { describe, expect, it } from "vitest";
import { needsLabel, newlyAdded } from "./a11y";

describe("accessibility helpers", () => {
  it("labels icon-only controls that have a title and no name", () => {
    expect(needsLabel({ title: "Save", ariaLabel: null, labelledBy: null, text: "" })).toBe(true);
    expect(needsLabel({ title: "Save", ariaLabel: "Save", labelledBy: null, text: "" })).toBe(false);
    expect(needsLabel({ title: "Save", ariaLabel: null, labelledBy: "x", text: "" })).toBe(false);
    expect(needsLabel({ title: "Save", ariaLabel: null, labelledBy: null, text: "Save" })).toBe(false);
    expect(needsLabel({ title: "", ariaLabel: null, labelledBy: null, text: "" })).toBe(false);
  });

  it("finds what started since last time", () => {
    expect(newlyAdded(["a"], ["a", "b", "c"])).toEqual(["b", "c"]);
    expect(newlyAdded([], [])).toEqual([]);
    expect(newlyAdded(["a", "b"], ["a"])).toEqual([]);
  });
});
