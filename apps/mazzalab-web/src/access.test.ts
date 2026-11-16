import { describe, expect, it } from "vitest";
import {
  FULL_ACCESS,
  accessLevel,
  authHeaders,
  effectiveMode,
  effectiveView,
  loadPasscode,
  needsPasscode,
  newPasscode,
  passcodeFromUrl,
  savePasscode,
  someoneCanEdit,
  uiFlags,
  withPasscode,
  withoutPasscode,
} from "./access";

const memory = () => {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v), removeItem: (k: string) => void data.delete(k) };
};

describe("passcode in links and requests", () => {
  it("reads and strips ?passcode=", () => {
    expect(passcodeFromUrl("?stage=Main&passcode=ab%20c")).toBe("ab c");
    expect(passcodeFromUrl("?stage=Main")).toBeNull();
    expect(passcodeFromUrl("?passcode=")).toBeNull();
    expect(withoutPasscode("?stage=Main&passcode=1234")).toBe("?stage=Main");
    expect(withoutPasscode("?passcode=1234")).toBe("");
  });

  it("adds the passcode to a WebSocket or a relative URL", () => {
    expect(withPasscode("ws://host:1/ws", "a&b")).toBe("ws://host:1/ws?passcode=a%26b");
    expect(withPasscode("ws://host:1/ws?x=1", "7")).toBe("ws://host:1/ws?x=1&passcode=7");
    expect(withPasscode("/api/cue-media?cue=5", "7")).toBe("/api/cue-media?cue=5&passcode=7");
    expect(withPasscode("http://h/api/x", null)).toBe("http://h/api/x");
  });

  it("sends a bearer header only when there is a passcode", () => {
    expect(authHeaders("1234")).toEqual({ authorization: "Bearer 1234" });
    expect(authHeaders(null)).toEqual({});
  });

  it("keeps a passcode per server", () => {
    const store = memory();
    savePasscode("a:1", "x", store);
    savePasscode("b:1", "y", store);
    expect(loadPasscode("a:1", store)).toBe("x");
    expect(loadPasscode("b:1", store)).toBe("y");
    savePasscode("a:1", null, store);
    expect(loadPasscode("a:1", store)).toBeNull();
    expect(loadPasscode("a:1", undefined)).toBeNull();
  });

  it("survives storage that throws", () => {
    const broken = {
      getItem: () => {
        throw new Error("no");
      },
      setItem: () => {
        throw new Error("no");
      },
      removeItem: () => {
        throw new Error("no");
      },
    };
    expect(loadPasscode("a", broken)).toBeNull();
    expect(() => savePasscode("a", "x", broken)).not.toThrow();
  });
});

describe("what an access means for the interface", () => {
  it("asks for a passcode only when required and not yet let in", () => {
    expect(needsPasscode({ required: true, view: false, control: false, edit: false })).toBe(true);
    expect(needsPasscode({ required: true, view: true, control: false, edit: false })).toBe(false);
    expect(needsPasscode({ required: false, view: true, control: true, edit: true })).toBe(false);
  });

  it("turns an access into flags and a mode", () => {
    expect(uiFlags({ view: true, control: true, edit: false })).toEqual({ canEdit: false, canControl: true });
    expect(effectiveMode("edit", { view: true, control: true, edit: false })).toBe("show");
    expect(effectiveMode("edit", FULL_ACCESS)).toBe("edit");
    expect(effectiveMode("show", FULL_ACCESS)).toBe("show");
  });

  it("names the highest permission", () => {
    expect(accessLevel(FULL_ACCESS)).toBe("edit");
    expect(accessLevel({ view: true, control: true, edit: false })).toBe("control");
    expect(accessLevel({ view: true, control: false, edit: false })).toBe("view");
    expect(accessLevel({ view: false, control: false, edit: false })).toBe("none");
  });
});

describe("passcode rows", () => {
  it("makes a new row with an unused four digit code", () => {
    const first = newPasscode([], () => 0);
    expect(first).toEqual({ code: "1000", name: "", view: true, control: false, edit: false });
    const sequence = [0, 0, 0.5];
    let step = 0;
    expect(newPasscode([first], () => sequence[step++]).code).toBe("5500");
  });

  it("counts control and edit as view", () => {
    expect(effectiveView({ code: "1", name: "", view: false, control: true, edit: false })).toBe(true);
    expect(effectiveView({ code: "1", name: "", view: false, control: false, edit: false })).toBe(false);
  });

  it("knows whether anyone can edit", () => {
    expect(someoneCanEdit([])).toBe(false);
    expect(someoneCanEdit([{ code: "1", name: "", view: true, control: true, edit: true }])).toBe(true);
  });
});
