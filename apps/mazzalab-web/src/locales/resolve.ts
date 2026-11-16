/** What a language setting means: `auto` follows the browser, anything else names a language. */
export const AUTO = "auto";

/**
 * The language to use. A choice that names an available language wins; with
 * `auto` the first of the browser's languages we have (by its primary part,
 * so `it-CH` finds `it`); otherwise English.
 */
export function resolveLanguage(choice: string | null, browser: readonly string[], available: readonly string[]): string {
  if (choice && choice !== AUTO && available.includes(choice)) return choice;
  for (const tag of browser) {
    const primary = tag.toLowerCase().split("-")[0];
    if (available.includes(primary)) return primary;
  }
  return "en";
}

/** The text for `key` in a language, or in English when that language lacks it. */
export function translateKey(messages: Readonly<Record<string, string | undefined>>, fallback: Readonly<Record<string, string>>, key: string): string {
  const text = messages[key];
  return text !== undefined && text !== "" ? text : (fallback[key] ?? key);
}

/** How a language's keys compare with English: what it lacks, and what English does not have. */
export function compareKeys(base: readonly string[], other: readonly string[]): { missing: string[]; extra: string[] } {
  const have = new Set(other);
  const want = new Set(base);
  return { missing: base.filter((key) => !have.has(key)), extra: other.filter((key) => !want.has(key)) };
}
