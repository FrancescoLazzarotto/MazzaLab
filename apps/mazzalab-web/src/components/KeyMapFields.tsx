import { useState } from "react";
import type { Cue } from "../api/types";
import { flatten } from "../cues";
import { hotkeyOf } from "../hotkeys";
import { useI18n } from "../i18n";
import { DEFAULT_KEYMAP, TRANSPORT_ACTIONS, bind, isDefaultKeyMap, type Conflict, type KeyMap, type TransportAction } from "../keymap";
import { Icon } from "./icons";

type Label = Parameters<ReturnType<typeof useI18n>["t"]>[0];

export const ACTION_LABELS: Record<TransportAction, Label> = {
  go: "action.go",
  stopAll: "action.stopAll",
  pauseAll: "action.pauseAll",
  resumeAll: "action.resumeAll",
  playheadNext: "action.playheadNext",
  playheadPrevious: "action.playheadPrevious",
  panic: "action.panic",
};

/** Rebinding of the transport keys: click a key, press the new one. */
export function KeyMapFields({ keymap, cues, onChange }: { keymap: KeyMap; cues: Cue[]; onChange: (keymap: KeyMap) => void }) {
  const { t } = useI18n();
  const [capturing, setCapturing] = useState<TransportAction | null>(null);
  const [problem, setProblem] = useState<{ action: TransportAction; conflict: Conflict } | null>(null);
  const cueHotkeys = new Set(flatten(cues).flatMap((row) => (row.cue.triggers?.hotkey ? [row.cue.triggers.hotkey] : [])));

  const assign = (action: TransportAction, hotkey: string | null) => {
    const result = bind(keymap, action, hotkey, cueHotkeys);
    setProblem(result.conflict ? { action, conflict: result.conflict } : null);
    if (!result.conflict) {
      onChange(result.keymap);
      setCapturing(null);
    }
  };

  const describe = (conflict: Conflict): string =>
    conflict.kind === "action" ? `${t("keys.usedBy")} ${t(ACTION_LABELS[conflict.action])}` : conflict.kind === "screen" ? t("keys.reserved") : t("keys.cueHotkey");

  return (
    <div className="keymap">
      <h3>{t("keys.transport")}</h3>
      {TRANSPORT_ACTIONS.map((action) => (
        <div key={action} className="keymap-row">
          <span className="keymap-label">{t(ACTION_LABELS[action])}</span>
          <button
            type="button"
            className={capturing === action ? "key-capture capturing" : "key-capture"}
            onClick={() => {
              setProblem(null);
              setCapturing(action);
            }}
            onBlur={() => setCapturing((current) => (current === action ? null : current))}
            onKeyDown={(event) => {
              if (capturing !== action) return;
              event.preventDefault();
              event.stopPropagation();
              if (event.key === "Escape") return setCapturing(null);
              if (event.key === "Backspace" || event.key === "Delete") return assign(action, null);
              const hotkey = hotkeyOf(event.nativeEvent);
              if (hotkey) assign(action, hotkey);
            }}
          >
            {capturing === action ? t("keys.press") : keymap[action] ? <kbd>{keymap[action]}</kbd> : <span className="none">{t("keys.none")}</span>}
          </button>
          <button type="button" className="icon-button quiet" disabled={keymap[action] === null} onClick={() => assign(action, null)} title={t("keys.clear")}>
            <Icon name="close" size={12} />
          </button>
          {problem?.action === action && <span className="keymap-problem">{describe(problem.conflict)}</span>}
        </div>
      ))}
      <button type="button" className="restore" disabled={isDefaultKeyMap(keymap)} onClick={() => onChange({ ...DEFAULT_KEYMAP })}>
        {t("keys.restore")}
      </button>
    </div>
  );
}
