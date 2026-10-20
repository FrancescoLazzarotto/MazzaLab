import type { ContinueMode, Cue, CueColor, FadeCurve, GroupMode } from "../api/types";
import { displayName, flatten, needsTarget, typeLabel } from "../cues";
import { useI18n } from "../i18n";
import { CheckField, NumberField, SelectField, TextField, TimeField } from "./fields";

const COLORS: CueColor[] = ["red", "orange", "yellow", "green", "blue", "purple", "grey"];
const CONTINUE_MODES: ContinueMode[] = ["do_not_continue", "auto_continue", "auto_follow"];
const GROUP_MODES: GroupMode[] = ["timeline", "start_first_and_enter", "start_first", "start_random", "playlist"];
const CURVES: FadeCurve[] = ["linear", "s_curve"];

export function Inspector({
  cue,
  allCues,
  onChange,
}: {
  cue: Cue | undefined;
  allCues: Cue[];
  onChange: (cue: Cue) => void;
}) {
  const { t } = useI18n();
  if (!cue) {
    return (
      <section className="panel inspector">
        <h2>{t("inspector.title")}</h2>
        <p className="muted">{t("inspector.none")}</p>
      </section>
    );
  }
  // Every edit sends the whole cue; the server keeps group children.
  const set = (patch: Record<string, unknown>) => onChange({ ...cue, ...patch } as Cue);

  const targets = flatten(allCues).filter((row) => row.cue.id !== cue.id);

  return (
    <section className="panel inspector">
      <h2>
        {t("inspector.title")} <span className="muted">· {typeLabel(cue.type)}</span>
      </h2>

      <fieldset>
        <legend>{t("inspector.basics")}</legend>
        <div className="row">
          <TextField label={t("inspector.number")} value={cue.number ?? ""} onCommit={(number) => set({ number: number || undefined })} />
          <SelectField
            label={t("inspector.color")}
            value={cue.color ?? ""}
            options={[{ value: "", label: t("inspector.noColor") }, ...COLORS.map((color) => ({ value: color, label: color }))]}
            onChange={(color) => set({ color: color || undefined })}
          />
        </div>
        <TextField label={t("inspector.name")} value={cue.name ?? ""} placeholder={typeLabel(cue.type)} onCommit={(name) => set({ name })} />
        <TextField label={t("inspector.notes")} value={cue.notes ?? ""} multiline onCommit={(notes) => set({ notes })} />
        {needsTarget(cue.type) && (
          <SelectField
            label={t("inspector.target")}
            value={cue.target ?? ""}
            options={[
              { value: "", label: t("inspector.noTarget") },
              ...targets.map((row) => ({
                value: row.cue.id,
                label: `${row.cue.number ?? "-"} · ${displayName(row.cue)}`,
              })),
            ]}
            onChange={(target) => set({ target: target || undefined })}
          />
        )}
      </fieldset>

      <fieldset>
        <legend>{t("inspector.timing")}</legend>
        <div className="row">
          <TimeField label={t("inspector.preWait")} value={cue.pre_wait ?? 0} onCommit={(pre_wait) => set({ pre_wait })} />
          <TimeField label={t("inspector.postWait")} value={cue.post_wait ?? 0} onCommit={(post_wait) => set({ post_wait })} />
        </div>
        <SelectField
          label={t("inspector.continue")}
          value={cue.continue_mode ?? "do_not_continue"}
          options={CONTINUE_MODES.map((mode) => ({ value: mode, label: t(`continue.${mode}`) }))}
          onChange={(continue_mode) => set({ continue_mode })}
        />
        <div className="row">
          <CheckField label={t("inspector.armed")} checked={cue.armed ?? true} onChange={(armed) => set({ armed })} />
          <CheckField label={t("inspector.flagged")} checked={cue.flagged ?? false} onChange={(flagged) => set({ flagged })} />
        </div>
      </fieldset>

      <TypeFields cue={cue} set={set} />
    </section>
  );
}

function TypeFields({ cue, set }: { cue: Cue; set: (patch: Record<string, unknown>) => void }) {
  const { t } = useI18n();
  switch (cue.type) {
    case "audio":
      return (
        <fieldset>
          <legend>{typeLabel(cue.type)}</legend>
          <TextField label={t("inspector.file")} value={cue.file ?? ""} placeholder="audio/file.wav" onCommit={(file) => set({ file: file || undefined, media_duration: undefined })} />
          <div className="row">
            <NumberField label={t("inspector.level")} value={cue.level_db ?? 0} step={0.5} onCommit={(level_db) => set({ level_db })} />
            <NumberField label={t("inspector.rate")} value={cue.rate ?? 1} step={0.05} min={0.05} onCommit={(rate) => set({ rate })} />
          </div>
          <div className="row">
            <TimeField label={t("inspector.startTime")} value={cue.start_time ?? 0} onCommit={(start_time) => set({ start_time })} />
            <TimeField label={t("inspector.endTime")} value={cue.end_time} optional onCommit={(end_time) => set({ end_time })} />
          </div>
          <div className="row">
            <NumberField label={t("inspector.playCount")} value={cue.play_count ?? 1} min={1} onCommit={(play_count) => set({ play_count: Math.round(play_count) })} />
            <CheckField label={t("inspector.loop")} checked={cue.infinite_loop ?? false} onChange={(infinite_loop) => set({ infinite_loop })} />
          </div>
        </fieldset>
      );
    case "video":
      return (
        <fieldset>
          <legend>{typeLabel(cue.type)}</legend>
          <TextField label={t("inspector.file")} value={cue.file ?? ""} onCommit={(file) => set({ file: file || undefined })} />
          <NumberField label={t("inspector.opacity")} value={cue.opacity ?? 1} step={0.05} min={0} onCommit={(opacity) => set({ opacity: Math.min(1, opacity) })} />
        </fieldset>
      );
    case "wait":
      return (
        <fieldset>
          <legend>{typeLabel(cue.type)}</legend>
          <TimeField label={t("inspector.duration")} value={cue.duration} onCommit={(duration) => set({ duration })} />
        </fieldset>
      );
    case "fade":
      return (
        <fieldset>
          <legend>{typeLabel(cue.type)}</legend>
          <div className="row">
            <TimeField label={t("inspector.duration")} value={cue.duration ?? 0} onCommit={(duration) => set({ duration })} />
            <NumberField label={t("inspector.level")} value={cue.level_db ?? 0} step={1} onCommit={(level_db) => set({ level_db })} />
          </div>
          <SelectField
            label={t("inspector.curve")}
            value={cue.curve ?? "s_curve"}
            options={CURVES.map((curve) => ({ value: curve, label: t(`curve.${curve}`) }))}
            onChange={(curve) => set({ curve })}
          />
          <CheckField label={t("inspector.stopTarget")} checked={cue.stop_target_when_done ?? false} onChange={(stop_target_when_done) => set({ stop_target_when_done })} />
        </fieldset>
      );
    case "light":
      return (
        <fieldset>
          <legend>{typeLabel(cue.type)}</legend>
          <TextField
            label={t("inspector.commands")}
            value={(cue.commands ?? []).join("\n")}
            multiline
            onCommit={(text) => set({ commands: text.split("\n").map((line) => line.trim()).filter(Boolean) })}
          />
          <TimeField label={t("inspector.duration")} value={cue.duration ?? 0} onCommit={(duration) => set({ duration })} />
        </fieldset>
      );
    case "network":
      return (
        <fieldset>
          <legend>{typeLabel(cue.type)}</legend>
          <TextField label={t("inspector.message")} value={cue.message} placeholder="/address arg1 arg2" onCommit={(message) => set({ message })} />
          <TimeField label={t("inspector.duration")} value={cue.duration ?? 0} onCommit={(duration) => set({ duration })} />
        </fieldset>
      );
    case "text":
      return (
        <fieldset>
          <legend>{typeLabel(cue.type)}</legend>
          <TextField label={t("inspector.text")} value={cue.text} multiline onCommit={(text) => set({ text })} />
        </fieldset>
      );
    case "group":
      return (
        <fieldset>
          <legend>{typeLabel(cue.type)}</legend>
          <SelectField
            label={t("inspector.mode")}
            value={cue.mode ?? "timeline"}
            options={GROUP_MODES.map((mode) => ({ value: mode, label: t(`group.${mode}`) }))}
            onChange={(mode) => set({ mode })}
          />
        </fieldset>
      );
    default:
      return null;
  }
}
