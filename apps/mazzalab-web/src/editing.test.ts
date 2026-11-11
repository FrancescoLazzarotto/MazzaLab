import { describe, expect, it } from "vitest";
import type { Command, Cue } from "./api/types";
import { meterFraction } from "./components/Meters";
import { playbackPosition } from "./components/Waveform";
import {
  cloneCues,
  defaultMatrix,
  displayName,
  fileStem,
  insertCommands,
  isAudioFile,
  locate,
  matrixFor,
  moveCommands,
  problemOf,
  renumber,
  topLevel,
} from "./cues";

const memo = (id: string, number?: string): Cue => ({ id, number, type: "memo" });
const group = (id: string, children: Cue[]): Cue => ({ id, type: "group", children });

/** a, g[b, c, h[d]], e */
const tree = (): Cue[] => [memo("a", "1"), group("g", [memo("b", "2"), memo("c", "3"), group("h", [memo("d")])]), memo("e", "4")];

/** Applies move and insert commands the way the server does. */
function apply(cues: Cue[], commands: Command[]): Cue[] {
  const result = structuredClone(cues);
  const children = (container: { kind: string; id: string }): Cue[] => {
    if (container.kind === "list") return result;
    const find = (siblings: Cue[]): Cue | undefined => {
      for (const cue of siblings) {
        if (cue.id === container.id) return cue;
        const inner = cue.type === "group" ? find(cue.children ?? []) : undefined;
        if (inner) return inner;
      }
      return undefined;
    };
    const target = find(result);
    if (!target || target.type !== "group") throw new Error(`no group ${container.id}`);
    target.children ??= [];
    return target.children;
  };
  const remove = (siblings: Cue[], id: string): Cue | undefined => {
    const index = siblings.findIndex((cue) => cue.id === id);
    if (index >= 0) return siblings.splice(index, 1)[0];
    for (const cue of siblings) {
      const found = cue.type === "group" ? remove(cue.children ?? [], id) : undefined;
      if (found) return found;
    }
    return undefined;
  };
  for (const command of commands) {
    if (command.command === "move_cue") {
      const cue = remove(result, command.cue)!;
      const target = children(command.container);
      target.splice(command.index ?? target.length, 0, cue);
    } else if (command.command === "insert_cue") {
      const target = children(command.container);
      target.splice(command.index ?? target.length, 0, command.cue);
    }
  }
  return result;
}

const shape = (cues: Cue[]): string =>
  cues.map((cue) => (cue.type === "group" ? `${cue.id}[${shape(cue.children ?? [])}]` : cue.id)).join(" ");

describe("moving cues", () => {
  it("keeps the order of several cues dropped before another", () => {
    const commands = moveCommands(tree(), "list", ["b", "e"], { where: "before", anchor: "a" });
    expect(shape(apply(tree(), commands))).toBe("b e a g[c h[d]]");
  });

  it("keeps the order of several cues dropped after another", () => {
    const commands = moveCommands(tree(), "list", ["a", "c"], { where: "after", anchor: "e" });
    expect(shape(apply(tree(), commands))).toBe("g[b h[d]] e a c");
  });

  it("moves down within the same container", () => {
    const commands = moveCommands(tree(), "list", ["b"], { where: "after", anchor: "c" });
    expect(shape(apply(tree(), commands))).toBe("a g[c b h[d]] e");
  });

  it("drops into a group and at the end of the list", () => {
    expect(shape(apply(tree(), moveCommands(tree(), "list", ["a", "e"], { where: "into", anchor: "h" })))).toBe("g[b c h[d a e]]");
    expect(shape(apply(tree(), moveCommands(tree(), "list", ["b"], { where: "end" })))).toBe("a g[c h[d]] e b");
  });

  it("refuses drops that make no sense", () => {
    expect(moveCommands(tree(), "list", ["a"], { where: "before", anchor: "a" })).toEqual([]);
    // A group cannot go inside itself or next to its own contents.
    expect(moveCommands(tree(), "list", ["g"], { where: "into", anchor: "h" })).toEqual([]);
    expect(moveCommands(tree(), "list", ["g"], { where: "after", anchor: "d" })).toEqual([]);
    expect(moveCommands(tree(), "list", ["a"], { where: "into", anchor: "e" })).toEqual([]);
  });
});

describe("selection", () => {
  it("leaves out cues inside a selected group", () => {
    expect(topLevel(tree(), new Set(["e", "g", "b", "d"]))).toEqual(["g", "e"]);
    expect(topLevel(tree(), new Set(["d", "b"]))).toEqual(["b", "d"]);
  });

  it("locates cues in lists and groups", () => {
    expect(locate(tree(), "list", "e")).toMatchObject({ container: { kind: "list", id: "list" }, index: 2 });
    expect(locate(tree(), "list", "d")).toMatchObject({ container: { kind: "group", id: "h" }, index: 0 });
    expect(locate(tree(), "list", "zz")).toBeNull();
  });
});

describe("copying cues", () => {
  it("gives copies new ids and numbers and keeps targets inside the copy", () => {
    const source: Cue[] = [
      { id: "s", number: "7", type: "audio", file: "media/a.wav" },
      { id: "f", number: "8", type: "fade", target: "s" },
      { id: "x", type: "stop", target: "outside" },
      group("g", [memo("m", "9")]),
    ];
    let next = 0;
    const copies = cloneCues(source, 20, () => `new${next++}`);
    expect(copies.map((cue) => cue.id)).toEqual(["new0", "new1", "new2", "new3"]);
    expect(copies.map((cue) => cue.number)).toEqual(["20", "21", undefined, undefined]);
    expect(copies[1].target).toBe("new0");
    expect(copies[2].target).toBe("outside");
    const inner = copies[3].type === "group" ? copies[3].children! : [];
    expect(inner[0]).toMatchObject({ id: "new4", number: "22" });
    // The originals are untouched.
    expect(source[1].target).toBe("s");
  });

  it("inserts in order after a cue, into a group or at the end", () => {
    const fresh = [memo("x"), memo("y")];
    expect(shape(apply(tree(), insertCommands(tree(), "list", fresh, { where: "after", anchor: "b" })))).toBe("a g[b x y c h[d]] e");
    expect(shape(apply(tree(), insertCommands(tree(), "list", fresh, { where: "into", anchor: "h" })))).toBe("a g[b c h[d x y]] e");
    expect(shape(apply(tree(), insertCommands(tree(), "list", fresh, { where: "end" })))).toBe("a g[b c h[d]] e x y");
  });
});

describe("numbers, names and files", () => {
  it("renumbers without floating point noise", () => {
    expect(renumber(["a", "b", "c"], 1, 0.1)).toEqual({
      command: "set_numbers",
      numbers: [
        ["a", "1"],
        ["b", "1.1"],
        ["c", "1.2"],
      ],
    });
  });

  it("names unnamed media cues after their file", () => {
    expect(fileStem("C:\\Music\\Rain 01.wav")).toBe("Rain 01");
    expect(fileStem("media/no-extension")).toBe("no-extension");
    expect(displayName({ id: "a", type: "audio", file: "media/Thunder.flac" })).toBe("Thunder");
    expect(displayName({ id: "a", type: "audio", name: "Storm", file: "media/Thunder.flac" })).toBe("Storm");
    expect(displayName({ id: "a", type: "audio" })).toBe("Audio");
  });

  it("recognises audio files by extension", () => {
    expect(isAudioFile("Rain.WAV")).toBe(true);
    expect(isAudioFile("song.mp3")).toBe(true);
    expect(isAudioFile("slides.pdf")).toBe(false);
    expect(isAudioFile("noextension")).toBe(false);
  });

  it("explains why a cue would do nothing", () => {
    const exists = (id: string) => id === "there";
    expect(problemOf({ id: "a", type: "audio" }, exists)).toBe("no_file");
    expect(problemOf({ id: "a", type: "audio", file: "x.wav" }, exists)).toBeNull();
    expect(problemOf({ id: "a", type: "fade" }, exists)).toBe("no_target");
    expect(problemOf({ id: "a", type: "fade", target: "gone" }, exists)).toBe("missing_target");
    expect(problemOf({ id: "a", type: "fade", target: "there" }, exists)).toBeNull();
    expect(problemOf({ id: "a", type: "network", message: "  " }, exists)).toBe("no_message");
    expect(problemOf({ id: "a", type: "video" }, exists)).toBe("not_supported");
  });
});

describe("audio routing and display", () => {
  it("builds the default matrix the mixer uses", () => {
    expect(defaultMatrix(1, 4)).toEqual([[0, 0, null, null]]);
    expect(defaultMatrix(2, 4)).toEqual([
      [0, null, null, null],
      [null, 0, null, null],
    ]);
    expect(defaultMatrix(2, 1)).toEqual([[-6], [-6]]);
  });

  it("resizes a stored matrix to the device", () => {
    expect(matrixFor([[0], [null, -3, 5]], 2, 2)).toEqual([
      [0, null],
      [null, -3],
    ]);
    expect(matrixFor(undefined, 1, 2)).toEqual([[0, 0]]);
  });

  it("places meter levels on the bar", () => {
    expect(meterFraction(-60)).toBe(0);
    expect(meterFraction(-30)).toBe(0.5);
    expect(meterFraction(0)).toBe(1);
    expect(meterFraction(6)).toBe(1);
    expect(meterFraction(-120)).toBe(0);
  });

  it("follows playback through trims, rate and loops", () => {
    const cue = { id: "a", type: "audio" as const, start_time: 2, end_time: 6 };
    expect(playbackPosition(cue, 1, 10)).toBe(3);
    expect(playbackPosition(cue, 9, 10)).toBe(6);
    expect(playbackPosition({ ...cue, rate: 2 }, 1, 10)).toBe(4);
    expect(playbackPosition({ ...cue, play_count: 2 }, 5, 10)).toBe(3);
    expect(playbackPosition({ ...cue, infinite_loop: true }, 41, 10)).toBe(3);
    expect(playbackPosition({ id: "a", type: "audio" as const }, 4, 10)).toBe(4);
  });
});
