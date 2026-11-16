import { describe, expect, it } from "vitest";
import { normalizeTheme } from "./theme";

describe("theme", () => {
  it("keeps the themes it knows", () => {
    expect(normalizeTheme("light")).toBe("light");
    expect(normalizeTheme("blackout")).toBe("blackout");
    expect(normalizeTheme("dark")).toBe("dark");
    expect(normalizeTheme("contrast")).toBe("contrast");
  });

  it("falls back to dark for anything else", () => {
    expect(normalizeTheme(undefined)).toBe("dark");
    expect(normalizeTheme("sepia")).toBe("dark");
    expect(normalizeTheme(3)).toBe("dark");
  });
});
