import { useEffect } from "react";
import type { ShowConnection, Toast } from "../api/connection";

const LIFETIME_MS = { info: 4000, warning: 8000, error: 10000 } as const;

function ToastItem({ toast, onDismiss }: { toast: Toast; onDismiss: () => void }) {
  useEffect(() => {
    const timer = setTimeout(onDismiss, LIFETIME_MS[toast.level]);
    return () => clearTimeout(timer);
    // The timer belongs to the toast, not to the callback's identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [toast.id]);
  return (
    <div className={`toast ${toast.level}`} role={toast.level === "info" ? "status" : "alert"} onClick={onDismiss}>
      {toast.message}
    </div>
  );
}

/** Messages from the server and failed requests, stacked in a corner. */
export function Toasts({ connection, toasts }: { connection: ShowConnection; toasts: Toast[] }) {
  if (toasts.length === 0) return null;
  return (
    <div className="toasts">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} onDismiss={() => connection.dismissToast(toast.id)} />
      ))}
    </div>
  );
}
