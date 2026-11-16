import { DEFAULT_KEYMAP, normalizeKeyMap, type KeyMap } from "./keymap";
import { NO_SPLIT, normalizeSplit, type Split } from "./panes";
import { normalizeTheme, type Theme } from "./theme";

/** Choices that belong to this device rather than to the show. */
export interface Preferences {
  /** Selecting a cue also makes it the next one to GO, as in QLab. */
  playheadFollowsSelection: boolean;
  /** Show mode locks editing, so a stray key cannot change the show. */
  mode: "edit" | "show";
  theme: Theme;
  /** The keys of the transport, as this device has them. */
  keymap: KeyMap;
  /** Two cue lists side by side. */
  split: Split;
}

export const DEFAULT_PREFERENCES: Preferences = { playheadFollowsSelection: true, mode: "edit", theme: "dark", keymap: DEFAULT_KEYMAP, split: NO_SPLIT };

const STORAGE_KEY = "mazzalab.preferences";

export function loadPreferences(): Preferences {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Partial<Preferences>;
    return {
      playheadFollowsSelection:
        typeof saved.playheadFollowsSelection === "boolean" ? saved.playheadFollowsSelection : DEFAULT_PREFERENCES.playheadFollowsSelection,
      mode: saved.mode === "show" ? "show" : "edit",
      theme: normalizeTheme(saved.theme),
      keymap: normalizeKeyMap(saved.keymap),
      split: normalizeSplit(saved.split),
    };
  } catch {
    // Storage can be unavailable (private mode) or hold something else.
    return DEFAULT_PREFERENCES;
  }
}

export function savePreferences(preferences: Preferences): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences));
  } catch {
    // Not persisted; the choice still applies to this session.
  }
}
