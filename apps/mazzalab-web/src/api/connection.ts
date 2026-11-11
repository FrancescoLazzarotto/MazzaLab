import { useSyncExternalStore } from "react";
import type {
  AudioInfo,
  AudioStatus,
  Command,
  EngineEvent,
  Id,
  MidiPorts,
  Notice,
  ServerMessage,
  Snapshot,
  Status,
  Waveform,
} from "./types";

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
  toasts: Toast[];
}

type Pending = { resolve: () => void; reject: (error: Error) => void };
type EventListener = (events: EngineEvent[]) => void;
type MeterListener = (peaks: number[]) => void;

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
  private retries = 0;
  private stopped = false;
  private state: ShowState = { connection: "connecting", snapshot: null, status: null, audio: null, toasts: [] };

  constructor(private readonly url: string) {}

  start(): void {
    this.stopped = false;
    this.connect();
  }

  stop(): void {
    this.stopped = true;
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

  getState = (): ShowState => this.state;

  send(command: Command): Promise<void> {
    return this.request({ type: "command", command });
  }

  save(): Promise<void> {
    return this.request({ type: "save" });
  }

  /** The server's HTTP address, derived from the WebSocket one. */
  get httpBase(): string {
    return this.url.replace(/^ws/, "http").replace(/\/ws$/, "");
  }

  /** The audio output in use and the devices that could be chosen. */
  async audioInfo(): Promise<AudioInfo> {
    const response = await fetch(`${this.httpBase}/api/audio`);
    if (!response.ok) throw new Error(await errorOf(response));
    return (await response.json()) as AudioInfo;
  }

  async midiPorts(): Promise<MidiPorts> {
    const response = await fetch(`${this.httpBase}/api/midi`);
    if (!response.ok) throw new Error(await errorOf(response));
    return (await response.json()) as MidiPorts;
  }

  async waveform(cue: Id, buckets: number, signal?: AbortSignal): Promise<Waveform> {
    const response = await fetch(`${this.httpBase}/api/waveform?cue=${cue}&buckets=${buckets}`, { signal });
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

  private request(message: { type: "command"; command: Command } | { type: "save" }): Promise<void> {
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
    const socket = new WebSocket(this.url);
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
        setTimeout(() => this.connect(), delay);
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

export function useShow(connection: ShowConnection): ShowState {
  return useSyncExternalStore(connection.subscribe, connection.getState);
}
