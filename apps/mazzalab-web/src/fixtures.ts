import type { Fixture, FixtureGroup } from "./api/types";

export type FixtureType = "dimmer" | "rgb" | "rgbw" | "dimmer_rgb" | "dimmer_rgbw" | "moving_cmy";

/** The kinds of fixture the patch offers, by the channels they have. */
export const FIXTURE_TYPES: [FixtureType, string[]][] = [
  ["dimmer", ["intensity"]],
  ["rgb", ["red", "green", "blue"]],
  ["rgbw", ["red", "green", "blue", "white"]],
  ["dimmer_rgb", ["intensity", "red", "green", "blue"]],
  ["dimmer_rgbw", ["intensity", "red", "green", "blue", "white"]],
  ["moving_cmy", ["pan", "tilt", "intensity", "cyan", "magenta", "yellow"]],
];

/** Names are matched ignoring case and runs of spaces, as the server does. */
export function normalize(name: string): string {
  return name.trim().split(/\s+/).join(" ").toLowerCase();
}

export function parametersOf(fixture: Fixture): string[] {
  return fixture.parameters ?? ["intensity"];
}

/** The type whose channels a fixture has, or `custom`. */
export function typeOf(fixture: Fixture): FixtureType | "custom" {
  const parameters = parametersOf(fixture).join(",");
  return (
    FIXTURE_TYPES.find(
      ([, channels]) => channels.join(",") === parameters,
    )?.[0] ?? "custom"
  );
}

/** Reads `intensity, red green` as channel names. */
export function parseParameters(text: string): string[] {
  return text
    .split(/[,;\s]+/)
    .map((name) => name.trim().toLowerCase())
    .filter(Boolean);
}

/** The last DMX address a fixture takes. */
export function lastAddress(fixture: Fixture): number {
  return fixture.address + Math.max(1, parametersOf(fixture).length) - 1;
}

/**
 * The fixtures that cannot be lit as patched: those sharing an address with
 * another, and those running past the end of their universe.
 */
export function clashes(fixtures: Fixture[]): Set<number> {
  const out = new Set<number>();
  fixtures.forEach((fixture, index) => {
    if (fixture.address < 1 || lastAddress(fixture) > 512) out.add(index);
    fixtures.forEach((other, position) => {
      if (
        position !== index &&
        (other.universe ?? 1) === (fixture.universe ?? 1) &&
        other.address <= lastAddress(fixture) &&
        fixture.address <= lastAddress(other)
      ) {
        out.add(index);
      }
    });
  });
  return out;
}

/** Names given to more than one fixture or group. */
export function duplicateNames(
  fixtures: Fixture[],
  groups: FixtureGroup[],
): Set<string> {
  const seen = new Set<string>();
  const twice = new Set<string>();
  for (const name of [
    ...fixtures.map((fixture) => fixture.name),
    ...groups.map((group) => group.name),
  ]) {
    const key = normalize(name);
    if (seen.has(key)) twice.add(key);
    seen.add(key);
  }
  return twice;
}

/** A fixture to add: the next free address after the last one patched. */
export function nextFixture(fixtures: Fixture[]): Fixture {
  const last = fixtures[fixtures.length - 1];
  const parameters = last?.parameters;
  const universe = last?.universe ?? 1;
  const width = Math.max(1, (parameters ?? ["intensity"]).length);
  let address =
    1 +
    Math.max(
      0,
      ...fixtures
        .filter((fixture) => (fixture.universe ?? 1) === universe)
        .map(lastAddress),
    );
  let nextUniverse = universe;
  if (address + width - 1 > 512) {
    nextUniverse += 1;
    address = 1;
  }
  let n = fixtures.length + 1;
  const taken = new Set(fixtures.map((fixture) => normalize(fixture.name)));
  while (taken.has(normalize(`Fixture ${n}`))) n++;
  return {
    name: `Fixture ${n}`,
    universe: nextUniverse,
    address,
    ...(parameters ? { parameters } : {}),
  };
}

/** Groups after a fixture or group was renamed: they follow it. */
export function renameMember(
  groups: FixtureGroup[],
  from: string,
  to: string,
): FixtureGroup[] {
  const key = normalize(from);
  return groups.map((group) => ({
    ...group,
    members: (group.members ?? []).map((member) =>
      normalize(member) === key ? to : member,
    ),
  }));
}

/** Groups after a fixture or group was deleted: it leaves them. */
export function removeMember(
  groups: FixtureGroup[],
  name: string,
): FixtureGroup[] {
  const key = normalize(name);
  return groups.map((group) => ({
    ...group,
    members: (group.members ?? []).filter(
      (member) => normalize(member) !== key,
    ),
  }));
}

/** Toggles one name in a group's members. */
export function toggleMember(group: FixtureGroup, name: string): FixtureGroup {
  const members = group.members ?? [];
  const key = normalize(name);
  return members.some((member) => normalize(member) === key)
    ? {
        ...group,
        members: members.filter((member) => normalize(member) !== key),
      }
    : { ...group, members: [...members, name] };
}

/** What each patched DMX slot is, as `Name.parameter`, keyed `universe/address`. */
export function slotLabels(fixtures: Fixture[]): Map<string, string> {
  const labels = new Map<string, string>();
  for (const fixture of fixtures) {
    const parameters = parametersOf(fixture);
    parameters.forEach((parameter, index) => {
      const key = `${fixture.universe ?? 1}/${fixture.address + index}`;
      if (!labels.has(key))
        labels.set(
          key,
          parameters.length === 1 && parameter === "intensity"
            ? fixture.name
            : `${fixture.name}.${parameter}`,
        );
    });
  }
  return labels;
}
