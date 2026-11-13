import { useEffect, useState } from "react";
import type { ConnectionState } from "../api/connection";
import type { AudioStatus, TimecodeIn } from "../api/types";
import { LANGUAGES, useI18n, type Language } from "../i18n";
import { FORMAT_LABELS } from "../timecode";

/** How long after the last frame incoming timecode still shows as running. */
const TIMECODE_STALE_MS = 1000;

/** Incoming timecode, dimmed once it stops coming. */
function TimecodeReading({ value }: { value: TimecodeIn & { at: number } }) {
  const { t } = useI18n();
  const [, tick] = useState(0);
  const running = performance.now() - value.at < TIMECODE_STALE_MS;
  useEffect(() => {
    if (!running) return;
    const timer = setTimeout(() => tick((n) => n + 1), TIMECODE_STALE_MS);
    return () => clearTimeout(timer);
  }, [value, running]);
  return (
    <span className={running ? "status-item timecode running" : "status-item timecode"} title={t("status.timecodeIn")}>
      TC <span className="timecode-value">{value.timecode}</span> {FORMAT_LABELS[value.format]}
    </span>
  );
}

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
  timecode,
  connection,
  onAudio,
}: {
  cueCount: number;
  listCount: number;
  selectedCount: number;
  audio: AudioStatus | null;
  timecode: (TimecodeIn & { at: number }) | null;
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
      {timecode && <TimecodeReading value={timecode} />}
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
