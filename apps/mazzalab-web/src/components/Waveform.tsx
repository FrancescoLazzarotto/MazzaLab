import { useEffect, useRef, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type { AudioCue, RunningCue, Waveform as WaveformData } from "../api/types";
import { formatTime } from "../format";
import { useI18n } from "../i18n";

const BUCKETS = 1600;
/** How close, in pixels, the pointer has to be to grab a trim handle. */
const GRAB = 8;

type Handle = "start" | "end";

/** Where the playback position is in the file, given how long the cue has run. */
export function playbackPosition(cue: AudioCue, elapsed: number, duration: number): number {
  const start = cue.start_time ?? 0;
  const end = cue.end_time ?? duration;
  const pass = Math.max(0, end - start);
  if (pass === 0) return start;
  const travelled = elapsed * (cue.rate ?? 1);
  const passes = cue.infinite_loop ? Infinity : Math.max(1, cue.play_count ?? 1);
  if (travelled >= pass * passes) return end;
  return start + (travelled % pass);
}

/**
 * The cue's audio file drawn as peaks, with the part that plays highlighted,
 * the built-in fades sketched on top, draggable start and end handles, and a
 * cursor following playback.
 */
export function Waveform({
  connection,
  cue,
  run,
  editable,
  onChange,
}: {
  connection: ShowConnection;
  cue: AudioCue;
  run: RunningCue | undefined;
  editable: boolean;
  onChange: (patch: { start_time?: number; end_time?: number }) => void;
}) {
  const { t } = useI18n();
  const canvas = useRef<HTMLCanvasElement>(null);
  const [data, setData] = useState<WaveformData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  /** A trim being dragged, not yet sent to the server. */
  const [draft, setDraft] = useState<{ handle: Handle; time: number } | null>(null);
  const [hover, setHover] = useState<Handle | null>(null);

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
      const accent = color("--accent", "#3fb68b");
      const muted = color("--line", "#2e333b");
      const text = color("--text", "#e7e9ec");
      const warn = color("--warn", "#e0a43a");

      const x = (time: number) => (duration > 0 ? (time / duration) * width : 0);
      const middle = height / 2;
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
          const top = middle - (high / 127) * (middle - 2);
          const bottom = middle - (low / 127) * (middle - 2);
          context.fillStyle = column >= startX && column <= endX ? accent : muted;
          context.fillRect(column, top, 1, Math.max(1, bottom - top));
        }
      } else {
        context.fillStyle = muted;
        context.fillRect(0, middle, width, 1);
      }

      if (duration > 0) {
        // Built-in fades, as the gain they apply over the played part.
        const length = Math.max(0, end - start);
        const fadeIn = Math.min(cue.fade_in ?? 0, length);
        const fadeOut = Math.min(cue.fade_out ?? 0, length);
        if (fadeIn > 0 || fadeOut > 0) {
          context.strokeStyle = warn;
          context.lineWidth = 1.5;
          context.beginPath();
          context.moveTo(x(start), fadeIn > 0 ? height - 2 : 2);
          context.lineTo(x(start + fadeIn), 2);
          context.lineTo(x(end - fadeOut), 2);
          context.lineTo(x(end), fadeOut > 0 ? height - 2 : 2);
          context.stroke();
        }
        // Trim handles.
        for (const [handle, time] of [
          ["start", start],
          ["end", end],
        ] as const) {
          const position = Math.min(width - 1, Math.max(1, x(time)));
          const active = draft?.handle === handle || hover === handle;
          context.fillStyle = active ? text : accent;
          context.fillRect(position - 1, 0, 2, height);
          context.beginPath();
          const direction = handle === "start" ? 1 : -1;
          context.moveTo(position, 0);
          context.lineTo(position + direction * 8, 0);
          context.lineTo(position, 10);
          context.fill();
        }
        const current = runRef.current;
        if (current) {
          const ahead = current.run.paused ? 0 : (performance.now() - current.at) / 1000;
          const position = x(playbackPosition(cue, current.run.elapsed + ahead, duration));
          context.fillStyle = text;
          context.fillRect(position - 0.5, 0, 1.5, height);
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
  }, [data, duration, start, end, cue, draft, hover, playing]);

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
    if (Math.min(toStart, toEnd) > GRAB) return null;
    // When both handles are within reach, take the one the pointer is
    // outside of, so that neither gets stuck behind the other.
    if (toStart <= GRAB && toEnd <= GRAB) return position <= (start / duration) * bounds.width ? "start" : "end";
    return toStart < toEnd ? "start" : "end";
  };

  const interactive = editable && duration > 0;
  return (
    <div className="waveform">
      <canvas
        ref={canvas}
        className={hover || draft ? "trimming" : undefined}
        onPointerDown={(event) => {
          if (!interactive) return;
          const handle = handleAt(event);
          if (!handle) return;
          event.currentTarget.setPointerCapture(event.pointerId);
          setDraft({ handle, time: handle === "start" ? start : end });
        }}
        onPointerMove={(event) => {
          if (!interactive) return;
          if (!draft) {
            setHover(handleAt(event));
            return;
          }
          // Keep at least a hundredth of a second between the handles.
          const time = timeAt(event);
          setDraft({
            handle: draft.handle,
            time: draft.handle === "start" ? Math.min(time, end - 0.01) : Math.max(time, start + 0.01),
          });
        }}
        onPointerUp={() => {
          if (!draft) return;
          const time = Math.round(draft.time * 1000) / 1000;
          if (draft.handle === "start") onChange({ start_time: Math.max(0, time) });
          // An end at the very end of the file means "play to the end".
          else onChange({ end_time: time >= duration - 0.005 ? undefined : time });
          setDraft(null);
        }}
        onPointerLeave={() => setHover(null)}
      />
      <div className="waveform-foot">
        <span className="mono">{formatTime(start)}</span>
        <span className="muted">
          {!file
            ? t("waveform.noFile")
            : error
              ? error
              : loading
                ? t("waveform.loading")
                : data
                  ? `${data.channels} ch · ${(data.sample_rate / 1000).toFixed(1)} kHz · ${formatTime(Math.max(0, end - start))}`
                  : ""}
        </span>
        <span className="mono">{formatTime(end)}</span>
      </div>
    </div>
  );
}
