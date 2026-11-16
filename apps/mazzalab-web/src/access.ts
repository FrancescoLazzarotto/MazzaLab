import type { Passcode } from "./api/types";

/** What this client may do, as `/api/access` reports it. */
export interface Access {
  view: boolean;
  control: boolean;
  edit: boolean;
}

export const FULL_ACCESS: Access = { view: true, control: true, edit: true };

/** The answer of `/api/access`: whether the show asks for a passcode, and what the one given grants. */
export interface AccessReply extends Access {
  required: boolean;
}

/** What the interface does with an access. */
export interface UiFlags {
  /** Editing affordances are there at all; without them the device is in show mode. */
  canEdit: boolean;
  /** GO, the transport, hotkeys and the playhead work. */
  canControl: boolean;
}

export function uiFlags(access: Access): UiFlags {
  return { canEdit: access.edit, canControl: access.control };
}

/** The mode to show: someone who cannot edit is always in show mode. */
export function effectiveMode(preferred: "edit" | "show", access: Access): "edit" | "show" {
  return access.edit ? preferred : "show";
}

/** The highest thing the access allows, for the status bar. */
export function accessLevel(access: Access): "edit" | "control" | "view" | "none" {
  if (access.edit) return "edit";
  if (access.control) return "control";
  return access.view ? "view" : "none";
}

/** The passcode a link carries as `?passcode=`, if any. */
export function passcodeFromUrl(search: string): string | null {
  const code = new URLSearchParams(search).get("passcode");
  return code ? code : null;
}

/** The same query without the passcode, so it does not stay in the address bar. */
export function withoutPasscode(search: string): string {
  const params = new URLSearchParams(search);
  params.delete("passcode");
  const rest = params.toString();
  return rest ? `?${rest}` : "";
}

/** `url` with the passcode in its query, for requests that cannot carry a header. */
export function withPasscode(url: string, code: string | null): string {
  if (!code) return url;
  const parsed = new URL(url, "http://placeholder.invalid");
  parsed.searchParams.set("passcode", code);
  const absolute = /^[a-z][a-z0-9+.-]*:/i.test(url);
  return absolute ? parsed.toString() : `${parsed.pathname}${parsed.search}${parsed.hash}`;
}

/** The headers a request carries for `code`. */
export function authHeaders(code: string | null): Record<string, string> {
  return code ? { authorization: `Bearer ${code}` } : {};
}

/** Where the passcode for one server is kept; a passcode belongs to the server it opened. */
export function passcodeKey(host: string): string {
  return `mazzalab.passcode.${host}`;
}

type Store = Pick<Storage, "getItem" | "setItem" | "removeItem">;

export function loadPasscode(host: string, store: Store | undefined = safeStorage()): string | null {
  try {
    return store?.getItem(passcodeKey(host)) || null;
  } catch {
    return null;
  }
}

export function savePasscode(host: string, code: string | null, store: Store | undefined = safeStorage()): void {
  try {
    if (code) store?.setItem(passcodeKey(host), code);
    else store?.removeItem(passcodeKey(host));
  } catch {
    // Not kept; it still applies to this session.
  }
}

function safeStorage(): Store | undefined {
  try {
    return typeof localStorage === "undefined" ? undefined : localStorage;
  } catch {
    return undefined;
  }
}

/** Whether a client has to ask the operator for a passcode before anything else. */
export function needsPasscode(reply: AccessReply): boolean {
  return reply.required && !reply.view;
}

/** A new passcode row, four digits not used yet, that lets a client look. */
export function newPasscode(existing: readonly Passcode[], random: () => number = Math.random): Passcode {
  const used = new Set(existing.map((passcode) => passcode.code));
  let code = "";
  do code = String(1000 + Math.floor(random() * 9000));
  while (used.has(code));
  return { code, name: "", view: true, control: false, edit: false };
}

/** What a row grants once control and edit imply view. */
export function effectiveView(passcode: Passcode): boolean {
  return passcode.view || passcode.control || passcode.edit;
}

/** Whether any passcode may edit; without one, only the server's own machine can. */
export function someoneCanEdit(passcodes: readonly Passcode[]): boolean {
  return passcodes.some((passcode) => passcode.edit);
}
