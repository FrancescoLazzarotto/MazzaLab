import { describe, expect, it } from "vitest";
import { parseDeviceId } from "./mscDevice";

describe("MSC device ID", () => {
  it("takes whole numbers from 0 to 127", () => {
    expect(parseDeviceId("0")).toBe(0);
    expect(parseDeviceId(" 5 ")).toBe(5);
    expect(parseDeviceId("127")).toBe(127);
  });

  it("is none for empty, too big or not a number", () => {
    for (const text of ["", "  ", "128", "-1", "1.5", "a"]) expect(parseDeviceId(text)).toBeUndefined();
  });
});
