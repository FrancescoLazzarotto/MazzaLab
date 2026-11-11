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
          {index > 0 && <span className="toolbox-gap" />}
          {group.map((type) => (
            <button
              key={type}
              className={WORKING_TYPES.has(type) ? "tool" : "tool planned"}
              title={WORKING_TYPES.has(type) ? typeLabel(type) : `${typeLabel(type)} — ${t("problem.not_supported")}`}
              onClick={() => onAdd(type)}
            >
              <CueIcon type={type} size={18} />
              <span>{typeLabel(type)}</span>
            </button>
          ))}
        </Fragment>
      ))}
    </div>
  );
}
