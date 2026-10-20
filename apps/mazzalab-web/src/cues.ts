import type { Cue, CueType, Id } from "./api/types";

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

export function displayName(cue: Cue): string {
  return cue.name && cue.name.length > 0 ? cue.name : typeLabel(cue.type);
}

/** Cue types whose action is aimed at another cue. */
export function needsTarget(type: CueType): boolean {
  return ["fade", "start", "stop", "pause", "load", "reset", "devamp", "goto", "arm", "disarm", "target"].includes(type);
}

export function newCue(type: CueType, number: string, id: Id = crypto.randomUUID()): Cue {
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
    case "audio":
    case "video":
    case "mic":
    case "camera":
    case "midi_file":
      return { ...base, type } as Cue;
    default:
      return { ...base, type } as Cue;
  }
}
