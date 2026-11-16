import { describe, expect, it } from "vitest";
import type { PreflightIssue } from "./api/types";
import { blocking, groupIssues } from "./preflight";

const issue = (severity: PreflightIssue["severity"], key: PreflightIssue["key"]): PreflightIssue => ({ severity, key, detail: "" });

describe("pre-show check", () => {
  it("groups issues by severity and keeps their order", () => {
    const groups = groupIssues([issue("warning", "empty_group"), issue("error", "no_file"), issue("error", "missing_media"), issue("info", "not_supported")]);
    expect(groups.error.map((entry) => entry.key)).toEqual(["no_file", "missing_media"]);
    expect(groups.warning).toHaveLength(1);
    expect(groups.info).toHaveLength(1);
  });

  it("counts the errors", () => {
    expect(blocking([])).toBe(0);
    expect(blocking([issue("error", "no_file"), issue("warning", "empty_group")])).toBe(1);
  });
});
