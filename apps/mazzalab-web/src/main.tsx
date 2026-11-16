import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ShowConnection, serverUrl } from "./api/connection";
import { ShowView } from "./components/ShowView";
import { StagePage } from "./components/StagePage";
import { I18nProvider } from "./i18n";
import { loadPreferences } from "./preferences";
import { applyTheme } from "./theme";
import "./styles.css";

// Before the first paint, so the page never flashes the wrong palette.
applyTheme(loadPreferences().theme);

// The app's files are kept by a service worker, so the page reloads offline.
if ("serviceWorker" in navigator && import.meta.env.PROD) {
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("/sw.js").catch(() => {
      // Not installable here (plain HTTP on a LAN, private mode): the app works without.
    });
  });
}

const connection = new ShowConnection(serverUrl(window.location));
connection.start();

// `?stage=Main` turns this window into that stage; `&monitor` keeps it silent.
const params = new URLSearchParams(window.location.search);
const stage = params.get("stage");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <I18nProvider>
      {stage !== null ? (
        <StagePage connection={connection} name={stage || "Main"} monitor={params.has("monitor")} />
      ) : params.get("view") === "show" ? (
        <ShowView connection={connection} />
      ) : (
        <App connection={connection} />
      )}
    </I18nProvider>
  </StrictMode>,
);
