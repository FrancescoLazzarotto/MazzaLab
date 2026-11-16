import { describe, expect, it } from "vitest";
import { checkPin, makeLock, normalizeLock, sha256Hex, validPin } from "./lock";

describe("show mode lock", () => {
  it("hashes with SHA-256", async () => {
    expect(await sha256Hex("abc")).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  });

  it("accepts the PIN it was made with, and no other", async () => {
    const lock = await makeLock("4321", "00ff");
    expect(await checkPin("4321", lock)).toBe(true);
    expect(await checkPin("4322", lock)).toBe(false);
    expect(await checkPin("", lock)).toBe(false);
    expect(lock.hash).not.toContain("4321");
  });

  it("salts: the same PIN gives different hashes", async () => {
    const [one, two] = await Promise.all([makeLock("4321"), makeLock("4321")]);
    expect(one.salt).not.toBe(two.salt);
    expect(one.hash).not.toBe(two.hash);
  });

  it("wants a PIN of four characters or more", () => {
    expect(validPin("123")).toBe(false);
    expect(validPin("1234")).toBe(true);
  });

  it("reads a stored lock only if it is one", async () => {
    const lock = await makeLock("1234");
    expect(normalizeLock(lock)).toEqual(lock);
    expect(normalizeLock(null)).toBeNull();
    expect(normalizeLock({ salt: "zz", hash: lock.hash })).toBeNull();
    expect(normalizeLock({ salt: "ab", hash: "ab" })).toBeNull();
    expect(normalizeLock("1234")).toBeNull();
  });
});
