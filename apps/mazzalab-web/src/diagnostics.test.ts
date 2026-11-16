import { describe, expect, it } from "vitest";
import { HISTORY, formatUptime, lateWithin, percent, pushSample, sampleOf, sparkline, type Sample } from "./diagnostics";

const sample = (load: number | null, late: number | null = 0): Sample => ({ load, late });

describe("diagnostics history", () => {
  it("keeps the last minute of readings", () => {
    let history: Sample[] = [];
    for (let n = 0; n < HISTORY + 5; n++) history = pushSample(history, sample(n / 100));
    expect(history).toHaveLength(HISTORY);
    expect(history[0].load).toBe(5 / 100);
  });

  it("reads a sample from a reading, with what is not reported as null", () => {
    expect(sampleOf({ uptime: 1, clients: 1, running: 0 })).toEqual({ load: null, late: null });
    expect(sampleOf({ uptime: 1, clients: 1, running: 0, audio_load: 0.4, audio_late: 3 })).toEqual({ load: 0.4, late: 3 });
  });

  it("counts the callbacks that came late within the history", () => {
    expect(lateWithin([])).toBe(0);
    expect(lateWithin([sample(0, 2)])).toBe(0);
    expect(lateWithin([sample(0, 2), sample(0, 2), sample(0, 5)])).toBe(3);
    expect(lateWithin([sample(0, null), sample(0, 4), sample(0, 4)])).toBe(0);
  });

  it("writes times and loads short", () => {
    expect(formatUptime(65)).toBe("1:05");
    expect(formatUptime(3723)).toBe("1:02:03");
    expect(formatUptime(-4)).toBe("0:00");
    expect(percent(0.374)).toBe("37%");
    expect(percent(2)).toBe("100%");
  });

  it("draws the newest reading at the right edge and skips what is not reported", () => {
    const line = sparkline([sample(0), sample(null), sample(1)], 100, 20, 3);
    expect(line).toBe("0.0,20.0 100.0,0.0");
    // A short history sits to the right.
    expect(sparkline([sample(0.5)], 100, 10, 5)).toBe("100.0,5.0");
  });
});
