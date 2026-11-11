import type { MidiMessage, MidiPatch } from "../api/types";
import { useI18n } from "../i18n";
import { NumberField, SelectField, TextField } from "./fields";

type Kind = MidiMessage["kind"];
const KINDS: Kind[] = ["note_on", "note_off", "control_change", "program_change", "msc", "sys_ex"];

/** MIDI Show Control command formats and commands worth offering by name. */
const MSC_FORMATS: [number, string][] = [
  [0x01, "Lighting"],
  [0x02, "Moving lights"],
  [0x10, "Sound"],
  [0x20, "Machinery"],
  [0x30, "Video"],
  [0x40, "Projection"],
  [0x50, "Process control"],
  [0x60, "Pyro"],
  [0x7f, "All types"],
];
const MSC_COMMANDS: [number, string][] = [
  [0x01, "GO"],
  [0x02, "STOP"],
  [0x03, "RESUME"],
  [0x04, "TIMED GO"],
  [0x05, "LOAD"],
  [0x06, "SET"],
  [0x07, "FIRE"],
  [0x08, "ALL OFF"],
  [0x09, "RESTORE"],
  [0x0a, "RESET"],
  [0x0b, "GO OFF"],
];

/** A message of the given kind, keeping the channel when both have one. */
export function defaultMessage(kind: Kind, previous?: MidiMessage): MidiMessage {
  const channel = previous && "channel" in previous ? previous.channel : 1;
  switch (kind) {
    case "note_on":
      return { kind, channel, note: 60, velocity: 100 };
    case "note_off":
      return { kind, channel, note: 60, velocity: 0 };
    case "control_change":
      return { kind, channel, controller: 7, value: 127 };
    case "program_change":
      return { kind, channel, program: 0 };
    case "msc":
      return { kind, device_id: 1, command_format: 0x01, command: 0x01 };
    case "sys_ex":
      return { kind, data: [] };
  }
}

/** `F0 7E 7F` or `7e,7f` → bytes; the F0/F7 frame is optional. Null when it is not hex. */
export function parseHex(text: string): number[] | null {
  const tokens = text.split(/[\s,]+/).filter(Boolean);
  const bytes: number[] = [];
  for (const token of tokens) {
    if (!/^[0-9a-fA-F]{1,2}$/.test(token)) return null;
    bytes.push(Number.parseInt(token, 16));
  }
  if (bytes[0] === 0xf0) bytes.shift();
  if (bytes.at(-1) === 0xf7) bytes.pop();
  return bytes.every((byte) => byte < 0x80) ? bytes : null;
}

export function formatHex(bytes: number[]): string {
  return bytes.map((byte) => byte.toString(16).toUpperCase().padStart(2, "0")).join(" ");
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, Math.round(value)));

export function MidiFields({
  patch,
  message,
  patches,
  onChange,
}: {
  patch: string | undefined;
  message: MidiMessage;
  patches: MidiPatch[];
  onChange: (change: { patch?: string | undefined; message?: MidiMessage }) => void;
}) {
  const { t } = useI18n();
  const set = (change: Record<string, unknown>) => onChange({ message: { ...message, ...change } as MidiMessage });
  const byte = (label: string, key: string, value: number, min = 0, max = 127) => (
    <NumberField label={label} value={value} min={min} onCommit={(next) => set({ [key]: clamp(next, min, max) })} />
  );
  return (
    <div className="form-columns">
      <div className="form-column">
        <SelectField
          label={t("inspector.patch")}
          value={patch ?? ""}
          options={[
            { value: "", label: patches.length > 0 ? `${t("inspector.firstPatch")} (${patches[0].name})` : t("inspector.noPatches") },
            ...patches.map((candidate) => ({ value: candidate.name, label: `${candidate.name} — ${candidate.port}` })),
            // A patch that was renamed or removed stays visible.
            ...(patch && !patches.some((candidate) => candidate.name === patch) ? [{ value: patch, label: `${patch} (?)` }] : []),
          ]}
          onChange={(next) => onChange({ patch: next || undefined })}
        />
        <SelectField
          label={t("midi.message")}
          value={message.kind}
          options={KINDS.map((kind) => ({ value: kind, label: t(`midi.${kind}`) }))}
          onChange={(kind) => onChange({ message: defaultMessage(kind, message) })}
        />
      </div>
      <div className="form-column">
        {(message.kind === "note_on" || message.kind === "note_off") && (
          <div className="row">
            {byte(t("midi.channel"), "channel", message.channel, 1, 16)}
            {byte(t("midi.note"), "note", message.note)}
            {byte(t("midi.velocity"), "velocity", message.velocity)}
          </div>
        )}
        {message.kind === "control_change" && (
          <div className="row">
            {byte(t("midi.channel"), "channel", message.channel, 1, 16)}
            {byte(t("midi.controller"), "controller", message.controller)}
            {byte(t("midi.value"), "value", message.value)}
          </div>
        )}
        {message.kind === "program_change" && (
          <div className="row">
            {byte(t("midi.channel"), "channel", message.channel, 1, 16)}
            {byte(t("midi.program"), "program", message.program)}
          </div>
        )}
        {message.kind === "msc" && (
          <>
            <div className="row">
              {byte(t("midi.deviceId"), "device_id", message.device_id)}
              <SelectField
                label={t("midi.commandFormat")}
                value={String(message.command_format)}
                options={[
                  ...MSC_FORMATS.map(([value, label]) => ({ value: String(value), label })),
                  ...(MSC_FORMATS.some(([value]) => value === message.command_format)
                    ? []
                    : [{ value: String(message.command_format), label: `0x${message.command_format.toString(16).toUpperCase()}` }]),
                ]}
                onChange={(value) => set({ command_format: Number(value) })}
              />
            </div>
            <div className="row">
              <SelectField
                label={t("midi.command")}
                value={String(message.command)}
                options={[
                  ...MSC_COMMANDS.map(([value, label]) => ({ value: String(value), label })),
                  ...(MSC_COMMANDS.some(([value]) => value === message.command)
                    ? []
                    : [{ value: String(message.command), label: `0x${message.command.toString(16).toUpperCase()}` }]),
                ]}
                onChange={(value) => set({ command: Number(value) })}
              />
              <TextField label={t("midi.cueNumber")} value={message.q_number ?? ""} onCommit={(q_number) => set({ q_number: q_number.trim() || undefined })} />
              <TextField label={t("midi.cueList")} value={message.q_list ?? ""} onCommit={(q_list) => set({ q_list: q_list.trim() || undefined })} />
            </div>
          </>
        )}
        {message.kind === "sys_ex" && (
          <TextField
            label={t("midi.sysex")}
            value={formatHex(message.data)}
            placeholder="7E 7F 06 01"
            onCommit={(text) => {
              const data = parseHex(text);
              if (data) set({ data });
            }}
          />
        )}
      </div>
    </div>
  );
}
