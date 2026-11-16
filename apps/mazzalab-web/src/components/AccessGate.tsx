import { useCallback, useEffect, useState, type ReactNode } from "react";
import { needsPasscode, savePasscode, type AccessReply } from "../access";
import { useShow, type ShowConnection } from "../api/connection";
import { useI18n } from "../i18n";
import { Icon } from "./icons";

/** What the page below the gate may do, and how to give another passcode. */
export interface Granted {
  access: AccessReply;
  onChangePasscode: () => void;
}

/**
 * Finds out what this client may do and, when the show wants a passcode and
 * none was accepted, asks for one before showing anything else. Looks again
 * whenever the connection drops: the passcodes may have changed.
 */
export function AccessGate({ connection, host, children }: { connection: ShowConnection; host: string; children: (granted: Granted) => ReactNode }) {
  const { t } = useI18n();
  const { connection: link, toasts } = useShow(connection);
  const [reply, setReply] = useState<AccessReply | null>(null);
  const [error, setError] = useState(false);
  const [code, setCode] = useState("");

  const refresh = useCallback(async (): Promise<AccessReply | null> => {
    try {
      const next = await connection.access();
      setReply(next);
      return next;
    } catch {
      // The server is out of reach; the connection keeps trying and we ask again.
      return null;
    }
  }, [connection]);

  useEffect(() => {
    if (link === "connecting") return;
    const timer = setTimeout(() => void refresh(), link === "closed" ? 800 : 0);
    return () => clearTimeout(timer);
  }, [link, refresh]);

  // A notice may say that a backup has taken over; ask again, so the banner goes. A backup is asked every two seconds too.
  useEffect(() => {
    if (toasts.length > 0) void refresh();
  }, [toasts.length, refresh]);
  const following = reply?.following === true;
  useEffect(() => {
    if (!following) return;
    const timer = setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
  }, [following, refresh]);

  const submit = async () => {
    connection.setPasscode(code);
    const next = await refresh();
    if (next && needsPasscode(next)) {
      setError(true);
      return;
    }
    setError(false);
    savePasscode(host, code);
    setCode("");
  };

  const change = useCallback(() => {
    savePasscode(host, null);
    connection.setPasscode(null);
    void refresh();
  }, [connection, host, refresh]);

  if (reply === null) return <div className="splash">{link === "closed" ? t("app.disconnected") : t("app.connecting")}</div>;
  if (needsPasscode(reply)) {
    return (
      <div className="passcode-screen">
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (code) void submit();
          }}
        >
          <Icon name="lock" size={20} />
          <label className="field">
            <span className="field-label">{t("passcode.title")}</span>
            <input
              type="password"
              autoFocus
              autoComplete="off"
              className={error ? "invalid" : undefined}
              value={code}
              onChange={(event) => {
                setCode(event.target.value);
                setError(false);
              }}
            />
          </label>
          <p className="passcode-error" role="alert">
            {error ? t("passcode.wrong") : ""}
          </p>
          <button type="submit" className="primary" disabled={!code}>
            {t("passcode.enter")}
          </button>
        </form>
      </div>
    );
  }
  return <>{children({ access: reply, onChangePasscode: change })}</>;
}
