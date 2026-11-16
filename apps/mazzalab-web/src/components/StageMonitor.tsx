import { useState } from "react";
import type { ShowConnection } from "../api/connection";
import type { WorkspaceSettings } from "../api/types";
import { useI18n } from "../i18n";
import { stageNames, stageSize, stageUrl } from "../stage";
import { useScreens } from "../useScreens";
import { Icon } from "./icons";

/** A live, silent picture of one stage, as the projector shows it. */
export function StageMonitor({ connection, settings }: { connection: ShowConnection; settings: WorkspaceSettings }) {
  const { t } = useI18n();
  const { screens, setShown } = useScreens(connection);
  // Only the desktop app has screens, and only stages given one go on them.
  const placed = screens !== null && (settings.stages ?? []).some((stage) => stage.screen);
  const names = stageNames(settings);
  const [chosen, setChosen] = useState(names[0]);
  const name = names.includes(chosen) ? chosen : names[0];
  const size = stageSize(settings.stages, name);
  return (
    <section className="panel stage-monitor">
      <h2>
        {t("stage.monitor")}
        {names.length > 1 ? (
          <select className="stage-select" value={name} aria-label={t("inspector.stage")} onChange={(event) => setChosen(event.target.value)}>
            {names.map((stage) => (
              <option key={stage}>{stage}</option>
            ))}
          </select>
        ) : (
          <span className="count">{name}</span>
        )}
        {placed && (
          <button
            className={screens.shown ? "icon-button quiet toggled" : "icon-button quiet"}
            title={t(screens.shown ? "stage.hideFromScreens" : "stage.showOnScreens")}
            aria-pressed={screens.shown}
            onClick={() => setShown(!screens.shown)}
          >
            <Icon name="screens" size={14} />
          </button>
        )}
        <button className="icon-button quiet" title={t("stage.open")} onClick={() => window.open(stageUrl(name), `stage-${name}`, "popup,width=960,height=540")}>
          <Icon name="external" size={13} />
        </button>
      </h2>
      <div className="stage-frame" style={{ aspectRatio: `${size.width} / ${size.height}` }}>
        <iframe title={name} src={stageUrl(name, true)} />
      </div>
    </section>
  );
}
