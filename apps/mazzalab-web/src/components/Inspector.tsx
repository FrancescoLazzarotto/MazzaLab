import { useRef, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type {
  AudioCue,
  AudioStatus,
  ContinueMode,
  Cue,
  CueColor,
  FadeCurve,
  GroupMode,
  Id,
  RunningCue,
  WorkspaceSettings,
} from "../api/types";
import { displayName, flatten, needsTarget, problemOf, typeLabel } from "../cues";
import { useI18n } from "../i18n";
import { hotkeyOf, isReserved } from "../hotkeys";
import { markers } from "../slices";
import { FORMAT_LABELS, normalizeTimecode, SMPTE_FORMATS } from "../timecode";
import { CheckField, HotkeyField, LevelField, NumberField, SelectField, TextField, TimeField } from "./fields";
import { CueIcon, Icon } from "./icons";
import { LightCommands } from "./LightCommands";
import { SliceList } from "./SliceList";
import { MatrixEditor } from "./MatrixEditor";
import { MidiFields } from "./MidiFields";
import { MidiTriggerField } from "./MidiTriggerField";
import { filePosition, Waveform } from "./Waveform";

const COLORS: CueColor[] = ["red", "orange", "yellow", "green", "blue", "purple", "grey"];
const CONTINUE_MODES: ContinueMode[] = ["do_not_continue", "auto_continue", "auto_follow"];
const GROUP_MODES: GroupMode[] = ["timeline", "start_first_and_enter", "start_first", "start_random", "playlist"];
const CURVES: FadeCurve[] = ["linear", "s_curve"];

type Tab = "basics" | "time" | "levels" | "settings";
type Patch = Record<string, unknown>;

function tabsFor(cue: Cue): Tab[] {
  if (cue.type === "audio") return ["basics", "time", "levels"];
  const plain = ["start", "stop", "pause", "reset", "devamp", "goto", "arm", "disarm", "memo", "mic", "camera", "midi_file", "script"];
  return plain.includes(cue.type) ? ["basics"] : ["basics", "settings"];
}

export function Inspector({
  connection,
  cues,
  allCues,
  settings,
  audio,
  running,
  loaded,
  editable,
  onChange,
  onChangeMany,
  onLoad,
}: {
  connection: ShowConnection;
  /** The selected cues. */
  cues: Cue[];
  allCues: Cue[];
  settings: WorkspaceSettings;
  audio: AudioStatus | null;
  running: Map<Id, RunningCue>;
  /** Cues loaded to a time, in seconds from the start of their pre-wait. */
  loaded: Map<Id, number>;
  editable: boolean;
  onChange: (cue: Cue) => void;
  onChangeMany: (cues: Cue[]) => void;
  onLoad: (cue: Id, time: number) => void;
}) {
  const { t } = useI18n();
  const [tab, setTab] = useState<Tab>("basics");

  if (cues.length === 0) {
    return (
      <section className="inspector empty-inspector">
        <p className="muted">{t("inspector.none")}</p>
      </section>
    );
  }
  if (cues.length > 1) {
    return <ManyInspector cues={cues} editable={editable} onChangeMany={onChangeMany} />;
  }

  const cue = cues[0];
  const tabs = tabsFor(cue);
  const current = tabs.includes(tab) ? tab : "basics";
  // Every edit sends the whole cue; the server keeps group children.
  const set = (patch: Patch) => onChange({ ...cue, ...patch } as Cue);
  const ids = new Set(flatten(allCues).map((row) => row.cue.id));
  const problem = problemOf(cue, (id) => ids.has(id));

  return (
    <section className="inspector">
      <header className="inspector-head">
        <nav className="inspector-tabs" role="tablist">
          {tabs.map((name) => (
            <button key={name} role="tab" aria-selected={name === current} className={name === current ? "active" : undefined} onClick={() => setTab(name)}>
              {name === "settings" ? typeLabel(cue.type) : t(`tab.${name}`)}
            </button>
          ))}
        </nav>
        {problem && (
          <span className={problem === "not_supported" ? "problem soft" : "problem"}>
            <Icon name="warning" size={13} /> {t(`problem.${problem}`)}
          </span>
        )}
        <span className="inspector-title">
          <CueIcon type={cue.type} size={14} />
          <span className="inspector-number">{cue.number ?? "–"}</span>
          <span className="inspector-name">{displayName(cue)}</span>
        </span>
      </header>
      <fieldset className="inspector-body" disabled={!editable}>
        {current === "basics" && <Basics connection={connection} cue={cue} allCues={allCues} set={set} />}
        {current === "time" && cue.type === "audio" && (
          <AudioTime
            connection={connection}
            cue={cue}
            run={running.get(cue.id)}
            loadedAt={loaded.get(cue.id)}
            auditioning={audio?.audition === cue.id}
            editable={editable}
            set={set}
            onLoad={(time) => onLoad(cue.id, time)}
          />
        )}
        {current === "levels" && cue.type === "audio" && <AudioLevels cue={cue} outputs={audio?.channels ?? 2} set={set} />}
        {current === "settings" && <TypeSettings cue={cue} allCues={allCues} settings={settings} set={set} />}
      </fieldset>
    </section>
  );
}

function Basics({
  connection,
  cue,
  allCues,
  set,
}: {
  connection: ShowConnection;
  cue: Cue;
  allCues: Cue[];
  set: (patch: Patch) => void;
}) {
  /** Triggers with a change applied, or nothing at all when none is left. */
  const triggers = (change: Partial<NonNullable<Cue["triggers"]>>) => {
    const next = { ...cue.triggers, ...change };
    return next.hotkey || next.midi || next.timecode ? next : undefined;
  };
  const { t } = useI18n();
  const targets = flatten(allCues).filter((row) => row.cue.id !== cue.id);
  return (
    <div className="form-columns">
      <div className="form-column">
        <div className="row">
          <TextField label={t("inspector.number")} value={cue.number ?? ""} onCommit={(number) => set({ number: number || undefined })} />
          <SelectField
            label={t("inspector.color")}
            value={cue.color ?? ""}
            options={[{ value: "", label: t("inspector.noColor") }, ...COLORS.map((color) => ({ value: color, label: t(`color.${color}`) }))]}
            onChange={(color) => set({ color: color || undefined })}
          />
        </div>
        <TextField label={t("inspector.name")} value={cue.name ?? ""} placeholder={displayName({ ...cue, name: "" } as Cue)} onCommit={(name) => set({ name })} />
        {needsTarget(cue.type) && (
          <SelectField
            label={t("inspector.target")}
            value={cue.target ?? ""}
            options={[
              { value: "", label: t("inspector.noTarget") },
              ...targets.map((row) => ({
                value: row.cue.id,
                label: `${row.cue.number ?? "–"} · ${displayName(row.cue)}`,
              })),
            ]}
            onChange={(target) => set({ target: target || undefined })}
          />
        )}
      </div>
      <div className="form-column">
        <div className="row">
          <TimeField label={t("inspector.preWait")} value={cue.pre_wait ?? 0} onCommit={(pre_wait) => set({ pre_wait })} />
          <TimeField label={t("inspector.postWait")} value={cue.post_wait ?? 0} onCommit={(post_wait) => set({ post_wait })} />
        </div>
        <div className="row">
          <SelectField
            label={t("inspector.continue")}
            value={cue.continue_mode ?? "do_not_continue"}
            options={CONTINUE_MODES.map((mode) => ({ value: mode, label: t(`continue.${mode}`) }))}
            onChange={(continue_mode) => set({ continue_mode })}
          />
          <HotkeyField
            label={t("inspector.hotkey")}
            value={cue.triggers?.hotkey}
            placeholder={t("inspector.hotkeyNone")}
            nameOf={hotkeyOf}
            refuse={(hotkey) => (isReserved(hotkey) ? t("inspector.hotkeyReserved") : null)}
            onCommit={(hotkey) => set({ triggers: triggers({ hotkey }) })}
          />
        </div>
        <MidiTriggerField connection={connection} label={t("inspector.midiTrigger")} value={cue.triggers?.midi} onChange={(midi) => set({ triggers: triggers({ midi }) })} />
        <TextField
          label={t("inspector.timecodeTrigger")}
          value={cue.triggers?.timecode ?? ""}
          placeholder={t("inspector.timecodeNone")}
          onCommit={(text) => {
            const timecode = text.trim() === "" ? undefined : normalizeTimecode(text);
            if (timecode === null) connection.toast("error", t("inspector.badTimecode"));
            else set({ triggers: triggers({ timecode }) });
          }}
        />
        <div className="row checks">
          <CheckField label={t("inspector.armed")} checked={cue.armed ?? true} onChange={(armed) => set({ armed })} />
          <CheckField label={t("inspector.flagged")} checked={cue.flagged ?? false} onChange={(flagged) => set({ flagged })} />
        </div>
      </div>
      <div className="form-column grow">
        <TextField label={t("inspector.notes")} value={cue.notes ?? ""} multiline onCommit={(notes) => set({ notes })} />
      </div>
    </div>
  );
}

function FadeFields({ cue, set }: { cue: Extract<Cue, { type: "fade" }>; set: (patch: Patch) => void }) {
  const { t } = useI18n();
  return (
    <>
      <div className="row">
        <TimeField label={t("inspector.duration")} value={cue.duration ?? 0} onCommit={(duration) => set({ duration })} />
        <SelectField
          label={t("inspector.curve")}
          value={cue.curve ?? "s_curve"}
          options={CURVES.map((curve) => ({ value: curve, label: t(`curve.${curve}`) }))}
          onChange={(curve) => set({ curve })}
        />
      </div>
      <LevelField label={t("inspector.fadeTo")} value={cue.level_db ?? 0} onCommit={(level_db) => set({ level_db })} />
      <CheckField
        label={t("inspector.stopTarget")}
        checked={cue.stop_target_when_done ?? false}
        onChange={(stop_target_when_done) => set({ stop_target_when_done })}
      />
    </>
  );
}

function AudioTime({
  connection,
  cue,
  run,
  loadedAt,
  auditioning,
  editable,
  set,
  onLoad,
}: {
  connection: ShowConnection;
  cue: AudioCue;
  run: RunningCue | undefined;
  loadedAt: number | undefined;
  /** Whether this cue is being played on the audition output right now. */
  auditioning: boolean;
  editable: boolean;
  set: (patch: Patch) => void;
  onLoad: (time: number) => void;
}) {
  const { t } = useI18n();
  const picker = useRef<HTMLInputElement>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const duration = cue.media_duration ?? 0;
  const sliced = markers(cue, duration).length > 0;
  // A new file starts from a clean slate: its length and channels are read
  // again by the server.
  const setFile = (file: string | undefined) =>
    set({ file, media_duration: undefined, media_channels: undefined, start_time: undefined, end_time: undefined });
  const importFile = (file: File) => {
    setProgress(0);
    connection
      .uploadMedia(file, setProgress)
      .then(setFile)
      .catch((error: Error) => connection.toast("error", `${file.name}: ${error.message}`))
      .finally(() => setProgress(null));
  };
  return (
    <div className="audio-time">
      <Waveform connection={connection} cue={cue} run={run} loadedAt={loadedAt} editable={editable} onChange={set} onLoad={onLoad} />
      <div className="row wrap">
        <div className="file-field">
          <TextField label={t("inspector.file")} value={cue.file ?? ""} placeholder="media/file.wav" onCommit={(file) => setFile(file || undefined)} />
          <button onClick={() => picker.current?.click()} disabled={progress !== null}>
            {progress === null ? t("inspector.chooseFile") : `${Math.round(progress * 100)}%`}
          </button>
          <button
            className={auditioning ? "audition active" : "audition"}
            disabled={!cue.file}
            title={t("inspector.auditionHint")}
            onClick={() => connection.audition(auditioning ? null : cue.id).catch(() => {})}
          >
            <Icon name={auditioning ? "stop" : "audition"} size={auditioning ? 11 : 14} />
            {auditioning ? t("action.stop") : t("inspector.audition")}
          </button>
          <input
            ref={picker}
            type="file"
            accept="audio/*,.wav,.aif,.aiff,.flac,.mp3,.m4a,.ogg,.opus"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (file) importFile(file);
              event.target.value = "";
            }}
          />
        </div>
        <TimeField label={t("inspector.startTime")} value={cue.start_time ?? 0} onCommit={(start_time) => set({ start_time })} />
        <TimeField label={t("inspector.endTime")} value={cue.end_time} optional onCommit={(end_time) => set({ end_time })} />
        <TimeField label={t("inspector.fadeIn")} value={cue.fade_in ?? 0} onCommit={(fade_in) => set({ fade_in })} />
        <TimeField label={t("inspector.fadeOut")} value={cue.fade_out ?? 0} onCommit={(fade_out) => set({ fade_out })} />
        {/* With slices, each slice has its own count instead. */}
        {!sliced && (
          <NumberField label={t("inspector.playCount")} value={cue.play_count ?? 1} min={1} onCommit={(play_count) => set({ play_count: Math.round(play_count) })} />
        )}
        <NumberField label={t("inspector.rate")} value={cue.rate ?? 1} step={0.05} min={0.05} onCommit={(rate) => set({ rate })} />
        {!sliced && <CheckField label={t("inspector.loop")} checked={cue.infinite_loop ?? false} onChange={(infinite_loop) => set({ infinite_loop })} />}
      </div>
      {cue.file && (
        <SliceList
          cue={cue}
          duration={duration}
          suggested={loadedAt === undefined ? undefined : filePosition(cue, loadedAt, duration)}
          editable={editable}
          onChange={set}
        />
      )}
    </div>
  );
}

function AudioLevels({ cue, outputs, set }: { cue: AudioCue; outputs: number; set: (patch: Patch) => void }) {
  const { t } = useI18n();
  return (
    <div className="form-columns">
      <div className="form-column">
        <LevelField label={t("inspector.level")} value={cue.level_db ?? 0} onCommit={(level_db) => set({ level_db })} />
      </div>
      <div className="form-column grow">
        <span className="field-label">{t("inspector.matrix")}</span>
        <MatrixEditor cue={cue} outputs={outputs} onChange={(matrix) => set({ matrix })} />
      </div>
    </div>
  );
}

function TypeSettings({
  cue,
  allCues,
  settings,
  set,
}: {
  cue: Cue;
  allCues: Cue[];
  settings: WorkspaceSettings;
  set: (patch: Patch) => void;
}) {
  const { t } = useI18n();
  const numbered = flatten(allCues)
    .map((row) => row.cue)
    .filter((candidate) => candidate.number);
  switch (cue.type) {
    case "video":
      return (
        <div className="form-columns">
          <div className="form-column">
            <TextField label={t("inspector.file")} value={cue.file ?? ""} onCommit={(file) => set({ file: file || undefined })} />
            <NumberField label={t("inspector.opacity")} value={cue.opacity ?? 1} step={0.05} min={0} onCommit={(opacity) => set({ opacity: Math.min(1, opacity) })} />
          </div>
        </div>
      );
    case "wait":
      return (
        <div className="form-columns">
          <div className="form-column">
            <TimeField label={t("inspector.duration")} value={cue.duration} onCommit={(duration) => set({ duration })} />
          </div>
        </div>
      );
    case "fade":
      return (
        <div className="form-columns">
          <div className="form-column">
            <FadeFields cue={cue} set={set} />
          </div>
        </div>
      );
    case "light":
      return (
        <div className="form-columns">
          <div className="form-column grow">
            <LightCommands
              commands={cue.commands ?? []}
              fixtures={settings.fixtures ?? []}
              groups={settings.fixture_groups ?? []}
              onCommit={(commands) => set({ commands })}
            />
          </div>
          <div className="form-column">
            <TimeField label={t("inspector.duration")} value={cue.duration ?? 0} onCommit={(duration) => set({ duration })} />
          </div>
        </div>
      );
    case "network": {
      const patches = settings.network_patches ?? [];
      return (
        <div className="form-columns">
          <div className="form-column">
            <SelectField
              label={t("inspector.patch")}
              value={cue.patch ?? ""}
              options={[
                { value: "", label: patches.length > 0 ? `${t("inspector.firstPatch")} (${patches[0].name})` : t("inspector.noPatches") },
                ...patches.map((patch) => ({ value: patch.name, label: `${patch.name} — ${patch.host}:${patch.port}` })),
                // A patch that was renamed or removed stays visible.
                ...(cue.patch && !patches.some((patch) => patch.name === cue.patch) ? [{ value: cue.patch, label: `${cue.patch} (?)` }] : []),
              ]}
              onChange={(patch) => set({ patch: patch || undefined })}
            />
          </div>
          <div className="form-column grow">
            <TextField
              label={t("inspector.message")}
              value={cue.message}
              placeholder={"/cue/12/start\n/eos/sub/3 0.75"}
              multiline
              onCommit={(message) => set({ message })}
            />
          </div>
        </div>
      );
    }
    case "timecode": {
      const signal = cue.signal ?? "mtc";
      const patches = settings.midi_patches ?? [];
      return (
        <div className="form-columns">
          <div className="form-column">
            <div className="row">
              <SelectField
                label={t("inspector.signal")}
                value={signal}
                options={[
                  { value: "mtc", label: "MTC" },
                  { value: "ltc", label: "LTC" },
                ]}
                onChange={(next) => set({ signal: next })}
              />
              <SelectField
                label={t("inspector.format")}
                value={cue.format ?? "fps30_non_drop"}
                options={SMPTE_FORMATS.map((format) => ({ value: format, label: `${FORMAT_LABELS[format]} fps` }))}
                onChange={(format) => set({ format })}
              />
            </div>
            <div className="row">
              <TextField
                label={t("inspector.timecodeStart")}
                value={cue.start}
                placeholder="01:00:00:00"
                // Kept even when it does not read: the cue is flagged until it does.
                onCommit={(text) => set({ start: normalizeTimecode(text) ?? text.trim() })}
              />
              <TimeField label={t("inspector.duration")} value={cue.duration} optional onCommit={(duration) => set({ duration })} />
            </div>
          </div>
          <div className="form-column">
            {signal === "mtc" ? (
              <SelectField
                label={t("inspector.patch")}
                value={cue.patch ?? ""}
                options={[
                  { value: "", label: patches.length > 0 ? `${t("inspector.firstPatch")} (${patches[0].name})` : t("inspector.noPatches") },
                  ...patches.map((patch) => ({ value: patch.name, label: `${patch.name} — ${patch.port}` })),
                  ...(cue.patch && !patches.some((patch) => patch.name === cue.patch) ? [{ value: cue.patch, label: `${cue.patch} (?)` }] : []),
                ]}
                onChange={(patch) => set({ patch: patch || undefined })}
              />
            ) : (
              <>
                <NumberField label={t("inspector.ltcOutput")} value={cue.output ?? 1} min={1} onCommit={(output) => set({ output: Math.round(output) })} />
                <LevelField label={t("inspector.level")} value={cue.level_db ?? -18} onCommit={(level_db) => set({ level_db })} />
              </>
            )}
          </div>
        </div>
      );
    }
    case "load":
      return (
        <div className="form-columns">
          <div className="form-column">
            <TimeField label={t("inspector.loadTime")} value={cue.time ?? 0} onCommit={(time) => set({ time: time ?? 0 })} />
          </div>
        </div>
      );
    case "target":
      return (
        <div className="form-columns">
          <div className="form-column">
            <SelectField
              label={t("inspector.newTarget")}
              value={cue.assigned_number}
              options={[
                { value: "", label: t("inspector.noTarget") },
                ...numbered.map((candidate) => ({ value: candidate.number!, label: `${candidate.number} · ${displayName(candidate)}` })),
                // A number that no cue has any more stays visible.
                ...(cue.assigned_number && !numbered.some((candidate) => candidate.number === cue.assigned_number)
                  ? [{ value: cue.assigned_number, label: `${cue.assigned_number} (?)` }]
                  : []),
              ]}
              onChange={(assigned_number) => set({ assigned_number })}
            />
          </div>
        </div>
      );
    case "midi":
      return <MidiFields patch={cue.patch} message={cue.message} patches={settings.midi_patches ?? []} onChange={set} />;
    case "text":
      return (
        <div className="form-columns">
          <div className="form-column grow">
            <TextField label={t("inspector.text")} value={cue.text} multiline onCommit={(text) => set({ text })} />
          </div>
        </div>
      );
    case "group":
      return (
        <div className="form-columns">
          <div className="form-column">
            <SelectField
              label={t("inspector.mode")}
              value={cue.mode ?? "timeline"}
              options={GROUP_MODES.map((mode) => ({ value: mode, label: t(`group.${mode}`) }))}
              onChange={(mode) => set({ mode })}
            />
            <p className="hint">
              {(cue.children ?? []).length} {t("inspector.children")}
            </p>
          </div>
        </div>
      );
    default:
      return null;
  }
}

/** Edits what several selected cues have in common. */
function ManyInspector({ cues, editable, onChangeMany }: { cues: Cue[]; editable: boolean; onChangeMany: (cues: Cue[]) => void }) {
  const { t } = useI18n();
  const set = (patch: Patch) => onChangeMany(cues.map((cue) => ({ ...cue, ...patch }) as Cue));
  /** The value every cue shares, or `undefined` when they differ. */
  const shared = <T,>(read: (cue: Cue) => T): T | undefined => {
    const first = read(cues[0]);
    return cues.every((cue) => read(cue) === first) ? first : undefined;
  };
  const mixed = { value: "__mixed", label: t("inspector.mixed") };
  const color = shared((cue) => cue.color ?? "");
  const mode = shared((cue) => cue.continue_mode ?? "do_not_continue");
  const audio = cues.filter((cue): cue is AudioCue => cue.type === "audio");
  return (
    <section className="inspector">
      <header className="inspector-head">
        <span className="inspector-many">
          {cues.length} {t("inspector.selected")}
        </span>
      </header>
      <fieldset className="inspector-body" disabled={!editable}>
        <div className="form-columns">
          <div className="form-column">
            <SelectField
              label={t("inspector.color")}
              value={color ?? mixed.value}
              options={[
                ...(color === undefined ? [mixed] : []),
                { value: "", label: t("inspector.noColor") },
                ...COLORS.map((name) => ({ value: name as string, label: t(`color.${name}`) })),
              ]}
              onChange={(value) => value !== mixed.value && set({ color: value || undefined })}
            />
            <SelectField
              label={t("inspector.continue")}
              value={mode ?? mixed.value}
              options={[...(mode === undefined ? [mixed] : []), ...CONTINUE_MODES.map((name) => ({ value: name as string, label: t(`continue.${name}`) }))]}
              onChange={(value) => value !== mixed.value && set({ continue_mode: value })}
            />
          </div>
          <div className="form-column">
            <div className="row">
              <TimeField label={t("inspector.preWait")} value={shared((cue) => cue.pre_wait ?? 0)} optional onCommit={(pre_wait) => set({ pre_wait: pre_wait ?? 0 })} />
              <TimeField label={t("inspector.postWait")} value={shared((cue) => cue.post_wait ?? 0)} optional onCommit={(post_wait) => set({ post_wait: post_wait ?? 0 })} />
            </div>
            <div className="row checks">
              <CheckField label={t("inspector.armed")} checked={shared((cue) => cue.armed ?? true) ?? false} onChange={(armed) => set({ armed })} />
              <CheckField label={t("inspector.flagged")} checked={shared((cue) => cue.flagged ?? false) ?? false} onChange={(flagged) => set({ flagged })} />
            </div>
          </div>
          {audio.length === cues.length && (
            <div className="form-column">
              <LevelField
                label={t("inspector.level")}
                value={shared((cue) => (cue.type === "audio" ? (cue.level_db ?? 0) : 0)) ?? 0}
                onCommit={(level_db) => set({ level_db })}
              />
            </div>
          )}
        </div>
      </fieldset>
    </section>
  );
}
