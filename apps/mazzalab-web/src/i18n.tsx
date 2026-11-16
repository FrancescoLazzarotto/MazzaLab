import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import { ENGLISH, LANGUAGE_OPTIONS, LOCALES, type Key } from "./locales";
import { AUTO, resolveLanguage, translateKey } from "./locales/resolve";

export type { Key };
export type Translate = (key: Key) => string;

/** The languages the picker offers; "auto" is offered besides them. */
export { LANGUAGE_OPTIONS };

const STORAGE_KEY = "mazzalab.language";

/** What was chosen on this device: a language, or `auto` to follow the browser. */
export function storedChoice(): string {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && saved in LOCALES) return saved;
  } catch {
    // Storage can be unavailable (private mode): follow the browser.
  }
  return AUTO;
}

export function languageFor(choice: string): string {
  return resolveLanguage(choice, typeof navigator === "undefined" ? [] : (navigator.languages?.length ? navigator.languages : [navigator.language]), Object.keys(LOCALES));
}

interface I18n {
  /** The language in use. */
  language: string;
  /** What was chosen: a language, or `auto`. */
  choice: string;
  setChoice: (choice: string) => void;
  t: Translate;
}

const I18nContext = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [choice, setChoiceState] = useState<string>(storedChoice);
  const value = useMemo<I18n>(() => {
    const language = languageFor(choice);
    const messages = LOCALES[language].messages as Record<string, string | undefined>;
    document.documentElement.lang = language;
    return {
      language,
      choice,
      setChoice: (next) => {
        setChoiceState(next);
        try {
          if (next === AUTO) localStorage.removeItem(STORAGE_KEY);
          else localStorage.setItem(STORAGE_KEY, next);
        } catch {
          // Not persisted; the choice still applies to this session.
        }
      },
      t: (key) => translateKey(messages, ENGLISH, key),
    };
  }, [choice]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  const context = useContext(I18nContext);
  if (!context) throw new Error("useI18n outside I18nProvider");
  return context;
}
