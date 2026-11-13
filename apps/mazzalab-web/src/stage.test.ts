import { describe, expect, it } from "vitest";
import type { StageItem } from "./api/types";
import { mediaTime, opacityAt, positionAt, stageNames, stageUrl } from "./stage";

const video: Extract<StageItem, { kind: "video" }> = {
  cue: "v",
  stage: "Main",
  layer: 0,
  opacity: 1,
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
    const fading = { ...video, opacity: 0, fade: { from: 1, started: 100, duration: 2 } };
    expect(opacityAt(fading, 101)).toBe(0.5);
    expect(opacityAt(fading, 103)).toBe(0);
  });

  it("are named and opened by address", () => {
    expect(stageNames({})).toEqual(["Main"]);
    expect(stageNames({ stages: [{ name: " Proiettore " }, { name: "" }] })).toEqual(["Proiettore"]);
    expect(stageUrl("Side", false, { pathname: "/", search: "" })).toBe("/?stage=Side");
    expect(stageUrl("Main", true, { pathname: "/", search: "?server=10.0.0.2:53980" })).toBe("/?server=10.0.0.2%3A53980&stage=Main&monitor");
  });
});
