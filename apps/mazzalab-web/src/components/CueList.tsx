import { useEffect, useRef, useState, type CSSProperties, type DragEvent } from "react";
import type { Cue, Id, RunningCue } from "../api/types";
import { actionLength, displayName, fileName, findCue, flatten, midiSummary, needsTarget, problemOf, typeLabel, type Drop } from "../cues";
import { formatTime } from "../format";
import { useI18n } from "../i18n";
import { CueIcon, Icon } from "./icons";

const DRAG_TYPE = "application/x-mazzalab-cues";

export type SelectMode = "single" | "toggle" | "range";

interface Editing {
  cue: Id;
  field: "number" | "name";
}

/** Where a drop would land, from the pointer's height inside a row. */
function dropFor(event: DragEvent<HTMLElement>, cue: Cue): Drop {
  const bounds = event.currentTarget.getBoundingClientRect();
  const fraction = (event.clientY - bounds.top) / bounds.height;
  if (cue.type === "group") {
    if (fraction < 0.25) return { where: "before", anchor: cue.id };
    if (fraction > 0.75 && (cue.children ?? []).length === 0) return { where: "after", anchor: cue.id };
    return { where: "into", anchor: cue.id };
  }
  return { where: fraction < 0.5 ? "before" : "after", anchor: cue.id };
}

function sameDrop(a: Drop | null, b: Drop | null): boolean {
  if (a === null || b === null) return a === b;
  if (a.where === "end" || b.where === "end") return a.where === b.where;
  return a.where === b.where && a.anchor === b.anchor;
}

/** A time that fills up as it elapses. */
function TimeCell({ text, progress }: { text: string; progress: number | null }) {
  if (progress === null) return <>{text}</>;
  return (
    <span className="elapsing" style={{ "--progress": `${Math.min(1, progress) * 100}%` } as CSSProperties}>
      {text}
    </span>
  );
}

export function CueList({
  cues,
  allCues,
  playhead,
  selection,
  running,
  loaded,
  editable,
  onSelect,
  onSetPlayhead,
  onMove,
  onDropFiles,
  onEdit,
}: {
  cues: Cue[];
  /** Every cue of the show, to resolve targets in other lists. */
  allCues: Cue[];
  playhead: Id | null;
  selection: ReadonlySet<Id>;
  running: Map<Id, RunningCue>;
  /** Cues loaded to a time, in seconds from the start of their pre-wait. */
  loaded: Map<Id, number>;
  editable: boolean;
  onSelect: (cue: Id, mode: SelectMode) => void;
  onSetPlayhead: (cue: Id) => void;
  /** Cues were dragged: `dragged` is the row the drag started on. */
  onMove: (dragged: Id, drop: Drop) => void;
  onDropFiles: (files: File[], drop: Drop) => void;
  onEdit: (cue: Cue) => void;
}) {
  const { t } = useI18n();
  const rows = flatten(cues);
  const [drop, setDrop] = useState<Drop | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const dragged = useRef<Id | null>(null);

  // Keep the cue the operator is working on in view: the last one selected.
  const focus = [...selection].pop();
  const focusRow = useRef<HTMLTableRowElement>(null);
  useEffect(() => {
    // Not returned: recent browsers hand back a promise, which React would
    // take for a cleanup function.
    void focusRow.current?.scrollIntoView({ block: "nearest" });
  }, [focus]);

  const accepts = (event: DragEvent) =>
    editable && (event.dataTransfer.types.includes(DRAG_TYPE) || event.dataTransfer.types.includes("Files"));
  const over = (event: DragEvent<HTMLElement>, next: Drop) => {
    if (!accepts(event)) return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer.dropEffect = event.dataTransfer.types.includes("Files") ? "copy" : "move";
    if (!sameDrop(drop, next)) setDrop(next);
  };
  const finish = (event: DragEvent<HTMLElement>, target: Drop) => {
    if (!accepts(event)) return;
    event.preventDefault();
    event.stopPropagation();
    setDrop(null);
    if (event.dataTransfer.files.length > 0) onDropFiles([...event.dataTransfer.files], target);
    else if (dragged.current) onMove(dragged.current, target);
    dragged.current = null;
  };

  const commit = (cue: Cue, field: Editing["field"], value: string) => {
    setEditing(null);
    const text = value.trim();
    if (field === "number" && text !== (cue.number ?? "")) onEdit({ ...cue, number: text || undefined });
    if (field === "name" && text !== (cue.name ?? "")) onEdit({ ...cue, name: text });
  };

  const exists = (id: Id) => findCue(allCues, id) !== undefined;
  const targetLabel = (cue: Cue): string => {
    if (cue.type === "audio" || cue.type === "video") return cue.file ? fileName(cue.file) : "";
    if (cue.type === "midi") return midiSummary(cue.message);
    if (cue.type === "network") return cue.message.split(/\r?\n/)[0];
    if (cue.type === "light") {
      const lines = cue.commands ?? [];
      return lines.length > 1 ? `${lines[0]}  +${lines.length - 1}` : (lines[0] ?? "");
    }
    if (!needsTarget(cue.type) || !cue.target) return "";
    const target = findCue(allCues, cue.target);
    return target ? `${target.number ?? "–"}  ${displayName(target)}` : "?";
  };

  return (
    <div
      className={`cue-list${drop?.where === "end" ? " drop-end" : ""}`}
      role="grid"
      onDragOver={(event) => over(event, { where: "end" })}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDrop(null);
      }}
      onDrop={(event) => finish(event, { where: "end" })}
    >
      <table>
        <thead>
          <tr>
            <th className="col-gutter" />
            <th className="col-number">{t("list.number")}</th>
            <th className="col-name">{t("list.name")}</th>
            <th className="col-target">{t("list.target")}</th>
            <th className="col-time">{t("list.preWait")}</th>
            <th className="col-time">{t("list.duration")}</th>
            <th className="col-time">{t("list.postWait")}</th>
            <th className="col-mark" />
            <th className="col-mark" />
          </tr>
        </thead>
        <tbody>
          {rows.map(({ cue, depth }) => {
            const run = running.get(cue.id);
            const length = actionLength(cue);
            const problem = problemOf(cue, exists);
            const mode = cue.continue_mode ?? "do_not_continue";
            const dropHere = drop && drop.where !== "end" && drop.anchor === cue.id ? `drop-${drop.where}` : null;
            const classes = [
              selection.has(cue.id) && "selected",
              cue.id === playhead && "playhead",
              run && (run.paused ? "paused" : run.phase === "pre_wait" ? "waiting" : "running"),
              cue.armed === false && "disarmed",
              cue.color && `color-${cue.color}`,
              dropHere,
            ]
              .filter(Boolean)
              .join(" ");
            const cell = (field: Editing["field"], shown: string) =>
              editing?.cue === cue.id && editing.field === field ? (
                <input
                  className="inline-edit"
                  autoFocus
                  defaultValue={field === "number" ? (cue.number ?? "") : (cue.name ?? "")}
                  onFocus={(event) => event.target.select()}
                  onBlur={(event) => commit(cue, field, event.target.value)}
                  onClick={(event) => event.stopPropagation()}
                  onKeyDown={(event) => {
                    event.stopPropagation();
                    if (event.key === "Enter") event.currentTarget.blur();
                    if (event.key === "Escape") setEditing(null);
                  }}
                />
              ) : (
                shown
              );
            // While a cue runs, the wait or the action it is in counts up
            // and fills; an action of unknown length is simply lit.
            const preProgress = run?.phase === "pre_wait" && run.length ? run.elapsed / run.length : null;
            const actionProgress = run?.phase === "action" ? (run.length ? run.elapsed / run.length : 1) : null;
            return (
              <tr
                key={cue.id}
                ref={cue.id === focus ? focusRow : undefined}
                className={classes}
                draggable={editable && editing === null}
                onClick={(event) => onSelect(cue.id, event.shiftKey ? "range" : event.ctrlKey || event.metaKey ? "toggle" : "single")}
                onDragStart={(event) => {
                  dragged.current = cue.id;
                  event.dataTransfer.setData(DRAG_TYPE, cue.id);
                  event.dataTransfer.effectAllowed = "move";
                }}
                onDragEnd={() => {
                  dragged.current = null;
                  setDrop(null);
                }}
                onDragOver={(event) => over(event, dropFor(event, cue))}
                onDrop={(event) => finish(event, dropFor(event, cue))}
              >
                <td
                  className={loaded.has(cue.id) ? "col-gutter loaded" : "col-gutter"}
                  onDoubleClick={() => onSetPlayhead(cue.id)}
                  title={loaded.has(cue.id) ? `${t("list.loaded")} ${formatTime(loaded.get(cue.id))}` : t("action.setPlayhead")}
                >
                  {loaded.has(cue.id) ? <Icon name="loaded" size={11} /> : cue.id === playhead && <Icon name="playhead" size={11} />}
                </td>
                <td className="col-number" onDoubleClick={() => editable && setEditing({ cue: cue.id, field: "number" })}>
                  {cell("number", cue.number ?? "")}
                </td>
                <td className="col-name" onDoubleClick={() => editable && setEditing({ cue: cue.id, field: "name" })}>
                  <span className="name-cell" style={{ paddingLeft: `${depth * 18}px` }}>
                    <span className="type-icon" title={typeLabel(cue.type)}>
                      <CueIcon type={cue.type} size={15} />
                    </span>
                    <span className="name-text">{cell("name", displayName(cue))}</span>
                    {cue.triggers?.hotkey && <kbd className="hotkey-badge">{cue.triggers.hotkey}</kbd>}
                    {cue.notes && <span className="notes-text">{cue.notes}</span>}
                  </span>
                </td>
                <td className="col-target">{targetLabel(cue)}</td>
                <td className="col-time">
                  <TimeCell text={run?.phase === "pre_wait" ? formatTime(run.elapsed) : cue.pre_wait ? formatTime(cue.pre_wait) : ""} progress={preProgress} />
                </td>
                <td className="col-time">
                  <TimeCell
                    text={run?.phase === "action" ? formatTime(run.elapsed) : length ? formatTime(length) : cue.type === "audio" && cue.infinite_loop ? "∞" : ""}
                    progress={actionProgress}
                  />
                </td>
                <td className="col-time">{cue.post_wait ? formatTime(cue.post_wait) : ""}</td>
                <td className="col-mark continue" title={t(`continue.${mode}`)}>
                  {mode !== "do_not_continue" && <Icon name={mode} size={13} />}
                </td>
                <td className="col-mark">
                  {problem ? (
                    <span className={problem === "not_supported" ? "problem soft" : "problem"} title={t(`problem.${problem}`)}>
                      <Icon name="warning" size={13} />
                    </span>
                  ) : cue.flagged ? (
                    <span className="flag">
                      <Icon name="flag" size={13} />
                    </span>
                  ) : null}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {rows.length === 0 && <p className="empty">{editable ? t("list.empty") : t("list.emptyShow")}</p>}
    </div>
  );
}
