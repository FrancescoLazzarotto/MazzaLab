import type { Cue, Id } from "./api/types";
import { actionLength } from "./cues";

/** One child of a timeline group as a bar: where it starts, and how long it runs if that is known. */
export interface Bar {
  id: Id;
  /** The child's pre-wait: its offset from the moment the group starts. */
  start: number;
  /** The action's length; null when it is not known in advance. */
  length: number | null;
}

export interface TimelineLayout {
  bars: Bar[];
  /** The time the scale covers, in seconds. */
  total: number;
}

/** Steps between ruler ticks, in seconds. */
const STEPS = [0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600];

/** The bars of a group's children, and a scale that fits them with some room to drag. */
export function layoutTimeline(children: Cue[]): TimelineLayout {
  const bars = children.map((cue): Bar => ({ id: cue.id, start: Math.max(0, cue.pre_wait ?? 0), length: actionLength(cue) }));
  const furthest = bars.reduce((end, bar) => Math.max(end, bar.start + (bar.length ?? 0)), 0);
  // Room beyond the last bar, to drag towards, and a floor so an empty group still has a scale.
  const total = Math.max(5, Math.ceil(furthest * 1.2));
  return { bars, total };
}

/** The distance between ruler ticks that keeps labels at least `minPixels` apart. */
export function tickStep(total: number, width: number, minPixels = 48): number {
  if (total <= 0 || width <= 0) return STEPS[STEPS.length - 1];
  return STEPS.find((step) => (step / total) * width >= minPixels) ?? STEPS[STEPS.length - 1];
}

/** Where a bar starts and how wide it is, as fractions of the scale. */
export function barBox(bar: Bar, total: number): { left: number; width: number } {
  const left = Math.min(1, bar.start / total);
  const end = bar.length === null ? 1 : Math.min(1, (bar.start + bar.length) / total);
  return { left, width: Math.max(0, end - left) };
}

/** The pre-wait a bar gets when dragged `deltaPixels` from where it started, in hundredths of a second. */
export function dragPreWait(began: number, deltaPixels: number, total: number, width: number): number {
  if (width <= 0) return began;
  const moved = began + (deltaPixels / width) * total;
  return Math.round(Math.min(total, Math.max(0, moved)) * 100) / 100;
}
