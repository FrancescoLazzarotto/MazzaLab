import { useEffect, useRef, useState } from "react";
import type { Cue, Id } from "../api/types";
import { newlyAdded } from "../a11y";
import { displayName } from "../cues";
import { useI18n } from "../i18n";

const named = (cue: Cue) => `${cue.number ? `${cue.number} ` : ""}${displayName(cue)}`;

/** Says, for screen readers, what GO started and which cue is in standby now. Not on screen. */
export function Announcer({ standby, running, find }: { standby: Cue | undefined; running: Id[]; find: (id: Id) => Cue | undefined }) {
  const { t } = useI18n();
  const [message, setMessage] = useState("");
  const before = useRef<Id[]>(running);
  const standbyId = useRef<Id | undefined>(standby?.id);
  const key = running.join(",");

  useEffect(() => {
    const started = newlyAdded(before.current, running)
      .map((id) => find(id))
      .filter((cue): cue is Cue => cue !== undefined);
    before.current = running;
    const parts: string[] = [];
    if (started.length > 0) parts.push(`${t("announce.started")} ${started.map(named).join(", ")}`);
    if (standby?.id !== standbyId.current) parts.push(standby ? `${t("standby.title")}: ${named(standby)}` : `${t("standby.title")}: ${t("standby.end")}`);
    standbyId.current = standby?.id;
    if (parts.length > 0) setMessage(parts.join(". "));
    // `running` is compared by its ids.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, standby?.id]);

  return (
    <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
      {message}
    </div>
  );
}
