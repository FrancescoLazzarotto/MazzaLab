import { describe, expect, it } from "vitest";
import type { Cue } from "./api/types";
import { cueFromTemplate, makeTemplate, suggestedName, withTemplate, withoutTemplate } from "./templates";

const wait: Cue = { id: "a", type: "wait", duration: 4, number: "12", name: "Beat", target: "zzz", triggers: { hotkey: "Q" }, color: "red" } as Cue;

describe("cue templates", () => {
  it("keeps what the cue does and drops what belongs to its place in the show", () => {
    const template = makeTemplate("  Beat ", wait);
    expect(template.name).toBe("Beat");
    expect(template.cue).toMatchObject({ type: "wait", duration: 4, name: "Beat", color: "red" });
    expect(template.cue.number).toBeUndefined();
    expect(template.cue.target).toBeUndefined();
    expect(template.cue.triggers).toBeUndefined();
    // The original is not touched.
    expect(wait.number).toBe("12");
  });

  it("strips children too", () => {
    const group = { id: "g", type: "group", number: "1", children: [{ ...wait, id: "c" }] } as Cue;
    const template = makeTemplate("G", group);
    const [child] = template.cue.type === "group" ? (template.cue.children ?? []) : [];
    expect(child.number).toBeUndefined();
  });

  it("adds, replaces by name in any case, and removes", () => {
    const one = makeTemplate("Beat", wait);
    const two = makeTemplate("beat", { ...wait, duration: 9 } as Cue);
    const three = makeTemplate("Other", wait);
    expect(withTemplate([], one)).toEqual([one]);
    expect(withTemplate([one], two)).toEqual([two]);
    expect(withTemplate([one], three)).toEqual([one, three]);
    expect(withoutTemplate([one, three], "Beat")).toEqual([three]);
  });

  it("makes a new cue with its own ids and the number it is given", () => {
    const group = { id: "g", type: "group", children: [{ ...wait, id: "c", target: undefined }] } as Cue;
    const ids = ["new-g", "new-c"];
    let n = 0;
    const cue = cueFromTemplate(makeTemplate("G", group), "7", () => ids[n++]);
    expect(cue.id).toBe("new-g");
    expect(cue.number).toBe("7");
    expect(cue.type === "group" && cue.children?.[0].id).toBe("new-c");
    expect(cue.type === "group" && cue.children?.[0].number).toBeUndefined();
  });

  it("suggests the cue's name, else its type", () => {
    expect(suggestedName(wait, "Wait")).toBe("Beat");
    expect(suggestedName({ ...wait, name: " " } as Cue, "Wait")).toBe("Wait");
  });
});
