import { useEffect, useState } from "react";
import { formatNumber } from "../cues";
import { useI18n } from "../i18n";

/** Asks for the first number and the step used to renumber the selection. */
export function RenumberDialog({
  count,
  suggestedStart,
  onApply,
  onClose,
}: {
  count: number;
  suggestedStart: number;
  onApply: (start: number, step: number) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [start, setStart] = useState(String(suggestedStart));
  const [step, setStep] = useState("1");
  const parse = (text: string) => Number.parseFloat(text.replace(",", "."));
  const values = [parse(start), parse(step)];
  const valid = values.every(Number.isFinite) && values[1] > 0 && values[0] >= 0;

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

  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <form
        className="dialog small"
        role="dialog"
        aria-modal
        onSubmit={(event) => {
          event.preventDefault();
          if (valid) onApply(values[0], values[1]);
        }}
      >
        <header>
          <h1>{t("renumber.title")}</h1>
        </header>
        <div className="dialog-body">
          <div className="row">
            <label className="field">
              <span className="field-label">{t("renumber.start")}</span>
              <input autoFocus inputMode="decimal" value={start} onChange={(event) => setStart(event.target.value)} onFocus={(event) => event.target.select()} />
            </label>
            <label className="field">
              <span className="field-label">{t("renumber.step")}</span>
              <input inputMode="decimal" value={step} onChange={(event) => setStep(event.target.value)} />
            </label>
          </div>
          <p className="hint">
            {valid
              ? `${count} ${t("inspector.selected")}: ${formatNumber(values[0])} … ${formatNumber(values[0] + (count - 1) * values[1])}`
              : t("renumber.invalid")}
          </p>
        </div>
        <footer>
          <button type="button" onClick={onClose}>
            {t("action.cancel")}
          </button>
          <button type="submit" className="primary" disabled={!valid}>
            {t("renumber.apply")}
          </button>
        </footer>
      </form>
    </div>
  );
}
