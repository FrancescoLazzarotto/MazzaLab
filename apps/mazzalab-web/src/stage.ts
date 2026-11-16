import type { CSSProperties } from "react";
import type { FadeCurve, Fit, Geometry, Stage, StageItem, TextStyle, WorkspaceSettings } from "./api/types";

export interface Size {
  width: number;
  height: number;
}

/** A stage's size when it names none. */
export const DEFAULT_STAGE_SIZE: Size = { width: 1920, height: 1080 };

/** How far into its action a stage item is at `now`, on the server's clock. */
export function positionAt(item: StageItem, now: number): number {
  return item.paused ? item.offset : item.offset + Math.max(0, now - item.started);
}

/** How far through a fade is at `now`, 0 to 1, along its curve. */
export function fadeProgress(fade: { started: number; duration: number; curve?: FadeCurve }, now: number): number {
  if (fade.duration <= 0) return 1;
  const t = Math.min(1, Math.max(0, (now - fade.started) / fade.duration));
  return fade.curve === "s_curve" ? t * t * (3 - 2 * t) : t;
}

/** The CSS easing that draws a fade's curve: the S-curve is exactly this Bézier. */
export function fadeEasing(curve: FadeCurve | undefined): string {
  return curve === "s_curve" ? "cubic-bezier(0.333, 0, 0.667, 1)" : "linear";
}

/** Its opacity at `now`, part way through a fade. */
export function opacityAt(item: StageItem, now: number): number {
  const fade = item.fade;
  if (!fade) return item.opacity;
  return fade.from + (item.opacity - fade.from) * fadeProgress(fade, now);
}

/** A geometry with every value filled in. */
export function fullGeometry(geometry: Geometry | undefined): Required<Geometry> {
  return {
    fit: geometry?.fit ?? "fit",
    x: geometry?.x ?? 0,
    y: geometry?.y ?? 0,
    scale_x: geometry?.scale_x ?? 1,
    scale_y: geometry?.scale_y ?? 1,
    rotation: geometry?.rotation ?? 0,
    crop_top: geometry?.crop_top ?? 0,
    crop_right: geometry?.crop_right ?? 0,
    crop_bottom: geometry?.crop_bottom ?? 0,
    crop_left: geometry?.crop_left ?? 0,
  };
}

/** A geometry as a show file keeps it: only what differs from the default, or nothing at all. */
export function compactGeometry(geometry: Geometry): Geometry | undefined {
  const full = fullGeometry(geometry);
  const defaults = fullGeometry(undefined);
  const kept = Object.fromEntries(Object.entries(full).filter(([key, value]) => value !== defaults[key as keyof Geometry]));
  return Object.keys(kept).length > 0 ? (kept as Geometry) : undefined;
}

/** Its geometry at `now`, part way through a fade. */
export function geometryAt(item: StageItem, now: number): Required<Geometry> {
  const to = fullGeometry(item.geometry);
  const fade = item.fade;
  if (!fade) return to;
  const from = fullGeometry(fade.from_geometry);
  const t = fadeProgress(fade, now);
  const mix = (a: number, b: number) => a + (b - a) * t;
  return {
    fit: to.fit,
    x: mix(from.x, to.x),
    y: mix(from.y, to.y),
    scale_x: mix(from.scale_x, to.scale_x),
    scale_y: mix(from.scale_y, to.scale_y),
    rotation: mix(from.rotation, to.rotation),
    crop_top: mix(from.crop_top, to.crop_top),
    crop_right: mix(from.crop_right, to.crop_right),
    crop_bottom: mix(from.crop_bottom, to.crop_bottom),
    crop_left: mix(from.crop_left, to.crop_left),
  };
}

/**
 * The box a picture of size `media` takes on a stage before it is moved:
 * centred, and as large as its fit says. Until the picture's size is known
 * it covers the stage.
 */
export function pictureBox(fit: Fit, stage: Size, media: Size | null): Size {
  if (!media || media.width <= 0 || media.height <= 0 || fit === "stretch") return stage;
  if (fit === "native") return media;
  const across = stage.width / media.width;
  const down = stage.height / media.height;
  const scale = fit === "fill" ? Math.max(across, down) : Math.min(across, down);
  return { width: media.width * scale, height: media.height * scale };
}

/** Where a picture's box goes on its stage: centred, then moved, turned, scaled and cropped. */
export function pictureStyle(geometry: Required<Geometry>, box: Size, stage: Size): CSSProperties {
  return {
    left: (stage.width - box.width) / 2,
    top: (stage.height - box.height) / 2,
    width: box.width,
    height: box.height,
    transform: transformOf(geometry),
    clipPath: clipOf(geometry),
  };
}

/** Where a text goes on its stage: its own box, centred, then moved, turned and scaled. */
export function textStyle(geometry: Required<Geometry>, style: TextStyle | undefined, stage: Size): CSSProperties {
  const background = style?.background;
  return {
    left: stage.width / 2,
    top: stage.height / 2,
    // As wide as its longest line, up to the stage's width, unless it has a width of its own.
    width: style?.width ?? "max-content",
    maxWidth: style?.width === undefined ? stage.width : undefined,
    transform: `translate(-50%, -50%) ${transformOf(geometry)}`,
    clipPath: clipOf(geometry),
    fontFamily: style?.font ?? "sans-serif",
    fontSize: style?.size ?? 72,
    color: style?.color ?? "#ffffff",
    textAlign: style?.align ?? "center",
    fontWeight: style?.bold ? 700 : 400,
    fontStyle: style?.italic ? "italic" : "normal",
    lineHeight: style?.line_height ?? 1.2,
    background,
    padding: background ? "0.2em 0.45em" : undefined,
  };
}

function transformOf(geometry: Required<Geometry>): string {
  return `translate(${geometry.x}px, ${geometry.y}px) rotate(${geometry.rotation}deg) scale(${geometry.scale_x}, ${geometry.scale_y})`;
}

function clipOf(geometry: Required<Geometry>): string {
  const percent = (value: number) => `${Math.round(Math.min(0.49, Math.max(0, value)) * 10000) / 100}%`;
  return `inset(${percent(geometry.crop_top)} ${percent(geometry.crop_right)} ${percent(geometry.crop_bottom)} ${percent(geometry.crop_left)})`;
}

/**
 * Where in the file a video should be, `position` seconds into its cue:
 * through its trims and rate, round its loop. `null` once it has played
 * out; a held video stays on its last frame instead.
 */
export function mediaTime(item: Extract<StageItem, { kind: "video" }>, position: number, duration: number | undefined): number | null {
  const start = item.start_time;
  const end = item.end_time ?? duration;
  const travelled = position * item.rate;
  if (end === undefined) return start + travelled;
  const length = end - start;
  if (length <= 0) return start;
  if (item.looped) return start + (travelled % length);
  if (travelled < length) return start + travelled;
  return item.hold ? end : null;
}

/** The show's stages; without any, the one called "Main". */
export function stageNames(settings: Pick<WorkspaceSettings, "stages">): string[] {
  const names = (settings.stages ?? []).map((stage) => stage.name.trim()).filter(Boolean);
  return names.length > 0 ? names : ["Main"];
}

/** The size of the stage called `name`, or the usual one. */
export function stageSize(stages: Stage[] | undefined, name: string | undefined): Size {
  const stage = (stages ?? []).find((candidate) => candidate.name.trim() === name) ?? (name === undefined ? stages?.[0] : undefined);
  const valid = (value: number | undefined, fallback: number) => (value !== undefined && Number.isFinite(value) && value > 0 ? value : fallback);
  return { width: valid(stage?.width, DEFAULT_STAGE_SIZE.width), height: valid(stage?.height, DEFAULT_STAGE_SIZE.height) };
}

/** The address that turns a window into a stage, keeping the server this one talks to. */
export function stageUrl(name: string, monitor = false, location: Pick<Location, "pathname" | "search"> = window.location): string {
  const params = new URLSearchParams();
  const server = new URLSearchParams(location.search).get("server");
  if (server) params.set("server", server);
  params.set("stage", name);
  if (monitor) params.set("monitor", "");
  return `${location.pathname}?${params.toString().replace(/monitor=$/, "monitor")}`;
}
