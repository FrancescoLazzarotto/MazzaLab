import type { Cue } from "../api/types";
import { displayName } from "../cues";
import { useI18n } from "../i18n";

export function Transport({
  standby,
  panicking,
  onGo,
  onStopAll,
  onPauseAll,
  onResumeAll,
  onPanic,
  onReset,
}: {
  standby: Cue | undefined;
  panicking: boolean;
  onGo: () => void;
  onStopAll: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onPanic: () => void;
  onReset: () => void;
}) {
  const { t } = useI18n();
  return (
    <section className="transport">
      <button className="go" onClick={onGo} disabled={!standby} title="Space">
        {t("action.go")}
      </button>
      <div className="standby">
        <span className="standby-label">{t("standby.title")}</span>
        {standby ? (
          <>
            <span className="standby-cue">
              <strong>{standby.number ?? "-"}</strong> {displayName(standby)}
            </span>
            {standby.notes && <span className="standby-notes">{standby.notes}</span>}
          </>
        ) : (
          <span className="standby-cue muted">{t("standby.end")}</span>
        )}
      </div>
      <div className="transport-buttons">
        <button onClick={onPauseAll}>{t("action.pauseAll")}</button>
        <button onClick={onResumeAll}>{t("action.resumeAll")}</button>
        <button onClick={onStopAll}>{t("action.stopAll")}</button>
        <button onClick={onReset}>{t("action.reset")}</button>
        <button className={`panic ${panicking ? "active" : ""}`} onClick={onPanic} title="Esc">
          {t("action.panic")}
        </button>
      </div>
    </section>
  );
}
