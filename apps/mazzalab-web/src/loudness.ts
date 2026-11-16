import type { Cue } from "./api/types";

/** The most a loudness normalization moves a file, either way, in dB. */
export const MAX_NORMALIZE_DB = 20;

/** Targets people reach for: broadcast, streaming and a quieter room. */
export const LUFS_PRESETS = [-23, -18, -16] as const;

/** What normalization adds to a file of `lufs`: the target less the file's loudness, within 20 dB; nothing when either is unknown. */
export function normalizeGain(target: number | undefined, lufs: number | undefined): number {
  if (target === undefined || lufs === undefined) return 0;
  return Math.min(MAX_NORMALIZE_DB, Math.max(-MAX_NORMALIZE_DB, target - lufs));
}

export type TargetChoice = "off" | "custom" | `${number}`;

/** The settings select's value for a target: off, one of the presets, or custom. */
export function targetChoice(target: number | undefined): TargetChoice {
  if (target === undefined) return "off";
  return (LUFS_PRESETS as readonly number[]).includes(target) ? (String(target) as TargetChoice) : "custom";
}

/** `−23.4 LUFS`, with a real minus sign. */
export function formatLufs(lufs: number): string {
  return `${String(Math.round(lufs * 10) / 10).replace("-", "−")} LUFS`;
}

/** `+3.1 dB`, `−2 dB`, or `0 dB`. */
export function formatTrim(db: number): string {
  const rounded = Math.round(db * 10) / 10;
  if (rounded === 0) return "0 dB";
  return `${rounded > 0 ? "+" : "−"}${Math.abs(rounded)} dB`;
}

/** A custom target typed as a number of LUFS: below zero, not absurd; anything else is none. */
export function parseTarget(text: string): number | undefined {
  const value = Number.parseFloat(text.trim().replace(",", ".").replace("−", "-"));
  return Number.isFinite(value) && value < 0 && value >= -70 ? Math.round(value * 10) / 10 : undefined;
}

/** The loudness measured for a file, from the first cue that playing it has one. */
export function lufsOfFile(users: readonly Cue[]): number | undefined {
  for (const cue of users) if (cue.type === "audio" && cue.media_lufs !== undefined) return cue.media_lufs;
  return undefined;
}
