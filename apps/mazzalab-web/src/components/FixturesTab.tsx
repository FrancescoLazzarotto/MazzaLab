import type { Fixture, FixtureGroup } from "../api/types";
import {
  FIXTURE_TYPES,
  clashes,
  duplicateNames,
  lastAddress,
  nextFixture,
  normalize,
  parametersOf,
  parseParameters,
  removeMember,
  renameMember,
  toggleMember,
  typeOf,
} from "../fixtures";
import { useI18n } from "../i18n";
import { NumberField, SelectField, TextField } from "./fields";
import { Icon } from "./icons";

/** The rig: fixtures at their addresses, and the groups they are called by. */
export function FixturesTab({
  fixtures,
  groups,
  onChange,
}: {
  fixtures: Fixture[];
  groups: FixtureGroup[];
  onChange: (fixtures: Fixture[], groups: FixtureGroup[]) => void;
}) {
  const { t } = useI18n();
  const clashing = clashes(fixtures);
  const twice = duplicateNames(fixtures, groups);

  const updateFixture = (index: number, patch: Partial<Fixture>) => {
    const old = fixtures[index];
    const next = fixtures.map((fixture, position) =>
      position === index ? { ...fixture, ...patch } : fixture,
    );
    onChange(
      next,
      patch.name !== undefined
        ? renameMember(groups, old.name, patch.name)
        : groups,
    );
  };
  const setParameters = (index: number, parameters: string[]) => {
    const { parameters: _, ...rest } = fixtures[index];
    const fixture =
      parameters.length === 0 || parameters.join(",") === "intensity"
        ? rest
        : { ...rest, parameters };
    onChange(
      fixtures.map((existing, position) =>
        position === index ? fixture : existing,
      ),
      groups,
    );
  };
  const updateGroup = (index: number, group: FixtureGroup) => {
    const old = groups[index];
    const next = groups.map((existing, position) =>
      position === index ? group : existing,
    );
    onChange(
      fixtures,
      group.name !== old.name ? renameMember(next, old.name, group.name) : next,
    );
  };
  const addGroup = () => {
    const base = t("settings.groupName");
    let n = groups.length + 1;
    const taken = new Set(
      [...groups, ...fixtures].map((item) => normalize(item.name)),
    );
    while (taken.has(normalize(`${base} ${n}`))) n++;
    onChange(fixtures, [...groups, { name: `${base} ${n}`, members: [] }]);
  };

  return (
    <div className="form-column">
      {fixtures.length === 0 && (
        <p className="placeholder">{t("settings.noFixtures")}</p>
      )}
      {fixtures.map((fixture, index) => {
        const type = typeOf(fixture);
        const warn = clashing.has(index) || twice.has(normalize(fixture.name));
        return (
          <div
            className={
              warn
                ? "row patch-row fixture-row clash"
                : "row patch-row fixture-row"
            }
            key={index}
          >
            <TextField
              label={t("settings.patchName")}
              value={fixture.name}
              onCommit={(name) =>
                name.trim() && updateFixture(index, { name: name.trim() })
              }
            />
            <NumberField
              label={t("settings.universe")}
              value={fixture.universe ?? 1}
              min={1}
              onCommit={(universe) =>
                updateFixture(index, { universe: Math.round(universe) })
              }
            />
            <NumberField
              label={t("settings.address")}
              value={fixture.address}
              min={1}
              onCommit={(address) =>
                updateFixture(index, {
                  address: Math.min(512, Math.round(address)),
                })
              }
            />
            <SelectField
              label={t("settings.fixtureType")}
              value={type}
              options={[
                ...FIXTURE_TYPES.map(([name]) => ({
                  value: name,
                  label: t(`fixture.${name}`),
                })),
                ...(type === "custom"
                  ? [{ value: "custom", label: t("fixture.custom") }]
                  : []),
              ]}
              onChange={(name) => {
                const channels = FIXTURE_TYPES.find(([id]) => id === name)?.[1];
                if (channels) setParameters(index, channels);
              }}
            />
            <TextField
              label={t("settings.fixtureChannels")}
              value={parametersOf(fixture).join(", ")}
              onCommit={(text) => setParameters(index, parseParameters(text))}
            />
            <span className="fixture-span" title={t("settings.fixtureSpan")}>
              {lastAddress(fixture) === fixture.address ? fixture.address : `${fixture.address}–${lastAddress(fixture)}`}
            </span>
            <button
              className="icon-button quiet"
              title={t("action.delete")}
              onClick={() =>
                onChange(
                  fixtures.filter((_, position) => position !== index),
                  removeMember(groups, fixture.name),
                )
              }
            >
              <Icon name="trash" size={14} />
            </button>
          </div>
        );
      })}
      <div>
        <button
          onClick={() => onChange([...fixtures, nextFixture(fixtures)], groups)}
        >
          <Icon name="plus" size={12} />
          {t("settings.addFixture")}
        </button>
      </div>
      {clashing.size > 0 && (
        <p className="hint warn">{t("settings.fixtureClash")}</p>
      )}
      {twice.size > 0 && (
        <p className="hint warn">{t("settings.duplicateName")}</p>
      )}

      <h3>{t("settings.groups")}</h3>
      {groups.length === 0 && (
        <p className="placeholder">{t("settings.noGroups")}</p>
      )}
      {groups.map((group, index) => {
        const members = new Set((group.members ?? []).map(normalize));
        const candidates = [
          ...groups
            .filter((_, position) => position !== index)
            .map((other) => ({ name: other.name, group: true })),
          ...fixtures.map((fixture) => ({ name: fixture.name, group: false })),
        ];
        // Members that no longer exist stay visible, so they can be removed.
        const missing = (group.members ?? []).filter(
          (member) =>
            !candidates.some(
              (candidate) => normalize(candidate.name) === normalize(member),
            ),
        );
        return (
          <div
            className={
              twice.has(normalize(group.name)) ? "group-row clash" : "group-row"
            }
            key={index}
          >
            <TextField
              label={t("settings.patchName")}
              value={group.name}
              onCommit={(name) =>
                name.trim() &&
                updateGroup(index, { ...group, name: name.trim() })
              }
            />
            <div
              className="group-members"
              role="group"
              aria-label={t("settings.members")}
            >
              {candidates.map(({ name, group: isGroup }) => (
                <button
                  key={`${isGroup ? "g" : "f"}:${name}`}
                  className={[
                    "patch-name",
                    isGroup ? "group" : "",
                    members.has(normalize(name)) ? "active" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  aria-pressed={members.has(normalize(name))}
                  onClick={() => updateGroup(index, toggleMember(group, name))}
                >
                  {name}
                </button>
              ))}
              {missing.map((name) => (
                <button
                  key={`missing:${name}`}
                  className="patch-name active missing"
                  title={t("settings.missingMember")}
                  onClick={() => updateGroup(index, toggleMember(group, name))}
                >
                  {name}
                </button>
              ))}
              {candidates.length === 0 && missing.length === 0 && (
                <span className="placeholder">{t("settings.noFixtures")}</span>
              )}
            </div>
            <button
              className="icon-button quiet"
              title={t("action.delete")}
              onClick={() =>
                onChange(
                  fixtures,
                  removeMember(
                    groups.filter((_, position) => position !== index),
                    group.name,
                  ),
                )
              }
            >
              <Icon name="trash" size={14} />
            </button>
          </div>
        );
      })}
      <div>
        <button onClick={addGroup}>
          <Icon name="plus" size={12} />
          {t("settings.addGroup")}
        </button>
      </div>
      <p className="hint">{t("settings.fixturesHint")}</p>
    </div>
  );
}
