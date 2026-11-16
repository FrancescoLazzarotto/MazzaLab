import type { EngineEvent, Id, Others, OthersAction } from "./api/types";

export type OthersMode = "off" | OthersAction;

export const DEFAULT_DUCK_DB = 10;

/** What a cue does to the others, as the inspector's first choice puts it. */
export function othersMode(others: Others | undefined): OthersMode {
  return others ? others.action : "off";
}

/**
 * The `others` setting after the mode is changed: off removes it; a mode
 * keeps the scope and fade already chosen, and starts from sensible values.
 */
export function withMode(others: Others | undefined, mode: OthersMode): Others | undefined {
  if (mode === "off") return undefined;
  return {
    action: mode,
    scope: others?.scope ?? "list",
    fade: others?.fade ?? (mode === "duck" ? 1 : 2),
    duck_db: others?.duck_db ?? DEFAULT_DUCK_DB,
  };
}

/** A duck amount as the engine wants it: a positive number of dB. */
export function cleanDuck(db: number): number {
  return Number.isFinite(db) && db > 0 ? Math.round(db * 10) / 10 : DEFAULT_DUCK_DB;
}

/** `−10 dB`, with a real minus sign. */
export function duckLabel(db: number): string {
  return `−${Math.round(db * 10) / 10} dB`;
}

/** Which cues are being faded out by another, and which are ducked, and by how much. */
export interface OthersState {
  ducked: ReadonlyMap<Id, number>;
  fading: ReadonlySet<Id>;
}

export const NO_OTHERS: OthersState = { ducked: new Map(), fading: new Set() };

/** Follows the engine's events: a cue is ducked from `ducked` until `unducked`, or until it ends. */
export function applyOthersEvents(state: OthersState, events: readonly EngineEvent[]): OthersState {
  let ducked: Map<Id, number> | null = null;
  let fading: Set<Id> | null = null;
  const ducks = () => (ducked ??= new Map(state.ducked));
  const fades = () => (fading ??= new Set(state.fading));
  const nowDucked = (): ReadonlyMap<Id, number> => ducked ?? state.ducked;
  const nowFading = (): ReadonlySet<Id> => fading ?? state.fading;
  for (const event of events) {
    switch (event.kind) {
      case "ducked":
        ducks().set(event.cue, event.by_db);
        break;
      case "unducked":
        ducks().delete(event.cue);
        break;
      case "fading_out":
        fades().add(event.cue);
        break;
      case "stopped":
      case "action_completed":
        // An ended cue is neither.
        if (nowDucked().has(event.cue)) ducks().delete(event.cue);
        if (nowFading().has(event.cue)) fades().delete(event.cue);
        break;
      default:
        break;
    }
  }
  return ducked === null && fading === null ? state : { ducked: ducked ?? state.ducked, fading: fading ?? state.fading };
}
