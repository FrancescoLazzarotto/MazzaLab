import { useEffect, useRef, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type { Cue, WorkspaceSettings } from "../api/types";
import { useI18n } from "../i18n";
import { stageNames } from "../stage";
import { CheckField, NumberField, SelectField, TextField, TimeField } from "./fields";

type VideoCue = Extract<Cue, { type: "video" }>;

/** Where a cue goes: one of the show's stages, keeping one that was removed visible. */
export function StageField({ settings, value, onChange }: { settings: WorkspaceSettings; value: string | undefined; onChange: (stage: string | undefined) => void }) {
  const { t } = useI18n();
  const names = stageNames(settings);
  const current = value ?? names[0];
  return (
    <SelectField
      label={t("inspector.stage")}
      value={current}
      options={[...names.map((name) => ({ value: name, label: name })), ...(names.includes(current) ? [] : [{ value: current, label: `${current} (?)` }])]}
      // The first stage is the default: a cue on it names none.
      onChange={(stage) => onChange(stage === names[0] && value === undefined ? undefined : stage)}
    />
  );
}

/** A Video cue: its file with a still of where it starts, the stage and layer it goes to, and how it plays. */
export function VideoFields({
  connection,
  cue,
  settings,
  set,
}: {
  connection: ShowConnection;
  cue: VideoCue;
  settings: WorkspaceSettings;
  set: (patch: Partial<VideoCue>) => void;
}) {
  const { t } = useI18n();
  const picker = useRef<HTMLInputElement>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const setFile = (file: string | undefined) => set({ file, media_duration: undefined, start_time: undefined, end_time: undefined });

  // The server does not read video files: the browser tells their length.
  const { id, file, media_duration } = cue;
  useEffect(() => {
    if (!file || media_duration !== undefined) return;
    const probe = document.createElement("video");
    probe.preload = "metadata";
    probe.muted = true;
    probe.onloadedmetadata = () => {
      if (Number.isFinite(probe.duration)) set({ media_duration: Math.round(probe.duration * 1000) / 1000 });
      probe.removeAttribute("src");
    };
    probe.src = connection.cueMediaUrl(id);
    return () => {
      probe.onloadedmetadata = null;
      probe.removeAttribute("src");
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connection, id, file, media_duration]);

  return (
    <div className="form-columns">
      <div className="form-column video-still">
        {file ? (
          <video key={`${file}-${cue.start_time ?? 0}`} src={`${connection.cueMediaUrl(id)}#t=${cue.start_time ?? 0}`} muted preload="metadata" />
        ) : (
          <span className="placeholder">{t("waveform.noFile")}</span>
        )}
      </div>
      <div className="form-column grow video-fields">
        <div className="file-field">
          <TextField label={t("inspector.file")} value={file ?? ""} placeholder="media/clip.mp4" onCommit={(next) => setFile(next || undefined)} />
          <button onClick={() => picker.current?.click()} disabled={progress !== null}>
            {progress === null ? t("inspector.chooseFile") : `${Math.round(progress * 100)}%`}
          </button>
          <input
            ref={picker}
            type="file"
            accept="video/*,.mp4,.m4v,.mov,.webm,.mkv"
            hidden
            onChange={(event) => {
              const chosen = event.target.files?.[0];
              event.target.value = "";
              if (!chosen) return;
              setProgress(0);
              connection
                .uploadMedia(chosen, setProgress)
                .then(setFile)
                .catch((error: Error) => connection.toast("error", `${chosen.name}: ${error.message}`))
                .finally(() => setProgress(null));
            }}
          />
        </div>
        <div className="row wrap">
          <StageField settings={settings} value={cue.stage} onChange={(stage) => set({ stage })} />
          <NumberField label={t("inspector.layer")} value={cue.layer ?? 0} onCommit={(layer) => set({ layer: Math.round(layer) })} />
          <NumberField
            label={t("inspector.opacity")}
            value={Math.round((cue.opacity ?? 1) * 100)}
            min={0}
            onCommit={(percent) => set({ opacity: Math.min(100, percent) / 100 })}
          />
          <TimeField label={t("inspector.startTime")} value={cue.start_time ?? 0} onCommit={(start_time) => set({ start_time })} />
          <TimeField label={t("inspector.endTime")} value={cue.end_time} optional onCommit={(end_time) => set({ end_time })} />
          <NumberField label={t("inspector.rate")} value={cue.rate ?? 1} step={0.05} min={0.05} onCommit={(rate) => set({ rate })} />
        </div>
        <div className="row checks">
          <CheckField label={t("inspector.loop")} checked={cue.infinite_loop ?? false} onChange={(infinite_loop) => set({ infinite_loop })} />
          <CheckField label={t("inspector.holdAtEnd")} checked={cue.hold_at_end ?? false} onChange={(hold_at_end) => set({ hold_at_end })} />
        </div>
      </div>
    </div>
  );
}
