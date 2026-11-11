import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useShow, type ShowConnection } from "./api/connection";
import type { Command, Cue, CueType, Id, WorkspaceSettings } from "./api/types";
import { ActiveCues } from "./components/ActiveCues";
import { AudioPanel } from "./components/AudioPanel";
import { CueList, type SelectMode } from "./components/CueList";
import { Icon } from "./components/icons";
import { Inspector } from "./components/Inspector";
import { RenumberDialog } from "./components/RenumberDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { Toasts } from "./components/Toasts";
import { Toolbox } from "./components/Toolbox";
import { TopBar } from "./components/TopBar";
import { Transport } from "./components/Transport";
import {
  cloneCues,
  fileStem,
  findCue,
  flatten,
  insertCommands,
  isAudioFile,
  locate,
  moveCommands,
  needsTarget,
  newCue,
  nextNumber,
  renumber,
  topLevel,
  type Drop,
} from "./cues";
import { useI18n } from "./i18n";
import { loadPreferences, savePreferences, type Preferences } from "./preferences";

export function App({ connection }: { connection: ShowConnection }) {
  const { t } = useI18n();
  const { snapshot, status, audio, toasts, connection: connectionState } = useShow(connection);
  // Selection and the visible list are per client, so several operators can
  // work on the same show without fighting over them.
  const [listId, setListId] = useState<Id | null>(null);
  /** Selected cues in the order they were picked; the last one has the focus. */
  const [picked, setPicked] = useState<Id[]>([]);
  /** Where a Shift-selection starts from. */
  const [anchor, setAnchor] = useState<Id | null>(null);
  const [preferences, setPreferencesState] = useState<Preferences>(loadPreferences);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [renumbering, setRenumbering] = useState(false);
  const [renamingList, setRenamingList] = useState<Id | null>(null);
  const clipboard = useRef<Cue[]>([]);

  const setPreferences = useCallback((next: Preferences) => {
    setPreferencesState(next);
    savePreferences(next);
  }, []);
  const editable = preferences.mode === "edit";

  const workspace = snapshot?.workspace;
  const list = workspace?.cue_lists.find((candidate) => candidate.id === listId) ?? workspace?.cue_lists[0];
  const allCues = useMemo(() => workspace?.cue_lists.flatMap((l) => l.cues) ?? [], [workspace]);
  const rows = useMemo(() => (list ? flatten(list.cues) : []), [list]);
  const playhead = status?.playheads.find((entry) => entry.list === list?.id)?.cue ?? null;
  const running = useMemo(() => new Map((status?.running ?? []).map((run) => [run.cue, run])), [status]);
  const standby = playhead ? findCue(allCues, playhead) : undefined;

  // Cues can disappear under the selection (deleted here or by someone else).
  const selected = useMemo(() => picked.filter((id) => rows.some((row) => row.cue.id === id)), [picked, rows]);
  const selection = useMemo(() => new Set(selected), [selected]);
  const selectedCues = useMemo(() => rows.filter((row) => selection.has(row.cue.id)).map((row) => row.cue), [rows, selection]);
  const focus = selected.at(-1) ?? null;

  const run = useCallback(
    (command: Command) => {
      // Failures come back as toasts.
      connection.send(command).catch(() => {});
    },
    [connection],
  );
  const batch = useCallback(
    (commands: Command[]): Promise<void> => {
      if (commands.length === 0) return Promise.resolve();
      return connection.send(commands.length === 1 ? commands[0] : { command: "batch", commands });
    },
    [connection],
  );
  const go = useCallback(() => list && run({ command: "go", list: list.id }), [list, run]);
  const save = useCallback(() => {
    connection.save().catch(() => {});
  }, [connection]);

  const select = useCallback(
    (id: Id, mode: SelectMode) => {
      if (mode === "toggle") {
        setPicked((current) => (current.includes(id) ? current.filter((other) => other !== id) : [...current, id]));
        setAnchor(id);
        return;
      }
      if (mode === "range" && anchor) {
        const from = rows.findIndex((row) => row.cue.id === anchor);
        const to = rows.findIndex((row) => row.cue.id === id);
        if (from >= 0 && to >= 0) {
          const span = rows.slice(Math.min(from, to), Math.max(from, to) + 1).map((row) => row.cue.id);
          // Keep the clicked cue last: it has the focus.
          setPicked(from <= to ? span : span.reverse());
          return;
        }
      }
      setPicked([id]);
      setAnchor(id);
      if (preferences.playheadFollowsSelection && id !== playhead) run({ command: "set_playhead", cue: id });
    },
    [anchor, playhead, preferences.playheadFollowsSelection, rows, run],
  );

  const moveSelection = useCallback(
    (step: number, extend: boolean) => {
      if (rows.length === 0) return;
      const index = rows.findIndex((row) => row.cue.id === focus);
      const next = index < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, index + step));
      select(rows[next].cue.id, extend ? "range" : "single");
    },
    [focus, rows, select],
  );

  /** Right after the focused cue, or at the end of the list. */
  const insertionPoint = useCallback((): Drop => (focus ? { where: "after", anchor: focus } : { where: "end" }), [focus]);

  const addCue = useCallback(
    (type: CueType) => {
      if (!list) return;
      const cue = newCue(type, nextNumber(allCues));
      // A fade, stop, start... aims at the cue the operator is on.
      if (needsTarget(type) && focus) cue.target = focus;
      batch(insertCommands(list.cues, list.id, [cue], insertionPoint()))
        .then(() => {
          setPicked([cue.id]);
          setAnchor(cue.id);
        })
        .catch(() => {});
    },
    [allCues, batch, focus, insertionPoint, list],
  );

  const deleteSelected = useCallback(() => {
    if (!list || selected.length === 0) return;
    const doomed = topLevel(list.cues, selection);
    const gone = new Set(doomed.flatMap((id) => flatten([findCue(list.cues, id)!]).map((row) => row.cue.id)));
    // Move on to the cue after the deleted ones, or to the last one left.
    const survivors = rows.filter((row) => !gone.has(row.cue.id));
    const last = rows.reduce((found, row, index) => (gone.has(row.cue.id) ? index : found), -1);
    const neighbour = rows.slice(last + 1).find((row) => !gone.has(row.cue.id)) ?? survivors.at(-1);
    batch(doomed.map((cue) => ({ command: "delete_cue", cue })))
      .then(() => {
        setPicked(neighbour ? [neighbour.cue.id] : []);
        setAnchor(neighbour?.cue.id ?? null);
      })
      .catch(() => {});
  }, [batch, list, rows, selected.length, selection]);

  const copy = useCallback(() => {
    if (!list) return false;
    const cues = topLevel(list.cues, selection).map((id) => findCue(list.cues, id)!);
    if (cues.length > 0) clipboard.current = structuredClone(cues);
    return cues.length > 0;
  }, [list, selection]);

  const pasteCues = useCallback(
    (source: Cue[], drop: Drop) => {
      if (!list || source.length === 0) return;
      const copies = cloneCues(source, Number(nextNumber(allCues)));
      batch(insertCommands(list.cues, list.id, copies, drop))
        .then(() => {
          setPicked(copies.map((cue) => cue.id));
          setAnchor(copies[0].id);
        })
        .catch(() => {});
    },
    [allCues, batch, list],
  );
  const paste = useCallback(() => pasteCues(clipboard.current, insertionPoint()), [insertionPoint, pasteCues]);
  const duplicate = useCallback(() => {
    if (!list) return;
    const ids = topLevel(list.cues, selection);
    if (ids.length === 0) return;
    pasteCues(
      ids.map((id) => findCue(list.cues, id)!),
      { where: "after", anchor: ids.at(-1)! },
    );
  }, [list, pasteCues, selection]);

  const groupSelected = useCallback(() => {
    if (!list) return;
    const ids = topLevel(list.cues, selection);
    if (ids.length === 0) return;
    const place = locate(list.cues, list.id, ids[0]);
    if (!place) return;
    const group = newCue("group", nextNumber(allCues));
    batch([
      { command: "insert_cue", container: place.container, index: place.index, cue: group },
      ...ids.map((cue): Command => ({ command: "move_cue", cue, container: { kind: "group", id: group.id } })),
    ])
      .then(() => {
        setPicked([group.id]);
        setAnchor(group.id);
      })
      .catch(() => {});
  }, [allCues, batch, list, selection]);

  const moveCues = useCallback(
    (dragged: Id, drop: Drop) => {
      if (!list) return;
      // Dragging a selected cue takes the whole selection along.
      const moved = selection.has(dragged) ? topLevel(list.cues, selection) : [dragged];
      batch(moveCommands(list.cues, list.id, moved, drop)).catch(() => {});
      if (!selection.has(dragged)) {
        setPicked([dragged]);
        setAnchor(dragged);
      }
    },
    [batch, list, selection],
  );

  const dropFiles = useCallback(
    async (files: File[], drop: Drop) => {
      if (!list) return;
      const audioFiles = files.filter((file) => isAudioFile(file.name));
      if (audioFiles.length < files.length) connection.toast("warning", t("drop.notAudio"));
      const cues: Cue[] = [];
      let number = Number(nextNumber(allCues));
      if (audioFiles.length > 0) {
        const names = audioFiles.length === 1 ? audioFiles[0].name : `${audioFiles.length} file`;
        connection.toast("info", `${t("drop.importing")} ${names}…`);
      }
      for (const file of audioFiles) {
        try {
          const path = await connection.uploadMedia(file);
          cues.push({ ...newCue("audio", String(number++)), type: "audio", file: path, name: fileStem(file.name) });
        } catch (error) {
          connection.toast("error", `${file.name}: ${error instanceof Error ? error.message : String(error)}`);
        }
      }
      if (cues.length === 0) return;
      batch(insertCommands(list.cues, list.id, cues, drop))
        .then(() => {
          setPicked(cues.map((cue) => cue.id));
          setAnchor(cues[0].id);
        })
        .catch(() => {});
    },
    [allCues, batch, connection, list, t],
  );

  const addList = useCallback(() => {
    const name = `${t("list.newName")} ${(workspace?.cue_lists.length ?? 0) + 1}`;
    run({ command: "add_cue_list", name });
  }, [run, t, workspace]);

  const modal = settingsOpen || renumbering;
  useEffect(() => {
    const isTyping = (target: EventTarget | null) =>
      target instanceof HTMLElement && target.closest("input, textarea, select") !== null;
    const onKeyDown = (event: KeyboardEvent) => {
      if (modal) return;
      const typing = isTyping(event.target);
      const command = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();
      if (command && key === "s") {
        event.preventDefault();
        save();
        return;
      }
      if (typing) return;
      // Transport and navigation work in both modes.
      if (event.key === "Escape") {
        event.preventDefault();
        run({ command: "panic" });
        return;
      }
      if (event.key === " ") {
        // Also stops a focused button from being clicked by the same key.
        event.preventDefault();
        if (!event.repeat) go();
        return;
      }
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        moveSelection(event.key === "ArrowDown" ? 1 : -1, event.shiftKey);
        return;
      }
      if (event.key === "Enter" && focus) {
        run({ command: "set_playhead", cue: focus });
        return;
      }
      if (!command && key === "v" && focus) {
        run({ command: "start", cue: focus });
        return;
      }
      if (command && key === "a") {
        event.preventDefault();
        setPicked(rows.map((row) => row.cue.id));
        return;
      }
      if (command && key === "c") {
        if (copy()) event.preventDefault();
        return;
      }
      if (!editable) return;
      if (command && key === "z") {
        event.preventDefault();
        run({ command: event.shiftKey ? "redo" : "undo" });
      } else if (command && key === "y") {
        event.preventDefault();
        run({ command: "redo" });
      } else if (command && key === "x") {
        if (copy()) {
          event.preventDefault();
          deleteSelected();
        }
      } else if (command && key === "v") {
        event.preventDefault();
        paste();
      } else if (command && key === "d") {
        event.preventDefault();
        duplicate();
      } else if (command && key === "g") {
        event.preventDefault();
        groupSelected();
      } else if (event.key === "Delete" || event.key === "Backspace") {
        event.preventDefault();
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
  }, [copy, deleteSelected, duplicate, editable, focus, go, groupSelected, modal, moveSelection, paste, rows, run, save]);

  // Files dropped outside the cue list would otherwise replace the page.
  useEffect(() => {
    const block = (event: DragEvent) => {
      if (event.dataTransfer?.types.includes("Files")) event.preventDefault();
    };
    window.addEventListener("dragover", block);
    window.addEventListener("drop", block);
    return () => {
      window.removeEventListener("dragover", block);
      window.removeEventListener("drop", block);
    };
  }, []);

  const updateSettings = (settings: WorkspaceSettings) => run({ command: "update_settings", settings });

  return (
    <div className={`app ${preferences.mode}-mode`}>
      <TopBar
        snapshot={snapshot}
        connection={connectionState}
        mode={preferences.mode}
        onMode={(mode) => setPreferences({ ...preferences, mode })}
        onUndo={() => run({ command: "undo" })}
        onRedo={() => run({ command: "redo" })}
        onSave={save}
        onSettings={() => setSettingsOpen(true)}
      />
      {!snapshot || !workspace ? (
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
            {editable && <Toolbox onAdd={addCue} />}
            <div className="list-bar">
              <nav className="tabs">
                {workspace.cue_lists.map((candidate) =>
                  renamingList === candidate.id ? (
                    <input
                      key={candidate.id}
                      className="tab-rename"
                      autoFocus
                      defaultValue={candidate.name}
                      onFocus={(event) => event.target.select()}
                      onBlur={(event) => {
                        const name = event.target.value.trim();
                        if (name && name !== candidate.name) run({ command: "rename_cue_list", list: candidate.id, name });
                        setRenamingList(null);
                      }}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") event.currentTarget.blur();
                        if (event.key === "Escape") setRenamingList(null);
                      }}
                    />
                  ) : (
                    <button
                      key={candidate.id}
                      className={candidate.id === list?.id ? "active" : undefined}
                      onClick={() => setListId(candidate.id)}
                      onDoubleClick={() => editable && setRenamingList(candidate.id)}
                      title={editable ? t("list.renameHint") : undefined}
                    >
                      {candidate.name}
                      <span className="count">{flatten(candidate.cues).length}</span>
                    </button>
                  ),
                )}
                {editable && (
                  <>
                    <button className="icon-button" onClick={addList} title={t("list.add")}>
                      <Icon name="plus" size={14} />
                    </button>
                    {workspace.cue_lists.length > 1 && list && (
                      <button className="icon-button" onClick={() => run({ command: "delete_cue_list", list: list.id })} title={t("list.delete")}>
                        <Icon name="trash" size={14} />
                      </button>
                    )}
                  </>
                )}
              </nav>
              {editable && (
                <div className="list-actions">
                  <span className="muted">{selected.length > 1 ? `${selected.length} ${t("inspector.selected")}` : ""}</span>
                  <button onClick={duplicate} disabled={selected.length === 0} title="Ctrl+D">
                    {t("action.duplicate")}
                  </button>
                  <button onClick={groupSelected} disabled={selected.length === 0} title="Ctrl+G">
                    {t("action.group")}
                  </button>
                  <button onClick={() => setRenumbering(true)} disabled={selected.length === 0}>
                    {t("action.renumber")}
                  </button>
                  <button onClick={deleteSelected} disabled={selected.length === 0} title="Delete">
                    {t("action.delete")}
                  </button>
                </div>
              )}
            </div>
            <CueList
              cues={list?.cues ?? []}
              allCues={allCues}
              playhead={playhead}
              selection={selection}
              running={running}
              editable={editable}
              onSelect={select}
              onSetPlayhead={(cue) => run({ command: "set_playhead", cue })}
              onMove={moveCues}
              onDropFiles={(files, drop) => void dropFiles(files, drop)}
              onEdit={(cue) => run({ command: "update_cue", cue })}
            />
            {editable && (
              <Inspector
                connection={connection}
                cues={selectedCues}
                allCues={allCues}
                settings={workspace.settings}
                audio={audio}
                running={running}
                editable={editable}
                onChange={(cue) => run({ command: "update_cue", cue })}
                onChangeMany={(cues) => void batch(cues.map((cue) => ({ command: "update_cue", cue }))).catch(() => {})}
              />
            )}
          </div>
          <aside className="right">
            <AudioPanel
              connection={connection}
              audio={audio}
              masterDb={workspace.settings.audio?.master_db ?? 0}
              editable={editable}
              onMaster={(master_db) => updateSettings({ ...workspace.settings, audio: { ...workspace.settings.audio, master_db } })}
              onOpenSettings={() => setSettingsOpen(true)}
            />
            <ActiveCues
              running={status?.running ?? []}
              cues={allCues}
              onPause={(cue) => run({ command: "pause", cue })}
              onResume={(cue) => run({ command: "resume", cue })}
              onStop={(cue) => run({ command: "stop", cue })}
            />
          </aside>
        </main>
      )}
      {settingsOpen && workspace && (
        <SettingsDialog
          connection={connection}
          workspace={workspace}
          preferences={preferences}
          onPreferences={setPreferences}
          onSettings={updateSettings}
          onRename={(name) => run({ command: "rename_workspace", name })}
          onClose={() => setSettingsOpen(false)}
        />
      )}
      {renumbering && list && (
        <RenumberDialog
          count={selectedCues.length}
          suggestedStart={Number.parseFloat(selectedCues[0]?.number ?? "") || 1}
          onClose={() => setRenumbering(false)}
          onApply={(start, step) => {
            setRenumbering(false);
            run(
              renumber(
                selectedCues.map((cue) => cue.id),
                start,
                step,
              ),
            );
          }}
        />
      )}
      <Toasts connection={connection} toasts={toasts} />
    </div>
  );
}
