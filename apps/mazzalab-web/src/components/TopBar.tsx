import type { ConnectionState } from "../api/connection";
import type { Snapshot } from "../api/types";
import { LANGUAGES, useI18n, type Language } from "../i18n";

export function TopBar({
  snapshot,
  connection,
  onUndo,
  onRedo,
  onSave,
}: {
  snapshot: Snapshot | null;
  connection: ConnectionState;
  onUndo: () => void;
  onRedo: () => void;
  onSave: () => void;
}) {
  const { t, language, setLanguage } = useI18n();
  return (
    <header className="top-bar">
      <div className="brand">
        <span className="logo">MazzaLab</span>
        {snapshot && <span className="show-name">{snapshot.workspace.name}</span>}
        {snapshot && (
          <span className={`save-state ${snapshot.dirty ? "dirty" : ""}`}>
            {snapshot.dirty ? t("app.unsaved") : t("app.saved")}
          </span>
        )}
      </div>
      <div className="top-actions">
        <span className={`connection ${connection}`} title={connection}>
          ●
        </span>
        <button onClick={onUndo} disabled={!snapshot?.can_undo} title="Ctrl+Z">
          {t("action.undo")}
        </button>
        <button onClick={onRedo} disabled={!snapshot?.can_redo} title="Ctrl+Shift+Z">
          {t("action.redo")}
        </button>
        <button onClick={onSave} disabled={!snapshot?.dirty} title="Ctrl+S">
          {t("action.save")}
        </button>
        <select
          aria-label={t("settings.language")}
          value={language}
          onChange={(event) => setLanguage(event.target.value as Language)}
        >
          {LANGUAGES.map((option) => (
            <option key={option.code} value={option.code}>
              {option.label}
            </option>
          ))}
        </select>
      </div>
    </header>
  );
}
