import type { Passcode } from "../api/types";
import { effectiveView, newPasscode, someoneCanEdit } from "../access";
import { useI18n } from "../i18n";
import { TextField } from "./fields";
import { Icon } from "./icons";

/** Who may do what from another machine. The machine the server runs on always may do everything. */
export function PasscodesTab({ passcodes, onChange }: { passcodes: Passcode[]; onChange: (passcodes: Passcode[]) => void }) {
  const { t } = useI18n();
  const change = (index: number, patch: Partial<Passcode>) => onChange(passcodes.map((passcode, at) => (at === index ? { ...passcode, ...patch } : passcode)));
  return (
    <div className="form-column passcodes">
      {passcodes.map((passcode, index) => {
        const implied = passcode.control || passcode.edit;
        return (
          <div className="passcode-row" key={index}>
            <TextField label={t("passcodes.code")} value={passcode.code} onCommit={(code) => code.trim() && change(index, { code: code.trim() })} />
            <TextField label={t("passcodes.name")} value={passcode.name ?? ""} onCommit={(name) => change(index, { name })} />
            <label className="check">
              <input type="checkbox" checked={effectiveView(passcode)} disabled={implied} onChange={(event) => change(index, { view: event.target.checked })} />
              {t("access.view")}
            </label>
            <label className="check">
              <input type="checkbox" checked={passcode.control} onChange={(event) => change(index, { control: event.target.checked })} />
              {t("access.control")}
            </label>
            <label className="check">
              <input type="checkbox" checked={passcode.edit} onChange={(event) => change(index, { edit: event.target.checked })} />
              {t("access.edit")}
            </label>
            <button className="icon-button quiet" onClick={() => onChange(passcodes.filter((_, at) => at !== index))} title={t("passcodes.remove")}>
              <Icon name="trash" size={13} />
            </button>
          </div>
        );
      })}
      <div>
        <button onClick={() => onChange([...passcodes, newPasscode(passcodes)])}>
          <Icon name="plus" size={13} />
          {t("passcodes.add")}
        </button>
      </div>
      {passcodes.length > 0 && <p className="hint warn">{t("passcodes.lockout")}</p>}
      {passcodes.length > 0 && !someoneCanEdit(passcodes) && <p className="hint">{t("passcodes.noEditor")}</p>}
    </div>
  );
}
