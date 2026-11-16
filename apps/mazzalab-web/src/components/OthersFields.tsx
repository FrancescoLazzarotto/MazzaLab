import type { Others } from "../api/types";
import { useI18n } from "../i18n";
import { cleanDuck, othersMode, withMode, type OthersMode } from "../others";
import { NumberField, SelectField, TimeField } from "./fields";

/** What the cue does to the sounds already playing when it starts: nothing, fade them out and stop them, or duck them. */
export function OthersFields({ others, onChange }: { others: Others | undefined; onChange: (others: Others | undefined) => void }) {
  const { t } = useI18n();
  const mode = othersMode(others);
  return (
    <>
      <SelectField
        label={t("others.title")}
        value={mode}
        options={(["off", "fade_and_stop", "duck"] as OthersMode[]).map((value) => ({ value, label: t(`others.${value}`) }))}
        onChange={(next) => onChange(withMode(others, next))}
      />
      {others && (
        <div className="row">
          <SelectField
            label={t("others.scope")}
            value={others.scope}
            options={[
              { value: "list", label: t("others.list") },
              { value: "all", label: t("others.all") },
            ]}
            onChange={(scope) => onChange({ ...others, scope })}
          />
          <TimeField label={t("others.fade")} value={others.fade} onCommit={(fade) => onChange({ ...others, fade: fade ?? 0 })} />
          {others.action === "duck" && (
            <NumberField label={t("others.duckDb")} value={others.duck_db} step={1} min={0.5} onCommit={(db) => onChange({ ...others, duck_db: cleanDuck(db) })} />
          )}
        </div>
      )}
    </>
  );
}
