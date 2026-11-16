import { describe, expect, it } from "vitest";
import type { Cue } from "./api/types";
import { filterMedia, formatSize, mediaKind, normalizePath, usageByFile, type MediaFile } from "./mediaLibrary";

const files: MediaFile[] = [
  { file: "media/Rain.wav", name: "Rain.wav", size: 1_400_000 },
  { file: "media/Intro.mp4", name: "Intro.mp4", size: 52_000_000 },
  { file: "media/notes.pdf", name: "notes.pdf", size: 900 },
];
const cues = [
  { id: "a", type: "audio", file: "media/Rain.wav" },
  { id: "g", type: "group", children: [{ id: "b", type: "audio", file: "media\\Rain.wav" }] },
  { id: "c", type: "wait", duration: 1 },
] as Cue[];

describe("media library", () => {
  it("tells audio from video from the rest", () => {
    expect(mediaKind("a.WAV")).toBe("audio");
    expect(mediaKind("a.mov")).toBe("video");
    expect(mediaKind("a.pdf")).toBe("other");
  });

  it("counts the cues that use each file, groups opened", () => {
    const usage = usageByFile(files, cues);
    expect(usage.get("media/Rain.wav")?.map((cue) => cue.id)).toEqual(["a", "b"]);
    expect(usage.get("media/Intro.mp4")).toEqual([]);
    expect(normalizePath("media\\a.wav")).toBe("media/a.wav");
  });

  it("filters by name, kind and use", () => {
    const usage = usageByFile(files, cues);
    expect(filterMedia(files, usage, { text: "rain", kind: "all" }).map((file) => file.name)).toEqual(["Rain.wav"]);
    expect(filterMedia(files, usage, { text: "", kind: "video" }).map((file) => file.name)).toEqual(["Intro.mp4"]);
    expect(filterMedia(files, usage, { text: "", kind: "unused" }).map((file) => file.name)).toEqual(["Intro.mp4", "notes.pdf"]);
    expect(filterMedia(files, usage, { text: "  ", kind: "all" })).toHaveLength(3);
  });

  it("writes sizes short", () => {
    expect(formatSize(37)).toBe("37 B");
    expect(formatSize(812_000)).toBe("812 kB");
    expect(formatSize(1_400_000)).toBe("1.4 MB");
    expect(formatSize(52_000_000)).toBe("52 MB");
  });
});
