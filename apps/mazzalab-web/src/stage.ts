import type { StageItem, WorkspaceSettings } from "./api/types";

/** How far into its action a stage item is at `now`, on the server's clock. */
export function positionAt(item: StageItem, now: number): number {
  return item.paused ? item.offset : item.offset + Math.max(0, now - item.started);
}

/** Its opacity at `now`, part way through a fade. */
export function opacityAt(item: StageItem, now: number): number {
  const fade = item.fade;
  if (!fade || fade.duration <= 0 || now >= fade.started + fade.duration) return item.opacity;
  const t = Math.min(1, Math.max(0, (now - fade.started) / fade.duration));
  return fade.from + (item.opacity - fade.from) * t;
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

/** The address that turns a window into a stage, keeping the server this one talks to. */
export function stageUrl(name: string, monitor = false, location: Pick<Location, "pathname" | "search"> = window.location): string {
  const params = new URLSearchParams();
  const server = new URLSearchParams(location.search).get("server");
  if (server) params.set("server", server);
  params.set("stage", name);
  if (monitor) params.set("monitor", "");
  return `${location.pathname}?${params.toString().replace(/monitor=$/, "monitor")}`;
}
