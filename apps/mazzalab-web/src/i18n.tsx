import { createContext, useContext, useMemo, useState, type ReactNode } from "react";

export type Language = "en" | "it";
export const LANGUAGES: { code: Language; label: string }[] = [
  { code: "en", label: "English" },
  { code: "it", label: "Italiano" },
];

const en = {
  "app.connecting": "Connecting to the show server…",
  "app.disconnected": "Disconnected — reconnecting…",
  "app.unsaved": "Unsaved changes",
  "app.saved": "Saved",
  "action.save": "Save",
  "action.undo": "Undo",
  "action.redo": "Redo",
  "action.go": "GO",
  "action.stopAll": "Stop all",
  "action.pauseAll": "Pause all",
  "action.resumeAll": "Resume all",
  "action.panic": "Panic",
  "action.reset": "Reset",
  "action.delete": "Delete",
  "action.addCue": "Add cue",
  "action.stop": "Stop",
  "action.pause": "Pause",
  "action.resume": "Resume",
  "action.start": "Start",
  "action.setPlayhead": "Set playhead",
  "standby.title": "Standby",
  "standby.end": "End of list",
  "list.number": "#",
  "list.name": "Name",
  "list.type": "Type",
  "list.preWait": "Pre-wait",
  "list.duration": "Duration",
  "list.postWait": "Post-wait",
  "list.empty": "This cue list is empty. Add a cue to start.",
  "active.title": "Active cues",
  "active.none": "Nothing running",
  "inspector.title": "Inspector",
  "inspector.none": "Select a cue to edit it.",
  "inspector.basics": "Basics",
  "inspector.number": "Number",
  "inspector.name": "Name",
  "inspector.notes": "Notes",
  "inspector.color": "Colour",
  "inspector.noColor": "None",
  "inspector.timing": "Timing",
  "inspector.preWait": "Pre-wait",
  "inspector.postWait": "Post-wait",
  "inspector.continue": "Continue",
  "inspector.armed": "Armed",
  "inspector.flagged": "Flagged",
  "inspector.target": "Target",
  "inspector.noTarget": "No target",
  "inspector.file": "File",
  "inspector.level": "Level (dB)",
  "inspector.startTime": "Start time",
  "inspector.endTime": "End time",
  "inspector.playCount": "Play count",
  "inspector.loop": "Loop forever",
  "inspector.rate": "Rate",
  "inspector.duration": "Duration",
  "inspector.curve": "Curve",
  "inspector.stopTarget": "Stop target when done",
  "inspector.mode": "Mode",
  "inspector.message": "Message",
  "inspector.text": "Text",
  "inspector.commands": "Light commands (one per line)",
  "inspector.opacity": "Opacity",
  "continue.do_not_continue": "Do not continue",
  "continue.auto_continue": "Auto-continue",
  "continue.auto_follow": "Auto-follow",
  "curve.linear": "Linear",
  "curve.s_curve": "S-curve",
  "group.timeline": "Timeline (start all)",
  "group.start_first_and_enter": "Start first and enter",
  "group.start_first": "Start first",
  "group.start_random": "Start random",
  "group.playlist": "Playlist",
  "settings.language": "Language",
  "phase.pre_wait": "pre-wait",
  "phase.action": "running",
  "phase.paused": "paused",
};

type Key = keyof typeof en;

const it: Record<Key, string> = {
  "app.connecting": "Connessione al server dello show…",
  "app.disconnected": "Disconnesso — riconnessione in corso…",
  "app.unsaved": "Modifiche non salvate",
  "app.saved": "Salvato",
  "action.save": "Salva",
  "action.undo": "Annulla",
  "action.redo": "Ripeti",
  "action.go": "GO",
  "action.stopAll": "Ferma tutto",
  "action.pauseAll": "Pausa tutto",
  "action.resumeAll": "Riprendi tutto",
  "action.panic": "Panic",
  "action.reset": "Reset",
  "action.delete": "Elimina",
  "action.addCue": "Aggiungi cue",
  "action.stop": "Ferma",
  "action.pause": "Pausa",
  "action.resume": "Riprendi",
  "action.start": "Avvia",
  "action.setPlayhead": "Sposta playhead",
  "standby.title": "In attesa",
  "standby.end": "Fine della lista",
  "list.number": "#",
  "list.name": "Nome",
  "list.type": "Tipo",
  "list.preWait": "Pre-attesa",
  "list.duration": "Durata",
  "list.postWait": "Post-attesa",
  "list.empty": "Questa cue list è vuota. Aggiungi un cue per iniziare.",
  "active.title": "Cue attivi",
  "active.none": "Niente in esecuzione",
  "inspector.title": "Inspector",
  "inspector.none": "Seleziona un cue per modificarlo.",
  "inspector.basics": "Generale",
  "inspector.number": "Numero",
  "inspector.name": "Nome",
  "inspector.notes": "Note",
  "inspector.color": "Colore",
  "inspector.noColor": "Nessuno",
  "inspector.timing": "Tempi",
  "inspector.preWait": "Pre-attesa",
  "inspector.postWait": "Post-attesa",
  "inspector.continue": "Continuazione",
  "inspector.armed": "Armato",
  "inspector.flagged": "Segnalato",
  "inspector.target": "Bersaglio",
  "inspector.noTarget": "Nessun bersaglio",
  "inspector.file": "File",
  "inspector.level": "Livello (dB)",
  "inspector.startTime": "Inizio",
  "inspector.endTime": "Fine",
  "inspector.playCount": "Ripetizioni",
  "inspector.loop": "Loop infinito",
  "inspector.rate": "Velocità",
  "inspector.duration": "Durata",
  "inspector.curve": "Curva",
  "inspector.stopTarget": "Ferma il bersaglio alla fine",
  "inspector.mode": "Modalità",
  "inspector.message": "Messaggio",
  "inspector.text": "Testo",
  "inspector.commands": "Comandi luci (uno per riga)",
  "inspector.opacity": "Opacità",
  "continue.do_not_continue": "Non continuare",
  "continue.auto_continue": "Auto-continue",
  "continue.auto_follow": "Auto-follow",
  "curve.linear": "Lineare",
  "curve.s_curve": "Curva a S",
  "group.timeline": "Timeline (avvia tutti)",
  "group.start_first_and_enter": "Avvia il primo ed entra",
  "group.start_first": "Avvia il primo",
  "group.start_random": "Avvia a caso",
  "group.playlist": "Playlist",
  "settings.language": "Lingua",
  "phase.pre_wait": "pre-attesa",
  "phase.action": "in corso",
  "phase.paused": "in pausa",
};

export const DICTIONARIES: Record<Language, Record<Key, string>> = { en, it };

export type Translate = (key: Key) => string;

const STORAGE_KEY = "mazzalab.language";

export function initialLanguage(): Language {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "en" || saved === "it") return saved;
  } catch {
    // Storage can be unavailable (private mode); fall back to the browser.
  }
  return navigator.language.toLowerCase().startsWith("it") ? "it" : "en";
}

interface I18n {
  language: Language;
  setLanguage: (language: Language) => void;
  t: Translate;
}

const I18nContext = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<Language>(initialLanguage);
  const value = useMemo<I18n>(
    () => ({
      language,
      setLanguage: (next) => {
        setLanguageState(next);
        try {
          localStorage.setItem(STORAGE_KEY, next);
        } catch {
          // Not persisted; the choice still applies to this session.
        }
      },
      t: (key) => DICTIONARIES[language][key],
    }),
    [language],
  );
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  const context = useContext(I18nContext);
  if (!context) throw new Error("useI18n outside I18nProvider");
  return context;
}
