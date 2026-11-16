import en from "./en.json";
import it from "./it.json";

/** Every text of the interface is named in English; the other languages are measured against it. */
export type Key = keyof typeof en;

/** A language may be partial: what it lacks is shown in English. */
export type Translation = Partial<Record<Key, string>>;

export interface Locale {
  /** The language's own name, as the picker shows it. */
  label: string;
  messages: Translation;
}

/** To add a language: a JSON file next to these, and a line here. See docs/translating.md. */
export const LOCALES: Record<string, Locale> = {
  en: { label: "English", messages: en },
  it: { label: "Italiano", messages: it },
};

export const ENGLISH: Record<Key, string> = en;

export const LANGUAGE_OPTIONS = Object.entries(LOCALES).map(([code, locale]) => ({ code, label: locale.label }));
