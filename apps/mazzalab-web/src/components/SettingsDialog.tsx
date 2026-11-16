import { useEffect, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type {
  AudioInfo,
  AudioSettings,
  DmxOutput,
  DmxProtocol,
  InputDevice,
  LtcInput,
  MidiControls,
  MidiPatch,
  NetworkPatch,
  NetworkProtocol,
  Stage,
  Workspace,
  WorkspaceSettings,
} from "../api/types";
import { LANGUAGE_OPTIONS, useI18n } from "../i18n";
import { AUTO } from "../locales/resolve";
import type { Preferences } from "../preferences";
import { TRANSPORT_ACTIONS, type KeyMap } from "../keymap";
import { parseDeviceId } from "../mscDevice";
import { THEMES } from "../theme";
import { CheckField, LevelField, NumberField, SelectField, TextField, TimeField } from "./fields";
import { screenLabels, screensInOrder, stageUrl } from "../stage";
import { useScreens } from "../useScreens";
import { FixturesTab } from "./FixturesTab";
import { Icon } from "./icons";
import { ACTION_LABELS, KeyMapFields } from "./KeyMapFields";
import { MidiTriggerField } from "./MidiTriggerField";
import { PasscodesTab } from "./PasscodesTab";

const BUFFER_SIZES = [64, 128, 256, 512, 1024, 2048];
const PROTOCOLS: NetworkProtocol[] = ["osc_udp", "osc_tcp", "plain_udp", "plain_tcp"];
type Tab = "general" | "audio" | "network" | "midi" | "lights" | "fixtures" | "video" | "passcodes" | "keys";

/** Without the right to edit, only what belongs to this device. */
const VIEWER_TABS: Tab[] = ["general", "keys"];
const EDITOR_TABS: Tab[] = ["general", "audio", "network", "midi", "lights", "fixtures", "video", "passcodes", "keys"];

const SHORTCUTS: [string, string][] = [
  ["↑ ↓", "keys.select"],
  ["Shift + ↑ ↓", "keys.extend"],
  ["Enter", "keys.playhead"],
  ["V", "keys.preview"],
  ["Ctrl + A", "keys.selectAll"],
  ["Ctrl + C / X / V", "keys.clipboard"],
  ["Ctrl + D", "keys.duplicate"],
  ["Ctrl + G", "keys.group"],
  ["Delete", "keys.delete"],
  ["Ctrl + Z / Ctrl + Shift + Z", "keys.undo"],
  ["Ctrl + S", "keys.save"],
  ["Ctrl + F", "filter.search"],
];

/** The transport keys as this device has them, for the shortcut list. */
function transportRows(keymap: KeyMap): [string, string][] {
  return TRANSPORT_ACTIONS.flatMap((action): [string, string][] => {
    const key = keymap[action];
    return key ? [[key.replace(/\+/g, " + "), ACTION_LABELS[action]]] : [];
  });
}

export function SettingsDialog({
  connection,
  workspace,
  preferences,
  canEdit,
  onPreferences,
  onSettings,
  onRename,
  onClose,
}: {
  connection: ShowConnection;
  workspace: Workspace;
  preferences: Preferences;
  /** The passcode may change the show. */
  canEdit: boolean;
  onPreferences: (preferences: Preferences) => void;
  onSettings: (settings: WorkspaceSettings) => void;
  onRename: (name: string) => void;
  onClose: () => void;
}) {
  const { t, choice, setChoice } = useI18n();
  const [tab, setTab] = useState<Tab>("general");
  const settings = workspace.settings;
  const set = (patch: Partial<WorkspaceSettings>) => onSettings({ ...settings, ...patch });

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // Escape is a key to bind, too, while a key is being captured.
      if (event.target instanceof Element && event.target.closest(".capturing")) return;
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    // Capture, so that Esc closes the dialog instead of panicking the show.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className={tab === "fixtures" ? "dialog wide" : "dialog"} role="dialog" aria-modal aria-label={t("settings.title")}>
        <header>
          <h1>{t("settings.title")}</h1>
          <nav className="inspector-tabs">
            {(canEdit ? EDITOR_TABS : VIEWER_TABS).map((name) => (
              <button key={name} className={name === tab ? "active" : undefined} onClick={() => setTab(name)}>
                {t(`settings.${name}`)}
              </button>
            ))}
          </nav>
          <button className="icon-button quiet" onClick={onClose} aria-label={t("action.close")}>
            <Icon name="close" size={14} />
          </button>
        </header>
        <div className="dialog-body">
          {tab === "general" && (
            <div className="form-column">
              <fieldset className="bare" disabled={!canEdit}>
              <TextField label={t("settings.showName")} value={workspace.name} onCommit={(name) => name.trim() && onRename(name.trim())} />
              <div className="row">
                <TimeField
                  label={t("settings.doubleGo")}
                  value={settings.double_go_protection}
                  onCommit={(value) => set({ double_go_protection: value ?? 0 })}
                />
                <TimeField label={t("settings.panic")} value={settings.panic_duration} onCommit={(value) => set({ panic_duration: value ?? 0 })} />
              </div>
              </fieldset>
              <h3>{t("settings.thisDevice")}</h3>
              <SelectField
                label={t("settings.language")}
                value={choice}
                options={[{ value: AUTO, label: t("settings.languageAuto") }, ...LANGUAGE_OPTIONS.map((option) => ({ value: option.code, label: option.label }))]}
                onChange={setChoice}
              />
              <SelectField
                label={t("settings.theme")}
                value={preferences.theme}
                options={THEMES.map((theme) => ({ value: theme, label: t(`theme.${theme}`) }))}
                onChange={(theme) => onPreferences({ ...preferences, theme })}
              />
              <CheckField
                label={t("settings.follow")}
                checked={preferences.playheadFollowsSelection}
                onChange={(playheadFollowsSelection) => onPreferences({ ...preferences, playheadFollowsSelection })}
              />
              <p className="hint">{t("settings.followHint")}</p>
              <KeyMapFields keymap={preferences.keymap} cues={workspace.cue_lists.flatMap((list) => list.cues)} onChange={(keymap) => onPreferences({ ...preferences, keymap })} />
            </div>
          )}
          {tab === "audio" && (
            <AudioTab
              connection={connection}
              audio={settings.audio ?? {}}
              ltc={settings.ltc_input}
              onChange={(audio) => set({ audio })}
              onLtc={(ltc_input) => set({ ltc_input })}
            />
          )}
          {tab === "network" && <NetworkTab patches={settings.network_patches ?? []} onChange={(network_patches) => set({ network_patches })} />}
          {tab === "midi" && (
            <MidiTab
              connection={connection}
              patches={settings.midi_patches ?? []}
              controls={settings.midi_controls ?? {}}
              onChange={(midi_patches) => set({ midi_patches })}
              onControls={(midi_controls) => set({ midi_controls })}
              mscDevice={settings.msc_device_id}
              onMscDevice={(msc_device_id) => set({ msc_device_id })}
            />
          )}
          {tab === "lights" && <LightsTab outputs={settings.dmx_outputs ?? []} onChange={(dmx_outputs) => set({ dmx_outputs })} />}
          {tab === "video" && <StagesTab connection={connection} stages={settings.stages ?? []} onChange={(stages) => set({ stages })} />}
          {tab === "fixtures" && (
            <FixturesTab
              fixtures={settings.fixtures ?? []}
              groups={settings.fixture_groups ?? []}
              onChange={(fixtures, fixture_groups) => set({ fixtures, fixture_groups })}
            />
          )}
          {tab === "passcodes" && canEdit && <PasscodesTab passcodes={settings.passcodes ?? []} onChange={(passcodes) => set({ passcodes })} />}
          {tab === "keys" && (
            <table className="shortcuts">
              <tbody>
                {[...transportRows(preferences.keymap), ...SHORTCUTS].map(([keys, label]) => (
                  <tr key={keys}>
                    <td>
                      <kbd>{keys}</kbd>
                    </td>
                    <td>{t(label as Parameters<typeof t>[0])}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}

function AudioTab({
  connection,
  audio,
  ltc,
  onChange,
  onLtc,
}: {
  connection: ShowConnection;
  audio: AudioSettings;
  ltc: LtcInput | undefined;
  onChange: (audio: AudioSettings) => void;
  onLtc: (ltc: LtcInput | undefined) => void;
}) {
  const { t } = useI18n();
  const [info, setInfo] = useState<AudioInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Ask again after every change: the status shows what was really opened.
  const signature = JSON.stringify(audio);
  useEffect(() => {
    let current = true;
    const load = () =>
      connection
        .audioInfo()
        .then((next) => current && setInfo(next))
        .catch((reason: Error) => current && setError(reason.message));
    load();
    // Opening a device takes a moment; look again once it has settled.
    const timer = setTimeout(load, 700);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [connection, signature]);

  const devices = info?.devices ?? [];
  const chosen = devices.find((device) => device.id === audio.device) ?? devices.find((device) => device.is_default);
  const rates = chosen?.sample_rates ?? [];
  const status = info?.status;
  const missing = audio.device !== undefined && info !== null && !devices.some((device) => device.id === audio.device);
  // An audition that comes out of the show's own output is heard in the house.
  const auditionId = audio.audition_device ?? devices.find((device) => device.is_default)?.id;
  const sameAsShow = status?.running === true && auditionId !== undefined && auditionId === status.device_id;
  return (
    <div className="form-column">
      <SelectField
        label={t("settings.device")}
        value={audio.device ?? ""}
        options={[
          { value: "", label: t("settings.defaultDevice") },
          ...devices.map((device) => ({
            value: device.id,
            label: `${device.name} — ${device.max_channels} ch${device.is_default ? ` (${t("settings.default")})` : ""}`,
          })),
          ...(missing ? [{ value: audio.device!, label: t("settings.missingDevice") }] : []),
        ]}
        onChange={(device) => onChange({ ...audio, device: device || undefined, sample_rate: undefined })}
      />
      <div className="row">
        <SelectField
          label={t("settings.sampleRate")}
          value={String(audio.sample_rate ?? "")}
          options={[
            { value: "", label: chosen?.default_sample_rate ? `${t("settings.default")} (${chosen.default_sample_rate / 1000} kHz)` : t("settings.default") },
            ...rates.map((rate) => ({ value: String(rate), label: `${rate / 1000} kHz` })),
          ]}
          onChange={(rate) => onChange({ ...audio, sample_rate: rate ? Number(rate) : undefined })}
        />
        <SelectField
          label={t("settings.buffer")}
          value={String(audio.buffer_size ?? "")}
          options={[{ value: "", label: t("settings.default") }, ...BUFFER_SIZES.map((size) => ({ value: String(size), label: `${size}` }))]}
          onChange={(size) => onChange({ ...audio, buffer_size: size ? Number(size) : undefined })}
        />
      </div>
      <LevelField label={t("audio.master")} value={audio.master_db ?? 0} onCommit={(master_db) => onChange({ ...audio, master_db })} />
      <SelectField
        label={t("settings.auditionDevice")}
        value={audio.audition_device ?? ""}
        options={[
          { value: "", label: t("settings.defaultDevice") },
          ...devices.map((device) => ({ value: device.id, label: device.name })),
          ...(audio.audition_device !== undefined && info !== null && !devices.some((device) => device.id === audio.audition_device)
            ? [{ value: audio.audition_device, label: t("settings.missingDevice") }]
            : []),
        ]}
        onChange={(device) => onChange({ ...audio, audition_device: device || undefined })}
      />
      {sameAsShow && <p className="hint warn">{t("settings.auditionSame")}</p>}
      <div className={status?.running && !status.fallback ? "status-box" : "status-box trouble"}>
        {error ? (
          error
        ) : !info ? (
          t("app.connecting")
        ) : !status ? (
          t("audio.none")
        ) : status.running ? (
          <>
            <strong>{status.device_name}</strong>
            <span>
              {(status.sample_rate / 1000).toFixed(1)} kHz · {status.channels} {t("settings.channels")}
              {status.buffer_size
                ? ` · ${status.buffer_size} ${t("settings.frames")} (${((status.buffer_size / status.sample_rate) * 1000).toFixed(1)} ms)`
                : ""}
            </span>
            {status.fallback && <span>{t("audio.fallback")}</span>}
          </>
        ) : (
          (status.error ?? t("audio.none"))
        )}
      </div>
      <p className="hint">{t("settings.audioHint")}</p>
      <LtcInputFields inputs={info?.inputs ?? []} known={info !== null} ltc={ltc} onChange={onLtc} />
    </div>
  );
}

function NetworkTab({ patches, onChange }: { patches: NetworkPatch[]; onChange: (patches: NetworkPatch[]) => void }) {
  const { t } = useI18n();
  const update = (index: number, patch: Partial<NetworkPatch>) =>
    onChange(patches.map((existing, position) => (position === index ? { ...existing, ...patch } : existing)));
  const add = () => {
    let n = patches.length + 1;
    while (patches.some((patch) => patch.name === `Patch ${n}`)) n++;
    onChange([...patches, { name: `Patch ${n}`, host: "127.0.0.1", port: 53000, protocol: "osc_udp" }]);
  };
  return (
    <div className="form-column">
      {patches.length === 0 && <p className="muted">{t("settings.noPatches")}</p>}
      {patches.map((patch, index) => (
        <div className="row patch-row" key={index}>
          <TextField label={t("settings.patchName")} value={patch.name} onCommit={(name) => name.trim() && update(index, { name: name.trim() })} />
          <TextField label={t("settings.host")} value={patch.host} onCommit={(host) => host.trim() && update(index, { host: host.trim() })} />
          <NumberField label={t("settings.port")} value={patch.port} min={1} onCommit={(port) => update(index, { port: Math.min(65535, Math.round(port)) })} />
          <SelectField
            label={t("settings.protocol")}
            value={patch.protocol ?? "osc_udp"}
            options={PROTOCOLS.map((protocol) => ({ value: protocol, label: t(`protocol.${protocol}`) }))}
            onChange={(protocol) => update(index, { protocol })}
          />
          <button className="icon-button quiet" title={t("action.delete")} onClick={() => onChange(patches.filter((_, position) => position !== index))}>
            <Icon name="trash" size={14} />
          </button>
        </div>
      ))}
      <div>
        <button onClick={add}>
          <Icon name="plus" size={12} />
          {t("settings.addPatch")}
        </button>
      </div>
      <p className="hint">{t("settings.networkHint")}</p>
    </div>
  );
}

const MIDI_CONTROLS: (keyof MidiControls)[] = ["go", "stop_all", "panic", "playhead_next", "playhead_previous"];

function MidiTab({
  connection,
  patches,
  controls,
  mscDevice,
  onChange,
  onControls,
  onMscDevice,
}: {
  connection: ShowConnection;
  patches: MidiPatch[];
  controls: MidiControls;
  /** The device ID MIDI Show Control is answered to; none answers to all only. */
  mscDevice: number | undefined;
  onChange: (patches: MidiPatch[]) => void;
  onControls: (controls: MidiControls) => void;
  onMscDevice: (id: number | undefined) => void;
}) {
  const { t } = useI18n();
  const [outputs, setOutputs] = useState<string[] | null>(null);
  useEffect(() => {
    let current = true;
    connection
      .midiPorts()
      .then((ports) => current && setOutputs(ports.outputs))
      .catch(() => current && setOutputs([]));
    return () => {
      current = false;
    };
  }, [connection]);

  const update = (index: number, patch: Partial<MidiPatch>) =>
    onChange(patches.map((existing, position) => (position === index ? { ...existing, ...patch } : existing)));
  const add = () => {
    let n = patches.length + 1;
    while (patches.some((patch) => patch.name === `MIDI ${n}`)) n++;
    onChange([...patches, { name: `MIDI ${n}`, port: outputs?.[0] ?? "" }]);
  };
  return (
    <div className="form-column">
      {patches.length === 0 && <p className="placeholder">{t("settings.noMidiPatches")}</p>}
      {patches.map((patch, index) => (
        <div className="row patch-row" key={index}>
          <TextField label={t("settings.patchName")} value={patch.name} onCommit={(name) => name.trim() && update(index, { name: name.trim() })} />
          <SelectField
            label={t("settings.midiOutput")}
            value={patch.port}
            options={[
              ...(outputs ?? []).map((port) => ({ value: port, label: port })),
              // The port a show was patched to stays visible when unplugged.
              ...(outputs !== null && !outputs.includes(patch.port)
                ? [{ value: patch.port, label: patch.port ? `${patch.port} (${t("settings.unplugged")})` : "—" }]
                : []),
            ]}
            onChange={(port) => update(index, { port })}
          />
          <button className="icon-button quiet" title={t("action.delete")} onClick={() => onChange(patches.filter((_, position) => position !== index))}>
            <Icon name="trash" size={14} />
          </button>
        </div>
      ))}
      <div>
        <button onClick={add}>
          <Icon name="plus" size={12} />
          {t("settings.addPatch")}
        </button>
      </div>
      {outputs !== null && outputs.length === 0 && <p className="hint warn">{t("settings.noMidiOutputs")}</p>}
      <h3>{t("settings.msc")}</h3>
      <div className="row">
        <TextField
          label={t("settings.mscDevice")}
          value={mscDevice === undefined ? "" : String(mscDevice)}
          placeholder={t("settings.mscAll")}
          onCommit={(text) => onMscDevice(parseDeviceId(text))}
        />
      </div>
      <h3>{t("settings.midiControls")}</h3>
      <div className="midi-controls">
        {MIDI_CONTROLS.map((name) => (
          <MidiTriggerField
            key={name}
            connection={connection}
            label={t(`control.${name}`)}
            value={controls[name]}
            onChange={(trigger) => onControls({ ...controls, [name]: trigger })}
          />
        ))}
      </div>
    </div>
  );
}

/** The audio input LTC comes in on, and its channel; off when none is chosen. */
function LtcInputFields({
  inputs,
  known,
  ltc,
  onChange,
}: {
  inputs: InputDevice[];
  /** Whether the inputs have been listed yet. */
  known: boolean;
  ltc: LtcInput | undefined;
  onChange: (ltc: LtcInput | undefined) => void;
}) {
  const { t } = useI18n();
  // "" is off, "default" the system's default input, anything else a device.
  const value = ltc === undefined ? "" : (ltc.device ?? "default");
  const chosen = inputs.find((input) => input.id === ltc?.device) ?? inputs.find((input) => input.is_default);
  const missing = known && ltc?.device !== undefined && !inputs.some((input) => input.id === ltc.device);
  const channels = Math.max(1, chosen?.max_channels ?? 2);
  return (
    <>
      <h3>{t("settings.ltcIn")}</h3>
      <div className="row">
        <SelectField
          label={t("settings.ltcDevice")}
          value={value}
          options={[
            { value: "", label: t("settings.ltcOff") },
            { value: "default", label: t("settings.defaultInput") },
            ...inputs.map((input) => ({ value: input.id, label: `${input.name} — ${input.max_channels} ch` })),
            ...(missing ? [{ value: ltc!.device!, label: t("settings.missingDevice") }] : []),
          ]}
          onChange={(next) =>
            onChange(next === "" ? undefined : { device: next === "default" ? undefined : next, channel: Math.min(ltc?.channel ?? 1, channels) })
          }
        />
        {ltc && (
          <SelectField
            label={t("settings.ltcChannel")}
            value={String(ltc.channel ?? 1)}
            options={Array.from({ length: Math.max(channels, ltc.channel ?? 1) }, (_, index) => ({ value: String(index + 1), label: String(index + 1) }))}
            onChange={(channel) => onChange({ ...ltc, channel: Number(channel) })}
          />
        )}
      </div>
      <p className="hint">{t("settings.ltcHint")}</p>
    </>
  );
}

/** The screens the show puts video and text on. */
function StagesTab({ connection, stages, onChange }: { connection: ShowConnection; stages: Stage[]; onChange: (stages: Stage[]) => void }) {
  const { t } = useI18n();
  const { screens } = useScreens(connection);
  const labels = screenLabels(screens?.screens ?? []);
  const screenOptions = (current: string | undefined) => [
    { value: "", label: t("settings.noScreen") },
    ...screensInOrder(screens?.screens ?? []).map((screen) => ({
      value: screen.name,
      label: `${labels.get(screen.name)}${screen.main ? ` (${t("settings.screenMain")})` : ""}`,
    })),
    // A display that is not plugged in now stays visible.
    ...(current && !labels.has(current) ? [{ value: current, label: `${current} (${t("settings.screenMissing")})` }] : []),
  ];
  // A stage put on a display takes its resolution.
  const placeOn = (index: number, name: string) => {
    const screen = screens?.screens.find((candidate) => candidate.name === name);
    update(index, screen ? { screen: name, width: screen.width, height: screen.height } : { screen: undefined });
  };
  // Without stages the show has one called Main: list it, so that it can be renamed.
  const shown = stages.length > 0 ? stages : [{ name: "Main" }];
  const update = (index: number, change: Partial<Stage>) => onChange(shown.map((stage, position) => (position === index ? { ...stage, ...change } : stage)));
  const add = () => {
    let n = shown.length + 1;
    while (shown.some((stage) => stage.name === `${t("settings.stageName")} ${n}`)) n++;
    onChange([...shown, { name: `${t("settings.stageName")} ${n}` }]);
  };
  return (
    <div className="form-column">
      {shown.map((stage, index) => (
        <div className="row patch-row stage-row" key={index}>
          <TextField label={t("settings.patchName")} value={stage.name} onCommit={(name) => name.trim() && update(index, { name: name.trim() })} />
          <NumberField label={t("settings.stageWidth")} value={stage.width ?? 1920} min={16} onCommit={(width) => update(index, { width: Math.round(width) })} />
          <NumberField label={t("settings.stageHeight")} value={stage.height ?? 1080} min={16} onCommit={(height) => update(index, { height: Math.round(height) })} />
          {screens && <SelectField label={t("settings.screen")} value={stage.screen ?? ""} options={screenOptions(stage.screen)} onChange={(name) => placeOn(index, name)} />}
          <button onClick={() => window.open(stageUrl(stage.name), `stage-${stage.name}`, "popup,width=960,height=540")}>
            <Icon name="external" size={13} />
            {t("stage.open")}
          </button>
          <button
            className="icon-button quiet"
            title={t("action.delete")}
            disabled={shown.length === 1}
            onClick={() => onChange(shown.filter((_, position) => position !== index))}
          >
            <Icon name="trash" size={14} />
          </button>
        </div>
      ))}
      <div>
        <button onClick={add}>
          <Icon name="plus" size={12} />
          {t("settings.addStage")}
        </button>
      </div>
      <p className="hint">{t("settings.stagesHint")}</p>
      {screens && <p className="hint">{t("settings.screensHint")}</p>}
    </div>
  );
}

const DMX_PROTOCOLS: DmxProtocol[] = ["art_net", "sacn"];

function LightsTab({ outputs, onChange }: { outputs: DmxOutput[]; onChange: (outputs: DmxOutput[]) => void }) {
  const { t } = useI18n();
  const update = (index: number, patch: Partial<DmxOutput>) =>
    onChange(outputs.map((existing, position) => (position === index ? { ...existing, ...patch } : existing)));
  const add = () => {
    // The next universe of the show, on the matching Art-Net universe.
    const universe = Math.max(0, ...outputs.map((output) => output.universe)) + 1;
    onChange([...outputs, { universe, protocol: "art_net", net_universe: universe - 1 }]);
  };
  return (
    <div className="form-column">
      {outputs.length === 0 && <p className="placeholder">{t("settings.noDmxOutputs")}</p>}
      {outputs.map((output, index) => {
        const protocol = output.protocol ?? "art_net";
        return (
          <div className="row patch-row dmx-row" key={index}>
            <NumberField label={t("settings.universe")} value={output.universe} min={1} onCommit={(universe) => update(index, { universe: Math.round(universe) })} />
            <SelectField
              label={t("settings.protocol")}
              value={protocol}
              options={DMX_PROTOCOLS.map((name) => ({ value: name, label: t(`dmx.${name}`) }))}
              // Art-Net numbers universes from 0 and sACN from 1: keep pointing at the same one.
              onChange={(next) =>
                update(index, {
                  protocol: next,
                  net_universe: next === protocol ? output.net_universe : Math.max(next === "sacn" ? 1 : 0, output.net_universe + (next === "sacn" ? 1 : -1)),
                })
              }
            />
            <TextField
              label={t("settings.host")}
              value={output.host ?? ""}
              placeholder={protocol === "art_net" ? t("settings.broadcast") : t("settings.multicast")}
              onCommit={(host) => update(index, { host: host.trim() || undefined })}
            />
            <NumberField
              label={t("settings.netUniverse")}
              value={output.net_universe}
              min={protocol === "sacn" ? 1 : 0}
              onCommit={(net_universe) => update(index, { net_universe: Math.round(net_universe) })}
            />
            <button className="icon-button quiet" title={t("action.delete")} onClick={() => onChange(outputs.filter((_, position) => position !== index))}>
              <Icon name="trash" size={14} />
            </button>
          </div>
        );
      })}
      <div>
        <button onClick={add}>
          <Icon name="plus" size={12} />
          {t("settings.addOutput")}
        </button>
      </div>
      <p className="hint">{t("settings.lightsHint")}</p>
    </div>
  );
}
