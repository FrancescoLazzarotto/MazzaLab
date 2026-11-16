import { describe, expect, it } from "vitest";
import { ENGLISH, LOCALES } from "./index";
import { compareKeys, resolveLanguage, translateKey } from "./resolve";

/** Languages allowed to lack keys for now; they fall back to English. Empty on purpose. */
const ALLOW_PARTIAL: string[] = [];

describe("locales", () => {
  for (const [code, locale] of Object.entries(LOCALES)) {
    it(`${code} has no keys English does not have, and lacks none`, () => {
      const { missing, extra } = compareKeys(Object.keys(ENGLISH), Object.keys(locale.messages));
      // A readable report of what to fix.
      expect({ language: code, extra }).toEqual({ language: code, extra: [] });
      if (!ALLOW_PARTIAL.includes(code)) expect({ language: code, missing }).toEqual({ language: code, missing: [] });
    });

    it(`${code} has no empty texts`, () => {
      const empty = Object.entries(locale.messages).filter(([, text]) => !text || text.trim() === "");
      expect(empty).toEqual([]);
    });

    it(`${code} keeps the placeholders English has`, () => {
      const holes = (text: string) => (text.match(/\{[a-z]+\}/g) ?? []).sort().join();
      const wrong = Object.entries(locale.messages).filter(([key, text]) => text !== undefined && holes(text) !== holes(ENGLISH[key as keyof typeof ENGLISH] ?? ""));
      expect(wrong.map(([key]) => key)).toEqual([]);
    });
  }

  it("reports what is missing and extra", () => {
    expect(compareKeys(["a", "b"], ["b", "c"])).toEqual({ missing: ["a"], extra: ["c"] });
  });

  it("falls back to English for a missing key, and to the key for an unknown one", () => {
    expect(translateKey({ a: "uno" }, { a: "one", b: "two" }, "a")).toBe("uno");
    expect(translateKey({ a: "uno" }, { a: "one", b: "two" }, "b")).toBe("two");
    expect(translateKey({ b: "" }, { b: "two" }, "b")).toBe("two");
    expect(translateKey({}, {}, "zzz")).toBe("zzz");
  });

  it("picks the language", () => {
    const available = ["en", "it"];
    expect(resolveLanguage("it", ["en-US"], available)).toBe("it");
    expect(resolveLanguage("auto", ["it-CH", "en"], available)).toBe("it");
    expect(resolveLanguage("auto", ["fr-FR", "en-GB"], available)).toBe("en");
    expect(resolveLanguage(null, ["de"], available)).toBe("en");
    expect(resolveLanguage("fr", ["it"], available)).toBe("it");
  });
});
