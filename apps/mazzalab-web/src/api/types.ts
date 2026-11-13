// JSON shapes produced by mazzalab-server. Times are in seconds. Fields
// with default values are omitted by the server, hence the optionals.

export type Id = string;

export type ContinueMode = "do_not_continue" | "auto_continue" | "auto_follow";
export type CueColor = "red" | "orange" | "yellow" | "green" | "blue" | "purple" | "grey";
export type FadeCurve = "linear" | "s_curve";
export type GroupMode =
  | "timeline"
  | "start_first_and_enter"
  | "start_first"
  | "start_random"
  | "playlist";

interface CueBase {
  id: Id;
  number?: string;
  name?: string;
  notes?: string;
  color?: CueColor;
  pre_wait?: number;
  post_wait?: number;
  continue_mode?: ContinueMode;
  armed?: boolean;
  skip_if_disarmed?: boolean;
  flagged?: boolean;
  target?: Id;
  triggers?: Triggers;
}

export type MidiTriggerKind = "note_on" | "note_off" | "control_change" | "program_change";

/** An incoming MIDI message to react to. */
export interface MidiTrigger {
  kind: MidiTriggerKind;
  /** 1–16; absent listens on every channel. */
  channel?: number;
  /** The note, controller or program number. */
  number: number;
  /** The velocity or value that has to match; absent accepts any. */
  value?: number;
}

/** A channel message that arrived on one of the server's MIDI inputs. */
export interface MidiEvent {
  kind: MidiTriggerKind;
  channel: number;
  number: number;
  value: number;
}

/** MIDI messages that work the show's transport. */
export interface MidiControls {
  go?: MidiTrigger;
  stop_all?: MidiTrigger;
  panic?: MidiTrigger;
  playhead_next?: MidiTrigger;
  playhead_previous?: MidiTrigger;
}

/** Ways of starting a cue other than GO and other cues. */
export interface Triggers {
  /** A key or combination such as `F5`, `Q` or `Ctrl+Shift+1`. */
  hotkey?: string;
  /** A MIDI message that starts the cue when it arrives on any input. */
  midi?: MidiTrigger;
}

/** Crosspoint levels in dB, `matrix[file channel][output]`; null is silent. */
export type Matrix = (number | null)[][];

export interface AudioFields {
  file?: string;
  start_time?: number;
  end_time?: number;
  play_count?: number;
  infinite_loop?: boolean;
  rate?: number;
  level_db?: number;
  output_patch?: string;
  fade_in?: number;
  fade_out?: number;
  matrix?: Matrix;
  media_duration?: number;
  media_channels?: number;
}

export type MidiMessage =
  | { kind: "note_on" | "note_off"; channel: number; note: number; velocity: number }
  | { kind: "control_change"; channel: number; controller: number; value: number }
  | { kind: "program_change"; channel: number; program: number }
  | { kind: "msc"; device_id: number; command_format: number; command: number; q_number?: string; q_list?: string }
  | { kind: "sys_ex"; data: number[] };

export type Cue = CueBase &
  (
    | ({ type: "audio" } & AudioFields)
    | { type: "video"; file?: string; stage?: string; layer?: number; opacity?: number; media_duration?: number; hold_at_end?: boolean }
    | { type: "text"; text: string; stage?: string }
    | { type: "mic"; input_patch?: string; output_patch?: string }
    | { type: "camera"; input_patch?: string; stage?: string }
    | { type: "light"; commands?: string[]; duration?: number }
    | { type: "fade"; duration?: number; curve?: FadeCurve; level_db?: number; opacity?: number; stop_target_when_done?: boolean }
    | { type: "network"; patch?: string; message: string; duration?: number }
    | { type: "midi"; patch?: string; message: MidiMessage }
    | { type: "midi_file"; file?: string; rate?: number }
    | { type: "timecode"; start: string; duration?: number }
    | { type: "group"; mode?: GroupMode; children?: Cue[] }
    | { type: "start" | "stop" | "pause" | "reset" | "devamp" | "goto" | "arm" | "disarm" | "memo" }
    | { type: "load"; time?: number }
    | { type: "target"; assigned_number: string }
    | { type: "wait"; duration: number }
    | { type: "script"; language: string; source: string }
  );

export type CueType = Cue["type"];
export type AudioCue = Extract<Cue, { type: "audio" }>;
export type GroupCue = Extract<Cue, { type: "group" }>;

export interface CueList {
  id: Id;
  name: string;
  /** Shown as a grid of buttons instead of a list to GO through. */
  cart?: boolean;
  cues: Cue[];
}

export interface AudioSettings {
  device?: string;
  sample_rate?: number;
  buffer_size?: number;
  master_db?: number;
  /** Device cues are auditioned on; absent is the system default output. */
  audition_device?: string;
}

export type NetworkProtocol = "osc_udp" | "osc_tcp" | "plain_udp" | "plain_tcp";

export interface NetworkPatch {
  name: string;
  host: string;
  port: number;
  protocol?: NetworkProtocol;
}

export interface MidiPatch {
  name: string;
  /** Name of the output port, as the system lists it. */
  port: string;
}

export type DmxProtocol = "art_net" | "sacn";

/** Sends one of the show's DMX universes to the network. */
export interface DmxOutput {
  /** The show's universe, counted from 1. */
  universe: number;
  protocol?: DmxProtocol;
  /** Empty broadcasts (Art-Net) or uses the multicast group (sACN). */
  host?: string;
  /** The universe on the wire: Art-Net counts from 0, sACN from 1. */
  net_universe: number;
}

/** A light in the rig, which light cues call by its name. */
export interface Fixture {
  name: string;
  /** The show's universe, counted from 1; absent is 1. */
  universe?: number;
  /** Its first DMX address, from 1 to 512. */
  address: number;
  /** What each channel does, in order from the first address; absent is a dimmer, `["intensity"]`. */
  parameters?: string[];
}

/** Fixtures (or other groups) that light cues set together by one name. */
export interface FixtureGroup {
  name: string;
  members?: string[];
}

/** A lit DMX slot: universe, address and value (1–255). */
export type DmxLevel = [number, number, number];

export interface WorkspaceSettings {
  double_go_protection: number;
  panic_duration: number;
  audio?: AudioSettings;
  network_patches?: NetworkPatch[];
  midi_patches?: MidiPatch[];
  dmx_outputs?: DmxOutput[];
  fixtures?: Fixture[];
  fixture_groups?: FixtureGroup[];
  midi_controls?: MidiControls;
}

export interface Workspace {
  id: Id;
  name: string;
  settings: WorkspaceSettings;
  cue_lists: CueList[];
}

export interface Snapshot {
  revision: number;
  dirty: boolean;
  /** The unsaved changes come from an autosave of an earlier session. */
  recovered?: boolean;
  can_undo: boolean;
  can_redo: boolean;
  current_list: Id;
  selection: Id | null;
  workspace: Workspace;
}

export interface RunningCue {
  cue: Id;
  phase: "pre_wait" | "action";
  elapsed: number;
  length: number | null;
  paused: boolean;
}

export interface Status {
  now: number;
  panicking: boolean;
  playheads: { list: Id; cue: Id | null }[];
  running: RunningCue[];
  /** Cues loaded to a time (seconds from the start of their pre-wait). */
  loaded?: { cue: Id; time: number }[];
}

/** The audio output the server is playing on. */
export interface AudioStatus {
  running: boolean;
  device_id?: string;
  device_name?: string;
  sample_rate: number;
  channels: number;
  buffer_size?: number;
  /** The show's device is missing and the default one is in use. */
  fallback: boolean;
  error?: string;
  /** The cue being auditioned, if any. */
  audition?: Id;
}

export interface OutputDevice {
  id: string;
  name: string;
  host: string;
  is_default: boolean;
  max_channels: number;
  sample_rates: number[];
  default_sample_rate: number | null;
}

export interface AudioInfo {
  status: AudioStatus | null;
  devices: OutputDevice[];
}

/** The MIDI ports of the machine the server runs on. */
export interface MidiPorts {
  outputs: string[];
  inputs: string[];
}

export interface Waveform {
  duration: number;
  channels: number;
  sample_rate: number;
  /** `[min, max, min, max, ...]`, each from -127 to 127. */
  peaks: number[];
}

export interface Notice {
  level: "info" | "warning" | "error";
  message: string;
}

export type Container = { kind: "list"; id: Id } | { kind: "group"; id: Id };

export type Command =
  | { command: "go"; list?: Id }
  | { command: "start" | "stop" | "pause" | "resume"; cue: Id }
  | { command: "load"; cue: Id; time?: number }
  | { command: "stop_all" | "pause_all" | "resume_all" | "panic" | "reset" | "undo" | "redo" }
  | { command: "set_playhead"; list?: Id; cue: Id | null }
  | { command: "playhead_next" | "playhead_previous"; list?: Id }
  | { command: "update_cue"; cue: Cue }
  | { command: "insert_cue"; container: Container; index?: number; cue: Cue }
  | { command: "delete_cue"; cue: Id }
  | { command: "move_cue"; cue: Id; container: Container; index?: number }
  | { command: "add_cue_list"; name: string; cart?: boolean }
  | { command: "rename_cue_list"; list: Id; name: string }
  | { command: "delete_cue_list"; list: Id }
  | { command: "rename_workspace"; name: string }
  | { command: "update_settings"; settings: WorkspaceSettings }
  | { command: "set_numbers"; numbers: [Id, string | null][] }
  | { command: "batch"; commands: Command[] };

export type EngineEvent = { at: number } & (
  | { kind: "triggered" | "skipped" | "action_completed" | "stopped" | "paused" | "resumed" | "unloaded" | "devamped"; cue: Id }
  | { kind: "loaded"; cue: Id; time: number }
  | { kind: "action_started"; cue: Id; armed: boolean }
  | { kind: "panicking"; cue: Id; fade: number }
  | { kind: "playhead_moved"; list: Id; cue: Id | null }
);

export type ServerMessage =
  | { type: "snapshot"; snapshot: Snapshot }
  | { type: "status"; status: Status }
  | { type: "events"; events: EngineEvent[] }
  | { type: "meters"; peaks: number[] }
  | { type: "audio"; audio: AudioStatus }
  | { type: "notice"; notice: Notice }
  | { type: "dmx"; levels: DmxLevel[] }
  | { type: "midi_in"; event: MidiEvent }
  | { type: "result"; id: number; error?: string };
