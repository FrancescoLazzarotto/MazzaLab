import { useEffect, useRef, useState } from "react";
import type { Fixture, FixtureGroup } from "../api/types";
import { useI18n } from "../i18n";
import { Field } from "./fields";

/**
 * The lines of a light cue, with the patch's groups and fixtures at hand:
 * clicking one starts a line for it, ready for the level.
 */
export function LightCommands({
  commands,
  fixtures,
  groups,
  onCommit,
}: {
  commands: string[];
  fixtures: Fixture[];
  groups: FixtureGroup[];
  onCommit: (commands: string[]) => void;
}) {
  const { t } = useI18n();
  const value = commands.join("\n");
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const area = useRef<HTMLTextAreaElement>(null);

  const commit = (text: string) => {
    const lines = text
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
    if (lines.join("\n") !== value) onCommit(lines);
  };
  const call = (name: string) => {
    const before = draft.replace(/\s+$/, "");
    const next = `${before}${before ? "\n" : ""}${name} @ `;
    setDraft(next);
    // Keep typing on that line: the level is what comes next.
    requestAnimationFrame(() => {
      const element = area.current;
      if (!element) return;
      element.focus();
      element.setSelectionRange(next.length, next.length);
    });
  };

  const names = [
    ...groups.map((group) => ({ name: group.name, group: true })),
    ...fixtures.map((fixture) => ({ name: fixture.name, group: false })),
  ];
  return (
    <>
      <Field label={t("inspector.commands")}>
        <textarea
          ref={area}
          className="light-commands"
          rows={4}
          spellCheck={false}
          value={draft}
          placeholder={
            names.length > 0
              ? `${names[0].name} @ 75\n${names[names.length - 1].name} @ #ff8000\n1 thru 10 @ 50`
              : "1 thru 10 @ 50\n12 + 14 @ full\n2/1 thru 2/8 @ out"
          }
          onChange={(event) => setDraft(event.target.value)}
          onBlur={() => commit(draft)}
          onKeyDown={(event) => {
            if (event.key === "Escape") setDraft(value);
          }}
        />
      </Field>
      {names.length > 0 && (
        <div className="patch-names" aria-label={t("inspector.fixtures")}>
          {names.map(({ name, group }) => (
            <button
              key={`${group ? "g" : "f"}:${name}`}
              className={group ? "patch-name group" : "patch-name"}
              // Keep the text area focused, so that the draft is not committed half typed.
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => call(name)}
            >
              {name}
            </button>
          ))}
        </div>
      )}
    </>
  );
}
