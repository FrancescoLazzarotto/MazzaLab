import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ShowConnection } from "../api/connection";
import type { Cue } from "../api/types";
import { useI18n } from "../i18n";
import { MEDIA_DRAG, filterMedia, formatSize, mediaKind, usageByFile, type MediaFile, type MediaFilter } from "../mediaLibrary";
import { CueIcon, Icon } from "./icons";

interface Upload {
  name: string;
  progress: number;
  error?: string;
}

/** The show's media folder: what is in it, what plays it, and a way to add more from any device. */
export function MediaPanel({
  connection,
  cues,
  editable,
  showId,
  revision,
}: {
  connection: ShowConnection;
  cues: Cue[];
  editable: boolean;
  /** Changes when another show is opened. */
  showId: string;
  /** Changes when the show does, so a file that appeared or went is noticed. */
  revision: number;
}) {
  const { t } = useI18n();
  const [files, setFiles] = useState<MediaFile[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<MediaFilter>({ text: "", kind: "all" });
  const [uploads, setUploads] = useState<Upload[]>([]);
  const picker = useRef<HTMLInputElement>(null);

  const load = useCallback(() => {
    connection
      .media()
      .then((list) => {
        setFiles(list);
        setError(null);
      })
      .catch((reason: Error) => setError(reason.message));
  }, [connection]);
  // On opening, after another show, and when the show changes under us (a
  // cue gains a file another client uploaded).
  useEffect(load, [load, showId, revision]);

  const usage = useMemo(() => usageByFile(files ?? [], cues), [files, cues]);
  const shown = useMemo(() => filterMedia(files ?? [], usage, filter), [files, usage, filter]);

  const upload = async (picked: File[]) => {
    for (const file of picked) {
      setUploads((current) => [...current, { name: file.name, progress: 0 }]);
      try {
        await connection.uploadMedia(file, (progress) => setUploads((current) => current.map((entry) => (entry.name === file.name ? { ...entry, progress } : entry))));
        setUploads((current) => current.filter((entry) => entry.name !== file.name));
      } catch (reason) {
        setUploads((current) => current.map((entry) => (entry.name === file.name ? { ...entry, error: reason instanceof Error ? reason.message : String(reason) } : entry)));
      }
    }
    load();
  };

  return (
    <section className="panel media-panel">
      <h2>
        {t("media.title")}
        {files && <span className="count">{files.length}</span>}
        <span className="media-actions">
          <button className="icon-button quiet" onClick={load} title={t("media.refresh")}>
            <Icon name="restore" size={13} />
          </button>
          {editable && (
            <button className="icon-button quiet" onClick={() => picker.current?.click()} title={t("media.upload")}>
              <Icon name="import" size={13} />
            </button>
          )}
        </span>
      </h2>
      <input
        ref={picker}
        type="file"
        multiple
        accept="audio/*,video/*"
        hidden
        onChange={(event) => {
          const picked = [...(event.target.files ?? [])];
          event.target.value = "";
          if (picked.length > 0) void upload(picked);
        }}
      />
      {error && <p className="placeholder">{error}</p>}
      {files && files.length > 0 && (
        <div className="media-filter">
          <input value={filter.text} placeholder={t("media.search")} aria-label={t("media.search")} onChange={(event) => setFilter({ ...filter, text: event.target.value })} />
          <select value={filter.kind} aria-label={t("media.kind")} onChange={(event) => setFilter({ ...filter, kind: event.target.value as MediaFilter["kind"] })}>
            <option value="all">{t("media.all")}</option>
            <option value="audio">Audio</option>
            <option value="video">Video</option>
            <option value="unused">{t("media.unused")}</option>
          </select>
        </div>
      )}
      {files && files.length === 0 && !error && <p className="placeholder">{t("media.empty")}</p>}
      <ul className="media-list">
        {shown.map((file) => {
          const kind = mediaKind(file.name);
          const users = usage.get(file.file.replace(/\\/g, "/")) ?? [];
          return (
            <li
              key={file.file}
              className={users.length === 0 ? "media-row unused" : "media-row"}
              draggable={editable && kind !== "other"}
              onDragStart={(event) => {
                event.dataTransfer.setData(MEDIA_DRAG, JSON.stringify([file.file]));
                event.dataTransfer.effectAllowed = "copy";
              }}
              title={file.file}
            >
              {kind === "other" ? <Icon name="file" size={14} /> : <CueIcon type={kind} size={14} />}
              <span className="media-name">{file.name}</span>
              <span className="media-size">{formatSize(file.size)}</span>
              <span className="media-use" title={users.map((cue) => cue.number ?? "–").join(", ")}>
                {users.length > 0 ? `×${users.length}` : t("media.unusedShort")}
              </span>
            </li>
          );
        })}
      </ul>
      {uploads.map((entry) => (
        <div key={entry.name} className={entry.error ? "media-upload failed" : "media-upload"} title={entry.error}>
          <span className="media-name">{entry.name}</span>
          <span>{entry.error ? t("media.failed") : `${Math.round(entry.progress * 100)}%`}</span>
          <div className="progress" aria-hidden>
            <div style={{ width: `${entry.progress * 100}%` }} />
          </div>
        </div>
      ))}
    </section>
  );
}
