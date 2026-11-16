import { useEffect, useState } from "react";
import type { ShowConnection } from "../api/connection";
import { useI18n } from "../i18n";
import { ShowFileError, breadcrumbs, joinPath, stripExtension, validFileName, type FolderListing } from "../showFiles";
import { Icon } from "./icons";

export type FileMode = "open" | "new" | "save-as";

/** Picks a show on the server's machine to open, create or save as. */
export function FileDialog({
  connection,
  mode,
  suggestedName,
  dirty,
  onClose,
}: {
  connection: ShowConnection;
  mode: FileMode;
  suggestedName: string;
  /** The open show has unsaved changes: ask before replacing it. */
  dirty: boolean;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [listing, setListing] = useState<FolderListing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [chosen, setChosen] = useState<string | null>(null);
  const [name, setName] = useState(stripExtension(suggestedName));
  const [busy, setBusy] = useState(false);
  /** The path waiting on a decision about the unsaved changes. */
  const [asking, setAsking] = useState<string | null>(null);
  const opening = mode === "open";

  const browse = (dir?: string) => {
    setError(null);
    setChosen(null);
    connection
      .showFiles(dir)
      .then(setListing)
      .catch((reason: Error) => setError(reason.message));
  };
  useEffect(() => browse(), []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const target = (): string | null => {
    if (!listing) return null;
    if (opening) return chosen ? joinPath(listing.dir, chosen, listing.separator) : null;
    return validFileName(name) ? joinPath(listing.dir, name.trim(), listing.separator) : null;
  };

  const run = (path: string, discard = false, asked = false) => {
    // Asked here rather than on the server's refusal, which would fail a
    // request; the server still refuses if the show changed meanwhile.
    if (dirty && !discard && !asked && mode !== "save-as") {
      setAsking(path);
      return;
    }
    setBusy(true);
    setError(null);
    connection
      .changeShow(mode, path, discard)
      .then(onClose)
      .catch((reason: Error) => {
        setBusy(false);
        if (reason instanceof ShowFileError && reason.unsaved) setAsking(path);
        else setError(reason.message);
      });
  };

  const submit = () => {
    const path = target();
    if (path && !busy) run(path);
  };

  const saveThenRun = () => {
    const path = asking;
    if (!path) return;
    setAsking(null);
    setBusy(true);
    connection
      .save()
      .then(() => run(path, false, true))
      .catch((reason: Error) => {
        setBusy(false);
        setError(reason.message);
      });
  };

  const titles = { open: t("file.open"), new: t("file.new"), "save-as": t("file.saveAs") } as const;
  const verbs = { open: t("file.openVerb"), new: t("file.newVerb"), "save-as": t("action.save") } as const;
  const crumbs = listing ? breadcrumbs(listing.dir, listing.separator) : [];

  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <form
        className="dialog file-dialog"
        role="dialog"
        aria-modal
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <header>
          <h1>{titles[mode]}</h1>
        </header>
        <div className="dialog-body">
          <nav className="crumbs" aria-label={t("file.location")}>
            <button type="button" className="icon-button quiet" disabled={!listing?.parent} onClick={() => listing?.parent && browse(listing.parent)} title={t("file.up")}>
              <Icon name="up" size={13} />
            </button>
            {crumbs.map((crumb, index) => (
              <button type="button" key={crumb.path} className={index === crumbs.length - 1 ? "crumb current" : "crumb"} onClick={() => browse(crumb.path)}>
                {crumb.name}
              </button>
            ))}
          </nav>
          <ul className="file-list">
            {listing?.folders.map((folder) => (
              <li key={`d:${folder}`}>
                <button type="button" className="file-row" onClick={() => browse(joinPath(listing.dir, folder, listing.separator))}>
                  <Icon name="folder" size={15} />
                  {folder}
                </button>
              </li>
            ))}
            {listing?.shows.map((show) => (
              <li key={`f:${show}`}>
                <button
                  type="button"
                  className={show === chosen ? "file-row selected" : "file-row"}
                  onClick={() => {
                    setChosen(show);
                    if (!opening) setName(stripExtension(show));
                  }}
                  onDoubleClick={() => opening && listing && run(joinPath(listing.dir, show, listing.separator))}
                >
                  <Icon name="file" size={15} />
                  {show}
                </button>
              </li>
            ))}
            {listing && listing.folders.length === 0 && listing.shows.length === 0 && <li className="empty">{t("file.empty")}</li>}
          </ul>
          {!opening && (
            <label className="field">
              <span className="field-label">{t("file.name")}</span>
              <input autoFocus value={name} onChange={(event) => setName(event.target.value)} onFocus={(event) => event.target.select()} />
            </label>
          )}
          {error && <p className="hint warn">{error}</p>}
          {asking && (
            <div className="unsaved-ask" role="alert">
              <span>{t("file.unsaved")}</span>
              <span className="unsaved-buttons">
                <button type="button" className="primary" onClick={saveThenRun}>
                  {t("action.save")}
                </button>
                <button type="button" onClick={() => run(asking, true)}>
                  {t("file.discard")}
                </button>
                <button type="button" onClick={() => setAsking(null)}>
                  {t("action.cancel")}
                </button>
              </span>
            </div>
          )}
        </div>
        <footer>
          <button type="button" onClick={onClose}>
            {t("action.cancel")}
          </button>
          <button type="submit" className="primary" disabled={busy || target() === null || asking !== null}>
            {verbs[mode]}
          </button>
        </footer>
      </form>
    </div>
  );
}
