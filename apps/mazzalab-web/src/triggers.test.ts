import { describe, expect, it } from "vitest";
import type { Cue } from "./api/types";
import { defaultMessage, formatHex, parseHex } from "./components/MidiFields";
import { midiSummary } from "./cues";
import { cuesForHotkey, hotkeyOf, isReserved, type KeyPress } from "./hotkeys";

const press = (key: string, code: string, modifiers: Partial<KeyPress> = {}): KeyPress => ({
  key,
  code,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...modifiers,
});

describe("hotkeys", () => {
  it("names keys after the physical key", () => {
    expect(hotkeyOf(press("q", "KeyQ"))).toBe("Q");
    expect(hotkeyOf(press("F5", "F5"))).toBe("F5");
    expect(hotkeyOf(press("3", "Numpad3"))).toBe("Num3");
    expect(hotkeyOf(press(" ", "Space"))).toBe("Space");
    // Shift+1 types "!" on one layout and something else on another.
    expect(hotkeyOf(press("!", "Digit1", { shiftKey: true }))).toBe("Shift+1");
    expect(hotkeyOf(press("à", "Quote"))).toBe("À");
  });

  it("orders modifiers the same way every time", () => {
    expect(hotkeyOf(press("a", "KeyA", { shiftKey: true, altKey: true, ctrlKey: true }))).toBe("Ctrl+Alt+Shift+A");
    expect(hotkeyOf(press("a", "KeyA", { metaKey: true }))).toBe("Ctrl+A");
  });

  it("ignores a modifier pressed on its own", () => {
    expect(hotkeyOf(press("Shift", "ShiftLeft", { shiftKey: true }))).toBeNull();
    expect(hotkeyOf(press("Control", "ControlLeft", { ctrlKey: true }))).toBeNull();
  });

  it("keeps the screen's own keys out of reach", () => {
    for (const taken of ["Space", "Escape", "V", "Ctrl+S", "Ctrl+Shift+Z", "ArrowDown"]) expect(isReserved(taken)).toBe(true);
    for (const free of ["Q", "F5", "Shift+V", "Ctrl+1", "Num0"]) expect(isReserved(free)).toBe(false);
  });

  it("finds every cue a key starts, inside groups too", () => {
    const cues: Cue[] = [
      { id: "a", type: "memo", triggers: { hotkey: "Q" } },
      { id: "g", type: "group", children: [{ id: "b", type: "memo", triggers: { hotkey: "Q" } }] },
      { id: "c", type: "memo", triggers: { hotkey: "W" } },
      { id: "d", type: "memo" },
    ];
    expect(cuesForHotkey(cues, "Q")).toEqual(["a", "b"]);
    expect(cuesForHotkey(cues, "E")).toEqual([]);
  });
});

describe("midi messages", () => {
  it("reads System Exclusive data typed as hex", () => {
    expect(parseHex("7E 7F 06 01")).toEqual([0x7e, 0x7f, 0x06, 0x01]);
    expect(parseHex("7e,7f, 6 1")).toEqual([0x7e, 0x7f, 0x06, 0x01]);
    // The frame is added when sending, so it is dropped if typed.
    expect(parseHex("F0 7E 01 F7")).toEqual([0x7e, 0x01]);
    expect(parseHex("")).toEqual([]);
    expect(parseHex("7E ZZ")).toBeNull();
    expect(parseHex("7E 123")).toBeNull();
    // Data bytes stay below 0x80.
    expect(parseHex("7E 80")).toBeNull();
    expect(formatHex([0x7e, 0x0a, 0])).toBe("7E 0A 00");
  });

  it("sums a message up for the cue list", () => {
    expect(midiSummary({ kind: "control_change", channel: 1, controller: 7, value: 127 })).toBe("CC 7 = 127 · ch 1");
    expect(midiSummary({ kind: "program_change", channel: 10, program: 4 })).toBe("PC 4 · ch 10");
    expect(midiSummary({ kind: "msc", device_id: 5, command_format: 1, command: 1, q_number: "12.5" })).toBe("MSC cue 12.5 · dev 5");
    expect(midiSummary({ kind: "sys_ex", data: [1, 2, 3] })).toBe("SysEx 3 B");
  });

  it("keeps the channel when the kind of message changes", () => {
    const note = defaultMessage("note_on", { kind: "program_change", channel: 9, program: 4 });
    expect(note).toEqual({ kind: "note_on", channel: 9, note: 60, velocity: 100 });
    expect(defaultMessage("control_change")).toMatchObject({ channel: 1 });
    expect(defaultMessage("msc", note)).toEqual({ kind: "msc", device_id: 1, command_format: 1, command: 1 });
  });
});
