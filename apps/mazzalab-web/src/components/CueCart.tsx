import { useRef, useState, type CSSProperties, type DragEvent } from "react";
import type { Cue, Id, RunningCue } from "../api/types";
import { displayName, findCue, problemOf, typeLabel, type Drop } from "../cues";
import { formatTime } from "../format";
import { useI18n } from "../i18n";
import type { SelectMode } from "./CueList";
import { CueIcon, Icon } from "./icons";

const DRAG_TYPE = "application/x-mazzalab-cues";

/** Before or after a tile, from which half of it the pointer is over. */
function dropFor(event: DragEvent<HTMLElement>, cue: Cue): Drop {
  const bounds = event.currentTarget.getBoundingClientRect();
  return { where: event.clientX - bounds.left < bounds.width / 2 ? "before" : "after", anchor: cue.id };
}

/**
 * A cue list shown as a grid of buttons, for cues that are called up in no
 * particular order. While running the show a click starts a cue and another
 * stops it; while editing a click selects and a double click tries it out.
 */
export function CueCart({
  cues,
  allCues,
  selection,
  running,
  loaded,
  editable,
  onSelect,
  onToggle,
  onMove,
  onDropFiles,
}: {
  cues: Cue[];
  /** Every cue of the show, to check targets in other lists. */
  allCues: Cue[];
  selection: ReadonlySet<Id>;
  running: Map<Id, RunningCue>;
  loaded: Map<Id, number>;
  editable: boolean;
  onSelect: (cue: Id, mode: SelectMode) => void;
  /** Start the cue, or stop it if it is running. */
  onToggle: (cue: Id) => void;
  onMove: (dragged: Id, drop: Drop) => void;
  onDropFiles: (files: File[], drop: Drop) => void;
}) {
  const { t } = useI18n();
  const [drop, setDrop] = useState<Drop | null>(null);
  const dragged = useRef<Id | null>(null);
  const exists = (id: Id) => findCue(allCues, id) !== undefined;

  const accepts = (event: DragEvent) =>
    editable && (event.dataTransfer.types.includes(DRAG_TYPE) || event.dataTransfer.types.includes("Files"));
  const over = (event: DragEvent<HTMLElement>, next: Drop) => {
    if (!accepts(event)) return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer.dropEffect = event.dataTransfer.types.includes("Files") ? "copy" : "move";
    if (JSON.stringify(drop) !== JSON.stringify(next)) setDrop(next);
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

  return (
    <div
      className="cue-cart"
      onDragOver={(event) => over(event, { where: "end" })}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDrop(null);
      }}
      onDrop={(event) => finish(event, { where: "end" })}
    >
      {cues.length === 0 && <p className="empty">{editable ? t("list.empty") : t("list.emptyShow")}</p>}
      {cues.map((cue) => {
        const run = running.get(cue.id);
        const problem = problemOf(cue, exists);
        const progress = run ? (run.length ? Math.min(1, run.elapsed / run.length) : 1) : 0;
        const remaining = run?.length ? Math.max(0, run.length - run.elapsed) : null;
        const dropHere = drop && drop.where !== "end" && drop.anchor === cue.id ? `drop-${drop.where}` : null;
        const classes = [
          "cart-tile",
          selection.has(cue.id) && "selected",
          run && (run.paused ? "paused" : run.phase === "pre_wait" ? "waiting" : "running"),
          cue.armed === false && "disarmed",
          cue.color && `color-${cue.color}`,
          dropHere,
        ]
          .filter(Boolean)
          .join(" ");
        return (
          <button
            key={cue.id}
            className={classes}
            style={{ "--progress": `${progress * 100}%` } as CSSProperties}
            draggable={editable}
            title={cue.notes || typeLabel(cue.type)}
            onClick={(event) => {
              if (editable) onSelect(cue.id, event.shiftKey ? "range" : event.ctrlKey || event.metaKey ? "toggle" : "single");
              else onToggle(cue.id);
            }}
            onDoubleClick={() => editable && onToggle(cue.id)}
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
            <span className="cart-head">
              <span className="cart-number">{cue.number ?? ""}</span>
              {cue.triggers?.hotkey && <kbd className="hotkey-badge">{cue.triggers.hotkey}</kbd>}
              <span className="cart-mark">
                {problem && problem !== "not_supported" ? (
                  <span className="problem" title={t(`problem.${problem}`)}>
                    <Icon name="warning" size={13} />
                  </span>
                ) : loaded.has(cue.id) ? (
                  <span className="cart-loaded">
                    <Icon name="loaded" size={11} />
                  </span>
                ) : (
                  <CueIcon type={cue.type} size={14} />
                )}
              </span>
            </span>
            <span className="cart-name">{displayName(cue)}</span>
            <span className="cart-time">{run ? (remaining !== null ? `−${formatTime(remaining)}` : formatTime(run.elapsed)) : ""}</span>
          </button>
        );
      })}
    </div>
  );
}
