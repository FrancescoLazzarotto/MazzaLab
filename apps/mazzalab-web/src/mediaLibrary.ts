import type { Cue } from "./api/types";
import { flatten, isAudioFile } from "./cues";

/** What a media item carries while it is dragged: a JSON list of paths relative to the show. */
export const MEDIA_DRAG = "application/x-mazzalab-media";

/** A cue and the media file it names. */
export interface MediaReference {
  cue: string;
  file: string;
}

/** A cue's file that was changed: where it was, and where it is now. */
export interface MediaMove {
  cue: string;
  from: string;
  file: string;
}

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
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  // Containers that can hold both (webm, mkv) are taken for video: that is what they are in a theatre.
  if (VIDEO_EXTENSIONS.includes(extension)) return "video";
  return isAudioFile(name) ? "audio" : "other";
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

/** The files of a list of references that are missing, each once, in order. */
export function missingFiles(missing: readonly MediaReference[]): string[] {
  return [...new Set(missing.map((entry) => entry.file))];
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
