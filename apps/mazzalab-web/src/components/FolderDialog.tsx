import { useEffect, useState } from "react";
import type { ShowConnection } from "../api/connection";
import { useI18n } from "../i18n";
import { breadcrumbs, joinPath, type FolderListing } from "../showFiles";
import { Icon } from "./icons";

/** Picks a folder on the server's machine, starting at the open show's. */
export function FolderDialog({
  connection,
  title,
  verb,
  onPick,
  onClose,
}: {
  connection: ShowConnection;
  title: string;
  verb: string;
  onPick: (dir: string) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [listing, setListing] = useState<FolderListing | null>(null);
  const [error, setError] = useState<string | null>(null);

  const browse = (dir?: string) => {
    setError(null);
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

  const crumbs = listing ? breadcrumbs(listing.dir, listing.separator) : [];
  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <form
        className="dialog file-dialog"
        role="dialog"
        aria-modal
        onSubmit={(event) => {
          event.preventDefault();
          if (listing) onPick(listing.dir);
        }}
      >
        <header>
          <h1>{title}</h1>
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
              <li key={folder}>
                <button type="button" className="file-row" onClick={() => browse(joinPath(listing.dir, folder, listing.separator))}>
                  <Icon name="folder" size={15} />
                  {folder}
                </button>
              </li>
            ))}
            {listing && listing.folders.length === 0 && <li className="empty">{t("file.empty")}</li>}
          </ul>
          {error && <p className="hint warn">{error}</p>}
        </div>
        <footer>
          <button type="button" onClick={onClose}>
            {t("action.cancel")}
          </button>
          <button type="submit" className="primary" disabled={!listing}>
            {verb}
          </button>
        </footer>
      </form>
    </div>
  );
}
