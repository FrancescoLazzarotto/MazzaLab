import type { CSSProperties } from "react";
import type { DmxLevel } from "../api/types";
import { useI18n } from "../i18n";

/** How many lit slots are drawn before the rest is summed up. */
const SHOWN = 96;

/** `7` in universe 1, `2/7` elsewhere, the way light cues name slots. */
export function slotName(universe: number, address: number): string {
  return universe === 1 ? String(address) : `${universe}/${address}`;
}

/** A DMX value as the percentage an operator would type. */
export function percent(value: number): number {
  return Math.round((value / 255) * 100);
}

/** The DMX slots that are lit right now, each with its level. */
export function DmxPanel({ levels }: { levels: DmxLevel[] }) {
  const { t } = useI18n();
  return (
    <section className="panel dmx-panel">
      <h2>
        {t("dmx.title")}
        {levels.length > 0 && <span className="count">{levels.length}</span>}
      </h2>
      {levels.length === 0 ? (
        <p className="placeholder">{t("dmx.dark")}</p>
      ) : (
        <div className="dmx-grid">
          {levels.slice(0, SHOWN).map(([universe, address, value]) => (
            <span
              key={`${universe}/${address}`}
              className="dmx-slot"
              style={{ "--level": `${(value / 255) * 100}%` } as CSSProperties}
              title={`${slotName(universe, address)} @ ${percent(value)}%`}
            >
              <span className="dmx-address">{slotName(universe, address)}</span>
              <span className="dmx-value">{percent(value)}</span>
            </span>
          ))}
          {levels.length > SHOWN && <span className="dmx-more">+{levels.length - SHOWN}</span>}
        </div>
      )}
    </section>
  );
}
