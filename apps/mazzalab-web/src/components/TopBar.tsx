import type { ConnectionState } from "../api/connection";
import type { Snapshot } from "../api/types";
import { LANGUAGES, useI18n, type Language } from "../i18n";
import type { Preferences } from "../preferences";
import { Icon } from "./icons";

export function TopBar({
  snapshot,
  connection,
  mode,
  onMode,
  onUndo,
  onRedo,
  onSave,
  onSettings,
}: {
  snapshot: Snapshot | null;
  connection: ConnectionState;
  mode: Preferences["mode"];
  onMode: (mode: Preferences["mode"]) => void;
  onUndo: () => void;
  onRedo: () => void;
  onSave: () => void;
  onSettings: () => void;
}) {
  const { t, language, setLanguage } = useI18n();
  const editing = mode === "edit";
  return (
    <header className="top-bar">
      <div className="brand">
        <span className="logo">MazzaLab</span>
        {snapshot && <span className="show-name">{snapshot.workspace.name}</span>}
        {snapshot && (
          <span className={`save-state ${snapshot.dirty ? "dirty" : ""}`}>{snapshot.dirty ? t("app.unsaved") : t("app.saved")}</span>
        )}
      </div>
      <div className="mode-switch" role="group" aria-label={t("mode.title")}>
        <button className={editing ? "active" : undefined} onClick={() => onMode("edit")} title={t("mode.editHint")}>
          <Icon name="edit" size={14} /> {t("mode.edit")}
        </button>
        <button className={editing ? undefined : "active show"} onClick={() => onMode("show")} title={t("mode.showHint")}>
          <Icon name="lock" size={14} /> {t("mode.show")}
        </button>
      </div>
      <div className="top-actions">
        <span className={`connection ${connection}`} title={t(`connection.${connection}`)}>
          ●
        </span>
        <button className="icon-button" onClick={onUndo} disabled={!editing || !snapshot?.can_undo} title={`${t("action.undo")} (Ctrl+Z)`}>
          <Icon name="undo" />
        </button>
        <button className="icon-button" onClick={onRedo} disabled={!editing || !snapshot?.can_redo} title={`${t("action.redo")} (Ctrl+Shift+Z)`}>
          <Icon name="redo" />
        </button>
        <button onClick={onSave} disabled={!snapshot?.dirty} title="Ctrl+S">
          <Icon name="save" size={14} /> {t("action.save")}
        </button>
        <button className="icon-button" onClick={onSettings} disabled={!snapshot} title={t("settings.title")}>
          <Icon name="settings" />
        </button>
        <select aria-label={t("settings.language")} value={language} onChange={(event) => setLanguage(event.target.value as Language)}>
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
