import { useState } from "react";
import type { WorkspaceSettings } from "../api/types";
import { useI18n } from "../i18n";
import { stageNames, stageUrl } from "../stage";
import { Icon } from "./icons";

/** A live, silent picture of one stage, as the projector shows it. */
export function StageMonitor({ settings }: { settings: WorkspaceSettings }) {
  const { t } = useI18n();
  const names = stageNames(settings);
  const [chosen, setChosen] = useState(names[0]);
  const name = names.includes(chosen) ? chosen : names[0];
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
        <button className="icon-button quiet" title={t("stage.open")} onClick={() => window.open(stageUrl(name), `stage-${name}`, "popup,width=960,height=540")}>
          <Icon name="external" size={13} />
        </button>
      </h2>
      <div className="stage-frame">
        <iframe title={name} src={stageUrl(name, true)} />
      </div>
    </section>
  );
}
