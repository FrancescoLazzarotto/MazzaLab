import { useEffect } from "react";
import type { CsvImport } from "../cueExport";
import { useI18n } from "../i18n";

/** Shows what a CSV file would add, before it does. */
export function ImportDialog({
  fileName,
  listName,
  result,
  onApply,
  onClose,
}: {
  fileName: string;
  listName: string;
  result: CsvImport;
  onApply: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
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

  const count = result.cues.length;
  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <form
        className="dialog small"
        role="dialog"
        aria-modal
        onSubmit={(event) => {
          event.preventDefault();
          if (count > 0) onApply();
        }}
      >
        <header>
          <h1>{t("import.title")}</h1>
        </header>
        <div className="dialog-body">
          <dl className="import-summary">
            <dt>{fileName}</dt>
            <dd className="import-count">
              {count} {t("status.cues")} → {listName}
            </dd>
            {result.read.length > 0 && (
              <>
                <dt>{t("import.read")}</dt>
                <dd>{result.read.join(", ")}</dd>
              </>
            )}
            {result.ignored.length > 0 && (
              <>
                <dt>{t("import.ignored")}</dt>
                <dd>{result.ignored.join(", ")}</dd>
              </>
            )}
          </dl>
        </div>
        <footer>
          <button type="button" onClick={onClose}>
            {t("action.cancel")}
          </button>
          <button type="submit" className="primary" autoFocus disabled={count === 0}>
            {t("import.add")}
          </button>
        </footer>
      </form>
    </div>
  );
}
