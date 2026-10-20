import { useCallback, useEffect, useMemo, useState } from "react";
import { useShow, type ShowConnection } from "./api/connection";
import type { Command, Container, Cue, CueType, Id } from "./api/types";
import { ActiveCues } from "./components/ActiveCues";
import { CueList } from "./components/CueList";
import { Inspector } from "./components/Inspector";
import { TopBar } from "./components/TopBar";
import { Transport } from "./components/Transport";
import { CUE_TYPES, findCue, flatten, needsTarget, newCue, nextNumber, typeLabel } from "./cues";
import { useI18n } from "./i18n";

export function App({ connection }: { connection: ShowConnection }) {
  const { t } = useI18n();
  const { snapshot, status, connection: connectionState, lastError } = useShow(connection);
  // Selection and the visible list are per client, so several operators can
  // work on the same show without fighting over them.
  const [listId, setListId] = useState<Id | null>(null);
  const [selection, setSelection] = useState<Id | null>(null);
  const [newType, setNewType] = useState<CueType>("audio");

  const workspace = snapshot?.workspace;
  const list = workspace?.cue_lists.find((candidate) => candidate.id === listId) ?? workspace?.cue_lists[0];
  const allCues = useMemo(() => workspace?.cue_lists.flatMap((l) => l.cues) ?? [], [workspace]);
  const rows = useMemo(() => (list ? flatten(list.cues) : []), [list]);
  const playhead = status?.playheads.find((entry) => entry.list === list?.id)?.cue ?? null;
  const running = useMemo(() => new Map((status?.running ?? []).map((run) => [run.cue, run])), [status]);
  const selectedCue = selection ? findCue(allCues, selection) : undefined;
  const standby = playhead ? findCue(allCues, playhead) : undefined;

  const run = useCallback(
    (command: Command) => {
      connection.send(command).catch(() => {
        // Errors are surfaced through `lastError`.
      });
    },
    [connection],
  );
  const go = useCallback(() => list && run({ command: "go", list: list.id }), [list, run]);
  const save = useCallback(() => {
    connection.save().catch(() => {});
  }, [connection]);

  const moveSelection = useCallback(
    (step: number) => {
      if (rows.length === 0) return;
      const index = rows.findIndex((row) => row.cue.id === selection);
      const next = index < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, index + step));
      setSelection(rows[next].cue.id);
    },
    [rows, selection],
  );

  const addCue = useCallback(() => {
    if (!list) return;
    const cue = newCue(newType, nextNumber(allCues));
    if (needsTarget(newType) && selection) cue.target = selection;
    const row = rows.find((candidate) => candidate.cue.id === selection);
    let container: Container = { kind: "list", id: list.id };
    let index: number | undefined;
    if (row) {
      const siblings: Cue[] = row.parent
        ? ((findCue(allCues, row.parent) as Extract<Cue, { type: "group" }> | undefined)?.children ?? [])
        : list.cues;
      container = row.parent ? { kind: "group", id: row.parent } : container;
      index = siblings.findIndex((sibling) => sibling.id === row.cue.id) + 1;
    }
    connection
      .send({ command: "insert_cue", container, index, cue })
      .then(() => setSelection(cue.id))
      .catch(() => {});
  }, [allCues, connection, list, newType, rows, selection]);

  const deleteSelected = useCallback(() => {
    if (!selection) return;
    const index = rows.findIndex((row) => row.cue.id === selection);
    const neighbour = rows[index + 1]?.cue.id ?? rows[index - 1]?.cue.id ?? null;
    connection
      .send({ command: "delete_cue", cue: selection })
      .then(() => setSelection(neighbour))
      .catch(() => {});
  }, [connection, rows, selection]);

  useEffect(() => {
    const isTyping = (target: EventTarget | null) =>
      target instanceof HTMLElement && target.closest("input, textarea, select") !== null;
    const onKeyDown = (event: KeyboardEvent) => {
      const typing = isTyping(event.target);
      const command = event.ctrlKey || event.metaKey;
      if (command && event.key.toLowerCase() === "s") {
        event.preventDefault();
        save();
        return;
      }
      if (typing) return;
      if (event.key === "Escape") {
        event.preventDefault();
        run({ command: "panic" });
      } else if (event.key === " ") {
        // Also stops a focused button from being clicked by the same key.
        event.preventDefault();
        if (!event.repeat) go();
      } else if (command && event.key.toLowerCase() === "z") {
        event.preventDefault();
        run({ command: event.shiftKey ? "redo" : "undo" });
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        moveSelection(1);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        moveSelection(-1);
      } else if (event.key === "Enter" && selection) {
        run({ command: "set_playhead", cue: selection });
      } else if (event.key === "Delete") {
        deleteSelected();
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === " " && !isTyping(event.target)) event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, [deleteSelected, go, moveSelection, run, save, selection]);

  useEffect(() => {
    if (!lastError) return;
    const timer = setTimeout(() => connection.clearError(), 4000);
    return () => clearTimeout(timer);
  }, [connection, lastError]);

  return (
    <div className="app">
      <TopBar
        snapshot={snapshot}
        connection={connectionState}
        onUndo={() => run({ command: "undo" })}
        onRedo={() => run({ command: "redo" })}
        onSave={save}
      />
      {!snapshot ? (
        <div className="splash">{connectionState === "closed" ? t("app.disconnected") : t("app.connecting")}</div>
      ) : (
        <main>
          <div className="left">
            <Transport
              standby={standby}
              panicking={status?.panicking ?? false}
              onGo={go}
              onStopAll={() => run({ command: "stop_all" })}
              onPauseAll={() => run({ command: "pause_all" })}
              onResumeAll={() => run({ command: "resume_all" })}
              onPanic={() => run({ command: "panic" })}
              onReset={() => run({ command: "reset" })}
            />
            {workspace && workspace.cue_lists.length > 1 && (
              <nav className="tabs">
                {workspace.cue_lists.map((candidate) => (
                  <button
                    key={candidate.id}
                    className={candidate.id === list?.id ? "active" : undefined}
                    onClick={() => setListId(candidate.id)}
                  >
                    {candidate.name}
                  </button>
                ))}
              </nav>
            )}
            <div className="toolbar">
              <select value={newType} onChange={(event) => setNewType(event.target.value as CueType)}>
                {CUE_TYPES.map((type) => (
                  <option key={type} value={type}>
                    {typeLabel(type)}
                  </option>
                ))}
              </select>
              <button onClick={addCue}>{t("action.addCue")}</button>
              <button onClick={deleteSelected} disabled={!selection}>
                {t("action.delete")}
              </button>
              <span className="spacer" />
              <button onClick={() => selection && run({ command: "set_playhead", cue: selection })} disabled={!selection}>
                {t("action.setPlayhead")}
              </button>
              <button onClick={() => selection && run({ command: "start", cue: selection })} disabled={!selection}>
                {t("action.start")}
              </button>
            </div>
            <CueList
              cues={list?.cues ?? []}
              playhead={playhead}
              selection={selection}
              running={running}
              onSelect={setSelection}
              onSetPlayhead={(cue) => run({ command: "set_playhead", cue })}
            />
          </div>
          <div className="right">
            <ActiveCues
              running={status?.running ?? []}
              cues={allCues}
              onPause={(cue) => run({ command: "pause", cue })}
              onResume={(cue) => run({ command: "resume", cue })}
              onStop={(cue) => run({ command: "stop", cue })}
            />
            <Inspector cue={selectedCue} allCues={allCues} onChange={(cue) => run({ command: "update_cue", cue })} />
          </div>
        </main>
      )}
      {lastError && (
        <div className="toast" role="alert" onClick={() => connection.clearError()}>
          {lastError}
        </div>
      )}
    </div>
  );
}
