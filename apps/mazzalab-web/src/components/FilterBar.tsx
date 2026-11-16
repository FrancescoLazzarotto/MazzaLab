import type { Ref } from "react";
import type { CueColor, CueType } from "../api/types";
import { CUE_TYPES, typeLabel } from "../cues";
import { isFiltering, type CueFilter, NO_FILTER } from "../cueFilter";
import { useI18n } from "../i18n";
import { Icon } from "./icons";

const COLORS: CueColor[] = ["red", "orange", "yellow", "green", "blue", "purple", "grey"];

/** Search and filters above the cue list; they only change what is shown. */
export function FilterBar({
  filter,
  onChange,
  shown,
  total,
  inputRef,
}: {
  filter: CueFilter;
  onChange: (filter: CueFilter) => void;
  shown: number;
  total: number;
  inputRef: Ref<HTMLInputElement>;
}) {
  const { t } = useI18n();
  const active = isFiltering(filter);
  return (
    <div className={active ? "filter-bar active" : "filter-bar"}>
      <label className="filter-search">
        <Icon name="search" size={13} />
        <input
          ref={inputRef}
          value={filter.text}
          placeholder={t("filter.search")}
          aria-label={t("filter.search")}
          onChange={(event) => onChange({ ...filter, text: event.target.value })}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.stopPropagation();
              onChange(NO_FILTER);
              event.currentTarget.blur();
            }
          }}
        />
      </label>
      <select
        value={filter.type ?? ""}
        aria-label={t("filter.allTypes")}
        onChange={(event) => onChange({ ...filter, type: (event.target.value || null) as CueType | null })}
      >
        <option value="">{t("filter.allTypes")}</option>
        {CUE_TYPES.map((type) => (
          <option key={type} value={type}>
            {typeLabel(type)}
          </option>
        ))}
      </select>
      <select
        value={filter.color ?? ""}
        aria-label={t("filter.anyColor")}
        onChange={(event) => onChange({ ...filter, color: (event.target.value || null) as CueFilter["color"] })}
      >
        <option value="">{t("filter.anyColor")}</option>
        <option value="none">{t("filter.noColor")}</option>
        {COLORS.map((color) => (
          <option key={color} value={color}>
            {t(`color.${color}`)}
          </option>
        ))}
      </select>
      <button
        className={filter.flagged ? "icon-button quiet toggled flagged" : "icon-button quiet"}
        onClick={() => onChange({ ...filter, flagged: !filter.flagged })}
        title={t("filter.flagged")}
        aria-pressed={filter.flagged}
      >
        <Icon name="flag" size={13} />
      </button>
      {active && (
        <>
          <span className="filter-count">
            {shown} / {total}
          </span>
          <button className="icon-button quiet" onClick={() => onChange(NO_FILTER)} title={t("filter.clear")}>
            <Icon name="close" size={12} />
          </button>
        </>
      )}
    </div>
  );
}
