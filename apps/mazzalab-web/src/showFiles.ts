/** A folder of the server's machine, as the picker lists it. */
export interface FolderListing {
  dir: string;
  parent: string | null;
  separator: string;
  folders: string[];
  shows: string[];
}

/** What the server says about the open show. */
export interface ShowFile {
  path: string | null;
  folder: string | null;
}

/** A failed open, new or save-as; `unsaved` means the open show has changes to keep or drop. */
export class ShowFileError extends Error {
  constructor(
    message: string,
    readonly unsaved: boolean,
  ) {
    super(message);
  }
}

export interface Crumb {
  name: string;
  path: string;
}

/** The folders from the root down to `dir`, each with the path that opens it. */
export function breadcrumbs(dir: string, separator: string): Crumb[] {
  const parts = dir.split(separator).filter((part) => part !== "");
  const crumbs: Crumb[] = [];
  // A Windows path starts at a drive (`C:`), a Unix one at the root.
  const windows = separator === "\\";
  let path = windows ? "" : separator;
  parts.forEach((part, index) => {
    path = windows && index === 0 ? `${part}${separator}` : joinPath(path, part, separator);
    crumbs.push({ name: part, path });
  });
  if (!windows) crumbs.unshift({ name: separator, path: separator });
  return crumbs;
}

/** `name` inside `dir`, with the right separator and no doubling. */
export function joinPath(dir: string, name: string, separator: string): string {
  return dir.endsWith(separator) ? `${dir}${name}` : `${dir}${separator}${name}`;
}

/** The name without the show extension, for the name field. */
export function stripExtension(name: string, extension = ".mazzalab"): string {
  return name.toLowerCase().endsWith(extension) ? name.slice(0, -extension.length) : name;
}

/** Whether a typed name can be a file name: not empty, and no path in it. */
export function validFileName(name: string): boolean {
  const trimmed = name.trim();
  return trimmed !== "" && trimmed !== "." && trimmed !== ".." && !/[\\/:*?"<>|\u0000-\u001f]/.test(trimmed);
}
