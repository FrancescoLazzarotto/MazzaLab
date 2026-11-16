import { describe, expect, it } from "vitest";
import { DEFAULT_KEYMAP, actionFor, bind, conflictFor, isDefaultKeyMap, normalizeKeyMap } from "./keymap";

const none = new Set<string>();

describe("key map", () => {
  it("starts with GO on Space and Panic on Escape", () => {
    expect(actionFor(DEFAULT_KEYMAP, "Space")).toBe("go");
    expect(actionFor(DEFAULT_KEYMAP, "Escape")).toBe("panic");
    expect(actionFor(DEFAULT_KEYMAP, "Q")).toBeNull();
  });

  it("repairs what was saved", () => {
    expect(normalizeKeyMap(undefined)).toEqual(DEFAULT_KEYMAP);
    expect(normalizeKeyMap({ go: 5, panic: null, stopAll: "F5", bogus: "X" })).toEqual({ ...DEFAULT_KEYMAP, panic: null, stopAll: "F5" });
    // A key two actions share stays with the first one.
    expect(normalizeKeyMap({ go: "F1", pauseAll: "F1" })).toMatchObject({ go: "F1", pauseAll: null });
  });

  it("rebinds a free key", () => {
    const { keymap, conflict } = bind(DEFAULT_KEYMAP, "stopAll", "F6", none);
    expect(conflict).toBeNull();
    expect(keymap.stopAll).toBe("F6");
    expect(isDefaultKeyMap(keymap)).toBe(false);
  });

  it("refuses a key another action has, the screen keeps, or a cue uses", () => {
    expect(conflictFor(DEFAULT_KEYMAP, "stopAll", "Space", none)).toEqual({ kind: "action", action: "go" });
    expect(conflictFor(DEFAULT_KEYMAP, "stopAll", "Delete", none)).toEqual({ kind: "screen" });
    expect(conflictFor(DEFAULT_KEYMAP, "stopAll", "Ctrl+Z", none)).toEqual({ kind: "screen" });
    expect(conflictFor(DEFAULT_KEYMAP, "stopAll", "Q", new Set(["Q"]))).toEqual({ kind: "cue" });
    const refused = bind(DEFAULT_KEYMAP, "stopAll", "Space", none);
    expect(refused.keymap).toBe(DEFAULT_KEYMAP);
  });

  it("lets an action keep its own key and frees Space once GO leaves it", () => {
    expect(conflictFor(DEFAULT_KEYMAP, "go", "Space", none)).toBeNull();
    const moved = bind(DEFAULT_KEYMAP, "go", "Enter", none);
    expect(moved.conflict).toEqual({ kind: "screen" });
    const freed = bind(bind(DEFAULT_KEYMAP, "go", "F2", none).keymap, "stopAll", "Space", none);
    expect(freed.keymap.stopAll).toBe("Space");
  });

  it("clears a key", () => {
    expect(bind(DEFAULT_KEYMAP, "panic", null, none).keymap.panic).toBeNull();
  });
});
