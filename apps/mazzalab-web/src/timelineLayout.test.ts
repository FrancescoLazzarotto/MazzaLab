import { describe, expect, it } from "vitest";
import type { Cue } from "./api/types";
import { barBox, dragPreWait, layoutTimeline, tickStep } from "./timelineLayout";

const cue = (id: string, extra: Record<string, unknown>): Cue => ({ id, type: "wait", duration: 2, ...extra }) as Cue;

describe("timeline layout", () => {
  it("puts each child at its pre-wait, as long as its action", () => {
    const { bars, total } = layoutTimeline([cue("a", { pre_wait: 1, duration: 4 }), cue("b", { duration: 2 })]);
    expect(bars).toEqual([
      { id: "a", start: 1, length: 4 },
      { id: "b", start: 0, length: 2 },
    ]);
    // Room beyond the last bar.
    expect(total).toBe(6);
  });

  it("leaves a length open when the action's length is not known", () => {
    const { bars } = layoutTimeline([{ id: "m", type: "mic" } as Cue]);
    expect(bars[0].length).toBeNull();
  });

  it("keeps a scale for an empty group", () => {
    expect(layoutTimeline([]).total).toBe(5);
  });

  it("boxes bars as fractions of the scale", () => {
    expect(barBox({ id: "a", start: 1, length: 2 }, 4)).toEqual({ left: 0.25, width: 0.5 });
    expect(barBox({ id: "a", start: 1, length: null }, 4)).toEqual({ left: 0.25, width: 0.75 });
  });

  it("turns a drag into a pre-wait inside the scale", () => {
    expect(dragPreWait(1, 100, 10, 500)).toBe(3);
    expect(dragPreWait(1, -500, 10, 500)).toBe(0);
    expect(dragPreWait(1, 5000, 10, 500)).toBe(10);
    expect(dragPreWait(1, 1, 10, 500)).toBe(1.02);
    expect(dragPreWait(1, 50, 10, 0)).toBe(1);
  });

  it("picks the closest tick step that stays readable", () => {
    expect(tickStep(10, 480)).toBe(1);
    expect(tickStep(600, 480)).toBe(60);
  });
});
