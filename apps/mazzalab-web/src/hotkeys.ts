import type { Cue, Id } from "./api/types";
import { flatten } from "./cues";

/** What a key press needs to tell us, so the logic can be tested without a browser. */
export interface KeyPress {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph", "OS"]);

/** Keys the operator screen already uses, with and without Ctrl. */
const RESERVED = new Set([
  "Space",
  "Escape",
  "Enter",
  "ArrowUp",
  "ArrowDown",
  "Shift+ArrowUp",
  "Shift+ArrowDown",
  "Delete",
  "Backspace",
  "Tab",
  "V",
  ...["S", "Z", "Y", "A", "C", "X", "V", "D", "G"].map((key) => `Ctrl+${key}`),
  "Ctrl+Shift+Z",
]);

/**
 * The name of a key press, such as `Q`, `F5`, `Num3` or `Ctrl+Shift+1`; null
 * for a modifier on its own. Letters and digits are named after the physical
 * key, so Shift+1 is `Shift+1` on every keyboard layout.
 */
export function hotkeyOf(event: KeyPress): string | null {
  if (MODIFIER_KEYS.has(event.key)) return null;
  let base: string;
  if (/^Key[A-Z]$/.test(event.code)) base = event.code.slice(3);
  else if (/^Digit[0-9]$/.test(event.code)) base = event.code.slice(5);
  else if (/^Numpad/.test(event.code)) base = `Num${event.code.slice(6)}`;
  else if (event.key === " ") base = "Space";
  else if (event.key.length === 1) base = event.key.toUpperCase();
  else base = event.key;
  const modifiers = [(event.ctrlKey || event.metaKey) && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift"].filter(Boolean);
  return [...modifiers, base].join("+");
}

/** Whether the operator screen keeps this key for itself. */
export function isReserved(hotkey: string): boolean {
  return RESERVED.has(hotkey);
}

/** The cues a key press starts, in list order. */
export function cuesForHotkey(cues: Cue[], hotkey: string): Id[] {
  return flatten(cues)
    .filter((row) => row.cue.triggers?.hotkey === hotkey)
    .map((row) => row.cue.id);
}
