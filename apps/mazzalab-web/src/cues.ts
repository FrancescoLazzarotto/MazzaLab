import type { Command, Container, Cue, CueType, Id, Matrix } from "./api/types";

/** A cue with its nesting depth, as shown in the cue list. */
export interface Row {
  cue: Cue;
  depth: number;
  parent: Id | null;
}

export function flatten(cues: Cue[], depth = 0, parent: Id | null = null): Row[] {
  return cues.flatMap((cue) => [
    { cue, depth, parent },
    ...(cue.type === "group" ? flatten(cue.children ?? [], depth + 1, cue.id) : []),
  ]);
}

export function findCue(cues: Cue[], id: Id): Cue | undefined {
  for (const cue of cues) {
    if (cue.id === id) return cue;
    if (cue.type === "group") {
      const found = findCue(cue.children ?? [], id);
      if (found) return found;
    }
  }
  return undefined;
}

/** Action length in seconds when known; mirrors the engine's rules. */
export function actionLength(cue: Cue): number | null {
  switch (cue.type) {
    case "wait":
    case "fade":
    case "light":
    case "network":
      return cue.duration ?? 0;
    case "audio": {
      if (cue.infinite_loop) return null;
      const end = cue.end_time ?? cue.media_duration;
      if (end === undefined) return null;
      const rate = cue.rate ?? 1;
      return (Math.max(0, end - (cue.start_time ?? 0)) * Math.max(1, cue.play_count ?? 1)) / rate;
    }
    case "video":
      return cue.hold_at_end ? null : (cue.media_duration ?? null);
    case "timecode":
      return cue.duration ?? null;
    case "group":
    case "text":
    case "mic":
    case "camera":
    case "midi_file":
      return null;
    default:
      return 0;
  }
}

/** The next free whole cue number after the highest numeric one. */
export function nextNumber(cues: Cue[]): string {
  const numbers = flatten(cues)
    .map((row) => Number.parseFloat(row.cue.number ?? ""))
    .filter((value) => Number.isFinite(value));
  return String(Math.floor(Math.max(0, ...numbers)) + 1);
}

export const CUE_TYPES: CueType[] = [
  "audio",
  "video",
  "text",
  "mic",
  "camera",
  "light",
  "fade",
  "network",
  "midi",
  "timecode",
  "group",
  "start",
  "stop",
  "pause",
  "load",
  "reset",
  "devamp",
  "goto",
  "arm",
  "disarm",
  "wait",
  "memo",
  "script",
];

/** The toolbox, grouped the way operators think of cues. */
export const TOOLBOX: CueType[][] = [
  ["group", "audio", "mic", "video", "camera", "text"],
  ["light", "fade", "network", "midi", "timecode"],
  ["start", "stop", "pause", "load", "reset", "devamp", "goto", "arm", "disarm"],
  ["wait", "memo", "script"],
];

/** Cue types whose action this build really performs. */
export const WORKING_TYPES: ReadonlySet<CueType> = new Set<CueType>([
  "audio",
  "fade",
  "network",
  "group",
  "start",
  "stop",
  "pause",
  "goto",
  "arm",
  "disarm",
  "wait",
  "memo",
]);

const TYPE_LABELS: Record<CueType, string> = {
  audio: "Audio",
  video: "Video",
  text: "Text",
  mic: "Mic",
  camera: "Camera",
  light: "Light",
  fade: "Fade",
  network: "Network",
  midi: "MIDI",
  midi_file: "MIDI File",
  timecode: "Timecode",
  group: "Group",
  start: "Start",
  stop: "Stop",
  pause: "Pause",
  load: "Load",
  reset: "Reset",
  devamp: "Devamp",
  goto: "Go To",
  arm: "Arm",
  disarm: "Disarm",
  target: "Target",
  wait: "Wait",
  memo: "Memo",
  script: "Script",
};

export function typeLabel(type: CueType): string {
  return TYPE_LABELS[type];
}

/** `C:\Music\Rain 01.wav` → `Rain 01`. */
export function fileStem(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? path;
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** The explicit name, else the media file's name, else the cue type. */
export function displayName(cue: Cue): string {
  if (cue.name && cue.name.length > 0) return cue.name;
  if ((cue.type === "audio" || cue.type === "video") && cue.file) return fileStem(cue.file);
  return typeLabel(cue.type);
}

/** Cue types whose action is aimed at another cue. */
export function needsTarget(type: CueType): boolean {
  return ["fade", "start", "stop", "pause", "load", "reset", "devamp", "goto", "arm", "disarm", "target"].includes(type);
}

export function newCue(type: CueType, number: string | undefined, id: Id = crypto.randomUUID()): Cue {
  const base = { id, number, armed: true, continue_mode: "do_not_continue" as const };
  switch (type) {
    case "wait":
      return { ...base, type, duration: 5 };
    case "fade":
      return { ...base, type, duration: 5, curve: "s_curve", level_db: -60, stop_target_when_done: true };
    case "light":
      return { ...base, type, commands: [], duration: 3 };
    case "network":
      return { ...base, type, message: "" };
    case "text":
      return { ...base, type, text: "" };
    case "midi":
      return { ...base, type, message: { kind: "program_change", channel: 1, program: 0 } };
    case "timecode":
      return { ...base, type, start: "00:00:00:00" };
    case "group":
      return { ...base, type, mode: "timeline", children: [] };
    case "load":
      return { ...base, type, time: 0 };
    case "target":
      return { ...base, type, assigned_number: "" };
    case "script":
      return { ...base, type, language: "lua", source: "" };
    default:
      return { ...base, type } as Cue;
  }
}

const AUDIO_EXTENSIONS = ["wav", "wave", "aif", "aiff", "flac", "mp3", "m4a", "aac", "ogg", "oga", "opus", "caf", "mka", "webm"];

/** Whether a dropped file is something an Audio cue can play. */
export function isAudioFile(name: string): boolean {
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  return AUDIO_EXTENSIONS.includes(extension);
}

/** What stops a cue from doing anything when it runs, if something does. */
export type Problem = "no_file" | "no_target" | "missing_target" | "no_message" | "not_supported";

export function problemOf(cue: Cue, exists: (id: Id) => boolean): Problem | null {
  if (needsTarget(cue.type)) {
    if (!cue.target) return "no_target";
    if (!exists(cue.target)) return "missing_target";
  }
  if (cue.type === "audio" && !cue.file) return "no_file";
  if (cue.type === "network" && cue.message.trim() === "") return "no_message";
  if (!WORKING_TYPES.has(cue.type)) return "not_supported";
  return null;
}

/** Where a cue sits: its container, its siblings and its index among them. */
export interface Place {
  container: Container;
  siblings: Cue[];
  index: number;
}

export function locate(cues: Cue[], listId: Id, id: Id): Place | null {
  const search = (siblings: Cue[], container: Container): Place | null => {
    for (const [index, cue] of siblings.entries()) {
      if (cue.id === id) return { container, siblings, index };
      if (cue.type === "group") {
        const found = search(cue.children ?? [], { kind: "group", id: cue.id });
        if (found) return found;
      }
    }
    return null;
  };
  return search(cues, { kind: "list", id: listId });
}

/**
 * The ids among `ids` that are not inside another selected cue, in list
 * order: what "the selection" means for moving, copying and deleting.
 */
export function topLevel(cues: Cue[], ids: ReadonlySet<Id>): Id[] {
  const out: Id[] = [];
  const visit = (siblings: Cue[]) => {
    for (const cue of siblings) {
      if (ids.has(cue.id)) out.push(cue.id);
      else if (cue.type === "group") visit(cue.children ?? []);
    }
  };
  visit(cues);
  return out;
}

/** Where dragged or pasted cues land. */
export type Drop = { where: "before" | "after" | "into"; anchor: Id } | { where: "end" };

function take(cues: Cue[], id: Id): Cue | null {
  for (const [index, cue] of cues.entries()) {
    if (cue.id === id) return cues.splice(index, 1)[0];
    if (cue.type === "group") {
      const found = take(cue.children ?? [], id);
      if (found) return found;
    }
  }
  return null;
}

/**
 * The commands that move `moved` (in list order) to `drop`, keeping their
 * order. Empty when the drop makes no sense, such as a group into itself.
 */
export function moveCommands(cues: Cue[], listId: Id, moved: Id[], drop: Drop): Command[] {
  if (drop.where !== "end" && moved.includes(drop.anchor)) return [];
  // The server wants each cue's index as it will be once the cues before it
  // have moved: replay the moves on a copy to learn them.
  const tree = structuredClone(cues);
  const commands: Command[] = [];
  // Every cue dropped "after" lands right behind the anchor, so the last
  // one has to go first.
  const order = drop.where === "after" ? [...moved].reverse() : moved;
  for (const id of order) {
    const cue = take(tree, id);
    if (!cue) return [];
    let container: Container = { kind: "list", id: listId };
    let siblings = tree;
    let index: number | undefined;
    if (drop.where === "into") {
      const group = findCue(tree, drop.anchor);
      if (!group || group.type !== "group") return [];
      group.children ??= [];
      container = { kind: "group", id: group.id };
      siblings = group.children;
    } else if (drop.where !== "end") {
      const place = locate(tree, listId, drop.anchor);
      if (!place) return [];
      container = place.container;
      siblings = place.siblings;
      index = place.index + (drop.where === "after" ? 1 : 0);
    }
    siblings.splice(index ?? siblings.length, 0, cue);
    commands.push({ command: "move_cue", cue: id, container, index });
  }
  return commands;
}

/**
 * Copies of `cues` with fresh ids. Targets pointing inside the copied set
 * follow the copies; numbered cues get new numbers counting up from
 * `firstNumber`.
 */
export function cloneCues(cues: Cue[], firstNumber: number, makeId: () => Id = () => crypto.randomUUID()): Cue[] {
  const ids = new Map<Id, Id>();
  for (const row of flatten(cues)) ids.set(row.cue.id, makeId());
  let next = firstNumber;
  const copy = (cue: Cue): Cue => {
    const clone = { ...cue, id: ids.get(cue.id)! } as Cue;
    if (cue.number !== undefined) clone.number = String(next++);
    if (clone.target && ids.has(clone.target)) clone.target = ids.get(clone.target);
    if (clone.type === "group" && cue.type === "group") clone.children = (cue.children ?? []).map(copy);
    return clone;
  };
  return cues.map(copy);
}

/** The commands that insert `cues`, in order, at `drop`. */
export function insertCommands(tree: Cue[], listId: Id, cues: Cue[], drop: Drop): Command[] {
  let container: Container = { kind: "list", id: listId };
  let index: number | undefined;
  if (drop.where === "into") {
    container = { kind: "group", id: drop.anchor };
  } else if (drop.where !== "end") {
    const place = locate(tree, listId, drop.anchor);
    if (place) {
      container = place.container;
      index = place.index + (drop.where === "after" ? 1 : 0);
    }
  }
  return cues.map((cue, offset) => ({
    command: "insert_cue",
    container,
    index: index === undefined ? undefined : index + offset,
    cue,
  }));
}

/** Formats a cue number without floating point noise: 1, 1.5, 12.25. */
export function formatNumber(value: number): string {
  return String(Math.round(value * 1000) / 1000);
}

/** Numbers `ids` (in the order given) from `start` in steps of `step`. */
export function renumber(ids: Id[], start: number, step: number): Command {
  return {
    command: "set_numbers",
    numbers: ids.map((id, index) => [id, formatNumber(start + index * step)]),
  };
}

/** The routing a cue has without a matrix, as a matrix that can be edited. */
export function defaultMatrix(inputs: number, outputs: number): Matrix {
  const matrix: Matrix = Array.from({ length: inputs }, () => Array<number | null>(outputs).fill(null));
  if (outputs === 1) {
    const db = Math.round(20 * Math.log10(1 / inputs) * 10) / 10;
    for (const row of matrix) row[0] = db;
  } else if (inputs === 1) {
    matrix[0][0] = 0;
    matrix[0][1] = 0;
  } else {
    for (let channel = 0; channel < Math.min(inputs, outputs); channel++) matrix[channel][channel] = 0;
  }
  return matrix;
}

/** A cue's matrix resized to the given shape, starting from the default. */
export function matrixFor(matrix: Matrix | undefined, inputs: number, outputs: number): Matrix {
  if (!matrix) return defaultMatrix(inputs, outputs);
  return Array.from({ length: inputs }, (_, input) =>
    Array.from({ length: outputs }, (_, output) => matrix[input]?.[output] ?? null),
  );
}
