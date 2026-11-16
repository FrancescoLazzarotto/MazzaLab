import { useEffect } from "react";
import type { Cue } from "../api/types";
import { sheetRows } from "../cueExport";
import { useI18n } from "../i18n";

/**
 * The cue list as a plain table for paper. It is not on screen; the print
 * stylesheet swaps the app for it while the browser's print dialog is open.
 */
export function PrintSheet({
  show,
  listName,
  cues,
  allCues,
  onDone,
}: {
  show: string;
  listName: string;
  cues: Cue[];
  allCues: Cue[];
  onDone: () => void;
}) {
  const { t } = useI18n();
  useEffect(() => {
    window.addEventListener("afterprint", onDone, { once: true });
    // After the sheet has been laid out.
    const timer = setTimeout(() => window.print(), 50);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("afterprint", onDone);
    };
  }, [onDone]);

  const headers = [t("list.number"), t("list.name"), t("sheet.type"), t("list.target"), t("list.preWait"), t("list.duration"), t("list.postWait"), t("sheet.continue"), t("sheet.notes")];
  return (
    <section className="print-sheet">
      <header>
        <h1>{show}</h1>
        <span>
          {listName} · {new Date().toLocaleDateString()}
        </span>
      </header>
      <table>
        <thead>
          <tr>
            {headers.map((header) => (
              <th key={header}>{header}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sheetRows(cues, allCues).map(({ cue, depth, cells }) => (
            <tr key={cue.id} className={cue.flagged ? "flagged" : undefined}>
              {cells.slice(0, 9).map((cell, index) => (
                <td key={index} className={index >= 4 && index <= 6 ? "time" : undefined} style={index === 1 && depth > 0 ? { paddingLeft: `${depth * 12}px` } : undefined}>
                  {index === 7 ? (cell === "do_not_continue" ? "" : t(`continue.${cell as "auto_continue"}`)) : cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
