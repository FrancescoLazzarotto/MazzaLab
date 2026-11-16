import { useEffect, useRef, useState } from "react";
import type { CueTemplate } from "../api/types";
import { typeLabel } from "../cues";
import { useI18n } from "../i18n";
import { CueIcon, Icon } from "./icons";

/** A small menu of the show's cue templates: a click makes a cue from one. */
export function TemplateMenu({ templates, onCreate, onDelete }: { templates: readonly CueTemplate[]; onCreate: (template: CueTemplate) => void; onDelete: (name: string) => void }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const away = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        setOpen(false);
      }
    };
    window.addEventListener("pointerdown", away);
    window.addEventListener("keydown", escape, true);
    return () => {
      window.removeEventListener("pointerdown", away);
      window.removeEventListener("keydown", escape, true);
    };
  }, [open]);

  return (
    <div className="template-menu" ref={root}>
      <button className="tool" onClick={() => setOpen(!open)} aria-expanded={open} aria-haspopup="menu" title={t("template.menu")} aria-label={t("template.menu")}>
        <Icon name="template" size={17} />
      </button>
      {open && (
        <ul className="template-list" role="menu">
          {templates.length === 0 && <li className="empty">{t("template.none")}</li>}
          {templates.map((template) => (
            <li key={template.name} role="none">
              <button
                role="menuitem"
                className="template-item"
                onClick={() => {
                  setOpen(false);
                  onCreate(template);
                }}
              >
                <CueIcon type={template.cue.type} size={14} />
                <span className="template-name">{template.name}</span>
                <span className="template-type">{typeLabel(template.cue.type)}</span>
              </button>
              <button className="icon-button quiet" onClick={() => onDelete(template.name)} title={t("template.delete")} aria-label={`${t("template.delete")}: ${template.name}`}>
                <Icon name="close" size={11} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
