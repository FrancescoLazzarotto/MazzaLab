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
}

export interface AudioFields {
  file?: string;
  start_time?: number;
  end_time?: number;
  play_count?: number;
  infinite_loop?: boolean;
  rate?: number;
  level_db?: number;
  output_patch?: string;
  media_duration?: number;
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

export interface CueList {
  id: Id;
  name: string;
  cues: Cue[];
}

export interface WorkspaceSettings {
  double_go_protection: number;
  panic_duration: number;
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
}

export type Container = { kind: "list"; id: Id } | { kind: "group"; id: Id };

export type Command =
  | { command: "go"; list?: Id }
  | { command: "start" | "stop" | "pause" | "resume"; cue: Id }
  | { command: "stop_all" | "pause_all" | "resume_all" | "panic" | "reset" | "undo" | "redo" }
  | { command: "set_playhead"; list?: Id; cue: Id | null }
  | { command: "playhead_next" | "playhead_previous"; list?: Id }
  | { command: "update_cue"; cue: Cue }
  | { command: "insert_cue"; container: Container; index?: number; cue: Cue }
  | { command: "delete_cue"; cue: Id }
  | { command: "move_cue"; cue: Id; container: Container; index?: number }
  | { command: "add_cue_list"; name: string }
  | { command: "rename_cue_list"; list: Id; name: string }
  | { command: "delete_cue_list"; list: Id }
  | { command: "rename_workspace"; name: string }
  | { command: "update_settings"; settings: WorkspaceSettings };

export type EngineEvent = { at: number } & (
  | { kind: "triggered" | "skipped" | "action_completed" | "stopped" | "paused" | "resumed"; cue: Id }
  | { kind: "action_started"; cue: Id; armed: boolean }
  | { kind: "panicking"; cue: Id; fade: number }
  | { kind: "playhead_moved"; list: Id; cue: Id | null }
);

export type ServerMessage =
  | { type: "snapshot"; snapshot: Snapshot }
  | { type: "status"; status: Status }
  | { type: "events"; events: EngineEvent[] }
  | { type: "result"; id: number; error?: string };
