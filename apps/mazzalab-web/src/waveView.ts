/** The stretch of the file the waveform shows: from `start`, `span` seconds long. */
export interface View {
  start: number;
  span: number;
}

/** The shortest stretch worth showing, in seconds. */
export const MIN_SPAN = 0.05;

export function wholeFile(duration: number): View {
  return { start: 0, span: Math.max(duration, MIN_SPAN) };
}

/** A view that lies inside the file and is not shorter than `MIN_SPAN`. */
export function clampView(view: View, duration: number): View {
  const span = Math.min(Math.max(view.span, Math.min(MIN_SPAN, duration)), Math.max(duration, MIN_SPAN));
  const start = Math.min(Math.max(view.start, 0), Math.max(0, duration - span));
  return { start, span };
}

/** Zooms by `factor` (above 1 closer in), keeping the time at `anchor` where it is on screen. */
export function zoomView(view: View, duration: number, factor: number, anchor: number): View {
  const span = view.span / factor;
  const fraction = view.span > 0 ? (anchor - view.start) / view.span : 0;
  return clampView({ start: anchor - fraction * span, span }, duration);
}

/** Moves the view by `delta` seconds. */
export function panView(view: View, duration: number, delta: number): View {
  return clampView({ start: view.start + delta, span: view.span }, duration);
}

/** How many times closer than the whole file the view is. */
export function zoomLevel(view: View, duration: number): number {
  return view.span > 0 ? Math.max(1, duration / view.span) : 1;
}

export function timeToX(time: number, view: View, width: number): number {
  return ((time - view.start) / view.span) * width;
}

export function xToTime(x: number, view: View, width: number): number {
  return width > 0 ? view.start + (x / width) * view.span : view.start;
}

/**
 * The view that follows two fingers: the time under each finger when the
 * pinch began stays under it, so the span scales with their distance.
 */
export function pinchView(began: View, duration: number, beganX: [number, number], nowX: [number, number], width: number): View {
  const times: [number, number] = [xToTime(beganX[0], began, width), xToTime(beganX[1], began, width)];
  const apart = nowX[1] - nowX[0];
  if (Math.abs(apart) < 1 || width <= 0 || times[1] === times[0]) return began;
  const span = ((times[1] - times[0]) / apart) * width;
  const start = times[0] - (nowX[0] / width) * span;
  return clampView({ start, span }, duration);
}

/** Buckets worth asking for at this zoom: doubling steps from `base`, so the peaks stay sharp. */
export function bucketsFor(base: number, level: number, max = 16): number {
  const steps = Math.min(Math.ceil(Math.log2(Math.max(1, level))), Math.log2(max));
  return base * 2 ** Math.max(0, steps);
}

/**
 * The fade lengths after a fade handle was dragged to `time`. A fade cannot
 * be longer than what is left of the played part after the other one.
 */
export function dragFade(kind: "fadeIn" | "fadeOut", time: number, start: number, end: number, fadeIn: number, fadeOut: number): { fade_in: number; fade_out: number } {
  const length = Math.max(0, end - start);
  const round = (value: number) => Math.round(value * 1000) / 1000;
  if (kind === "fadeIn") return { fade_in: round(Math.min(Math.max(0, time - start), Math.max(0, length - fadeOut))), fade_out: fadeOut };
  return { fade_in: fadeIn, fade_out: round(Math.min(Math.max(0, end - time), Math.max(0, length - fadeIn))) };
}
