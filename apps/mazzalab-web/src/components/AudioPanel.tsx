import type { ShowConnection } from "../api/connection";
import type { AudioStatus } from "../api/types";
import { useI18n } from "../i18n";
import { LevelField } from "./fields";
import { Icon } from "./icons";
import { Meters } from "./Meters";

/** The audio output at a glance: device, meters and the master level. */
export function AudioPanel({
  connection,
  audio,
  masterDb,
  editable,
  onMaster,
  onOpenSettings,
}: {
  connection: ShowConnection;
  audio: AudioStatus | null;
  masterDb: number;
  editable: boolean;
  onMaster: (db: number) => void;
  onOpenSettings: () => void;
}) {
  const { t } = useI18n();
  const trouble = !audio?.running || audio.fallback;
  return (
    <section className="panel audio-panel">
      <h2>
        {t("audio.title")}
        <button className="link" onClick={onOpenSettings} title={t("settings.title")}>
          <Icon name="settings" size={14} />
        </button>
      </h2>
      {audio === null ? (
        <p className="muted">{t("audio.none")}</p>
      ) : (
        <>
          <p className={trouble ? "device trouble" : "device"} title={audio.error}>
            {trouble && <Icon name="warning" size={14} />}
            <span className="device-name">{audio.running ? audio.device_name : (audio.error ?? t("audio.none"))}</span>
            {audio.running && (
              <span className="muted">
                {(audio.sample_rate / 1000).toFixed(1)} kHz · {audio.channels} ch
                {audio.buffer_size ? ` · ${audio.buffer_size}` : ""}
              </span>
            )}
          </p>
          {audio.fallback && <p className="hint warn">{t("audio.fallback")}</p>}
          <Meters connection={connection} channels={audio.running ? audio.channels : 0} />
          <fieldset disabled={!editable} className="bare">
            <LevelField label={t("audio.master")} value={masterDb} onCommit={onMaster} />
          </fieldset>
        </>
      )}
    </section>
  );
}
