import { describe, expect, it } from "vitest";
import { breadcrumbs, joinPath, stripExtension, validFileName } from "./showFiles";

describe("show file paths", () => {
  it("walks a Windows path from the drive down", () => {
    expect(breadcrumbs("C:\\Users\\me\\Shows", "\\")).toEqual([
      { name: "C:", path: "C:\\" },
      { name: "Users", path: "C:\\Users" },
      { name: "me", path: "C:\\Users\\me" },
      { name: "Shows", path: "C:\\Users\\me\\Shows" },
    ]);
    expect(breadcrumbs("C:\\", "\\")).toEqual([{ name: "C:", path: "C:\\" }]);
  });

  it("walks a Unix path from the root down", () => {
    expect(breadcrumbs("/home/me", "/")).toEqual([
      { name: "/", path: "/" },
      { name: "home", path: "/home" },
      { name: "me", path: "/home/me" },
    ]);
    expect(breadcrumbs("/", "/")).toEqual([{ name: "/", path: "/" }]);
  });

  it("joins without doubling the separator", () => {
    expect(joinPath("C:\\", "a.mazzalab", "\\")).toBe("C:\\a.mazzalab");
    expect(joinPath("/home", "a", "/")).toBe("/home/a");
  });

  it("drops the show extension, once", () => {
    expect(stripExtension("Hamlet.mazzalab")).toBe("Hamlet");
    expect(stripExtension("Hamlet.MAZZALAB")).toBe("Hamlet");
    expect(stripExtension("Hamlet")).toBe("Hamlet");
  });

  it("rejects names that are paths or empty", () => {
    expect(validFileName("Hamlet act 1")).toBe(true);
    expect(validFileName("  ")).toBe(false);
    expect(validFileName("a/b")).toBe(false);
    expect(validFileName("..")).toBe(false);
  });
});
