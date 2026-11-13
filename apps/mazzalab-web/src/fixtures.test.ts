import { describe, expect, it } from "vitest";
import type { Fixture, FixtureGroup } from "./api/types";
import {
  clashes,
  duplicateNames,
  lastAddress,
  nextFixture,
  parseParameters,
  removeMember,
  renameMember,
  slotLabels,
  toggleMember,
  typeOf,
} from "./fixtures";

const rgb = ["red", "green", "blue"];

describe("the fixture patch", () => {
  it("knows a fixture's type and the addresses it takes", () => {
    expect(typeOf({ name: "A", address: 1 })).toBe("dimmer");
    expect(typeOf({ name: "B", address: 1, parameters: rgb })).toBe("rgb");
    expect(typeOf({ name: "C", address: 1, parameters: ["pan", "tilt"] })).toBe("custom");
    expect(lastAddress({ name: "A", address: 7 })).toBe(7);
    expect(lastAddress({ name: "B", address: 10, parameters: rgb })).toBe(12);
    expect(parseParameters(" Intensity, red  green;blue ")).toEqual(["intensity", "red", "green", "blue"]);
  });

  it("marks fixtures that overlap or run past the universe", () => {
    const fixtures: Fixture[] = [
      { name: "A", address: 1, parameters: rgb },
      { name: "B", address: 3 },
      { name: "C", address: 3, universe: 2 },
      { name: "D", address: 4 },
      { name: "E", address: 511, parameters: rgb },
    ];
    expect([...clashes(fixtures)].sort()).toEqual([0, 1, 4]);
  });

  it("adds the next fixture after the last one", () => {
    expect(nextFixture([])).toEqual({ name: "Fixture 1", universe: 1, address: 1 });
    const fixtures: Fixture[] = [{ name: "Fixture 2", address: 1, parameters: rgb }];
    expect(nextFixture(fixtures)).toEqual({ name: "Fixture 3", universe: 1, address: 4, parameters: rgb });
    // A full universe moves on to the next.
    expect(nextFixture([{ name: "Edge", address: 511, parameters: rgb }])).toMatchObject({ universe: 2, address: 1 });
  });

  it("keeps groups in step with renames and deletions", () => {
    const groups: FixtureGroup[] = [
      { name: "Front", members: ["Front left", "front RIGHT"] },
      { name: "All", members: ["Front", "Cyc"] },
    ];
    expect(renameMember(groups, "Front Right", "FR")[0].members).toEqual(["Front left", "FR"]);
    expect(renameMember(groups, "front", "Wash")[1].members).toEqual(["Wash", "Cyc"]);
    expect(removeMember(groups, "cyc")[1].members).toEqual(["Front"]);
    expect(toggleMember(groups[0], "front left").members).toEqual(["front RIGHT"]);
    expect(toggleMember(groups[0], "Back").members).toEqual(["Front left", "front RIGHT", "Back"]);
  });

  it("finds names used twice and labels the patched slots", () => {
    expect([...duplicateNames([{ name: "Cyc", address: 1 }], [{ name: " cyc " }, { name: "Front" }])]).toEqual(["cyc"]);
    const labels = slotLabels([
      { name: "Front", address: 1 },
      { name: "Cyc", universe: 2, address: 10, parameters: rgb },
    ]);
    expect(labels.get("1/1")).toBe("Front");
    expect(labels.get("2/11")).toBe("Cyc.green");
    expect(labels.get("1/2")).toBeUndefined();
  });
});
