import { useSyncExternalStore } from "react";
import type { Command, EngineEvent, ServerMessage, Snapshot, Status } from "./types";

export type ConnectionState = "connecting" | "open" | "closed";

export interface ShowState {
  connection: ConnectionState;
  snapshot: Snapshot | null;
  status: Status | null;
  /** Last error returned by the server for one of our requests. */
  lastError: string | null;
}

type Pending = { resolve: () => void; reject: (error: Error) => void };
type EventListener = (events: EngineEvent[]) => void;

/**
 * A WebSocket connection to mazzalab-server. Keeps the latest snapshot and
 * status, reconnects automatically, and turns requests into promises.
 */
export class ShowConnection {
  private socket: WebSocket | null = null;
  private nextId = 1;
  private pending = new Map<number, Pending>();
  private listeners = new Set<() => void>();
  private eventListeners = new Set<EventListener>();
  private retries = 0;
  private stopped = false;
  private state: ShowState = { connection: "connecting", snapshot: null, status: null, lastError: null };

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

  getState = (): ShowState => this.state;

  send(command: Command): Promise<void> {
    return this.request({ type: "command", command });
  }

  save(): Promise<void> {
    return this.request({ type: "save" });
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
      case "result": {
        const pending = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) {
          this.update({ lastError: message.error });
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

  clearError(): void {
    this.update({ lastError: null });
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
