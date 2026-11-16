/** What `GET /api/diagnostics` says about how the server is doing. */
export interface Diagnostics {
  uptime: number;
  clients: number;
  running: number;
  /** The audio thread's load since the previous reading, 0 to 1. */
  audio_load?: number;
  audio_peak_load?: number;
  /** Audio callbacks that came late since the device opened: likely dropouts. */
  audio_late?: number;
  audio_callbacks?: number;
  record?: string;
  record_error?: string;
}

/** One reading, as kept for the sparkline. */
export interface Sample {
  load: number | null;
  late: number | null;
}

/** How many readings are kept: a minute, one every two seconds. */
export const HISTORY = 30;

export function sampleOf(reading: Diagnostics): Sample {
  return { load: reading.audio_load ?? null, late: reading.audio_late ?? null };
}

/** The history with a reading added, oldest dropped. */
export function pushSample(history: readonly Sample[], sample: Sample, keep = HISTORY): Sample[] {
  return [...history, sample].slice(-keep);
}

/** How many callbacks came late across the history: what a dropout looks like. */
export function lateWithin(history: readonly Sample[]): number {
  const counts = history.map((sample) => sample.late).filter((late): late is number => late !== null);
  // The counter starts over when the device is reopened: only rises count.
  return counts.slice(1).reduce((total, late, index) => total + Math.max(0, late - counts[index]), 0);
}

/** Seconds as `1:02:03` or `4:05`. */
export function formatUptime(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  const pad = (value: number) => String(value).padStart(2, "0");
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(secs)}` : `${minutes}:${pad(secs)}`;
}

/** `37%`, from a load between 0 and 1. */
export function percent(load: number): string {
  return `${Math.round(Math.min(1, Math.max(0, load)) * 100)}%`;
}

/** The points of a polyline that draws the loads left to right, 0 at the bottom and 1 at the top. */
export function sparkline(history: readonly Sample[], width: number, height: number, keep = HISTORY): string {
  const step = keep > 1 ? width / (keep - 1) : width;
  // The newest reading is at the right edge.
  const offset = keep - history.length;
  return history
    .flatMap((sample, index) => (sample.load === null ? [] : [`${((offset + index) * step).toFixed(1)},${(height - Math.min(1, Math.max(0, sample.load)) * height).toFixed(1)}`]))
    .join(" ");
}
