import type { Snapshot } from "../api/types";
import { useI18n } from "../i18n";
import type { Preferences } from "../preferences";
import { Icon } from "./icons";

export function TopBar({
  snapshot,
  mode,
  onMode,
  onUndo,
  onRedo,
  onSave,
  onSettings,
}: {
  snapshot: Snapshot | null;
  mode: Preferences["mode"];
  onMode: (mode: Preferences["mode"]) => void;
  onUndo: () => void;
  onRedo: () => void;
  onSave: () => void;
  onSettings: () => void;
}) {
  const { t } = useI18n();
  const editing = mode === "edit";
  return (
    <header className="top-bar">
      <div className="brand">
        <span className="wordmark">MazzaLab</span>
        {snapshot && <span className="show-name">{snapshot.workspace.name}</span>}
        {snapshot?.dirty && (
          <span className={snapshot.recovered ? "edited recovered" : "edited"} title={snapshot.recovered ? t("app.recoveredHint") : undefined}>
            {snapshot.recovered ? t("app.recovered") : t("app.unsaved")}
          </span>
        )}
      </div>
      <div className="segmented mode-switch" role="group" aria-label={t("mode.title")}>
        <button className={editing ? "active" : undefined} aria-pressed={editing} onClick={() => onMode("edit")} title={t("mode.editHint")}>
          {t("mode.edit")}
        </button>
        <button className={editing ? undefined : "active"} aria-pressed={!editing} onClick={() => onMode("show")} title={t("mode.showHint")}>
          {!editing && <Icon name="lock" size={12} />}
          {t("mode.show")}
        </button>
      </div>
      <div className="top-actions">
        <button className="icon-button" onClick={onUndo} disabled={!editing || !snapshot?.can_undo} title={`${t("action.undo")} (Ctrl+Z)`}>
          <Icon name="undo" />
        </button>
        <button className="icon-button" onClick={onRedo} disabled={!editing || !snapshot?.can_redo} title={`${t("action.redo")} (Ctrl+Shift+Z)`}>
          <Icon name="redo" />
        </button>
        <span className="divider" />
        <button className="icon-button" onClick={onSave} disabled={!snapshot?.dirty} title={`${t("action.save")} (Ctrl+S)`}>
          <Icon name="save" />
        </button>
        <button className="icon-button" onClick={onSettings} disabled={!snapshot} title={t("settings.title")}>
          <Icon name="settings" />
        </button>
      </div>
    </header>
  );
}
