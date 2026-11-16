import { describe, expect, it } from "vitest";
import { MIN_SPAN, bucketsFor, clampView, dragFade, panView, pinchView, timeToX, wholeFile, xToTime, zoomLevel, zoomView } from "./waveView";

describe("waveform view", () => {
  it("starts on the whole file", () => {
    expect(wholeFile(10)).toEqual({ start: 0, span: 10 });
    expect(zoomLevel(wholeFile(10), 10)).toBe(1);
  });

  it("stays inside the file", () => {
    expect(clampView({ start: -3, span: 4 }, 10)).toEqual({ start: 0, span: 4 });
    expect(clampView({ start: 9, span: 4 }, 10)).toEqual({ start: 6, span: 4 });
    expect(clampView({ start: 0, span: 50 }, 10)).toEqual({ start: 0, span: 10 });
    expect(clampView({ start: 0, span: 0 }, 10).span).toBe(MIN_SPAN);
  });

  it("zooms around the point under the pointer", () => {
    const view = zoomView(wholeFile(10), 10, 2, 5);
    expect(view).toEqual({ start: 2.5, span: 5 });
    // The anchor stays at the same fraction of the screen.
    const again = zoomView(view, 10, 2, 3);
    expect((3 - again.start) / again.span).toBeCloseTo((3 - view.start) / view.span);
    expect(zoomLevel(again, 10)).toBe(4);
  });

  it("zooms out to the whole file and no further", () => {
    expect(zoomView({ start: 2.5, span: 5 }, 10, 0.1, 5)).toEqual({ start: 0, span: 10 });
  });

  it("pans without leaving the file", () => {
    expect(panView({ start: 2, span: 4 }, 10, 3)).toEqual({ start: 5, span: 4 });
    expect(panView({ start: 2, span: 4 }, 10, 30)).toEqual({ start: 6, span: 4 });
    expect(panView({ start: 2, span: 4 }, 10, -30)).toEqual({ start: 0, span: 4 });
  });

  it("maps time and pixels both ways", () => {
    const view = { start: 2, span: 4 };
    expect(timeToX(4, view, 400)).toBe(200);
    expect(xToTime(200, view, 400)).toBe(4);
  });

  it("follows two fingers", () => {
    const began = wholeFile(10);
    // Fingers at 1 s and 3 s are spread to the screen's quarter and three quarters.
    const view = pinchView(began, 10, [100, 300], [100, 300], 1000);
    expect(view).toEqual(began);
    const spread = pinchView(began, 10, [100, 300], [0, 1000], 1000);
    expect(spread.span).toBeCloseTo(2);
    expect(spread.start).toBeCloseTo(1);
  });

  it("asks for finer peaks as it zooms in, up to a limit", () => {
    expect(bucketsFor(1600, 1)).toBe(1600);
    expect(bucketsFor(1600, 3)).toBe(6400);
    expect(bucketsFor(1600, 1000)).toBe(1600 * 16);
  });

  it("limits fades to the played part", () => {
    expect(dragFade("fadeIn", 3, 1, 9, 0, 0)).toEqual({ fade_in: 2, fade_out: 0 });
    expect(dragFade("fadeIn", 0, 1, 9, 0, 0)).toEqual({ fade_in: 0, fade_out: 0 });
    expect(dragFade("fadeIn", 9, 1, 9, 0, 3)).toEqual({ fade_in: 5, fade_out: 3 });
    expect(dragFade("fadeOut", 7, 1, 9, 2, 0)).toEqual({ fade_in: 2, fade_out: 2 });
    expect(dragFade("fadeOut", 1, 1, 9, 2, 0)).toEqual({ fade_in: 2, fade_out: 6 });
  });
});
