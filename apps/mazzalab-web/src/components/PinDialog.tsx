import { useEffect, useState } from "react";
import { useI18n } from "../i18n";
import { checkPin, makeLock, validPin, type PinLock } from "../lock";

/** Asks for the show mode PIN: to unlock editing or to remove the lock, or to choose a new one. */
export function PinDialog({
  mode,
  lock,
  onSet,
  onVerified,
  onClose,
}: {
  mode: "unlock" | "remove" | "set";
  lock: PinLock | null;
  onSet?: (lock: PinLock) => void;
  onVerified?: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [pin, setPin] = useState("");
  const [again, setAgain] = useState("");
  const [error, setError] = useState<string | null>(null);

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

  const setting = mode === "set";
  const ready = setting ? validPin(pin) && pin === again : pin !== "";
  const title = { unlock: t("lock.unlock"), remove: t("lock.remove"), set: t("lock.set") }[mode];

  const submit = async () => {
    if (!ready) return;
    if (setting) {
      onSet?.(await makeLock(pin));
      return;
    }
    if (lock && (await checkPin(pin, lock))) onVerified?.();
    else {
      setError(t("lock.wrong"));
      setPin("");
    }
  };

  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <form
        className="dialog small"
        role="dialog"
        aria-modal
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <header>
          <h1>{title}</h1>
        </header>
        <div className="dialog-body form-column">
          <label className="field">
            <span className="field-label">{t("lock.pin")}</span>
            <input
              type="password"
              inputMode="numeric"
              autoComplete="off"
              autoFocus
              className={error ? "invalid" : undefined}
              value={pin}
              onChange={(event) => {
                setPin(event.target.value);
                setError(null);
              }}
            />
          </label>
          {setting && (
            <label className="field">
              <span className="field-label">{t("lock.again")}</span>
              <input type="password" inputMode="numeric" autoComplete="off" value={again} onChange={(event) => setAgain(event.target.value)} />
            </label>
          )}
          <p className="passcode-error" role="alert">
            {error ?? (setting && pin !== "" && !validPin(pin) ? t("lock.short") : "")}
          </p>
        </div>
        <footer>
          <button type="button" onClick={onClose}>
            {t("action.cancel")}
          </button>
          <button type="submit" className="primary" disabled={!ready}>
            {t("lock.ok")}
          </button>
        </footer>
      </form>
    </div>
  );
}
