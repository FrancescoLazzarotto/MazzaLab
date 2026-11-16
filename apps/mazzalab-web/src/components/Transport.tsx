import type { Cue } from "../api/types";
import { displayName } from "../cues";
import { useI18n } from "../i18n";
import { Icon } from "./icons";

export function Transport({
  standby,
  panicking,
  disabled = false,
  onGo,
  onStopAll,
  onPauseAll,
  onResumeAll,
  onPanic,
  onReset,
}: {
  standby: Cue | undefined;
  panicking: boolean;
  /** The passcode does not let this device run the show. */
  disabled?: boolean;
  onGo: () => void;
  onStopAll: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onPanic: () => void;
  onReset: () => void;
}) {
  const { t } = useI18n();
  return (
    <section className={disabled ? "transport dimmed" : "transport"}>
      <button className="go" onClick={onGo} disabled={!standby || disabled} title={`${t("action.go")} (${t("keys.space")})`}>
        {t("action.go")}
      </button>
      <div className="standby">
        <span className="eyebrow">{t("standby.title")}</span>
        {standby ? (
          <span className="standby-cue">
            <span className="standby-number">{standby.number ?? "–"}</span>
            <span className="standby-name">{displayName(standby)}</span>
          </span>
        ) : (
          <span className="standby-cue standby-end">{t("standby.end")}</span>
        )}
        <span className="standby-notes">{standby?.notes}</span>
      </div>
      <div className="transport-buttons">
        <button onClick={onPauseAll} disabled={disabled} title={t("action.pauseAll")}>
          <Icon name="pause" size={12} />
          {t("action.pause")}
        </button>
        <button onClick={onResumeAll} disabled={disabled} title={t("action.resumeAll")}>
          <Icon name="play" size={12} />
          {t("action.resume")}
        </button>
        <button onClick={onStopAll} disabled={disabled} title={t("action.stopAll")}>
          <Icon name="stop" size={12} />
          {t("action.stop")}
        </button>
        <button onClick={onReset} disabled={disabled} title={t("action.resetHint")}>
          <Icon name="reset" size={13} />
          {t("action.reset")}
        </button>
        <button className={`panic ${panicking ? "active" : ""}`} onClick={onPanic} disabled={disabled} title={`${t("keys.panic")} (Esc)`}>
          {t("action.panic")}
        </button>
      </div>
    </section>
  );
}
