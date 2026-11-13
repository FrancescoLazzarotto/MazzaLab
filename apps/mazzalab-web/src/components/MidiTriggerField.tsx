import { useEffect, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type { MidiEvent, MidiTrigger } from "../api/types";
import { useI18n } from "../i18n";
import { Field } from "./fields";
import { Icon } from "./icons";

const KIND_NAMES: Record<MidiTrigger["kind"], string> = {
  note_on: "Note",
  note_off: "Note Off",
  control_change: "CC",
  program_change: "PC",
};

/** What a trigger waits for, in a few characters: `Note 36 · ch 10`. */
export function triggerSummary(trigger: MidiTrigger, anyChannel: string): string {
  const value = trigger.value === undefined ? "" : ` = ${trigger.value}`;
  return `${KIND_NAMES[trigger.kind]} ${trigger.number}${value} · ${trigger.channel === undefined ? anyChannel : `ch ${trigger.channel}`}`;
}

/**
 * The trigger a received message teaches: that kind of message, that number
 * and that channel, whatever the velocity or value. Letting go of a key
 * teaches nothing, so that pressing a pad learns the press.
 */
export function triggerFromEvent(event: MidiEvent): MidiTrigger | null {
  if (event.kind === "note_off") return null;
  return { kind: event.kind, channel: event.channel, number: event.number };
}

/**
 * A MIDI trigger, set by pressing the pad, key or pedal that should fire
 * it: "Learn" waits for the next message from any input.
 */
export function MidiTriggerField({
  connection,
  label,
  value,
  onChange,
}: {
  connection: ShowConnection;
  label: string;
  value: MidiTrigger | undefined;
  onChange: (trigger: MidiTrigger | undefined) => void;
}) {
  const { t } = useI18n();
  const [learning, setLearning] = useState(false);

  useEffect(() => {
    if (!learning) return;
    return connection.onMidi((event) => {
      const trigger = triggerFromEvent(event);
      if (!trigger) return;
      setLearning(false);
      onChange(trigger);
    });
  }, [connection, learning, onChange]);

  return (
    <Field label={label}>
      <span className={learning ? "midi-trigger learning" : "midi-trigger"}>
        <span className={value || learning ? "midi-trigger-value" : "midi-trigger-value unset"}>
          {learning ? t("midi.learning") : value ? triggerSummary(value, t("midi.anyChannel")) : t("midi.noTrigger")}
        </span>
        <button type="button" className={learning ? "active" : undefined} onClick={() => setLearning(!learning)}>
          {learning ? t("action.cancel") : t("midi.learn")}
        </button>
        {value && !learning && (
          <button type="button" className="icon-button quiet" title={t("action.delete")} onClick={() => onChange(undefined)}>
            <Icon name="close" size={12} />
          </button>
        )}
      </span>
    </Field>
  );
}
