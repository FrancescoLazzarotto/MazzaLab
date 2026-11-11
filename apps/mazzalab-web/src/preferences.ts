/** Choices that belong to this device rather than to the show. */
export interface Preferences {
  /** Selecting a cue also makes it the next one to GO, as in QLab. */
  playheadFollowsSelection: boolean;
  /** Show mode locks editing, so a stray key cannot change the show. */
  mode: "edit" | "show";
}

export const DEFAULT_PREFERENCES: Preferences = { playheadFollowsSelection: true, mode: "edit" };

const STORAGE_KEY = "mazzalab.preferences";

export function loadPreferences(): Preferences {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Partial<Preferences>;
    return {
      playheadFollowsSelection:
        typeof saved.playheadFollowsSelection === "boolean" ? saved.playheadFollowsSelection : DEFAULT_PREFERENCES.playheadFollowsSelection,
      mode: saved.mode === "show" ? "show" : "edit",
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
