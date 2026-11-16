import type { Cue, CueColor, CueType, Id } from "./api/types";
import { displayName, flatten } from "./cues";

/** What the operator narrowed the cue list down to. Empty fields filter nothing. */
export interface CueFilter {
  text: string;
  type: CueType | null;
  /** `"none"` picks the cues without a colour. */
  color: CueColor | "none" | null;
  flagged: boolean;
}

export const NO_FILTER: CueFilter = { text: "", type: null, color: null, flagged: false };

export function isFiltering(filter: CueFilter): boolean {
  return filter.text.trim() !== "" || filter.type !== null || filter.color !== null || filter.flagged;
}

/** Lower case and without accents, so "e" finds "è" and typing never needs shift. */
function fold(text: string): string {
  return text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

/** Whether one cue passes every part of the filter; the text may match the number, name or notes. */
export function matches(cue: Cue, filter: CueFilter): boolean {
  if (filter.type !== null && cue.type !== filter.type) return false;
  if (filter.color !== null && (cue.color ?? "none") !== filter.color) return false;
  if (filter.flagged && !cue.flagged) return false;
  const words = fold(filter.text).split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const haystack = fold([cue.number ?? "", displayName(cue), cue.notes ?? ""].join("\n"));
  return words.every((word) => haystack.includes(word));
}

/**
 * The cues to show: those that match, plus the groups they sit in so they keep
 * their place. Null when nothing is filtered, meaning every cue is shown.
 */
export function visibleIds(cues: Cue[], filter: CueFilter): Set<Id> | null {
  if (!isFiltering(filter)) return null;
  const shown = new Set<Id>();
  for (const { cue } of flatten(cues)) if (matches(cue, filter)) shown.add(cue.id);
  // Ancestors of a match stay, found by walking the tree once more.
  const keepAncestors = (list: Cue[]): boolean => {
    let any = false;
    for (const cue of list) {
      const inside = cue.type === "group" && keepAncestors(cue.children ?? []);
      if (inside) shown.add(cue.id);
      if (inside || shown.has(cue.id)) any = true;
    }
    return any;
  };
  keepAncestors(cues);
  return shown;
}
