import { useEffect, useState } from "react";
import type { AudioCue, Matrix } from "../api/types";
import { matrixFor } from "../cues";
import { useI18n } from "../i18n";

function Cell({ value, onCommit }: { value: number | null; onCommit: (value: number | null) => void }) {
  const shown = value === null ? "" : String(value);
  const [draft, setDraft] = useState(shown);
  useEffect(() => setDraft(shown), [shown]);
  const commit = () => {
    const text = draft.trim().replace(",", ".");
    if (text === shown) return;
    if (text === "") return onCommit(null);
    const parsed = Number.parseFloat(text);
    if (Number.isFinite(parsed)) onCommit(Math.min(12, parsed));
    else setDraft(shown);
  };
  return (
    <input
      className={value === null ? "matrix-cell" : "matrix-cell on"}
      inputMode="decimal"
      value={draft}
      placeholder="·"
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onFocus={(event) => event.target.select()}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
        if (event.key === "Escape") setDraft(shown);
      }}
    />
  );
}

/**
 * The cue's routing: one row per channel of the file, one column per output
 * of the device, a level in dB at each crosspoint (empty is silent).
 */
export function MatrixEditor({
  cue,
  outputs,
  onChange,
}: {
  cue: AudioCue;
  /** Output channels of the audio device in use. */
  outputs: number;
  onChange: (matrix: Matrix | undefined) => void;
}) {
  const { t } = useI18n();
  const inputs = Math.max(1, cue.media_channels ?? cue.matrix?.length ?? 2);
  // A show patched for more outputs than this machine has keeps its columns.
  const widest = Math.max(0, ...(cue.matrix ?? []).map((row) => row.length));
  const columns = Math.min(64, Math.max(2, outputs, widest));
  const matrix = matrixFor(cue.matrix, inputs, columns);

  const set = (input: number, output: number, value: number | null) => {
    const next = matrix.map((row) => [...row]);
    next[input][output] = value;
    onChange(next);
  };

  return (
    <div className="matrix">
      <div className="matrix-scroll">
        <table>
          <thead>
            <tr>
              <th />
              {Array.from({ length: columns }, (_, output) => (
                <th key={output} className={output >= outputs ? "absent" : undefined} title={output >= outputs ? t("matrix.absent") : undefined}>
                  {output + 1}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {matrix.map((row, input) => (
              <tr key={input}>
                <th>{inputs === 2 ? ["L", "R"][input] : input + 1}</th>
                {row.map((value, output) => (
                  <td key={output}>
                    <Cell value={value} onCommit={(next) => set(input, output, next)} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="matrix-foot">
        <span className="muted">{cue.matrix ? t("matrix.custom") : t("matrix.default")}</span>
        <button disabled={!cue.matrix} onClick={() => onChange(undefined)}>
          {t("matrix.reset")}
        </button>
      </div>
    </div>
  );
}
