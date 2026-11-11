import type { Cue, Id, RunningCue } from "../api/types";
import { displayName, findCue } from "../cues";
import { formatTime } from "../format";
import { useI18n } from "../i18n";
import { CueIcon, Icon } from "./icons";

export function ActiveCues({
  running,
  cues,
  onPause,
  onResume,
  onStop,
}: {
  running: RunningCue[];
  cues: Cue[];
  onPause: (cue: Id) => void;
  onResume: (cue: Id) => void;
  onStop: (cue: Id) => void;
}) {
  const { t } = useI18n();
  return (
    <section className="panel active">
      <h2>
        {t("active.title")}
        {running.length > 0 && <span className="count">{running.length}</span>}
      </h2>
      {running.length === 0 && <p className="placeholder">{t("active.none")}</p>}
      <ul>
        {running.map((run) => {
          const cue = findCue(cues, run.cue);
          if (!cue) return null;
          const fraction = run.length ? Math.min(1, run.elapsed / run.length) : null;
          const remaining = run.length !== null ? Math.max(0, run.length - run.elapsed) : null;
          const state = run.paused ? "paused" : run.phase;
          return (
            <li key={run.cue} className={state}>
              <div className="active-head">
                <span className="active-name">
                  <CueIcon type={cue.type} size={14} />
                  <span className="active-number">{cue.number ?? "–"}</span>
                  <span className="active-title">{displayName(cue)}</span>
                </span>
                <span className="active-buttons">
                  {run.paused ? (
                    <button className="icon-button" onClick={() => onResume(run.cue)} title={t("action.resume")}>
                      <Icon name="play" size={11} />
                    </button>
                  ) : (
                    <button className="icon-button" onClick={() => onPause(run.cue)} title={t("action.pause")}>
                      <Icon name="pause" size={11} />
                    </button>
                  )}
                  <button className="icon-button" onClick={() => onStop(run.cue)} title={t("action.stop")}>
                    <Icon name="stop" size={11} />
                  </button>
                </span>
              </div>
              <div className="progress" aria-hidden>
                <div
                  className={fraction === null ? "indeterminate" : undefined}
                  style={fraction === null ? undefined : { width: `${fraction * 100}%` }}
                />
              </div>
              <div className="active-foot">
                <span>{formatTime(run.elapsed)}</span>
                <span className="active-state">{t(`phase.${state}`)}</span>
                <span>{remaining !== null ? `−${formatTime(remaining)}` : ""}</span>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
