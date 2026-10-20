/** Formats seconds as `m:ss.cc` (or `h:mm:ss.cc` past an hour). */
export function formatTime(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds)) return "—";
  const negative = seconds < 0;
  const centis = Math.round(Math.abs(seconds) * 100);
  const hours = Math.floor(centis / 360000);
  const minutes = Math.floor((centis % 360000) / 6000);
  const secs = Math.floor((centis % 6000) / 100);
  const fraction = centis % 100;
  const pad = (value: number) => String(value).padStart(2, "0");
  const body =
    hours > 0
      ? `${hours}:${pad(minutes)}:${pad(secs)}.${pad(fraction)}`
      : `${minutes}:${pad(secs)}.${pad(fraction)}`;
  return negative ? `-${body}` : body;
}

/**
 * Parses what operators type into time fields: `5`, `5.5`, `1:05`,
 * `1:02:03.5`, with `,` accepted as decimal separator. Returns null for
 * anything else.
 */
export function parseTime(text: string): number | null {
  const trimmed = text.trim().replace(",", ".");
  if (trimmed === "") return null;
  const parts = trimmed.split(":");
  if (parts.length > 3 || parts.some((part) => !/^\d*\.?\d*$/.test(part) || part === "" || part === ".")) {
    return null;
  }
  const value = parts.reduce((total, part) => total * 60 + Number.parseFloat(part), 0);
  return Number.isFinite(value) ? value : null;
}
