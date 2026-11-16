import type { Cue } from "./api/types";
import { flatten, isAudioFile } from "./cues";

/** What a media item carries while it is dragged: a JSON list of paths relative to the show. */
export const MEDIA_DRAG = "application/x-mazzalab-media";

/** A file of the show's media folder, as `GET /api/media` lists it. */
export interface MediaFile {
  /** The path relative to the show, as a cue's `file` names it. */
  file: string;
  name: string;
  size: number;
}

export type MediaKind = "audio" | "video" | "other";

const VIDEO_EXTENSIONS = ["mp4", "m4v", "mov", "mkv", "avi", "mpg", "mpeg", "wmv", "mxf", "ogv", "webm"];

/** What a file is for: an Audio cue, a Video cue, or neither. */
export function mediaKind(name: string): MediaKind {
  if (isAudioFile(name)) return "audio";
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  return VIDEO_EXTENSIONS.includes(extension) ? "video" : "other";
}

/** A path with one kind of separator, so `media\a.wav` and `media/a.wav` are the same file. */
export function normalizePath(path: string): string {
  return path.replace(/\\/g, "/");
}

/** The cues that play each file, by the file's path; a cue counts once. */
export function usageByFile(files: readonly MediaFile[], cues: Cue[]): Map<string, Cue[]> {
  const usage = new Map<string, Cue[]>(files.map((file) => [normalizePath(file.file), []]));
  for (const { cue } of flatten(cues)) {
    if ((cue.type !== "audio" && cue.type !== "video") || !cue.file) continue;
    usage.get(normalizePath(cue.file))?.push(cue);
  }
  return usage;
}

export type MediaFilter = { text: string; kind: "all" | "audio" | "video" | "unused" };

/** The files that pass the filter: part of the name, and the kind or being unused. */
export function filterMedia(files: readonly MediaFile[], usage: Map<string, Cue[]>, filter: MediaFilter): MediaFile[] {
  const words = filter.text.toLowerCase().split(/\s+/).filter(Boolean);
  return files.filter((file) => {
    const name = file.name.toLowerCase();
    if (!words.every((word) => name.includes(word))) return false;
    if (filter.kind === "unused") return (usage.get(normalizePath(file.file)) ?? []).length === 0;
    return filter.kind === "all" || mediaKind(file.name) === filter.kind;
  });
}

/** `1.4 MB`, `812 kB`, `37 B`. */
export function formatSize(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  const units = ["kB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = -1;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  return `${value >= 100 ? Math.round(value) : Math.round(value * 10) / 10} ${units[unit]}`;
}
