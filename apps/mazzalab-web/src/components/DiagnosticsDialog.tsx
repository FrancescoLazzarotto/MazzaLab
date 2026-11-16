import { useEffect, useRef, useState } from "react";
import type { ShowConnection } from "../api/connection";
import { HISTORY, formatUptime, lateWithin, percent, pushSample, sampleOf, sparkline, type Diagnostics, type Sample } from "../diagnostics";
import { useI18n } from "../i18n";
import { Icon } from "./icons";

const POLL_MS = 2000;
const GRAPH = { width: 240, height: 36 };

/**
 * How the server is doing: the audio thread's load over the last minute,
 * the callbacks that came late (the likely dropouts), uptime and clients.
 * The load is measured since the previous reading, so it is read every two
 * seconds while this is open.
 */
export function DiagnosticsDialog({ connection, onClose }: { connection: ShowConnection; onClose: () => void }) {
  const { t } = useI18n();
  const [reading, setReading] = useState<Diagnostics | null>(null);
  const [history, setHistory] = useState<Sample[]>([]);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    const read = () =>
      connection
        .diagnostics()
        .then((next) => {
          if (!alive.current) return;
          setReading(next);
          setError(null);
          setHistory((current) => pushSample(current, sampleOf(next)));
        })
        .catch((reason: Error) => alive.current && setError(reason.message));
    void read();
    const timer = setInterval(read, POLL_MS);
    return () => {
      alive.current = false;
      clearInterval(timer);
    };
  }, [connection]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const late = lateWithin(history);
  const hasAudio = reading?.audio_load !== undefined;
  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="dialog small diagnostics" role="dialog" aria-modal aria-label={t("diagnostics.title")}>
        <header>
          <h1>{t("diagnostics.title")}</h1>
          <button className="icon-button quiet" onClick={onClose} aria-label={t("action.close")}>
            <Icon name="close" size={14} />
          </button>
        </header>
        <div className="dialog-body">
          {error && <p className="hint warn">{error}</p>}
          {reading && (
            <dl className="diagnostics-list">
              <dt>{t("diagnostics.uptime")}</dt>
              <dd>{formatUptime(reading.uptime)}</dd>
              <dt>{t("diagnostics.clients")}</dt>
              <dd>{reading.clients}</dd>
              <dt>{t("diagnostics.running")}</dt>
              <dd>{reading.running}</dd>
              {hasAudio ? (
                <>
                  <dt>{t("diagnostics.load")}</dt>
                  <dd>
                    {percent(reading.audio_load ?? 0)}
                    {reading.audio_peak_load !== undefined && (
                      <span className="diagnostics-peak">
                        {" "}
                        · {t("diagnostics.peak")} {percent(reading.audio_peak_load)}
                      </span>
                    )}
                    <svg className="sparkline" width={GRAPH.width} height={GRAPH.height} viewBox={`0 0 ${GRAPH.width} ${GRAPH.height}`} role="img" aria-label={t("diagnostics.load")}>
                      <line x1="0" y1={GRAPH.height} x2={GRAPH.width} y2={GRAPH.height} className="baseline" />
                      <polyline points={sparkline(history, GRAPH.width, GRAPH.height, HISTORY)} />
                    </svg>
                  </dd>
                  <dt>{t("diagnostics.late")}</dt>
                  <dd className={late > 0 ? "late rising" : "late"} role={late > 0 ? "alert" : undefined}>
                    {reading.audio_late ?? 0}
                    {late > 0 && (
                      <span>
                        {" "}
                        (+{late} {t("diagnostics.lastMinute")})
                      </span>
                    )}
                  </dd>
                  {reading.audio_callbacks !== undefined && (
                    <>
                      <dt>{t("diagnostics.callbacks")}</dt>
                      <dd>{reading.audio_callbacks}</dd>
                    </>
                  )}
                </>
              ) : (
                <>
                  <dt>{t("diagnostics.audio")}</dt>
                  <dd>{t("audio.none")}</dd>
                </>
              )}
              {(reading.record || reading.record_error) && (
                <>
                  <dt>{t("diagnostics.record")}</dt>
                  <dd className={reading.record_error ? "late rising" : undefined}>{reading.record_error ?? reading.record}</dd>
                </>
              )}
            </dl>
          )}
        </div>
      </div>
    </div>
  );
}
