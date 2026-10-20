import { useEffect, useState, type ReactNode } from "react";
import { formatTime, parseTime } from "../format";

/** Shared behaviour: edit locally, commit on Enter or blur, revert on Escape. */
function useDraft(value: string): [string, (next: string) => void, () => void] {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return [draft, setDraft, () => setDraft(value)];
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
    </label>
  );
}

export function TextField({
  label,
  value,
  onCommit,
  multiline = false,
  placeholder,
}: {
  label: string;
  value: string;
  onCommit: (value: string) => void;
  multiline?: boolean;
  placeholder?: string;
}) {
  const [draft, setDraft, revert] = useDraft(value);
  const commit = () => {
    if (draft !== value) onCommit(draft);
  };
  const common = {
    value: draft,
    placeholder,
    onChange: (event: { target: { value: string } }) => setDraft(event.target.value),
    onBlur: commit,
  };
  return (
    <Field label={label}>
      {multiline ? (
        <textarea
          {...common}
          rows={3}
          onKeyDown={(event) => {
            if (event.key === "Escape") revert();
          }}
        />
      ) : (
        <input
          {...common}
          onKeyDown={(event) => {
            if (event.key === "Enter") commit();
            if (event.key === "Escape") revert();
          }}
        />
      )}
    </Field>
  );
}

export function TimeField({
  label,
  value,
  onCommit,
  optional = false,
}: {
  label: string;
  value: number | undefined;
  onCommit: (value: number | undefined) => void;
  optional?: boolean;
}) {
  const shown = value === undefined ? "" : formatTime(value);
  const [draft, setDraft, revert] = useDraft(shown);
  const [invalid, setInvalid] = useState(false);
  const commit = () => {
    if (draft === shown) return;
    if (draft.trim() === "" && optional) {
      setInvalid(false);
      onCommit(undefined);
      return;
    }
    const parsed = parseTime(draft);
    setInvalid(parsed === null);
    if (parsed !== null) onCommit(parsed);
  };
  return (
    <Field label={label}>
      <input
        className={invalid ? "invalid" : undefined}
        value={draft}
        placeholder={optional ? "—" : "0:00.00"}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit();
          if (event.key === "Escape") {
            setInvalid(false);
            revert();
          }
        }}
      />
    </Field>
  );
}

export function NumberField({
  label,
  value,
  onCommit,
  step = 1,
  min,
}: {
  label: string;
  value: number;
  onCommit: (value: number) => void;
  step?: number;
  min?: number;
}) {
  const [draft, setDraft, revert] = useDraft(String(value));
  const commit = () => {
    const parsed = Number.parseFloat(draft.replace(",", "."));
    if (Number.isFinite(parsed) && parsed !== value) onCommit(min === undefined ? parsed : Math.max(min, parsed));
    else revert();
  };
  return (
    <Field label={label}>
      <input
        type="number"
        step={step}
        min={min}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit();
          if (event.key === "Escape") revert();
        }}
      />
    </Field>
  );
}

export function SelectField<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <Field label={label}>
      <select value={value} onChange={(event) => onChange(event.target.value as T)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </Field>
  );
}

export function CheckField({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="check">
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
      {label}
    </label>
  );
}
