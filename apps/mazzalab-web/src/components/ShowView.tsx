import { useMemo } from "react";
import type { Access } from "../access";
import { useShow, type ShowConnection } from "../api/connection";
import type { Command } from "../api/types";
import { displayName, findCue, flatten } from "../cues";
import { formatTime } from "../format";
import { useI18n } from "../i18n";
import { CueIcon, Icon } from "./icons";

/** The show reduced to what a tablet beside the stage manager needs: standby, GO, what runs, and the way out. */
export function ShowView({ connection, access }: { connection: ShowConnection; access: Access }) {
  const { t } = useI18n();
  const { snapshot, status, connection: link } = useShow(connection);
  const workspace = snapshot?.workspace;
  const list = workspace?.cue_lists.find((candidate) => candidate.id === snapshot?.current_list && !candidate.cart) ?? workspace?.cue_lists.find((candidate) => !candidate.cart);
  const allCues = useMemo(() => workspace?.cue_lists.flatMap((l) => l.cues) ?? [], [workspace]);
  const standbyId = status?.playheads.find((entry) => entry.list === list?.id)?.cue ?? null;
  const standby = standbyId ? findCue(allCues, standbyId) : undefined;
  const order = useMemo(() => (list ? flatten(list.cues).map((row) => row.cue.id) : []), [list]);
  const position = standbyId ? order.indexOf(standbyId) : -1;
  const running = status?.running ?? [];

  const run = (command: Command) => {
    connection.send(command).catch(() => {});
  };
  const listId = list?.id;

  if (!workspace || !list) {
    return (
      <div className="show-view">
        <div className="splash">{link === "closed" ? t("app.disconnected") : t("app.connecting")}</div>
      </div>
    );
  }
  return (
    <div className="show-view">
      <header>
        <span className="show-name">{workspace.name}</span>
        <span className={`connection-dot ${link}`} title={t(`connection.${link}`)} />
      </header>
      <section className="sv-standby">
        <span className="eyebrow">{t("standby.title")}</span>
        {standby ? (
          <>
            <div className="sv-cue">
              <span className="sv-number">{standby.number ?? "–"}</span>
              <span className="sv-name">{displayName(standby)}</span>
            </div>
            {standby.notes && <p className="sv-notes">{standby.notes}</p>}
          </>
        ) : (
          <div className="sv-cue sv-end">{t("standby.end")}</div>
        )}
      </section>
      <button className="go sv-go" onClick={() => run({ command: "go", list: listId })} disabled={!standby || !access.control}>
        {t("action.go")}
      </button>
      <div className="sv-steps">
        <button onClick={() => run({ command: "playhead_previous", list: listId })} disabled={!access.control || (position <= 0 && standbyId !== null)} aria-label={t("action.playheadPrevious")}>
          <Icon name="previous" size={22} />
        </button>
        <button onClick={() => run({ command: "playhead_next", list: listId })} disabled={!access.control || (standbyId !== null && position >= order.length - 1)} aria-label={t("action.playheadNext")}>
          <Icon name="next" size={22} />
        </button>
      </div>
      <section className="sv-running">
        {running.length === 0 && <span className="placeholder">{t("active.none")}</span>}
        {running.map((entry) => {
          const cue = findCue(allCues, entry.cue);
          if (!cue) return null;
          const fraction = entry.length ? Math.min(1, entry.elapsed / entry.length) : null;
          const remaining = entry.length !== null ? Math.max(0, entry.length - entry.elapsed) : null;
          return (
            <div key={entry.cue} className={`sv-run ${entry.paused ? "paused" : entry.phase}`}>
              <span className="sv-run-name">
                <CueIcon type={cue.type} size={18} />
                <span className="sv-run-number">{cue.number ?? "–"}</span>
                {displayName(cue)}
              </span>
              <span className="sv-run-time">{remaining !== null ? `−${formatTime(remaining)}` : formatTime(entry.elapsed)}</span>
              <div className="progress" aria-hidden>
                <div className={fraction === null ? "indeterminate" : undefined} style={fraction === null ? undefined : { width: `${fraction * 100}%` }} />
              </div>
            </div>
          );
        })}
      </section>
      <footer className="sv-actions">
        <button onClick={() => run({ command: "stop_all" })} disabled={!access.control}>
          <Icon name="stop" size={16} />
          {t("action.stop")}
        </button>
        <button className={`panic ${status?.panicking ? "active" : ""}`} onClick={() => run({ command: "panic" })} disabled={!access.control}>
          {t("action.panic")}
        </button>
      </footer>
    </div>
  );
}
