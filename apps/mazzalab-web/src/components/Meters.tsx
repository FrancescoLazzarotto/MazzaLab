import { useEffect, useRef } from "react";
import type { ShowConnection } from "../api/connection";

const FLOOR_DB = -60;
/** How fast the bars fall once the signal drops, in dB per second. */
const FALL_DB_PER_SECOND = 36;
/** How long the peak marker and the clip light stay put, in ms. */
const HOLD_MS = 1500;
const ROW = 6;
const GAP = 4;
const LABEL = 14;
const CLIP = 5;
/** Height of the dB scale under the bars. */
const SCALE = 15;
const TICKS = [-48, -36, -24, -12, -6, 0];

interface Channel {
  level: number;
  hold: number;
  heldAt: number;
  clippedAt: number;
}

function blank(): Channel {
  return { level: FLOOR_DB, hold: FLOOR_DB, heldAt: 0, clippedAt: -Infinity };
}

/** Position of a level along the bar, 0 at the floor and 1 at full scale. */
export function meterFraction(db: number): number {
  return Math.min(1, Math.max(0, (db - FLOOR_DB) / -FLOOR_DB));
}

function heightFor(channels: number): number {
  return channels * (ROW + GAP) - GAP + SCALE;
}

/**
 * Peak meters for every output channel, drawn on a canvas so that twenty
 * updates a second cost nothing. Bars jump up with the signal and fall at a
 * steady rate; a marker holds the recent peak and a light shows clipping.
 */
export function Meters({ connection, channels }: { connection: ShowConnection; channels: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const element = canvas.current;
    if (!element || channels === 0) return;
    const state: Channel[] = Array.from({ length: channels }, blank);
    let frame = 0;
    let last = performance.now();

    const draw = (now: number) => {
      frame = 0;
      const context = element.getContext("2d");
      if (!context) return;
      const ratio = window.devicePixelRatio || 1;
      const width = element.clientWidth;
      const height = heightFor(channels);
      if (element.width !== Math.round(width * ratio) || element.height !== Math.round(height * ratio)) {
        element.width = Math.round(width * ratio);
        element.height = Math.round(height * ratio);
      }
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      context.clearRect(0, 0, width, height);

      const style = getComputedStyle(element);
      const color = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
      const colors = {
        track: color("--meter-track", "#0b0c0d"),
        low: color("--meter-low", "#3fae6f"),
        mid: color("--meter-mid", "#d9a93a"),
        high: color("--meter-high", "#e0524a"),
        text: color("--text-3", "#6d7076"),
        tick: color("--border", "#303236"),
      };
      const elapsed = Math.min(0.25, (now - last) / 1000);
      last = now;
      const barWidth = Math.max(10, width - LABEL - CLIP - 3);
      let moving = false;
      context.font = '10px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
      context.textBaseline = "middle";

      state.forEach((channel, index) => {
        channel.level = Math.max(FLOOR_DB, channel.level - FALL_DB_PER_SECOND * elapsed);
        if (now - channel.heldAt > HOLD_MS) channel.hold = Math.max(channel.level, channel.hold - FALL_DB_PER_SECOND * elapsed);
        const clipping = now - channel.clippedAt < HOLD_MS;
        if (channel.level > FLOOR_DB || channel.hold > FLOOR_DB || clipping) moving = true;

        const y = index * (ROW + GAP);
        context.textAlign = "left";
        context.fillStyle = colors.text;
        context.fillText(String(index + 1), 0, y + ROW / 2 + 0.5);
        context.fillStyle = colors.track;
        context.fillRect(LABEL, y, barWidth, ROW);

        // Green up to -12 dB, amber up to -3 dB, red above.
        const segments: [number, number, string][] = [
          [FLOOR_DB, -12, colors.low],
          [-12, -3, colors.mid],
          [-3, 0, colors.high],
        ];
        for (const [from, to, fill] of segments) {
          if (channel.level <= from) break;
          const start = meterFraction(from) * barWidth;
          const end = meterFraction(Math.min(channel.level, to)) * barWidth;
          context.fillStyle = fill;
          context.fillRect(LABEL + start, y, end - start, ROW);
        }
        if (channel.hold > FLOOR_DB) {
          context.fillStyle = channel.hold > -3 ? colors.high : channel.hold > -12 ? colors.mid : colors.low;
          context.fillRect(LABEL + Math.round(meterFraction(channel.hold) * barWidth) - 1, y, 1, ROW);
        }
        context.fillStyle = clipping ? colors.high : colors.track;
        context.fillRect(LABEL + barWidth + 3, y, CLIP, ROW);
      });

      // The dB scale the bars are read against.
      const scaleTop = channels * (ROW + GAP) - GAP + 2;
      context.textBaseline = "top";
      for (const tick of TICKS) {
        const x = LABEL + Math.round(meterFraction(tick) * barWidth);
        context.fillStyle = colors.tick;
        context.fillRect(Math.min(x, LABEL + barWidth - 1), scaleTop, 1, 3);
        context.fillStyle = colors.text;
        context.textAlign = tick === 0 ? "right" : "center";
        context.fillText(String(tick), tick === 0 ? LABEL + barWidth : x, scaleTop + 4);
      }
      if (moving) frame = requestAnimationFrame(draw);
    };

    const schedule = () => {
      if (!frame) {
        last = performance.now();
        frame = requestAnimationFrame(draw);
      }
    };
    const stopListening = connection.onMeters((peaks) => {
      const now = performance.now();
      peaks.slice(0, channels).forEach((peak, index) => {
        const channel = state[index];
        channel.level = Math.max(channel.level, peak);
        if (peak >= channel.hold) {
          channel.hold = peak;
          channel.heldAt = now;
        }
        if (peak >= -0.1) channel.clippedAt = now;
      });
      schedule();
    });
    const observer = new ResizeObserver(schedule);
    observer.observe(element);
    schedule();
    return () => {
      stopListening();
      observer.disconnect();
      if (frame) cancelAnimationFrame(frame);
    };
  }, [connection, channels]);

  if (channels === 0) return null;
  return <canvas ref={canvas} className="meters" style={{ height: heightFor(channels) }} role="img" aria-label="Output meters" />;
}
