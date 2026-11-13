import type { AudioFields, SliceMarker } from "./api/types";

/** One stretch of the file, played `passes` times; `null` loops until devamped. */
export interface Segment {
  from: number;
  to: number;
  passes: number | null;
}

/** Where playback is: which segment, which pass of it, how far into that pass. */
export interface Place {
  segment: number;
  pass: number;
  within: number;
}

type Timed = Pick<AudioFields, "start_time" | "end_time" | "play_count" | "infinite_loop" | "slices" | "last_slice_play_count">;

const passes = (count: number | undefined) => ((count ?? 1) > 0 ? (count ?? 1) : null);

/** The markers that cut the played region, in order; the ones outside it do nothing. */
export function markers(cue: Timed, duration: number): SliceMarker[] {
  const start = cue.start_time ?? 0;
  const end = cue.end_time ?? duration;
  const inside = (cue.slices ?? []).filter((marker) => marker.time > start && (end <= 0 || marker.time < end));
  inside.sort((a, b) => a.time - b.time);
  return inside.filter((marker, index) => index === 0 || marker.time !== inside[index - 1].time);
}

/** The stretches the cue plays, as the server cuts them. */
export function segments(cue: Timed, duration: number): Segment[] {
  const start = cue.start_time ?? 0;
  const end = cue.end_time ?? duration;
  const cuts = markers(cue, duration);
  if (cuts.length === 0) return [{ from: start, to: end, passes: cue.infinite_loop ? null : Math.max(1, cue.play_count ?? 1) }];
  const out: Segment[] = [];
  let from = start;
  for (const marker of cuts) {
    out.push({ from, to: marker.time, passes: passes(marker.play_count) });
    from = marker.time;
  }
  out.push({ from, to: end, passes: passes(cue.last_slice_play_count) });
  return out;
}

/** Where `travelled` seconds of file playback lands; `null` past the end. */
export function locate(cue: Timed, travelled: number, duration: number): Place | null {
  let left = travelled;
  const all = segments(cue, duration);
  for (let index = 0; index < all.length; index++) {
    const { from, to, passes: count } = all[index];
    const length = to - from;
    if (length <= 0) return { segment: index, pass: 0, within: left };
    const pass = Math.floor(left / length);
    if (count === null || pass < count) return { segment: index, pass, within: left - pass * length };
    left -= length * count;
  }
  return null;
}

/** How long the cue plays in file time, or `null` when a slice loops until devamped. */
export function fileLength(cue: Timed, duration: number): number | null {
  let total = 0;
  for (const segment of segments(cue, duration)) {
    if (segment.passes === null) return null;
    total += Math.max(0, segment.to - segment.from) * segment.passes;
  }
  return total;
}

/**
 * The file time travelled when a point of the file first plays; `null` when
 * it is outside the region, or behind a slice that loops until devamped.
 */
export function firstReach(cue: Timed, position: number, duration: number): number | null {
  let travelled = 0;
  const all = segments(cue, duration);
  for (let index = 0; index < all.length; index++) {
    const { from, to, passes: count } = all[index];
    const last = index === all.length - 1;
    // A file whose length is not known yet runs on from where it starts.
    const open = to <= from;
    if (position >= from && (open || position < to || (last && position <= to))) return travelled + (position - from);
    if (count === null) return null;
    travelled += Math.max(0, to - from) * count;
  }
  return null;
}

/** The markers with one more at `time`, played once. */
export function addMarker(cue: Timed, time: number): SliceMarker[] {
  const rounded = Math.round(time * 1000) / 1000;
  const existing = cue.slices ?? [];
  if (existing.some((marker) => marker.time === rounded)) return existing;
  return [...existing, { time: rounded, play_count: 1 }].sort((a, b) => a.time - b.time);
}

/** Where a new marker goes: the point given, or the middle of the longest slice. */
export function newMarkerTime(cue: Timed, duration: number, at?: number): number | null {
  const all = segments(cue, duration);
  if (at !== undefined && all.some((segment) => at > segment.from + 0.01 && at < segment.to - 0.01)) return at;
  const longest = all.reduce((best, segment) => (segment.to - segment.from > best.to - best.from ? segment : best), all[0]);
  if (longest.to - longest.from < 0.02) return null;
  return (longest.from + longest.to) / 2;
}

/** Whether the cue goes round until devamped: a loop, or a slice that does. */
export function loopsUntilDevamped(cue: Timed, duration: number): boolean {
  return segments(cue, duration).some((segment) => segment.passes === null);
}
