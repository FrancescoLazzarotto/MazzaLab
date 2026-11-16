import type { ContinueMode, Cue, CueColor, CueType } from "./api/types";
import { CUE_TYPES, actionLength, displayName, fileName, findCue, flatten, formatNumber, midiSummary, needsTarget, newCue, typeLabel } from "./cues";
import { parseTime } from "./format";
import { FORMAT_LABELS } from "./timecode";

/** What a cue's Target column says: the file, the message, or the cue it aims at. */
export function targetLabel(cue: Cue, allCues: Cue[]): string {
  if (cue.type === "audio" || cue.type === "video") return cue.file ? fileName(cue.file) : "";
  if (cue.type === "midi") return midiSummary(cue.message);
  if (cue.type === "timecode") return `${(cue.signal ?? "mtc").toUpperCase()} ${cue.start} · ${FORMAT_LABELS[cue.format ?? "fps30_non_drop"]}`;
  if (cue.type === "network") return cue.message.split(/\r?\n/)[0];
  if (cue.type === "light") {
    const lines = cue.commands ?? [];
    return lines.length > 1 ? `${lines[0]}  +${lines.length - 1}` : (lines[0] ?? "");
  }
  if (!needsTarget(cue.type) || !cue.target) return "";
  const target = findCue(allCues, cue.target);
  return target ? `${target.number ?? "–"}  ${displayName(target)}` : "?";
}

/** The columns of an exported cue list, in order, with their English header. */
export const EXPORT_COLUMNS = ["Number", "Name", "Type", "Target", "Pre-wait", "Action", "Post-wait", "Continue", "Notes", "Colour", "Flagged"] as const;

function seconds(value: number | null | undefined): string {
  return value ? formatNumber(value) : "";
}

/** One cue as the cells of the export, matching `EXPORT_COLUMNS`. */
export function cueCells(cue: Cue, allCues: Cue[]): string[] {
  return [
    cue.number ?? "",
    displayName(cue),
    typeLabel(cue.type),
    targetLabel(cue, allCues),
    seconds(cue.pre_wait),
    seconds(actionLength(cue)),
    seconds(cue.post_wait),
    cue.continue_mode ?? "do_not_continue",
    cue.notes ?? "",
    cue.color ?? "",
    cue.flagged ? "yes" : "",
  ];
}

/** Every cue of a list, groups opened, with how deep each one sits. */
export function sheetRows(cues: Cue[], allCues: Cue[]): { depth: number; cue: Cue; cells: string[] }[] {
  return flatten(cues).map(({ cue, depth }) => ({ depth, cue, cells: cueCells(cue, allCues) }));
}

/** One CSV field: quoted when it holds the delimiter, a quote or a line break. */
export function csvField(value: string, delimiter = ","): string {
  return value.includes(delimiter) || /["\r\n]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

/** The whole CSV text of a cue list, a header line first, lines ending in CRLF. */
export function buildCsv(cues: Cue[], allCues: Cue[], delimiter = ","): string {
  const lines = [[...EXPORT_COLUMNS], ...sheetRows(cues, allCues).map((row) => row.cells)];
  return lines.map((cells) => cells.map((cell) => csvField(cell, delimiter)).join(delimiter)).join("\r\n") + "\r\n";
}

function countOutsideQuotes(line: string, delimiter: string): number {
  let count = 0;
  let quoted = false;
  for (const char of line) {
    if (char === '"') quoted = !quoted;
    else if (char === delimiter && !quoted) count++;
  }
  return count;
}

/** Splits CSV text into rows of fields; the delimiter is the one the first line uses most. */
export function parseCsv(text: string): string[][] {
  const source = text.replace(/^﻿/, "");
  const firstLine = source.split(/\r?\n/, 1)[0] ?? "";
  const delimiter = [",", ";", "\t"]
    .map((candidate) => ({ candidate, count: countOutsideQuotes(firstLine, candidate) }))
    .reduce((best, entry) => (entry.count > best.count ? entry : best)).candidate;
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  let started = false;
  const endField = () => {
    row.push(field);
    field = "";
  };
  const endRow = () => {
    endField();
    rows.push(row);
    row = [];
    started = false;
  };
  for (let index = 0; index < source.length; index++) {
    const char = source[index];
    if (quoted) {
      if (char === '"') {
        if (source[index + 1] === '"') {
          field += '"';
          index++;
        } else quoted = false;
      } else field += char;
      continue;
    }
    if (char === '"' && field === "") {
      quoted = true;
      started = true;
    } else if (char === delimiter) {
      endField();
      started = true;
    } else if (char === "\n" || char === "\r") {
      if (char === "\r" && source[index + 1] === "\n") index++;
      endRow();
    } else {
      field += char;
      started = true;
    }
  }
  if (started || field !== "") endRow();
  return rows;
}

type Field = "number" | "name" | "type" | "pre_wait" | "duration" | "post_wait" | "continue" | "notes" | "colour" | "flagged";

/** Header names, folded to lower case letters and digits, in English and Italian. */
const HEADERS: Record<string, Field> = {
  number: "number",
  no: "number",
  n: "number",
  num: "number",
  numero: "number",
  cuenumber: "number",
  name: "name",
  nome: "name",
  cue: "name",
  title: "name",
  titolo: "name",
  type: "type",
  tipo: "type",
  prewait: "pre_wait",
  preattesa: "pre_wait",
  attesaprima: "pre_wait",
  action: "duration",
  azione: "duration",
  duration: "duration",
  durata: "duration",
  postwait: "post_wait",
  postattesa: "post_wait",
  attesadopo: "post_wait",
  continue: "continue",
  continuemode: "continue",
  continua: "continue",
  continuazione: "continue",
  notes: "notes",
  note: "notes",
  colour: "colour",
  color: "colour",
  colore: "colour",
  flagged: "flagged",
  flag: "flagged",
  segnalato: "flagged",
};

function fold(text: string): string {
  return text
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "");
}

const COLOURS: Record<string, CueColor> = {
  red: "red",
  rosso: "red",
  orange: "orange",
  arancione: "orange",
  arancio: "orange",
  yellow: "yellow",
  giallo: "yellow",
  green: "green",
  verde: "green",
  blue: "blue",
  blu: "blue",
  azzurro: "blue",
  purple: "purple",
  viola: "purple",
  grey: "grey",
  gray: "grey",
  grigio: "grey",
};

const CONTINUES: Record<string, ContinueMode> = {
  donotcontinue: "do_not_continue",
  none: "do_not_continue",
  no: "do_not_continue",
  nonconcontinuare: "do_not_continue",
  nonconcontinua: "do_not_continue",
  autocontinue: "auto_continue",
  continue: "auto_continue",
  continua: "auto_continue",
  autofollow: "auto_follow",
  follow: "auto_follow",
  segui: "auto_follow",
};

/** Cue types by what the export calls them and by their id, folded. */
function typeByName(name: string): CueType | null {
  const wanted = fold(name);
  if (wanted === "") return null;
  return CUE_TYPES.find((type) => fold(type) === wanted || fold(typeLabel(type)) === wanted) ?? null;
}

export interface CsvImport {
  cues: Cue[];
  /** Columns of the file that were read, as the file names them. */
  read: string[];
  /** Columns that were left alone. */
  ignored: string[];
}

/** Reads a cue list from CSV; blank lines are skipped and every cue is new. */
export function parseCueCsv(text: string): CsvImport {
  const [header = [], ...body] = parseCsv(text);
  const columns = header.map((name) => HEADERS[fold(name)] ?? null);
  const read = header.filter((_, index) => columns[index] !== null);
  const ignored = header.filter((name, index) => columns[index] === null && name.trim() !== "");
  const cues: Cue[] = [];
  for (const cells of body) {
    if (cells.every((cell) => cell.trim() === "")) continue;
    const value = (field: Field) => {
      const index = columns.indexOf(field);
      return index < 0 ? "" : (cells[index] ?? "").trim();
    };
    const cue = newCue(typeByName(value("type")) ?? "memo", value("number") || undefined) as Cue & Record<string, unknown>;
    if (value("name")) cue.name = value("name");
    if (value("notes")) cue.notes = value("notes");
    const preWait = parseTime(value("pre_wait"));
    if (preWait) cue.pre_wait = preWait;
    const postWait = parseTime(value("post_wait"));
    if (postWait) cue.post_wait = postWait;
    const duration = parseTime(value("duration"));
    if (duration !== null && "duration" in cue) cue.duration = duration;
    const mode = CONTINUES[fold(value("continue"))];
    if (mode) cue.continue_mode = mode;
    const colour = COLOURS[fold(value("colour"))];
    if (colour) cue.color = colour;
    if (/^(1|x|yes|true|si|s)$/.test(fold(value("flagged")))) cue.flagged = true;
    cues.push(cue);
  }
  return { cues, read, ignored };
}
