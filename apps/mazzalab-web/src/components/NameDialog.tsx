import { useEffect, useState } from "react";
import { useI18n } from "../i18n";

/** Asks for one short name. */
export function NameDialog({ title, label, initial, verb, onApply, onClose }: { title: string; label: string; initial: string; verb: string; onApply: (name: string) => void; onClose: () => void }) {
  const { t } = useI18n();
  const [name, setName] = useState(initial);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);
  const valid = name.trim() !== "";
  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <form
        className="dialog small"
        role="dialog"
        aria-modal
        onSubmit={(event) => {
          event.preventDefault();
          if (valid) onApply(name.trim());
        }}
      >
        <header>
          <h1>{title}</h1>
        </header>
        <div className="dialog-body">
          <label className="field">
            <span className="field-label">{label}</span>
            <input autoFocus value={name} onChange={(event) => setName(event.target.value)} onFocus={(event) => event.target.select()} />
          </label>
        </div>
        <footer>
          <button type="button" onClick={onClose}>
            {t("action.cancel")}
          </button>
          <button type="submit" className="primary" disabled={!valid}>
            {verb}
          </button>
        </footer>
      </form>
    </div>
  );
}
