import { useEffect, useRef } from "react";
import type { Cue, Id, RunningCue } from "../api/types";
import { actionLength, displayName, flatten, typeLabel } from "../cues";
import { formatTime } from "../format";
import { useI18n } from "../i18n";

const CONTINUE_GLYPH = { do_not_continue: "", auto_continue: "↓", auto_follow: "⤓" } as const;

export function CueList({
  cues,
  playhead,
  selection,
  running,
  onSelect,
  onSetPlayhead,
}: {
  cues: Cue[];
  playhead: Id | null;
  selection: Id | null;
  running: Map<Id, RunningCue>;
  onSelect: (cue: Id) => void;
  onSetPlayhead: (cue: Id) => void;
}) {
  const { t } = useI18n();
  const rows = flatten(cues);
  const selectedRow = useRef<HTMLTableRowElement>(null);
  useEffect(() => selectedRow.current?.scrollIntoView({ block: "nearest" }), [selection]);

  if (rows.length === 0) {
    return <p className="muted empty">{t("list.empty")}</p>;
  }
  return (
    <div className="cue-list" role="grid">
      <table>
        <thead>
          <tr>
            <th className="col-marker" />
            <th className="col-number">{t("list.number")}</th>
            <th>{t("list.name")}</th>
            <th className="col-type">{t("list.type")}</th>
            <th className="col-time">{t("list.preWait")}</th>
            <th className="col-time">{t("list.duration")}</th>
            <th className="col-time">{t("list.postWait")}</th>
            <th className="col-marker" />
          </tr>
        </thead>
        <tbody>
          {rows.map(({ cue, depth }) => {
            const run = running.get(cue.id);
            const length = actionLength(cue);
            const progress =
              run && run.length ? Math.min(1, run.elapsed / run.length) : run ? 1 : 0;
            const classes = [
              cue.id === selection && "selected",
              cue.id === playhead && "playhead",
              run && (run.paused ? "paused" : "running"),
              cue.armed === false && "disarmed",
              cue.color && `color-${cue.color}`,
            ]
              .filter(Boolean)
              .join(" ");
            return (
              <tr
                key={cue.id}
                ref={cue.id === selection ? selectedRow : undefined}
                className={classes}
                onClick={() => onSelect(cue.id)}
                onDoubleClick={() => onSetPlayhead(cue.id)}
                style={run ? { ["--progress" as string]: `${progress * 100}%` } : undefined}
              >
                <td className="col-marker">{cue.id === playhead ? "▶" : ""}</td>
                <td className="col-number">{cue.number ?? ""}</td>
                <td className="col-name" style={{ paddingLeft: `${0.6 + depth * 1.2}rem` }}>
                  {cue.type === "group" && <span className="group-glyph">▾ </span>}
                  {displayName(cue)}
                  {cue.flagged && <span className="flag"> ⚑</span>}
                </td>
                <td className="col-type">{typeLabel(cue.type)}</td>
                <td className="col-time">{cue.pre_wait ? formatTime(cue.pre_wait) : ""}</td>
                <td className="col-time">
                  {run?.phase === "action" ? formatTime(run.elapsed) : length ? formatTime(length) : ""}
                </td>
                <td className="col-time">{cue.post_wait ? formatTime(cue.post_wait) : ""}</td>
                <td className="col-marker continue" title={t(`continue.${cue.continue_mode ?? "do_not_continue"}`)}>
                  {CONTINUE_GLYPH[cue.continue_mode ?? "do_not_continue"]}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
