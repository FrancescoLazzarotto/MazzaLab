/** Whether two cue lists are shown side by side on this device, and which. */
export interface Split {
  on: boolean;
  /** The list in the left pane and in the right pane; null until chosen. */
  ids: [string | null, string | null];
}

export const NO_SPLIT: Split = { on: false, ids: [null, null] };

const idOrNull = (value: unknown): string | null => (typeof value === "string" && value !== "" ? value : null);

export function normalizeSplit(saved: unknown): Split {
  if (typeof saved !== "object" || saved === null) return NO_SPLIT;
  const { on, ids } = saved as { on?: unknown; ids?: unknown };
  const pair = Array.isArray(ids) ? ids : [];
  return { on: on === true, ids: [idOrNull(pair[0]), idOrNull(pair[1])] };
}

/**
 * The two lists the panes show, or null when there is nothing to split: it
 * needs two lists. A pane whose list is gone, or that would show the same
 * list as the other, takes the first list not shown in the other.
 */
export function resolveSplit(split: Split, listIds: string[]): [string, string] | null {
  if (!split.on || listIds.length < 2) return null;
  const known = (id: string | null) => (id !== null && listIds.includes(id) ? id : null);
  const left = known(split.ids[0]) ?? listIds[0];
  const right = known(split.ids[1]);
  const other = right !== null && right !== left ? right : (listIds.find((id) => id !== left) as string);
  return [left, other];
}

/** Shows `list` in a pane; if the other pane has it, the two swap lists. */
export function showInPane(current: [string, string], pane: 0 | 1, list: string): [string, string] {
  if (current[pane] === list) return current;
  const next: [string, string] = [...current];
  if (current[1 - pane] === list) next[1 - pane] = current[pane];
  next[pane] = list;
  return next;
}

/** Turns the split on with the open list on the left and another beside it, or off. */
export function toggleSplit(split: Split, open: string, listIds: string[]): Split {
  if (split.on) return { ...split, on: false };
  const resolved = resolveSplit({ on: true, ids: [open, split.ids[1]] }, listIds);
  return resolved ? { on: true, ids: resolved } : split;
}
