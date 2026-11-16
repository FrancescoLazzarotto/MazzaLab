import type { Cue, CueTemplate, Id } from "./api/types";
import { cloneCues } from "./cues";

/** A cue without what belongs to one place in one show: its numbers, triggers and target. */
function stripped(cue: Cue): Cue {
  const copy = { ...cue } as Cue;
  delete copy.number;
  delete copy.target;
  delete copy.triggers;
  if (copy.type === "group" && cue.type === "group") copy.children = (cue.children ?? []).map(stripped);
  return copy;
}

/** A template of `cue`, kept under `name`. */
export function makeTemplate(name: string, cue: Cue): CueTemplate {
  return { name: name.trim(), cue: stripped(structuredClone(cue)) };
}

/** The templates with this one added; one with the same name (any case) is replaced. */
export function withTemplate(templates: readonly CueTemplate[], template: CueTemplate): CueTemplate[] {
  const same = (other: CueTemplate) => other.name.toLowerCase() === template.name.toLowerCase();
  return templates.some(same) ? templates.map((other) => (same(other) ? template : other)) : [...templates, template];
}

export function withoutTemplate(templates: readonly CueTemplate[], name: string): CueTemplate[] {
  return templates.filter((template) => template.name !== name);
}

/** A new cue from a template, with its own ids (children's too) and the number given. */
export function cueFromTemplate(template: CueTemplate, number: string, makeId?: () => Id): Cue {
  const [cue] = cloneCues([structuredClone(template.cue)], 0, makeId);
  return { ...cue, number };
}

/** A name for a template of this cue: its own, else its type. */
export function suggestedName(cue: Cue, typeName: string): string {
  return cue.name && cue.name.trim() !== "" ? cue.name.trim() : typeName;
}
