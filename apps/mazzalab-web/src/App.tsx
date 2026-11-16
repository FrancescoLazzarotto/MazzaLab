import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useShow, type ShowConnection } from "./api/connection";
import type { Command, Cue, CueList as CueListData, CueTemplate, CueType, Id, WorkspaceSettings } from "./api/types";
import { ActiveCues } from "./components/ActiveCues";
import { AudioPanel } from "./components/AudioPanel";
import { CueCart } from "./components/CueCart";
import { CueList, type SelectMode } from "./components/CueList";
import { FileDialog, type FileMode } from "./components/FileDialog";
import { FilterBar } from "./components/FilterBar";
import { ImportDialog } from "./components/ImportDialog";
import { PrintSheet } from "./components/PrintSheet";
import { buildCsv, parseCueCsv, type CsvImport } from "./cueExport";
import { downloadText, safeFileName } from "./download";
import { DmxPanel } from "./components/DmxPanel";
import { StageMonitor } from "./components/StageMonitor";
import { slotLabels } from "./fixtures";
import { Icon } from "./components/icons";
import { Inspector } from "./components/Inspector";
import { RenumberDialog } from "./components/RenumberDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { StatusBar } from "./components/StatusBar";
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
  typeLabel,
  newCue,
  nextNumber,
  renumber,
  topLevel,
  type Drop,
} from "./cues";
import { NO_FILTER, visibleIds, type CueFilter } from "./cueFilter";
import { cuesForHotkey, hotkeyOf } from "./hotkeys";
import { actionFor } from "./keymap";
import { useTitleLabels } from "./a11y";
import { MediaPanel } from "./components/MediaPanel";
import { DiagnosticsDialog } from "./components/DiagnosticsDialog";
import { NameDialog } from "./components/NameDialog";
import { PinDialog } from "./components/PinDialog";
import { TemplateMenu } from "./components/TemplateMenu";
import { cueFromTemplate, makeTemplate, suggestedName, withTemplate, withoutTemplate } from "./templates";
import { PreflightDialog } from "./components/PreflightDialog";
import { mediaKind } from "./mediaLibrary";
import { NO_OTHERS, applyOthersEvents, type OthersState } from "./others";
import { effectiveMode, type AccessReply } from "./access";
import { Announcer } from "./components/Announcer";
import { resolveSplit, showInPane, toggleSplit } from "./panes";
import { applyTheme } from "./theme";
import { useI18n } from "./i18n";
import { loadPreferences, savePreferences, type Preferences } from "./preferences";

const NO_SELECTION: ReadonlySet<Id> = new Set();

export function App({ connection, access, onChangePasscode }: { connection: ShowConnection; access: AccessReply; onChangePasscode: () => void }) {
  const { t } = useI18n();
  const { snapshot, status, audio, dmx, timecode, toasts, connection: connectionState } = useShow(connection);
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
  const [printing, setPrinting] = useState(false);
  const [fileDialog, setFileDialog] = useState<FileMode | null>(null);
  const [checking, setChecking] = useState(false);
  const [namingTemplate, setNamingTemplate] = useState(false);
  const [unlocking, setUnlocking] = useState(false);
  const [diagnosing, setDiagnosing] = useState(false);
  const [others, setOthers] = useState<OthersState>(NO_OTHERS);
  useEffect(() => connection.onEvents((events) => setOthers((current) => applyOthersEvents(current, events))), [connection]);
  const [importing, setImporting] = useState<{ fileName: string; result: CsvImport } | null>(null);
  const importInput = useRef<HTMLInputElement>(null);
  const [renamingList, setRenamingList] = useState<Id | null>(null);
  const [pendingList, setPendingList] = useState<string | null>(null);
  const clipboard = useRef<Cue[]>([]);

  const setPreferences = useCallback((next: Preferences) => {
    setPreferencesState(next);
    savePreferences(next);
  }, []);
  // What the passcode allows: without edit the device is in show mode, without control nothing is run from it.
  const mode = effectiveMode(preferences.mode, access);
  const editable = mode === "edit";
  const canControl = access.control;
  useEffect(() => applyTheme(preferences.theme), [preferences.theme]);
  useTitleLabels();

  const workspace = snapshot?.workspace;
  // Two panes, when the device asks for them and the show has two lists; the one worked in is the active list.
  const panes = resolveSplit(preferences.split, workspace?.cue_lists.map((candidate) => candidate.id) ?? []);
  const [activePane, setActivePane] = useState<0 | 1>(0);
  const list = panes
    ? workspace?.cue_lists.find((candidate) => candidate.id === panes[activePane])
    : (workspace?.cue_lists.find((candidate) => candidate.id === listId) ?? workspace?.cue_lists[0]);
  const selectList = useCallback(
    (id: Id) => {
      if (panes) setPreferences({ ...preferences, split: { on: true, ids: showInPane(panes, activePane, id) } });
      else setListId(id);
    },
    [activePane, panes, preferences, setPreferences],
  );
  const allCues = useMemo(() => workspace?.cue_lists.flatMap((l) => l.cues) ?? [], [workspace]);
  const rows = useMemo(() => (list ? flatten(list.cues) : []), [list]);
  // The filter only changes what is shown; GO and the playhead follow the real list.
  const [filter, setFilter] = useState<CueFilter>(NO_FILTER);
  const filterInput = useRef<HTMLInputElement>(null);
  // Another show was opened under us: nothing of the old one may linger.
  const workspaceId = workspace?.id;
  const shownWorkspace = useRef(workspaceId);
  useEffect(() => {
    if (shownWorkspace.current === workspaceId) return;
    shownWorkspace.current = workspaceId;
    setListId(null);
    setPicked([]);
    setAnchor(null);
    setFilter(NO_FILTER);
    setImporting(null);
    setRenamingList(null);
    setPendingList(null);
    setRenumbering(false);
    clipboard.current = [];
    setOthers(NO_OTHERS);
  }, [workspaceId]);
  const visible = useMemo(() => (list ? visibleIds(list.cues, filter) : null), [list, filter]);
  const shownRows = useMemo(() => (visible ? rows.filter((row) => visible.has(row.cue.id)) : rows), [rows, visible]);
  const playhead = status?.playheads.find((entry) => entry.list === list?.id)?.cue ?? null;
  const running = useMemo(() => new Map((status?.running ?? []).map((run) => [run.cue, run])), [status]);
  const loaded = useMemo(() => new Map((status?.loaded ?? []).map((entry) => [entry.cue, entry.time])), [status]);
  const standby = playhead ? findCue(allCues, playhead) : undefined;

  // Cues can disappear under the selection (deleted here or by someone else).
  const selected = useMemo(() => picked.filter((id) => shownRows.some((row) => row.cue.id === id)), [picked, shownRows]);
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
        const from = shownRows.findIndex((row) => row.cue.id === anchor);
        const to = shownRows.findIndex((row) => row.cue.id === id);
        if (from >= 0 && to >= 0) {
          const span = shownRows.slice(Math.min(from, to), Math.max(from, to) + 1).map((row) => row.cue.id);
          // Keep the clicked cue last: it has the focus.
          setPicked(from <= to ? span : span.reverse());
          return;
        }
      }
      setPicked([id]);
      setAnchor(id);
      if (canControl && preferences.playheadFollowsSelection && id !== playhead) run({ command: "set_playhead", cue: id });
    },
    [anchor, canControl, playhead, preferences.playheadFollowsSelection, run, shownRows],
  );

  const moveSelection = useCallback(
    (step: number, extend: boolean) => {
      if (shownRows.length === 0) return;
      const index = shownRows.findIndex((row) => row.cue.id === focus);
      const next = index < 0 ? 0 : Math.min(shownRows.length - 1, Math.max(0, index + step));
      select(shownRows[next].cue.id, extend ? "range" : "single");
    },
    [focus, select, shownRows],
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

  const createFromTemplate = useCallback(
    (template: CueTemplate) => {
      if (!list) return;
      const cue = cueFromTemplate(template, nextNumber(allCues));
      batch(insertCommands(list.cues, list.id, [cue], insertionPoint()))
        .then(() => {
          setPicked([cue.id]);
          setAnchor(cue.id);
        })
        .catch(() => {});
    },
    [allCues, batch, insertionPoint, list],
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

  /** Cues for files of the media folder: Audio or Video by what they are. */
  const dropMedia = useCallback(
    (paths: string[], drop: Drop) => {
      if (!list) return;
      let number = Number(nextNumber(allCues));
      const cues: Cue[] = [];
      for (const path of paths) {
        const kind = mediaKind(path);
        if (kind === "other") {
          connection.toast("warning", t("drop.notAudio"));
          continue;
        }
        cues.push({ ...newCue(kind, String(number++)), file: path, name: fileStem(path) } as Cue);
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

  const addList = useCallback(
    (cart: boolean) => {
      const lists = workspace?.cue_lists ?? [];
      const count = lists.filter((existing) => Boolean(existing.cart) === cart).length;
      const name = `${t(cart ? "list.newCart" : "list.newName")} ${count + 1}`;
      // Show the new list once the server has added it.
      connection
        .send({ command: "add_cue_list", name, cart })
        .then(() => setListId(null))
        .then(() => setPendingList(name))
        .catch(() => {});
    },
    [connection, t, workspace],
  );
  // A list just added by this client is the one to look at.
  useEffect(() => {
    if (!pendingList || !workspace) return;
    const added = [...workspace.cue_lists].reverse().find((candidate) => candidate.name === pendingList);
    if (added) {
      selectList(added.id);
      setPendingList(null);
    }
  }, [pendingList, workspace]);
  const toggleCue = useCallback((cue: Id) => canControl && run({ command: running.has(cue) ? "stop" : "start", cue }), [canControl, run, running]);

  const exportCsv = useCallback(() => {
    if (!list || !workspace) return;
    downloadText(`${safeFileName(`${workspace.name} - ${list.name}`)}.csv`, buildCsv(list.cues, allCues));
  }, [allCues, list, workspace]);
  const chooseImport = useCallback(
    (file: File) => {
      file
        .text()
        .then((text) => setImporting({ fileName: file.name, result: parseCueCsv(text) }))
        .catch(() => {});
    },
    [],
  );
  const applyImport = useCallback(() => {
    if (!list || !importing) return;
    const cues = importing.result.cues;
    setImporting(null);
    batch(insertCommands(list.cues, list.id, cues, insertionPoint()))
      .then(() => {
        setPicked(cues.map((cue) => cue.id));
        setAnchor(cues[0]?.id ?? null);
      })
      .catch(() => {});
  }, [batch, importing, insertionPoint, list]);

  const modal = settingsOpen || renumbering || importing !== null || fileDialog !== null || checking || namingTemplate || unlocking;
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
      if (command && key === "f") {
        event.preventDefault();
        filterInput.current?.focus();
        filterInput.current?.select();
        return;
      }
      if (typing) return;
      // A cue's own hotkey comes first; the screen's keys cannot be taken.
      const hotkey = hotkeyOf(event);
      const triggered = hotkey && canControl ? cuesForHotkey(allCues, hotkey) : [];
      if (triggered.length > 0) {
        event.preventDefault();
        if (!event.repeat) for (const cue of triggered) run({ command: "start", cue });
        return;
      }
      // Transport and navigation work in both modes.
      const action = hotkey && canControl ? actionFor(preferences.keymap, hotkey) : null;
      if (action) {
        // Also stops a focused button from being clicked by the same key.
        event.preventDefault();
        if (action === "panic") run({ command: "panic" });
        else if (!event.repeat) {
          if (action === "go") go();
          else if (action === "stopAll") run({ command: "stop_all" });
          else if (action === "pauseAll") run({ command: "pause_all" });
          else if (action === "resumeAll") run({ command: "resume_all" });
          else run({ command: action === "playheadNext" ? "playhead_next" : "playhead_previous", list: list?.id });
        }
        return;
      }
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        moveSelection(event.key === "ArrowDown" ? 1 : -1, event.shiftKey);
        return;
      }
      if (event.key === "Enter" && focus && canControl) {
        run({ command: "set_playhead", cue: focus });
        return;
      }
      if (!command && key === "v" && focus && canControl) {
        run({ command: "start", cue: focus });
        return;
      }
      if (command && key === "a") {
        event.preventDefault();
        setPicked(shownRows.map((row) => row.cue.id));
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
  }, [allCues, canControl, copy, deleteSelected, duplicate, editable, focus, go, groupSelected, list?.id, modal, moveSelection, paste, preferences.keymap, run, save, shownRows]);

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

  /** One list, as a cart or as rows. With two panes, only the active one is worked in. */
  const renderPane = (target: CueListData, pane: 0 | 1 | null) => {
    const active = pane === null || pane === activePane;
    const body = target.cart ? (
      <CueCart
        cues={target.cues}
        allCues={allCues}
        selection={active ? selection : NO_SELECTION}
        running={running}
        loaded={loaded}
        editable={editable && active}
        onSelect={select}
        onToggle={toggleCue}
        onMove={moveCues}
        onDropFiles={(files, drop) => void dropFiles(files, drop)}
      />
    ) : (
      <>
        {active && <FilterBar filter={filter} onChange={setFilter} shown={shownRows.length} total={rows.length} inputRef={filterInput} />}
        <CueList
          cues={target.cues}
          allCues={allCues}
          playhead={status?.playheads.find((entry) => entry.list === target.id)?.cue ?? null}
          selection={active ? selection : NO_SELECTION}
          running={running}
          loaded={loaded}
          visible={active ? visible : null}
          editable={editable && active}
          onSelect={select}
          onSetPlayhead={(cue) => canControl && run({ command: "set_playhead", cue })}
          onMove={moveCues}
          onDropFiles={(files, drop) => void dropFiles(files, drop)}
          onDropMedia={dropMedia}
          onEdit={(cue) => run({ command: "update_cue", cue })}
        />
      </>
    );
    if (pane === null) return body;
    return (
      <div key={pane} className={active ? "pane active" : "pane"} onPointerDownCapture={() => setActivePane(pane)}>
        <div className="pane-title">{target.name}</div>
        {body}
      </div>
    );
  };

  return (
    <div className={`app ${mode}-mode`}>
      <TopBar
        snapshot={snapshot}
        mode={mode}
        canEdit={access.edit}
        onMode={(next) => {
          // Back to editing needs the PIN, when this device has one.
          if (next === "edit" && mode === "show" && preferences.lock) setUnlocking(true);
          else setPreferences({ ...preferences, mode: next });
        }}
        onUndo={() => run({ command: "undo" })}
        onRedo={() => run({ command: "redo" })}
        onSave={save}
        onFile={setFileDialog}
        onPreflight={() => setChecking(true)}
        onSettings={() => setSettingsOpen(true)}
      />
      {access.following && (
        <div className="backup-banner" role="status">
          <Icon name="lock" size={12} />
          {t("backup.following")}
        </div>
      )}
      {snapshot && connectionState !== "open" && (
        <div className="reconnecting" role="status">
          {t("app.disconnected")}
        </div>
      )}
      {!snapshot || !workspace ? (
        <div className="splash">{connectionState === "closed" ? t("app.disconnected") : t("app.connecting")}</div>
      ) : (
        <main>
          <div className="left">
            <Transport
              standby={standby}
              panicking={status?.panicking ?? false}
              disabled={!canControl}
              onGo={go}
              onStopAll={() => run({ command: "stop_all" })}
              onPauseAll={() => run({ command: "pause_all" })}
              onResumeAll={() => run({ command: "resume_all" })}
              onPanic={() => run({ command: "panic" })}
              onReset={() => run({ command: "reset" })}
            />
            {editable && (
              <div className="toolbar">
                <Toolbox onAdd={addCue} />
                <TemplateMenu
                  templates={workspace.settings.cue_templates ?? []}
                  onCreate={createFromTemplate}
                  onDelete={(name) => updateSettings({ ...workspace.settings, cue_templates: withoutTemplate(workspace.settings.cue_templates ?? [], name) })}
                />
                <div className="list-actions">
                  <button className="icon-button" onClick={() => setNamingTemplate(true)} disabled={selected.length !== 1} title={t("template.save")}>
                    <Icon name="template-save" />
                  </button>
                  <button className="icon-button" onClick={duplicate} disabled={selected.length === 0} title={`${t("action.duplicate")} (Ctrl+D)`}>
                    <Icon name="copy" />
                  </button>
                  <button className="icon-button" onClick={groupSelected} disabled={selected.length === 0} title={`${t("action.group")} (Ctrl+G)`}>
                    <Icon name="group" />
                  </button>
                  <button className="icon-button" onClick={() => setRenumbering(true)} disabled={selected.length === 0} title={t("action.renumber")}>
                    <Icon name="renumber" />
                  </button>
                  <button className="icon-button" onClick={deleteSelected} disabled={selected.length === 0} title={`${t("action.delete")} (Del)`}>
                    <Icon name="trash" />
                  </button>
                </div>
              </div>
            )}
            {(editable || workspace.cue_lists.length > 1) && (
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
                      className={candidate.id === list?.id ? "tab active" : "tab"}
                      onClick={() => selectList(candidate.id)}
                      onDoubleClick={() => editable && setRenamingList(candidate.id)}
                      title={editable ? t("list.renameHint") : undefined}
                    >
                      {candidate.cart && <Icon name="cart" size={13} />}
                      {candidate.name}
                    </button>
                  ),
                )}
                {editable && (
                  <>
                    <button className="icon-button quiet" onClick={() => addList(false)} title={t("list.add")}>
                      <Icon name="plus" size={13} />
                    </button>
                    <button className="icon-button quiet" onClick={() => addList(true)} title={t("list.addCart")}>
                      <Icon name="cart" size={13} />
                    </button>
                  </>
                )}
                {editable && workspace.cue_lists.length > 1 && list && (
                  <button className="icon-button quiet" onClick={() => run({ command: "delete_cue_list", list: list.id })} title={t("list.delete")}>
                    <Icon name="trash" size={13} />
                  </button>
                )}
                <span className="tabs-end">
                  {workspace.cue_lists.length > 1 && (
                    <button
                      className={panes ? "icon-button quiet toggled" : "icon-button quiet"}
                      onClick={() => list && setPreferences({ ...preferences, split: toggleSplit(preferences.split, list.id, workspace.cue_lists.map((candidate) => candidate.id)) })}
                      title={t("list.split")}
                      aria-label={t("list.split")}
                      aria-pressed={panes !== null}
                    >
                      <Icon name="split" size={13} />
                    </button>
                  )}
                </span>
                {!list?.cart && (
                  <span className="tabs-end-actions">
                    {editable && (
                      <button className="icon-button quiet" onClick={() => importInput.current?.click()} title={t("list.import")}>
                        <Icon name="import" size={13} />
                      </button>
                    )}
                    <button className="icon-button quiet" onClick={exportCsv} title={t("list.export")}>
                      <Icon name="export" size={13} />
                    </button>
                    <button className="icon-button quiet" onClick={() => setPrinting(true)} title={t("list.print")}>
                      <Icon name="print" size={13} />
                    </button>
                  </span>
                )}
              </nav>
            )}
            {panes ? (
              <div className="split-panes">{panes.map((id, index) => renderPane(workspace.cue_lists.find((candidate) => candidate.id === id)!, index as 0 | 1))}</div>
            ) : (
              list && renderPane(list, null)
            )}
            {editable && (
              <Inspector
                connection={connection}
                cues={selectedCues}
                allCues={allCues}
                settings={workspace.settings}
                audio={audio}
                running={running}
                loaded={loaded}
                editable={editable}
                onLoad={(cue, time) => run({ command: "load", cue, time })}
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
            {dmx !== null && ((workspace.settings.dmx_outputs ?? []).length > 0 || dmx.length > 0) && <DmxPanel levels={dmx} labels={slotLabels(workspace.settings.fixtures ?? [])} />}
            {flatten(workspace.cue_lists.flatMap((list) => list.cues)).some((row) => row.cue.type === "video" || row.cue.type === "text") && (
              <StageMonitor connection={connection} settings={workspace.settings} />
            )}
            <ActiveCues
              disabled={!canControl}
              others={others}
              running={status?.running ?? []}
              cues={allCues}
              onPause={(cue) => run({ command: "pause", cue })}
              onResume={(cue) => run({ command: "resume", cue })}
              onStop={(cue) => run({ command: "stop", cue })}
            />
            <MediaPanel connection={connection} cues={allCues} editable={editable} showId={workspace.id} revision={snapshot.revision} />
          </aside>
        </main>
      )}
      {settingsOpen && workspace && (
        <SettingsDialog
          connection={connection}
          workspace={workspace}
          preferences={preferences}
          canEdit={access.edit}
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
      {diagnosing && <DiagnosticsDialog connection={connection} onClose={() => setDiagnosing(false)} />}
      {unlocking && (
        <PinDialog
          mode="unlock"
          lock={preferences.lock}
          onClose={() => setUnlocking(false)}
          onVerified={() => {
            setUnlocking(false);
            setPreferences({ ...preferences, mode: "edit" });
          }}
        />
      )}
      {namingTemplate && selectedCues.length === 1 && workspace && (
        <NameDialog
          title={t("template.save")}
          label={t("template.name")}
          verb={t("action.save")}
          initial={suggestedName(selectedCues[0], typeLabel(selectedCues[0].type))}
          onClose={() => setNamingTemplate(false)}
          onApply={(name) => {
            setNamingTemplate(false);
            updateSettings({ ...workspace.settings, cue_templates: withTemplate(workspace.settings.cue_templates ?? [], makeTemplate(name, selectedCues[0])) });
          }}
        />
      )}
      {checking && (
        <PreflightDialog
          connection={connection}
          cues={allCues}
          onSelect={(id) => {
            const owner = workspace?.cue_lists.find((candidate) => findCue(candidate.cues, id));
            if (owner) selectList(owner.id);
            setFilter(NO_FILTER);
            setPicked([id]);
            setAnchor(id);
            setChecking(false);
          }}
          onClose={() => setChecking(false)}
        />
      )}
      {fileDialog && <FileDialog connection={connection} mode={fileDialog} suggestedName={workspace?.name ?? ""} dirty={snapshot?.dirty ?? false} onClose={() => setFileDialog(null)} />}
      {importing && list && (
        <ImportDialog fileName={importing.fileName} listName={list.name} result={importing.result} onApply={applyImport} onClose={() => setImporting(null)} />
      )}
      <input
        ref={importInput}
        type="file"
        accept=".csv,.tsv,.txt,text/csv"
        hidden
        onChange={(event) => {
          const file = event.target.files?.[0];
          event.target.value = "";
          if (file) chooseImport(file);
        }}
      />
      {printing && list && workspace && (
        <PrintSheet show={workspace.name} listName={list.name} cues={list.cues} allCues={allCues} onDone={() => setPrinting(false)} />
      )}
      <StatusBar
        cueCount={rows.length}
        listCount={workspace?.cue_lists.length ?? 0}
        selectedCount={selected.length}
        audio={audio}
        timecode={timecode}
        access={access}
        onChangePasscode={onChangePasscode}
        connection={connectionState}
        onAudio={() => setSettingsOpen(true)}
        onDiagnostics={() => setDiagnosing(true)}
      />
      <Announcer standby={standby} running={(status?.running ?? []).map((entry) => entry.cue)} find={(id) => findCue(allCues, id)} />
      <Toasts connection={connection} toasts={toasts} />
    </div>
  );
}
