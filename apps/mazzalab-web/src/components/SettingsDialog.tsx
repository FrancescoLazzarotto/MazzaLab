import { useEffect, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type {
  AudioInfo,
  AudioSettings,
  DmxOutput,
  DmxProtocol,
  MidiControls,
  MidiPatch,
  NetworkPatch,
  NetworkProtocol,
  Workspace,
  WorkspaceSettings,
} from "../api/types";
import { useI18n } from "../i18n";
import type { Preferences } from "../preferences";
import { CheckField, LevelField, NumberField, SelectField, TextField, TimeField } from "./fields";
import { Icon } from "./icons";
import { MidiTriggerField } from "./MidiTriggerField";

const BUFFER_SIZES = [64, 128, 256, 512, 1024, 2048];
const PROTOCOLS: NetworkProtocol[] = ["osc_udp", "osc_tcp", "plain_udp", "plain_tcp"];
type Tab = "general" | "audio" | "network" | "midi" | "lights" | "keys";

const SHORTCUTS: [string, string][] = [
  ["Space", "action.go"],
  ["Esc", "keys.panic"],
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
];

export function SettingsDialog({
  connection,
  workspace,
  preferences,
  onPreferences,
  onSettings,
  onRename,
  onClose,
}: {
  connection: ShowConnection;
  workspace: Workspace;
  preferences: Preferences;
  onPreferences: (preferences: Preferences) => void;
  onSettings: (settings: WorkspaceSettings) => void;
  onRename: (name: string) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [tab, setTab] = useState<Tab>("general");
  const settings = workspace.settings;
  const set = (patch: Partial<WorkspaceSettings>) => onSettings({ ...settings, ...patch });

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
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
      <div className="dialog" role="dialog" aria-modal aria-label={t("settings.title")}>
        <header>
          <h1>{t("settings.title")}</h1>
          <nav className="inspector-tabs">
            {(["general", "audio", "network", "midi", "lights", "keys"] as const).map((name) => (
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
              <TextField label={t("settings.showName")} value={workspace.name} onCommit={(name) => name.trim() && onRename(name.trim())} />
              <div className="row">
                <TimeField
                  label={t("settings.doubleGo")}
                  value={settings.double_go_protection}
                  onCommit={(value) => set({ double_go_protection: value ?? 0 })}
                />
                <TimeField label={t("settings.panic")} value={settings.panic_duration} onCommit={(value) => set({ panic_duration: value ?? 0 })} />
              </div>
              <h3>{t("settings.thisDevice")}</h3>
              <CheckField
                label={t("settings.follow")}
                checked={preferences.playheadFollowsSelection}
                onChange={(playheadFollowsSelection) => onPreferences({ ...preferences, playheadFollowsSelection })}
              />
              <p className="hint">{t("settings.followHint")}</p>
            </div>
          )}
          {tab === "audio" && <AudioTab connection={connection} audio={settings.audio ?? {}} onChange={(audio) => set({ audio })} />}
          {tab === "network" && <NetworkTab patches={settings.network_patches ?? []} onChange={(network_patches) => set({ network_patches })} />}
          {tab === "midi" && (
            <MidiTab
              connection={connection}
              patches={settings.midi_patches ?? []}
              controls={settings.midi_controls ?? {}}
              onChange={(midi_patches) => set({ midi_patches })}
              onControls={(midi_controls) => set({ midi_controls })}
            />
          )}
          {tab === "lights" && <LightsTab outputs={settings.dmx_outputs ?? []} onChange={(dmx_outputs) => set({ dmx_outputs })} />}
          {tab === "keys" && (
            <table className="shortcuts">
              <tbody>
                {SHORTCUTS.map(([keys, label]) => (
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
  onChange,
}: {
  connection: ShowConnection;
  audio: AudioSettings;
  onChange: (audio: AudioSettings) => void;
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
  onChange,
  onControls,
}: {
  connection: ShowConnection;
  patches: MidiPatch[];
  controls: MidiControls;
  onChange: (patches: MidiPatch[]) => void;
  onControls: (controls: MidiControls) => void;
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
