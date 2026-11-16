import type { ShowConnection } from "../api/connection";
import type { Cue, Triggers, Weekday } from "../api/types";
import { useI18n } from "../i18n";
import { hotkeyOf, isReserved } from "../hotkeys";
import { normalizeTimecode } from "../timecode";
import { normalizeClockTime, toggleWeekday, WEEKDAYS } from "../triggers";
import { HotkeyField, TextField } from "./fields";
import { MidiTriggerField } from "./MidiTriggerField";

/** Everything that can start a cue besides GO: keys, MIDI, timecode, OSC and the time of day. */
export function TriggerFields({ connection, cue, set }: { connection: ShowConnection; cue: Cue; set: (patch: { triggers: Triggers | undefined }) => void }) {
  const { t } = useI18n();
  /** Triggers with a change applied, or nothing at all when none is left. */
  const triggers = (change: Partial<Triggers>) => {
    const next: Triggers = { ...cue.triggers, ...change };
    const kept = Object.fromEntries(Object.entries(next).filter(([, value]) => value !== undefined)) as Triggers;
    return Object.keys(kept).length > 0 ? kept : undefined;
  };
  const clock = cue.triggers?.wall_clock;
  const days = clock?.days ?? [];
  const toggleDay = (day: Weekday) => clock && set({ triggers: triggers({ wall_clock: { ...clock, days: toggleWeekday(clock.days, day) } }) });
  return (
    <div className="form-columns trigger-fields">
      <div className="form-column">
        <HotkeyField
          label={t("inspector.hotkey")}
          value={cue.triggers?.hotkey}
          placeholder={t("inspector.hotkeyNone")}
          nameOf={hotkeyOf}
          refuse={(hotkey) => (isReserved(hotkey) ? t("inspector.hotkeyReserved") : null)}
          onCommit={(hotkey) => set({ triggers: triggers({ hotkey }) })}
        />
        <MidiTriggerField connection={connection} label={t("inspector.midiTrigger")} value={cue.triggers?.midi} onChange={(midi) => set({ triggers: triggers({ midi }) })} />
      </div>
      <div className="form-column">
        <TextField
          label={t("inspector.timecodeTrigger")}
          value={cue.triggers?.timecode ?? ""}
          placeholder={t("inspector.timecodeNone")}
          onCommit={(text) => {
            const timecode = text.trim() === "" ? undefined : normalizeTimecode(text);
            if (timecode === null) connection.toast("error", t("inspector.badTimecode"));
            else set({ triggers: triggers({ timecode }) });
          }}
        />
        <TextField
          label={t("trigger.osc")}
          value={cue.triggers?.osc ?? ""}
          placeholder="/scene/1/go"
          onCommit={(text) => {
            const address = text.trim();
            if (address !== "" && !address.startsWith("/")) connection.toast("error", t("trigger.badOsc"));
            else set({ triggers: triggers({ osc: address || undefined }) });
          }}
        />
        <p className="hint">{t("trigger.oscHint")}</p>
      </div>
      <div className="form-column">
        <TextField
          label={t("trigger.wallClock")}
          value={clock?.time ?? ""}
          placeholder={t("trigger.wallClockNone")}
          onCommit={(text) => {
            if (text.trim() === "") return set({ triggers: triggers({ wall_clock: undefined }) });
            const time = normalizeClockTime(text);
            if (time === null) connection.toast("error", t("trigger.badTime"));
            else set({ triggers: triggers({ wall_clock: { ...clock, time } }) });
          }}
        />
        {clock && (
          <div className="field">
            <span className="field-label">{t("trigger.days")}</span>
            <div className="weekdays" role="group" aria-label={t("trigger.days")}>
              {WEEKDAYS.map((day) => (
                <button key={day} className={days.length === 0 || days.includes(day) ? "active" : undefined} aria-pressed={days.length === 0 || days.includes(day)} title={t(`day.${day}`)} onClick={() => toggleDay(day)}>
                  {t(`day.${day}`).slice(0, 2)}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
