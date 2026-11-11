import { Fragment } from "react";
import type { CueType } from "../api/types";
import { TOOLBOX, WORKING_TYPES, typeLabel } from "../cues";
import { useI18n } from "../i18n";
import { CueIcon } from "./icons";

/** One button per cue type; a click adds that cue after the selection. */
export function Toolbox({ onAdd }: { onAdd: (type: CueType) => void }) {
  const { t } = useI18n();
  return (
    <div className="toolbox" role="toolbar" aria-label={t("action.addCue")}>
      {TOOLBOX.map((group, index) => (
        <Fragment key={group[0]}>
          {index > 0 && <span className="divider" />}
          {group.map((type) => {
            const works = WORKING_TYPES.has(type);
            const label = works ? typeLabel(type) : `${typeLabel(type)} — ${t("problem.not_supported")}`;
            return (
              <button key={type} className={works ? "tool" : "tool planned"} data-type={type} title={label} aria-label={label} onClick={() => onAdd(type)}>
                <CueIcon type={type} size={17} />
              </button>
            );
          })}
        </Fragment>
      ))}
    </div>
  );
}
