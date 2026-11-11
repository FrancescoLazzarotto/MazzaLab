import { useEffect, useState } from "react";
import type { ConnectionState } from "../api/connection";
import type { AudioStatus } from "../api/types";
import { LANGUAGES, useI18n, type Language } from "../i18n";

function Clock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(timer);
  }, []);
  return <span className="clock">{now.toLocaleTimeString(undefined, { hour12: false })}</span>;
}

/** The quiet line at the bottom: what is in the show, what it plays on, and whether the server is there. */
export function StatusBar({
  cueCount,
  listCount,
  selectedCount,
  audio,
  connection,
  onAudio,
}: {
  cueCount: number;
  listCount: number;
  selectedCount: number;
  audio: AudioStatus | null;
  connection: ConnectionState;
  onAudio: () => void;
}) {
  const { t, language, setLanguage } = useI18n();
  const trouble = audio !== null && (!audio.running || audio.fallback);
  return (
    <footer className="status-bar">
      <span>
        {cueCount} {t("status.cues")}
        {listCount > 1 ? ` · ${listCount} ${t("status.lists")}` : ""}
        {selectedCount > 1 ? ` · ${selectedCount} ${t("status.selected")}` : ""}
      </span>
      <span className="spacer" />
      {audio && (
        <button className={trouble ? "status-item trouble" : "status-item"} onClick={onAudio} title={t("settings.audio")}>
          {audio.running
            ? `${audio.device_name} · ${audio.sample_rate / 1000} kHz · ${audio.channels} ${t("status.outputs")}`
            : (audio.error ?? t("audio.none"))}
        </button>
      )}
      <span className={`status-item connection ${connection}`}>
        <i className="dot" />
        {t(`connection.${connection}`)}
      </span>
      <select className="status-item" aria-label={t("settings.language")} value={language} onChange={(event) => setLanguage(event.target.value as Language)}>
        {LANGUAGES.map((option) => (
          <option key={option.code} value={option.code}>
            {option.label}
          </option>
        ))}
      </select>
      <Clock />
    </footer>
  );
}
