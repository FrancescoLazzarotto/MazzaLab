/** A MIDI Show Control device ID typed as a whole number from 0 to 127; empty or anything else is none. */
export function parseDeviceId(text: string): number | undefined {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return undefined;
  const id = Number(trimmed);
  return id <= 127 ? id : undefined;
}
