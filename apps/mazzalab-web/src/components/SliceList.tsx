import type { AudioCue, SliceMarker } from "../api/types";
import { formatTime } from "../format";
import { useI18n } from "../i18n";
import { addMarker, markers, newMarkerTime, segments } from "../slices";
import { NumberField } from "./fields";
import { Icon } from "./icons";

/**
 * The slices of an audio cue, in one strip under the waveform: each with the
 * part of the file it covers and how often it plays, or whether it loops
 * until a Devamp moves the cue on.
 */
export function SliceList({
  cue,
  duration,
  suggested,
  editable,
  onChange,
}: {
  cue: AudioCue;
  duration: number;
  /** Where a new slice would go first: where the cue is loaded to. */
  suggested: number | undefined;
  editable: boolean;
  onChange: (patch: { slices?: SliceMarker[]; last_slice_play_count?: number }) => void;
}) {
  const { t } = useI18n();
  const cuts = markers(cue, duration);
  const parts = segments(cue, duration);
  const at = newMarkerTime(cue, duration, suggested);
  const update = (time: number, patch: Partial<SliceMarker>) =>
    onChange({ slices: (cue.slices ?? []).map((marker) => (marker.time === time ? { ...marker, ...patch } : marker)) });
  // A slice's count lives on the marker that ends it; the last one's on the cue.
  const setCount = (index: number, play_count: number) =>
    index < cuts.length ? update(cuts[index].time, { play_count }) : onChange({ last_slice_play_count: play_count });

  return (
    <div className="slices">
      <span className="field-label">{t("inspector.slices")}</span>
      <button className="slice-add" disabled={!editable || at === null} onClick={() => at !== null && onChange({ slices: addMarker(cue, at) })}>
        <Icon name="plus" size={12} />
        {t("inspector.addSlice")}
      </button>
      {cuts.length === 0 ? (
        <span className="hint">{t("inspector.sliceHint")}</span>
      ) : (
        parts.map((part, index) => {
          const looping = part.passes === null;
          return (
            <div className={looping ? "slice looping" : "slice"} key={`${part.from}-${index}`}>
              <span className="slice-index">{index + 1}</span>
              <span className="slice-range">
                {formatTime(part.from)}–{formatTime(part.to)}
              </span>
              {!looping && (
                <NumberField
                  label={t("inspector.playCount")}
                  value={part.passes ?? 1}
                  min={1}
                  onCommit={(count) => setCount(index, Math.max(1, Math.round(count)))}
                />
              )}
              <button
                className={looping ? "slice-loop active" : "slice-loop"}
                aria-pressed={looping}
                title={t("inspector.untilDevamp")}
                disabled={!editable}
                onClick={() => setCount(index, looping ? 1 : 0)}
              >
                ∞
              </button>
              {index < cuts.length && (
                <button
                  className="icon-button quiet"
                  title={t("inspector.removeSlice")}
                  disabled={!editable}
                  onClick={() => onChange({ slices: (cue.slices ?? []).filter((marker) => marker.time !== cuts[index].time) })}
                >
                  <Icon name="close" size={11} />
                </button>
              )}
            </div>
          );
        })
      )}
    </div>
  );
}
