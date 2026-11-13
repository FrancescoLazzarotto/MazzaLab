import { useEffect, useRef, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type { AudioCue, RunningCue, SliceMarker, Waveform as WaveformData } from "../api/types";
import { formatTime } from "../format";
import { useI18n } from "../i18n";
import { addMarker, firstReach, locate, markers, segments } from "../slices";

const BUCKETS = 1600;
/** How close, in pixels, the pointer has to be to grab a trim handle. */
const GRAB = 8;
/** Height of the time ruler above the peaks. */
const RULER = 16;
/** Distances between ruler labels, in seconds, to choose from. */
const RULER_STEPS = [0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600];

/** A trim handle, or a slice marker by its time when the drag began. */
type Handle = "start" | "end" | { marker: number };

const isMarker = (handle: Handle | undefined | null): handle is { marker: number } => typeof handle === "object" && handle !== null;
const sameHandle = (a: Handle | null | undefined, b: Handle | null | undefined) =>
  a === b || (isMarker(a) && isMarker(b) && a.marker === b.marker);

/** Where the playback position is in the file, given how long the cue has run. */
export function playbackPosition(cue: AudioCue, elapsed: number, duration: number): number {
  const all = segments(cue, duration);
  const place = locate(cue, elapsed * (cue.rate ?? 1), duration);
  if (!place) return all[all.length - 1].to;
  return all[place.segment].from + place.within;
}

/** The position in the file of a time counted from the start of the cue's pre-wait. */
export function filePosition(cue: AudioCue, time: number, duration: number): number {
  return playbackPosition(cue, Math.max(0, time - (cue.pre_wait ?? 0)), duration);
}

/**
 * The cue time (from the start of its pre-wait) at which a position in the
 * file plays first; `null` if it never does by itself, behind a slice that
 * loops until devamped.
 */
export function cueTime(cue: AudioCue, position: number, duration = cue.media_duration ?? 0): number | null {
  const travelled = firstReach(cue, Math.max(cue.start_time ?? 0, position), duration);
  if (travelled === null) return null;
  return Math.round(((cue.pre_wait ?? 0) + travelled / (cue.rate ?? 1)) * 1000) / 1000;
}

/** The smallest step that keeps ruler labels at least `minPixels` apart. */
export function rulerStep(duration: number, width: number, minPixels = 72): number {
  if (duration <= 0 || width <= 0) return RULER_STEPS[RULER_STEPS.length - 1];
  return RULER_STEPS.find((step) => (step / duration) * width >= minPixels) ?? RULER_STEPS[RULER_STEPS.length - 1];
}

/** Ruler labels: `0:05`, with tenths only when the step needs them. */
function rulerLabel(seconds: number, step: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds - minutes * 60;
  const text = step < 1 ? rest.toFixed(1).padStart(4, "0") : String(Math.round(rest)).padStart(2, "0");
  return `${minutes}:${text}`;
}

/**
 * The cue's audio file drawn as peaks under a time ruler, with the part that
 * plays highlighted, the built-in fades sketched on top, draggable start and
 * end handles, and a cursor following playback.
 */
export function Waveform({
  connection,
  cue,
  run,
  loadedAt,
  editable,
  onChange,
  onLoad,
}: {
  connection: ShowConnection;
  cue: AudioCue;
  run: RunningCue | undefined;
  /** Where the cue is loaded to, in seconds from the start of its pre-wait. */
  loadedAt: number | undefined;
  editable: boolean;
  onChange: (patch: { start_time?: number; end_time?: number; slices?: SliceMarker[] }) => void;
  /** The waveform was clicked: load the cue to that time. */
  onLoad: (time: number) => void;
}) {
  const { t } = useI18n();
  const canvas = useRef<HTMLCanvasElement>(null);
  const [data, setData] = useState<WaveformData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  /** A trim being dragged, not yet sent to the server. */
  const [draft, setDraft] = useState<{ handle: Handle; time: number } | null>(null);
  const [hover, setHover] = useState<Handle | null>(null);
  const grab = useRef<{ handle: Handle; moved: boolean } | null>(null);

  const { id, file } = cue;
  useEffect(() => {
    setData(null);
    setError(null);
    if (!file) return;
    const abort = new AbortController();
    setLoading(true);
    connection
      .waveform(id, BUCKETS, abort.signal)
      .then((waveform) => setData(waveform))
      .catch((reason: unknown) => {
        if (!abort.signal.aborted) setError(reason instanceof Error ? reason.message : String(reason));
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoading(false);
      });
    return () => abort.abort();
  }, [connection, id, file]);

  const duration = data?.duration ?? cue.media_duration ?? 0;
  const start = draft?.handle === "start" ? draft.time : (cue.start_time ?? 0);
  const end = draft?.handle === "end" ? draft.time : (cue.end_time ?? duration);
  // The cue as drawn: with the handle being dragged where it is now.
  const dragged = draft && isMarker(draft.handle) ? draft.handle.marker : null;
  const moved = (cue.slices ?? []).map((marker) => (marker.time === dragged && draft ? { ...marker, time: draft.time } : marker));
  const shown: AudioCue = {
    ...cue,
    start_time: start,
    end_time: draft?.handle === "end" ? draft.time : cue.end_time,
    slices: moved,
  };
  const cuts = markers(shown, duration);

  // Remember when the last status arrived, to move the cursor smoothly
  // between updates.
  const runRef = useRef<{ run: RunningCue; at: number } | null>(null);
  runRef.current = run && run.phase === "action" ? { run, at: runRef.current?.run === run ? runRef.current.at : performance.now() } : null;
  const playing = runRef.current !== null;

  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    let frame = 0;
    const draw = () => {
      frame = 0;
      const context = element.getContext("2d");
      if (!context) return;
      const ratio = window.devicePixelRatio || 1;
      const width = element.clientWidth;
      const height = element.clientHeight;
      if (element.width !== Math.round(width * ratio) || element.height !== Math.round(height * ratio)) {
        element.width = Math.round(width * ratio);
        element.height = Math.round(height * ratio);
      }
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      context.clearRect(0, 0, width, height);
      const style = getComputedStyle(element);
      const color = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
      const colors = {
        wave: color("--wave", "#9fb0c6"),
        waveOff: color("--wave-off", "#3b4047"),
        line: color("--hairline", "#232427"),
        tick: color("--border", "#303236"),
        label: color("--text-3", "#6d7076"),
        handle: color("--text", "#e9e9ea"),
        active: color("--focus", "#5b9dff"),
        fade: color("--text-2", "#a2a4a9"),
        cursor: color("--go", "#2fbf71"),
        load: color("--warn", "#dba93c"),
      };

      const x = (time: number) => (duration > 0 ? (time / duration) * width : 0);
      const top = RULER;
      const middle = top + (height - top) / 2;
      const half = (height - top) / 2 - 3;

      // Ruler.
      context.fillStyle = colors.line;
      context.fillRect(0, top - 1, width, 1);
      if (duration > 0) {
        const step = rulerStep(duration, width);
        context.font = '10px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
        context.textBaseline = "middle";
        context.textAlign = "left";
        for (let time = 0; time < duration; time += step) {
          const position = Math.round(x(time));
          context.fillStyle = colors.tick;
          context.fillRect(position, top - 5, 1, 5);
          if (width - position > 34) {
            context.fillStyle = colors.label;
            context.fillText(rulerLabel(time, step), position + 4, top / 2);
          }
        }
      }

      // Peaks: the part that plays stands out from what is trimmed away.
      context.fillStyle = colors.line;
      context.fillRect(0, Math.round(middle), width, 1);
      if (data && data.peaks.length > 0) {
        const count = data.peaks.length / 2;
        const startX = x(start);
        const endX = x(end);
        for (let column = 0; column < width; column++) {
          const first = Math.floor((column / width) * count);
          const last = Math.max(first + 1, Math.floor(((column + 1) / width) * count));
          let low = 127;
          let high = -127;
          for (let bucket = first; bucket < last && bucket < count; bucket++) {
            low = Math.min(low, data.peaks[bucket * 2]);
            high = Math.max(high, data.peaks[bucket * 2 + 1]);
          }
          const from = middle - (high / 127) * half;
          const to = middle - (low / 127) * half;
          context.fillStyle = column >= startX && column <= endX ? colors.wave : colors.waveOff;
          context.fillRect(column, from, 1, Math.max(1, to - from));
        }
      }

      if (duration > 0) {
        // Built-in fades, as the gain they apply over the played part.
        const length = Math.max(0, end - start);
        const fadeIn = Math.min(cue.fade_in ?? 0, length);
        const fadeOut = Math.min(cue.fade_out ?? 0, length);
        if (fadeIn > 0 || fadeOut > 0) {
          const ceiling = top + 3;
          const floor = height - 2;
          context.strokeStyle = colors.fade;
          context.lineWidth = 1;
          context.beginPath();
          context.moveTo(x(start), fadeIn > 0 ? floor : ceiling);
          context.lineTo(x(start + fadeIn), ceiling);
          context.lineTo(x(end - fadeOut), ceiling);
          context.lineTo(x(end), fadeOut > 0 ? floor : ceiling);
          context.stroke();
        }
        // Slice markers, dashed, with how often each slice plays.
        if (cuts.length > 0) {
          context.lineWidth = 1;
          context.setLineDash([3, 3]);
          for (const marker of cuts) {
            const position = Math.round(x(marker.time)) + 0.5;
            const grabbed = (draft && isMarker(draft.handle) && draft.time === marker.time) || (isMarker(hover) && hover.marker === marker.time);
            context.strokeStyle = grabbed ? colors.active : colors.fade;
            context.beginPath();
            context.moveTo(position, top);
            context.lineTo(position, height);
            context.stroke();
            context.fillStyle = grabbed ? colors.active : colors.fade;
            context.fillRect(position - 3.5, top - 4, 7, 4);
          }
          context.setLineDash([]);
          context.font = '10px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
          context.textAlign = "left";
          context.textBaseline = "bottom";
          context.fillStyle = colors.fade;
          for (const segment of segments(shown, duration)) {
            if (x(segment.to) - x(segment.from) > 24) {
              context.fillText(segment.passes === null ? "∞" : `×${segment.passes}`, x(segment.from) + 5, height - 3);
            }
          }
        }
        // Trim handles: a line through the peaks with a flag in the ruler.
        for (const [handle, time] of [
          ["start", start],
          ["end", end],
        ] as const) {
          const position = Math.min(width - 1, Math.max(0, Math.round(x(time))));
          context.fillStyle = draft?.handle === handle || hover === handle ? colors.active : colors.handle;
          context.fillRect(position, 0, 1, height);
          const direction = handle === "start" ? 1 : -1;
          context.beginPath();
          context.moveTo(position + (direction > 0 ? 1 : 0), 0);
          context.lineTo(position + direction * 7, 0);
          context.lineTo(position + (direction > 0 ? 1 : 0), 8);
          context.fill();
        }
        // Where the cue is loaded to: the next start picks up from here.
        if (loadedAt !== undefined && !runRef.current) {
          const position = Math.round(x(filePosition(cue, loadedAt, duration)));
          context.fillStyle = colors.load;
          context.fillRect(position, top, 1, height - top);
          context.beginPath();
          context.moveTo(position - 4, top);
          context.lineTo(position + 5, top);
          context.lineTo(position + 0.5, top + 6);
          context.fill();
        }
        const current = runRef.current;
        if (current) {
          const ahead = current.run.paused ? 0 : (performance.now() - current.at) / 1000;
          const position = Math.round(x(playbackPosition(shown, current.run.elapsed + ahead, duration)));
          context.fillStyle = colors.cursor;
          context.fillRect(position, top, 1, height - top);
          frame = requestAnimationFrame(draw);
        }
      }
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(draw);
    };
    schedule();
    const observer = new ResizeObserver(schedule);
    observer.observe(element);
    return () => {
      observer.disconnect();
      if (frame) cancelAnimationFrame(frame);
    };
  }, [data, duration, start, end, cue, draft, hover, playing, loadedAt, shown, cuts]);

  const timeAt = (event: { clientX: number }) => {
    const element = canvas.current;
    if (!element || duration <= 0) return 0;
    const bounds = element.getBoundingClientRect();
    return Math.min(duration, Math.max(0, ((event.clientX - bounds.left) / bounds.width) * duration));
  };
  const handleAt = (event: { clientX: number }): Handle | null => {
    const element = canvas.current;
    if (!element || duration <= 0) return null;
    const bounds = element.getBoundingClientRect();
    const position = event.clientX - bounds.left;
    const distance = (time: number) => Math.abs((time / duration) * bounds.width - position);
    const [toStart, toEnd] = [distance(start), distance(end)];
    // Markers sit inside the region, so they win over a trim that is as close.
    const marker = cuts.reduce<SliceMarker | null>((best, cut) => (distance(cut.time) <= GRAB && (!best || distance(cut.time) < distance(best.time)) ? cut : best), null);
    if (marker && distance(marker.time) <= Math.min(toStart, toEnd)) return { marker: marker.time };
    if (Math.min(toStart, toEnd) > GRAB) return null;
    // When both handles are within reach, take the one the pointer is
    // outside of, so that neither gets stuck behind the other.
    if (toStart <= GRAB && toEnd <= GRAB) return position <= (start / duration) * bounds.width ? "start" : "end";
    return toStart < toEnd ? "start" : "end";
  };

  // Keep at least a hundredth of a second between the handles.
  const dragTime = (handle: Handle, time: number) => {
    const fixedStart = cue.start_time ?? 0;
    const fixedEnd = cue.end_time ?? duration;
    if (isMarker(handle)) return Math.min(fixedEnd - 0.01, Math.max(fixedStart + 0.01, time));
    return handle === "start" ? Math.min(time, fixedEnd - 0.01) : Math.max(time, fixedStart + 0.01);
  };

  const interactive = editable && duration > 0;
  const summary = !file
    ? t("waveform.noFile")
    : error
      ? error
      : loading
        ? t("waveform.loading")
        : data
          ? `${data.channels} ch · ${(data.sample_rate / 1000).toFixed(1)} kHz`
          : "";
  return (
    <div className="waveform">
      <canvas
        ref={canvas}
        className={hover || draft ? "trimming" : undefined}
        onPointerDown={(event) => {
          if (duration <= 0) return;
          const time = timeAt(event);
          // Alt-click cuts a slice there.
          if (event.altKey) {
            if (interactive && time > start + 0.01 && time < end - 0.01) onChange({ slices: addMarker(cue, time) });
            return;
          }
          const handle = interactive ? handleAt(event) : null;
          if (!handle) {
            // Anywhere else in the part that plays loads the cue there.
            const at = time >= start && time <= end ? cueTime(cue, time, duration) : null;
            if (at !== null) onLoad(at);
            return;
          }
          event.currentTarget.setPointerCapture(event.pointerId);
          grab.current = { handle, moved: false };
          setDraft({ handle, time: isMarker(handle) ? handle.marker : handle === "start" ? start : end });
        }}
        onPointerMove={(event) => {
          if (!interactive) return;
          // The grab is kept outside React state, so that a move right after
          // the press is not lost before the next render.
          const held = grab.current;
          if (!held) {
            const next = handleAt(event);
            if (!sameHandle(next, hover)) setHover(next);
            return;
          }
          held.moved = true;
          setDraft({ handle: held.handle, time: dragTime(held.handle, timeAt(event)) });
        }}
        onPointerUp={(event) => {
          const held = grab.current;
          grab.current = null;
          setDraft(null);
          // A click on a handle without moving it changes nothing.
          if (!held?.moved) return;
          const time = Math.round(dragTime(held.handle, timeAt(event)) * 1000) / 1000;
          if (isMarker(held.handle)) {
            const from = held.handle.marker;
            const others = (cue.slices ?? []).filter((marker) => marker.time !== from);
            const kept = (cue.slices ?? []).find((marker) => marker.time === from);
            // Dropped onto another marker, the two become one.
            if (time !== from && kept) {
              onChange({
                slices: [...others.filter((marker) => marker.time !== time), { ...kept, time }].sort((a, b) => a.time - b.time),
              });
            }
          } else if (held.handle === "start") onChange({ start_time: Math.max(0, time) });
          // An end at the very end of the file means "play to the end".
          else onChange({ end_time: time >= duration - 0.005 ? undefined : time });
        }}
        onPointerLeave={() => setHover(null)}
      />
      <div className="waveform-foot">
        <span>{summary}</span>
        {duration > 0 && (
          <span>
            {formatTime(start)} – {formatTime(end)} <span className="waveform-length">({formatTime(Math.max(0, end - start))})</span>
          </span>
        )}
      </div>
    </div>
  );
}
