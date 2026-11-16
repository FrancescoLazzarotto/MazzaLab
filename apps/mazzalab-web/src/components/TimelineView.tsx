import { useMemo, useRef, useState } from "react";
import type { Cue, Id } from "../api/types";
import { displayName } from "../cues";
import { formatTime } from "../format";
import { useI18n } from "../i18n";
import { barBox, dragPreWait, layoutTimeline, tickStep } from "../timelineLayout";
import { CueIcon } from "./icons";

/**
 * The children of a timeline group as bars along a time scale: each starts
 * at its pre-wait and lasts as long as its action. Dragging a bar sideways
 * changes the pre-wait.
 */
export function TimelineView({ children, editable, onChange }: { children: Cue[]; editable: boolean; onChange: (cue: Cue) => void }) {
  const { t } = useI18n();
  const layout = useMemo(() => layoutTimeline(children), [children]);
  const area = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<{ id: Id; began: number; x: number; value: number } | null>(null);
  const { total } = layout;

  const width = () => area.current?.getBoundingClientRect().width ?? 0;
  const commit = (cue: Cue, value: number) => {
    if (value !== (cue.pre_wait ?? 0)) onChange({ ...cue, pre_wait: value || undefined } as Cue);
  };

  const step = tickStep(total, area.current?.clientWidth ?? 480);
  const ticks: number[] = [];
  for (let time = 0; time <= total; time += step) ticks.push(time);

  if (children.length === 0) return null;
  return (
    <div className="timeline" aria-label={t("timeline.title")}>
      <div className="timeline-row timeline-ruler">
        <span />
        <div className="timeline-area" ref={area}>
          {ticks.map((time) => (
            <span key={time} className="timeline-tick" style={{ left: `${(time / total) * 100}%` }}>
              {formatTime(time).replace(/\.00$/, "")}
            </span>
          ))}
        </div>
      </div>
      {children.map((cue, index) => {
        const bar = layout.bars[index];
        const shown = drag?.id === cue.id ? { ...bar, start: drag.value } : bar;
        const box = barBox(shown, total);
        return (
          <div className="timeline-row" key={cue.id}>
            <span className="timeline-label" title={displayName(cue)}>
              <CueIcon type={cue.type} size={13} />
              <span className="timeline-number">{cue.number ?? "–"}</span>
              {displayName(cue)}
            </span>
            <div className="timeline-area">
              {ticks.map((time) => (
                <i key={time} className="timeline-grid" style={{ left: `${(time / total) * 100}%` }} />
              ))}
              <div
                className={`timeline-bar${bar.length === null ? " open" : ""}${drag?.id === cue.id ? " dragging" : ""}`}
                style={{ left: `${box.left * 100}%`, width: `${box.width * 100}%` }}
                role="slider"
                tabIndex={0}
                aria-label={`${displayName(cue)}: ${t("list.preWait")}`}
                aria-valuemin={0}
                aria-valuemax={total}
                aria-valuenow={shown.start}
                aria-valuetext={formatTime(shown.start)}
                aria-disabled={!editable}
                title={`${t("list.preWait")} ${formatTime(shown.start)}`}
                onPointerDown={(event) => {
                  if (!editable) return;
                  event.currentTarget.setPointerCapture(event.pointerId);
                  setDrag({ id: cue.id, began: bar.start, x: event.clientX, value: bar.start });
                }}
                onPointerMove={(event) => {
                  if (drag?.id !== cue.id) return;
                  setDrag({ ...drag, value: dragPreWait(drag.began, event.clientX - drag.x, total, width()) });
                }}
                onPointerUp={() => {
                  if (drag?.id === cue.id) commit(cue, drag.value);
                  setDrag(null);
                }}
                onPointerCancel={() => setDrag(null)}
                onKeyDown={(event) => {
                  if (!editable || (event.key !== "ArrowLeft" && event.key !== "ArrowRight")) return;
                  event.preventDefault();
                  const amount = (event.shiftKey ? 1 : 0.1) * (event.key === "ArrowLeft" ? -1 : 1);
                  commit(cue, Math.max(0, Math.round((bar.start + amount) * 100) / 100));
                }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
