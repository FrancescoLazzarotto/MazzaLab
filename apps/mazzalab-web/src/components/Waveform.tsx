import { useEffect, useRef, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type { AudioCue, RunningCue, SliceMarker, Waveform as WaveformData } from "../api/types";
import { formatTime } from "../format";
import { useI18n } from "../i18n";
import { addMarker, firstReach, locate, markers, segments } from "../slices";
import { bucketsFor, dragFade, panView, pinchView, timeToX, wholeFile, xToTime, zoomLevel, zoomView, type View } from "../waveView";
import { Icon } from "./icons";

const BUCKETS = 1600;
/** How close, in pixels, the pointer has to be to grab a trim handle. */
const GRAB = 8;
/** Height of the time ruler above the peaks. */
const RULER = 16;
/** Distances between ruler labels, in seconds, to choose from. */
const RULER_STEPS = [0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600];

/** A trim handle, or a slice marker by its time when the drag began. */
type Handle = "start" | "end" | "fadeIn" | "fadeOut" | { marker: number };

const isFade = (handle: Handle | undefined | null): handle is "fadeIn" | "fadeOut" => handle === "fadeIn" || handle === "fadeOut";
/** How far below the ruler the fade handles sit, in pixels. */
const FADE_BAND = 14;

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
  onChange: (patch: { start_time?: number; end_time?: number; slices?: SliceMarker[]; fade_in?: number; fade_out?: number }) => void;
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
  /** The stretch of the file on screen; the whole file until zoomed. */
  const [view, setView] = useState<View>({ start: 0, span: 1 });
  const [zoomed, setZoomed] = useState(false);
  const touches = useRef(new Map<number, number>());
  const pinch = useRef<{ began: View; xs: [number, number]; ids: [number, number] } | null>(null);

  const { id, file } = cue;
  const knownDuration = data?.duration ?? cue.media_duration ?? 0;
  const buckets = bucketsFor(BUCKETS, zoomed ? zoomLevel(view, knownDuration) : 1);
  useEffect(() => {
    setData(null);
    setError(null);
    setZoomed(false);
    setView(wholeFile(cue.media_duration ?? 0));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, file]);
  useEffect(() => {
    if (!file) return;
    const abort = new AbortController();
    setLoading(true);
    connection
      .waveform(id, buckets, abort.signal)
      .then((waveform) => setData(waveform))
      .catch((reason: unknown) => {
        if (!abort.signal.aborted) setError(reason instanceof Error ? reason.message : String(reason));
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoading(false);
      });
    return () => abort.abort();
  }, [connection, id, file, buckets]);

  const duration = knownDuration;
  // Until the file is known, or zoomed, the view is all of it.
  const shownView: View = zoomed ? view : wholeFile(duration);
  const fades = isFade(draft?.handle)
    ? dragFade(draft.handle, draft.time, cue.start_time ?? 0, cue.end_time ?? duration, cue.fade_in ?? 0, cue.fade_out ?? 0)
    : { fade_in: cue.fade_in ?? 0, fade_out: cue.fade_out ?? 0 };
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

      const x = (time: number) => (duration > 0 ? timeToX(time, shownView, width) : 0);
      const top = RULER;
      const middle = top + (height - top) / 2;
      const half = (height - top) / 2 - 3;

      // Ruler.
      context.fillStyle = colors.line;
      context.fillRect(0, top - 1, width, 1);
      if (duration > 0) {
        const step = rulerStep(shownView.span, width);
        context.font = '10px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
        context.textBaseline = "middle";
        context.textAlign = "left";
        for (let time = Math.floor(shownView.start / step) * step; time < Math.min(duration, shownView.start + shownView.span); time += step) {
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
          const first = Math.max(0, Math.floor((xToTime(column, shownView, width) / duration) * count));
          const last = Math.max(first + 1, Math.floor((xToTime(column + 1, shownView, width) / duration) * count));
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
        const fadeIn = Math.min(fades.fade_in, length);
        const fadeOut = Math.min(fades.fade_out, length);
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
        // Fade handles: small squares on the fade's ceiling, to drag along it.
        for (const [handle, time] of [
          ["fadeIn", start + fadeIn],
          ["fadeOut", end - fadeOut],
        ] as const) {
          const position = x(time);
          if (position < -4 || position > width + 4) continue;
          context.fillStyle = draft?.handle === handle || hover === handle ? colors.active : colors.fade;
          context.fillRect(Math.round(position) - 3, top + 1, 7, 7);
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
          const raw = x(time);
          if (raw < -1 || raw > width) continue;
          const position = Math.min(width - 1, Math.max(0, Math.round(raw)));
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
  }, [data, duration, start, end, cue, draft, hover, playing, loadedAt, shown, cuts, shownView, fades]);

  const timeAt = (event: { clientX: number }) => {
    const element = canvas.current;
    if (!element || duration <= 0) return 0;
    const bounds = element.getBoundingClientRect();
    return Math.min(duration, Math.max(0, xToTime(event.clientX - bounds.left, shownView, bounds.width)));
  };

  const zoomBy = (factor: number, anchor?: number) => {
    if (duration <= 0) return;
    const base = zoomed ? view : wholeFile(duration);
    const next = zoomView(base, duration, factor, anchor ?? base.start + base.span / 2);
    setView(next);
    setZoomed(next.span < duration - 1e-6);
  };

  // The wheel zooms around the pointer; with Shift, or sideways, it scrolls.
  // Native listener: React's wheel events cannot be cancelled.
  useEffect(() => {
    const element = canvas.current;
    if (!element || duration <= 0) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const bounds = element.getBoundingClientRect();
      setView((current) => {
        const base = zoomedRef.current ? current : wholeFile(duration);
        const sideways = event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY);
        const next = sideways
          ? panView(base, duration, ((event.deltaX || event.deltaY) / bounds.width) * base.span)
          : zoomView(base, duration, Math.exp(-event.deltaY * (event.ctrlKey ? 0.01 : 0.0025)), xToTime(event.clientX - bounds.left, base, bounds.width));
        zoomedRef.current = next.span < duration - 1e-6;
        setZoomed(zoomedRef.current);
        return next;
      });
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [duration]);
  const zoomedRef = useRef(zoomed);
  zoomedRef.current = zoomed;

  const handleAt = (event: { clientX: number; clientY: number }): Handle | null => {
    const element = canvas.current;
    if (!element || duration <= 0) return null;
    const bounds = element.getBoundingClientRect();
    const position = event.clientX - bounds.left;
    const distance = (time: number) => Math.abs(timeToX(time, shownView, bounds.width) - position);
    // Just under the ruler the fade handles win over the trim lines.
    const down = event.clientY - bounds.top;
    if (down >= RULER && down <= RULER + FADE_BAND) {
      const [toIn, toOut] = [distance(start + fades.fade_in), distance(end - fades.fade_out)];
      if (Math.min(toIn, toOut) <= GRAB) return toIn <= toOut ? "fadeIn" : "fadeOut";
    }
    const [toStart, toEnd] = [distance(start), distance(end)];
    // Markers sit inside the region, so they win over a trim that is as close.
    const marker = cuts.reduce<SliceMarker | null>((best, cut) => (distance(cut.time) <= GRAB && (!best || distance(cut.time) < distance(best.time)) ? cut : best), null);
    if (marker && distance(marker.time) <= Math.min(toStart, toEnd)) return { marker: marker.time };
    if (Math.min(toStart, toEnd) > GRAB) return null;
    // When both handles are within reach, take the one the pointer is
    // outside of, so that neither gets stuck behind the other.
    if (toStart <= GRAB && toEnd <= GRAB) return position <= timeToX(start, shownView, bounds.width) ? "start" : "end";
    return toStart < toEnd ? "start" : "end";
  };

  // Keep at least a hundredth of a second between the handles.
  const dragTime = (handle: Handle, time: number) => {
    const fixedStart = cue.start_time ?? 0;
    const fixedEnd = cue.end_time ?? duration;
    if (isFade(handle)) return Math.min(duration, Math.max(0, time));
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
          // A second finger turns the touch into a pinch.
          touches.current.set(event.pointerId, event.clientX);
          if (touches.current.size === 2) {
            const [a, b] = [...touches.current.entries()].sort((p, q) => p[1] - q[1]);
            const left = event.currentTarget.getBoundingClientRect().left;
            grab.current = null;
            setDraft(null);
            pinch.current = { began: zoomed ? view : wholeFile(duration), xs: [a[1] - left, b[1] - left], ids: [a[0], b[0]] };
            return;
          }
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
          setDraft({
            handle,
            time: isMarker(handle) ? handle.marker : handle === "fadeIn" ? start + fades.fade_in : handle === "fadeOut" ? end - fades.fade_out : handle === "start" ? start : end,
          });
        }}
        onPointerMove={(event) => {
          if (touches.current.has(event.pointerId)) touches.current.set(event.pointerId, event.clientX);
          const pinching = pinch.current;
          if (pinching) {
            const bounds = event.currentTarget.getBoundingClientRect();
            const now = pinching.ids.map((pointer) => (touches.current.get(pointer) ?? 0) - bounds.left) as [number, number];
            const next = pinchView(pinching.began, duration, pinching.xs, now, bounds.width);
            setView(next);
            setZoomed(next.span < duration - 1e-6);
            return;
          }
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
          touches.current.delete(event.pointerId);
          if (pinch.current) {
            if (touches.current.size < 2) pinch.current = null;
            return;
          }
          const held = grab.current;
          grab.current = null;
          setDraft(null);
          // A click on a handle without moving it changes nothing.
          if (!held?.moved) return;
          const time = Math.round(dragTime(held.handle, timeAt(event)) * 1000) / 1000;
          if (isFade(held.handle)) {
            const next = dragFade(held.handle, time, cue.start_time ?? 0, cue.end_time ?? duration, cue.fade_in ?? 0, cue.fade_out ?? 0);
            if (next.fade_in !== (cue.fade_in ?? 0) || next.fade_out !== (cue.fade_out ?? 0)) onChange(next);
          } else if (isMarker(held.handle)) {
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
        onPointerCancel={(event) => {
          touches.current.delete(event.pointerId);
          pinch.current = null;
          grab.current = null;
          setDraft(null);
        }}
      />
      {zoomed && duration > 0 && (
        <input
          type="range"
          className="wave-scroll"
          aria-label={t("waveform.scroll")}
          min={0}
          max={Math.max(0, duration - view.span)}
          step="any"
          value={view.start}
          onChange={(event) => setView({ start: Number(event.target.value), span: view.span })}
        />
      )}
      <div className="waveform-foot">
        <span>{summary}</span>
        {duration > 0 && (
          <span className="wave-zoom">
            <button className="icon-button quiet" onClick={() => zoomBy(1 / 2)} disabled={!zoomed} title={t("waveform.zoomOut")} aria-label={t("waveform.zoomOut")}>
              <Icon name="zoom-out" size={13} />
            </button>
            <button className="icon-button quiet" onClick={() => zoomBy(2)} title={t("waveform.zoomIn")} aria-label={t("waveform.zoomIn")}>
              <Icon name="zoom-in" size={13} />
            </button>
            <button className="icon-button quiet" onClick={() => zoomBy(1e-6)} disabled={!zoomed} title={t("waveform.fit")} aria-label={t("waveform.fit")}>
              <Icon name="zoom-fit" size={13} />
            </button>
          </span>
        )}
        {duration > 0 && (
          <span>
            {formatTime(start)} – {formatTime(end)} <span className="waveform-length">({formatTime(Math.max(0, end - start))})</span>
          </span>
        )}
      </div>
    </div>
  );
}
