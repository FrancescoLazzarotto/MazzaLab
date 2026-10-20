import type { Cue, Id, RunningCue } from "../api/types";
import { displayName, findCue } from "../cues";
import { formatTime } from "../format";
import { useI18n } from "../i18n";

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
      <h2>{t("active.title")}</h2>
      {running.length === 0 && <p className="muted">{t("active.none")}</p>}
      <ul>
        {running.map((run) => {
          const cue = findCue(cues, run.cue);
          if (!cue) return null;
          const fraction = run.length ? Math.min(1, run.elapsed / run.length) : null;
          const remaining = run.length !== null ? run.length - run.elapsed : null;
          const phase = run.paused ? t("phase.paused") : t(`phase.${run.phase}`);
          return (
            <li key={run.cue} className={run.paused ? "paused" : undefined}>
              <div className="active-head">
                <span className="active-name">
                  <strong>{cue.number ?? "-"}</strong> {displayName(cue)}
                </span>
                <span className="muted">{phase}</span>
              </div>
              <div className="meter" aria-hidden>
                <div
                  className={fraction === null ? "indeterminate" : undefined}
                  style={fraction === null ? undefined : { width: `${fraction * 100}%` }}
                />
              </div>
              <div className="active-foot">
                <span className="mono">{formatTime(run.elapsed)}</span>
                <span className="mono">{remaining !== null ? `-${formatTime(remaining)}` : ""}</span>
                <span className="buttons">
                  {run.paused ? (
                    <button onClick={() => onResume(run.cue)}>{t("action.resume")}</button>
                  ) : (
                    <button onClick={() => onPause(run.cue)}>{t("action.pause")}</button>
                  )}
                  <button onClick={() => onStop(run.cue)}>{t("action.stop")}</button>
                </span>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
