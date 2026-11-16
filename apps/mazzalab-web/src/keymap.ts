import { isReserved } from "./hotkeys";

/** The transport actions a key can be given to. */
export type TransportAction = "go" | "stopAll" | "panic" | "pauseAll" | "resumeAll" | "playheadNext" | "playheadPrevious";

export const TRANSPORT_ACTIONS: TransportAction[] = ["go", "stopAll", "pauseAll", "resumeAll", "playheadNext", "playheadPrevious", "panic"];

/** A key name as `hotkeyOf` writes it, or null for an action without a key. */
export type KeyMap = Record<TransportAction, string | null>;

export const DEFAULT_KEYMAP: KeyMap = {
  go: "Space",
  stopAll: null,
  pauseAll: null,
  resumeAll: null,
  playheadNext: null,
  playheadPrevious: null,
  panic: "Escape",
};

/**
 * What was saved, made safe: unknown actions and non-text values fall back
 * to the defaults, and a key given to two actions stays with the first.
 */
export function normalizeKeyMap(saved: unknown): KeyMap {
  const source = typeof saved === "object" && saved !== null ? (saved as Record<string, unknown>) : {};
  const map = { ...DEFAULT_KEYMAP };
  const taken = new Set<string>();
  for (const action of TRANSPORT_ACTIONS) {
    const value = source[action];
    let key: string | null = DEFAULT_KEYMAP[action];
    if (value === null) key = null;
    else if (typeof value === "string" && value.trim() !== "") key = value;
    if (key !== null && taken.has(key)) key = null;
    if (key !== null) taken.add(key);
    map[action] = key;
  }
  return map;
}

/** The action a key press runs, if any. */
export function actionFor(keymap: KeyMap, hotkey: string): TransportAction | null {
  return TRANSPORT_ACTIONS.find((action) => keymap[action] === hotkey) ?? null;
}

export type Conflict = { kind: "action"; action: TransportAction } | { kind: "screen" } | { kind: "cue" };

/**
 * What stops `hotkey` from being given to `action`: another action that has
 * it, a key the screen keeps for itself (arrows, Delete, Ctrl+Z...), or a
 * hotkey a cue already has.
 */
export function conflictFor(keymap: KeyMap, action: TransportAction, hotkey: string, cueHotkeys: ReadonlySet<string>): Conflict | null {
  const other = TRANSPORT_ACTIONS.find((candidate) => candidate !== action && keymap[candidate] === hotkey);
  if (other) return { kind: "action", action: other };
  // Space and Escape are only reserved as the defaults of GO and Panic.
  if (isReserved(hotkey) && hotkey !== "Space" && hotkey !== "Escape") return { kind: "screen" };
  if (cueHotkeys.has(hotkey)) return { kind: "cue" };
  return null;
}

/** Rebinds one action; null clears it. Returns the map unchanged when the key conflicts. */
export function bind(keymap: KeyMap, action: TransportAction, hotkey: string | null, cueHotkeys: ReadonlySet<string>): { keymap: KeyMap; conflict: Conflict | null } {
  if (hotkey !== null) {
    const conflict = conflictFor(keymap, action, hotkey, cueHotkeys);
    if (conflict) return { keymap, conflict };
  }
  return { keymap: { ...keymap, [action]: hotkey }, conflict: null };
}

export function isDefaultKeyMap(keymap: KeyMap): boolean {
  return TRANSPORT_ACTIONS.every((action) => keymap[action] === DEFAULT_KEYMAP[action]);
}
