import { useCallback, useEffect, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type { Cue, Id, PreflightIssue, Severity } from "../api/types";
import { displayName, findCue } from "../cues";
import { useI18n } from "../i18n";
import { groupIssues } from "../preflight";
import { Icon } from "./icons";

const ORDER: Severity[] = ["error", "warning", "info"];

/** What would not work if GO were pressed now, by how much it matters; a click goes to the cue. */
export function PreflightDialog({ connection, cues, onSelect, onClose }: { connection: ShowConnection; cues: Cue[]; onSelect: (cue: Id) => void; onClose: () => void }) {
  const { t } = useI18n();
  const [issues, setIssues] = useState<PreflightIssue[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    setError(null);
    connection
      .preflight()
      .then(setIssues)
      .catch((reason: Error) => setError(reason.message));
  }, [connection]);
  useEffect(load, [load]);

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

  const groups = groupIssues(issues ?? []);
  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="dialog preflight" role="dialog" aria-modal aria-label={t("preflight.title")}>
        <header>
          <h1>{t("preflight.title")}</h1>
          <button className="icon-button quiet" onClick={load} title={t("preflight.again")}>
            <Icon name="restore" size={13} />
          </button>
          <button className="icon-button quiet" onClick={onClose} aria-label={t("action.close")}>
            <Icon name="close" size={14} />
          </button>
        </header>
        <div className="dialog-body">
          {error && <p className="hint warn">{error}</p>}
          {issues === null && !error && <p className="hint">{t("app.connecting")}</p>}
          {issues !== null && issues.length === 0 && (
            <p className="preflight-clear">
              <Icon name="check" size={16} />
              {t("preflight.clear")}
            </p>
          )}
          {ORDER.map((severity) =>
            groups[severity].length === 0 ? null : (
              <section key={severity} className={`preflight-group ${severity}`}>
                <h3>
                  {t(`preflight.${severity}`)} <span className="count">{groups[severity].length}</span>
                </h3>
                <ul>
                  {groups[severity].map((issue, index) => {
                    const cue = issue.cue ? findCue(cues, issue.cue) : undefined;
                    const text = (
                      <>
                        <span className="preflight-message">{t(`preflight.key.${issue.key}`)}</span>
                        {issue.detail && <span className="preflight-detail">{issue.detail}</span>}
                      </>
                    );
                    return (
                      <li key={index}>
                        {cue ? (
                          <button className="preflight-row" onClick={() => onSelect(cue.id)}>
                            <span className="preflight-cue">
                              <span className="preflight-number">{cue.number ?? "–"}</span>
                              {displayName(cue)}
                            </span>
                            {text}
                          </button>
                        ) : (
                          <div className="preflight-row">{text}</div>
                        )}
                      </li>
                    );
                  })}
                </ul>
              </section>
            ),
          )}
        </div>
      </div>
    </div>
  );
}
