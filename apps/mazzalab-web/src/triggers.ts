import type { Weekday } from "./api/types";

/** The days of the week, Monday first. */
export const WEEKDAYS: Weekday[] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

/**
 * A time of day as typed, `19.30`, `7:5` or `07:05:09`, written the way
 * show files keep it (`HH:MM` or `HH:MM:SS`); `null` if it is not a time.
 */
export function normalizeClockTime(text: string): string | null {
  const parts = text.trim().split(/[:.]/);
  if (parts.length < 2 || parts.length > 3 || parts.some((part) => !/^\d{1,2}$/.test(part))) return null;
  const [hours, minutes, seconds] = parts.map(Number);
  if (hours > 23 || minutes > 59 || (seconds ?? 0) > 59) return null;
  const two = (value: number) => String(value).padStart(2, "0");
  return parts.length === 3 ? `${two(hours)}:${two(minutes)}:${two(seconds)}` : `${two(hours)}:${two(minutes)}`;
}

/**
 * The days a wall clock trigger is on after clicking `day`: clicking a day
 * it is on takes it off, and the other way round. Every day is written as
 * none named; the last day cannot be taken off.
 */
export function toggleWeekday(days: Weekday[] | undefined, day: Weekday): Weekday[] | undefined {
  const current = days && days.length > 0 ? days : WEEKDAYS;
  const next = current.includes(day) ? current.filter((other) => other !== day) : WEEKDAYS.filter((other) => other === day || current.includes(other));
  if (next.length === 0) return days;
  return next.length === WEEKDAYS.length ? undefined : next;
}
