import type { PreflightIssue, Severity } from "./api/types";

/** The issues of each severity, in the order the check reported them. */
export function groupIssues(issues: readonly PreflightIssue[]): Record<Severity, PreflightIssue[]> {
  const groups: Record<Severity, PreflightIssue[]> = { error: [], warning: [], info: [] };
  for (const issue of issues) groups[issue.severity].push(issue);
  return groups;
}

/** How many issues keep the show from working as it should. */
export function blocking(issues: readonly PreflightIssue[]): number {
  return issues.filter((issue) => issue.severity === "error").length;
}
