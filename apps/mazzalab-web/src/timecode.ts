import type { SmpteFormat } from "./api/types";

/** The formats a timecode cue can run at, in the order they are offered. */
export const SMPTE_FORMATS: SmpteFormat[] = ["fps24", "fps25", "fps30_non_drop", "fps30_drop", "fps23_976", "fps29_97_non_drop", "fps29_97_drop"];

/** Short labels, as on a timecode reader. */
export const FORMAT_LABELS: Record<SmpteFormat, string> = {
  fps24: "24",
  fps25: "25",
  fps30_non_drop: "30",
  fps30_drop: "30 DF",
  fps23_976: "23.976",
  fps29_97_non_drop: "29.97",
  fps29_97_drop: "29.97 DF",
};

/**
 * Reads `HH:MM:SS:FF` the way the server does (`;` or `.` before the frames,
 * fewer fields counting from the right) and gives it back written in full;
 * `null` if it is not a timecode.
 */
export function normalizeTimecode(text: string): string | null {
  const fields = text.trim().split(/[:;.]/);
  if (fields.length === 0 || fields.length > 4 || fields.some((field) => !/^\d{1,2}$/.test(field))) return null;
  const values = [0, 0, 0, 0];
  fields.forEach((field, index) => (values[4 - fields.length + index] = Number(field)));
  const [hours, minutes, seconds, frames] = values;
  if (hours > 23 || minutes > 59 || seconds > 59 || frames > 29) return null;
  return values.map((value) => String(value).padStart(2, "0")).join(":");
}
