import { describe, expect, it } from "vitest";
import type { StageItem } from "./api/types";
import { compactGeometry, fadeProgress, geometryAt, mediaTime, opacityAt, pictureBox, pictureStyle, positionAt, stageNames, stageSize, stageUrl } from "./stage";

const video: Extract<StageItem, { kind: "video" }> = {
  cue: "v",
  stage: "Main",
  layer: 0,
  opacity: 1,
  geometry: {},
  started: 100,
  offset: 2,
  paused: false,
  kind: "video",
  start_time: 1,
  end_time: 5,
  looped: false,
  hold: false,
  rate: 1,
};

describe("stages", () => {
  it("follow the server's clock through pauses, trims, rate and loops", () => {
    expect(positionAt(video, 101.5)).toBe(3.5);
    expect(positionAt({ ...video, paused: true }, 500)).toBe(2);
    expect(mediaTime(video, 3, 10)).toBe(4);
    // Played out: nothing more, unless it holds its last frame.
    expect(mediaTime(video, 4, 10)).toBeNull();
    expect(mediaTime({ ...video, hold: true }, 9, 10)).toBe(5);
    expect(mediaTime({ ...video, looped: true }, 9, 10)).toBe(2);
    expect(mediaTime({ ...video, rate: 2 }, 1.5, 10)).toBe(4);
    // Without an end, the file's own length, once known.
    expect(mediaTime({ ...video, end_time: undefined }, 20, undefined)).toBe(21);
    expect(mediaTime({ ...video, end_time: undefined }, 20, 8)).toBeNull();
  });

  it("fade opacity from where it was", () => {
    const fading = { ...video, opacity: 0, fade: { from: 1, from_geometry: {}, started: 100, duration: 2, curve: "linear" as const } };
    expect(opacityAt(fading, 101)).toBe(0.5);
    expect(opacityAt(fading, 103)).toBe(0);
    // An S-curve starts and ends slowly, and is half way at half time.
    const curved = { ...fading.fade, curve: "s_curve" as const };
    expect(fadeProgress(curved, 100.4)).toBeLessThan(0.2);
    expect(fadeProgress(curved, 101)).toBe(0.5);
  });

  it("move pictures from where they were, keeping how they fill the stage", () => {
    const moving = {
      ...video,
      geometry: { fit: "fill" as const, x: 200, rotation: 90 },
      fade: { from: 1, from_geometry: { y: -100 }, started: 100, duration: 2, curve: "linear" as const },
    };
    const half = geometryAt(moving, 101);
    expect([half.fit, half.x, half.y, half.rotation, half.scale_x]).toEqual(["fill", 100, -50, 45, 1]);
    expect(geometryAt(moving, 105).y).toBe(0);
    expect(compactGeometry({ fit: "fit", x: 0, y: 12, scale_x: 1 })).toEqual({ y: 12 });
    expect(compactGeometry({ fit: "fit", x: 0 })).toBeUndefined();
  });

  it("size a picture's box by its fit, then place it", () => {
    const stage = { width: 1920, height: 1080 };
    const square = { width: 1000, height: 1000 };
    expect(pictureBox("fit", stage, square)).toEqual({ width: 1080, height: 1080 });
    expect(pictureBox("fill", stage, square)).toEqual({ width: 1920, height: 1920 });
    expect(pictureBox("stretch", stage, square)).toEqual(stage);
    expect(pictureBox("native", stage, square)).toEqual(square);
    // Until the file says how large it is, the stage.
    expect(pictureBox("fit", stage, null)).toEqual(stage);
    const style = pictureStyle(geometryAt({ ...video, geometry: { x: 10, crop_left: 0.25 } }, 0), { width: 1080, height: 1080 }, stage);
    expect([style.left, style.top]).toEqual([420, 0]);
    expect(style.transform).toBe("translate(10px, 0px) rotate(0deg) scale(1, 1)");
    expect(style.clipPath).toBe("inset(0% 0% 0% 25%)");
  });

  it("are named and opened by address", () => {
    expect(stageNames({})).toEqual(["Main"]);
    expect(stageNames({ stages: [{ name: " Proiettore " }, { name: "" }] })).toEqual(["Proiettore"]);
    expect(stageSize([{ name: "Side", width: 1024, height: 768 }], "Side")).toEqual({ width: 1024, height: 768 });
    expect(stageSize([{ name: "Side", width: 1024, height: 768 }], undefined)).toEqual({ width: 1024, height: 768 });
    expect(stageSize([], "Main")).toEqual({ width: 1920, height: 1080 });
    expect(stageUrl("Side", false, { pathname: "/", search: "" })).toBe("/?stage=Side");
    expect(stageUrl("Main", true, { pathname: "/", search: "?server=10.0.0.2:53980" })).toBe("/?server=10.0.0.2%3A53980&stage=Main&monitor");
  });
});
