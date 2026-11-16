import { useSyncExternalStore } from "react";
import type {
  AudioInfo,
  AudioStatus,
  Command,
  DmxLevel,
  EngineEvent,
  Id,
  MidiEvent,
  MidiPorts,
  PreflightIssue,
  Notice,
  Screens,
  ServerMessage,
  Snapshot,
  StageState,
  Status,
  TimecodeIn,
  Waveform,
} from "./types";
import { authHeaders, withPasscode, type AccessReply } from "../access";
import type { MediaFile } from "../mediaLibrary";
import { ShowFileError, type FolderListing, type ShowFile } from "../showFiles";

export type ConnectionState = "connecting" | "open" | "closed";

/** A message shown to the operator until it times out or is dismissed. */
export interface Toast extends Notice {
  id: number;
}

export interface ShowState {
  connection: ConnectionState;
  snapshot: Snapshot | null;
  status: Status | null;
  audio: AudioStatus | null;
  /** The DMX slots that are lit; null when the server drives no lights. */
  dmx: DmxLevel[] | null;
  /** The last timecode that came in, with when it came (`performance.now()`). */
  timecode: (TimecodeIn & { at: number }) | null;
  /** What the stages show, with when it came (`performance.now()`). */
  stage: (StageState & { received: number }) | null;
  toasts: Toast[];
}

type Pending = { resolve: () => void; reject: (error: Error) => void };
type EventListener = (events: EngineEvent[]) => void;
type MeterListener = (peaks: number[]) => void;
type MidiListener = (event: MidiEvent) => void;

const MAX_TOASTS = 5;

/**
 * A WebSocket connection to mazzalab-server. Keeps the latest snapshot and
 * status, reconnects automatically, and turns requests into promises.
 */
export class ShowConnection {
  private socket: WebSocket | null = null;
  private nextId = 1;
  private nextToast = 1;
  private pending = new Map<number, Pending>();
  private listeners = new Set<() => void>();
  private eventListeners = new Set<EventListener>();
  private meterListeners = new Set<MeterListener>();
  private midiListeners = new Set<MidiListener>();
  private retries = 0;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private stopped = false;
  private state: ShowState = { connection: "connecting", snapshot: null, status: null, audio: null, dmx: null, timecode: null, stage: null, toasts: [] };

  /** What this client gives the server to be let in; null asks for nothing. */
  private passcode: string | null;

  constructor(
    private readonly url: string,
    passcode: string | null = null,
  ) {
    this.passcode = passcode;
  }

  /** Uses another passcode from now on, and reconnects with it. */
  setPasscode(passcode: string | null): void {
    this.passcode = passcode;
    if (this.retryTimer !== null) clearTimeout(this.retryTimer);
    this.retryTimer = null;
    if (this.stopped) return;
    const socket = this.socket;
    this.socket = null;
    socket?.close();
    this.retries = 0;
    this.connect();
  }

  /** A fetch that carries the passcode. */
  private fetchApi(path: string, init: RequestInit = {}): Promise<Response> {
    return fetch(`${this.httpBase}${path}`, { ...init, headers: { ...authHeaders(this.passcode), ...(init.headers as Record<string, string> | undefined) } });
  }

  /** What the passcode this client has grants; answers without one too. */
  async access(): Promise<AccessReply> {
    const response = await this.fetchApi("/api/access");
    if (!response.ok) throw new Error(await errorOf(response));
    return (await response.json()) as AccessReply;
  }

  start(): void {
    this.stopped = false;
    this.connect();
    // Back on the network, or back on the screen: do not wait out the backoff.
    window.addEventListener("online", this.retryNow);
    document.addEventListener("visibilitychange", this.retryNow);
  }

  private retryNow = (): void => {
    if (this.stopped || this.socket !== null || document.visibilityState === "hidden") return;
    if (this.retryTimer !== null) clearTimeout(this.retryTimer);
    this.retryTimer = null;
    this.retries = 0;
    this.connect();
  };

  stop(): void {
    this.stopped = true;
    if (this.retryTimer !== null) clearTimeout(this.retryTimer);
    window.removeEventListener("online", this.retryNow);
    document.removeEventListener("visibilitychange", this.retryNow);
    this.socket?.close();
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  onEvents(listener: EventListener): () => void {
    this.eventListeners.add(listener);
    return () => this.eventListeners.delete(listener);
  }

  /** Audio meters arrive many times a second, so they bypass the state. */
  onMeters(listener: MeterListener): () => void {
    this.meterListeners.add(listener);
    return () => this.meterListeners.delete(listener);
  }

  /** MIDI arriving on the server's inputs, to learn triggers from. */
  onMidi(listener: MidiListener): () => void {
    this.midiListeners.add(listener);
    return () => this.midiListeners.delete(listener);
  }

  getState = (): ShowState => this.state;

  send(command: Command): Promise<void> {
    return this.request({ type: "command", command });
  }

  save(): Promise<void> {
    return this.request({ type: "save" });
  }

  /**
   * Plays a cue by itself on the audition output, without running it in
   * the show; `null` stops what is being auditioned.
   */
  audition(cue: Id | null): Promise<void> {
    return this.request({ type: "audition", cue });
  }

  /** From a stage: the file of `cue` played to its end. */
  stageEnded(cue: Id): Promise<void> {
    return this.request({ type: "stage_ended", cue });
  }

  /** Where a stage fetches the file of a cue. */
  cueMediaUrl(cue: Id): string {
    return withPasscode(`${this.httpBase}/api/cue-media?cue=${cue}`, this.passcode);
  }

  /** The server's HTTP address, derived from the WebSocket one. */
  get httpBase(): string {
    return this.url.replace(/^ws/, "http").replace(/\/ws$/, "");
  }

  /** The audio output in use and the devices that could be chosen. */
  async audioInfo(): Promise<AudioInfo> {
    const response = await this.fetchApi("/api/audio");
    if (!response.ok) throw new Error(await errorOf(response));
    return (await response.json()) as AudioInfo;
  }

  /** The machine's displays, or `null` from a server that has none to put stages on. */
  async screens(): Promise<Screens | null> {
    const response = await this.fetchApi("/api/screens");
    if (!response.ok) throw new Error(await errorOf(response));
    const screens = (await response.json()) as Screens & { available: boolean };
    return screens.available ? { screens: screens.screens, shown: screens.shown } : null;
  }

  /** Puts the stages up on the screens they name, or takes them down. */
  async showOnScreens(shown: boolean): Promise<boolean> {
    const response = await this.fetchApi("/api/screens", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ shown }),
    });
    if (!response.ok) throw new Error(await errorOf(response));
    return ((await response.json()) as { shown: boolean }).shown;
  }

  /** Where the open show is on the server's machine. */
  async showFile(): Promise<ShowFile> {
    const response = await this.fetchApi("/api/show");
    if (!response.ok) throw new Error(await errorOf(response));
    return (await response.json()) as ShowFile;
  }

  /** The subfolders and shows of a folder of the server's machine; the open show's folder without `dir`. */
  async showFiles(dir?: string): Promise<FolderListing> {
    const query = dir === undefined ? "" : `?dir=${encodeURIComponent(dir)}`;
    const response = await this.fetchApi(`/api/show/files${query}`);
    if (!response.ok) throw new Error(await errorOf(response));
    return (await response.json()) as FolderListing;
  }

  /**
   * Opens or creates a show, or saves the open one under another name. A
   * show with unsaved changes is kept unless `discard`; that comes back as
   * a `ShowFileError` with `unsaved`.
   */
  async changeShow(action: "open" | "new" | "save-as", path: string, discard = false): Promise<void> {
    const response = await this.fetchApi(`/api/show/${action}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(action === "save-as" ? { path } : { path, discard }),
    });
    if (response.ok) return;
    let unsaved = false;
    try {
      unsaved = ((await response.clone().json()) as { unsaved?: boolean }).unsaved === true;
    } catch {
      // No JSON body: a plain failure.
    }
    throw new ShowFileError(await errorOf(response), unsaved);
  }

  /** What would not work if GO were pressed now. */
  async preflight(): Promise<PreflightIssue[]> {
    const response = await this.fetchApi("/api/preflight");
    if (!response.ok) throw new Error(await errorOf(response));
    return ((await response.json()) as { issues: PreflightIssue[] }).issues;
  }

  /** The files in the show's media folder. */
  async media(): Promise<MediaFile[]> {
    const response = await this.fetchApi("/api/media");
    if (!response.ok) throw new Error(await errorOf(response));
    return ((await response.json()) as { files: MediaFile[] }).files;
  }

  async midiPorts(): Promise<MidiPorts> {
    const response = await this.fetchApi("/api/midi");
    if (!response.ok) throw new Error(await errorOf(response));
    return (await response.json()) as MidiPorts;
  }

  async waveform(cue: Id, buckets: number, signal?: AbortSignal): Promise<Waveform> {
    const response = await this.fetchApi(`/api/waveform?cue=${cue}&buckets=${buckets}`, { signal });
    if (!response.ok) throw new Error(await errorOf(response));
    return (await response.json()) as Waveform;
  }

  /**
   * Copies a file into the show's media folder and returns its path
   * relative to the show, ready to be used as a cue's file.
   */
  uploadMedia(file: File, onProgress?: (fraction: number) => void): Promise<string> {
    return new Promise((resolve, reject) => {
      // XMLHttpRequest, unlike fetch, reports upload progress.
      const request = new XMLHttpRequest();
      request.open("POST", `${this.httpBase}/api/media?name=${encodeURIComponent(file.name)}`);
      for (const [name, value] of Object.entries(authHeaders(this.passcode))) request.setRequestHeader(name, value);
      request.upload.onprogress = (event) => {
        if (event.lengthComputable) onProgress?.(event.loaded / event.total);
      };
      request.onerror = () => reject(new Error("upload failed"));
      request.onload = () => {
        try {
          const body = JSON.parse(request.responseText) as { file?: string; error?: string };
          if (request.status === 200 && body.file) resolve(body.file);
          else reject(new Error(body.error ?? `upload failed (${request.status})`));
        } catch {
          reject(new Error(`upload failed (${request.status})`));
        }
      };
      request.send(file);
    });
  }

  toast(level: Notice["level"], message: string): void {
    const toast = { id: this.nextToast++, level, message };
    this.update({ toasts: [...this.state.toasts, toast].slice(-MAX_TOASTS) });
  }

  dismissToast(id: number): void {
    this.update({ toasts: this.state.toasts.filter((toast) => toast.id !== id) });
  }

  private request(
    message: { type: "command"; command: Command } | { type: "save" } | { type: "audition"; cue: Id | null } | { type: "stage_ended"; cue: Id },
  ): Promise<void> {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error("not connected"));
    }
    const id = this.nextId++;
    socket.send(JSON.stringify({ ...message, id }));
    return new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
  }

  private connect(): void {
    this.update({ connection: "connecting" });
    const socket = new WebSocket(withPasscode(this.url, this.passcode));
    this.socket = socket;
    socket.onopen = () => {
      this.retries = 0;
      this.update({ connection: "open" });
    };
    socket.onmessage = (event) => this.receive(JSON.parse(event.data as string) as ServerMessage);
    socket.onclose = () => {
      if (this.socket !== socket) return;
      this.socket = null;
      for (const pending of this.pending.values()) pending.reject(new Error("connection lost"));
      this.pending.clear();
      this.update({ connection: "closed" });
      if (!this.stopped) {
        const delay = Math.min(5000, 250 * 2 ** this.retries++);
        this.retryTimer = setTimeout(() => {
          this.retryTimer = null;
          this.connect();
        }, delay);
      }
    };
  }

  private receive(message: ServerMessage): void {
    switch (message.type) {
      case "snapshot":
        this.update({ snapshot: message.snapshot });
        break;
      case "status":
        this.update({ status: message.status });
        break;
      case "events":
        for (const listener of this.eventListeners) listener(message.events);
        break;
      case "meters":
        for (const listener of this.meterListeners) listener(message.peaks);
        break;
      case "audio":
        this.update({ audio: message.audio });
        break;
      case "notice":
        this.toast(message.notice.level, message.notice.message);
        break;
      case "dmx":
        this.update({ dmx: message.levels });
        break;
      case "midi_in":
        for (const listener of this.midiListeners) listener(message.event);
        break;
      case "timecode":
        this.update({ timecode: { timecode: message.timecode, format: message.format, at: performance.now() } });
        break;
      case "stage":
        this.update({ stage: { now: message.now, stages: message.stages, items: message.items, received: performance.now() } });
        break;
      case "result": {
        const pending = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) {
          this.toast("error", message.error);
          pending?.reject(new Error(message.error));
        } else {
          pending?.resolve();
        }
        break;
      }
    }
  }

  private update(partial: Partial<ShowState>): void {
    this.state = { ...this.state, ...partial };
    for (const listener of this.listeners) listener();
  }
}

async function errorOf(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as { error?: string };
    return body.error ?? response.statusText;
  } catch {
    return response.statusText;
  }
}

/** The server to talk to: `?server=host:port`, else the page's own origin. */
export function serverUrl(location: Pick<Location, "protocol" | "host" | "search">): string {
  const override = new URLSearchParams(location.search).get("server");
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  return `${scheme}://${override ?? location.host}/ws`;
}

/** The server's address as the passcode is filed under it: `?server=` or the page's own host. */
export function serverHost(location: Pick<Location, "host" | "search">): string {
  return new URLSearchParams(location.search).get("server") ?? location.host;
}

export function useShow(connection: ShowConnection): ShowState {
  return useSyncExternalStore(connection.subscribe, connection.getState);
}
