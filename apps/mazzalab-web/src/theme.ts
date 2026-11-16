export type Theme = "dark" | "light" | "blackout" | "contrast";

export const THEMES: Theme[] = ["dark", "light", "blackout", "contrast"];

/** Anything that is not a known theme is the default one. */
export function normalizeTheme(value: unknown): Theme {
  return THEMES.find((theme) => theme === value) ?? "dark";
}

/** Switches the whole page by the attribute the stylesheet's palettes hang on. */
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}
