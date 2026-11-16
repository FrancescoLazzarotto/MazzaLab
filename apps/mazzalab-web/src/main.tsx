import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { loadPasscode, passcodeFromUrl, savePasscode, withoutPasscode } from "./access";
import { ShowConnection, serverHost, serverUrl } from "./api/connection";
import { AccessGate } from "./components/AccessGate";
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

// A link may carry the passcode (`&passcode=`); it is kept and taken off the address.
const host = serverHost(window.location);
const linked = passcodeFromUrl(window.location.search);
if (linked) {
  savePasscode(host, linked);
  window.history.replaceState(null, "", `${window.location.pathname}${withoutPasscode(window.location.search)}${window.location.hash}`);
}

const connection = new ShowConnection(serverUrl(window.location), linked ?? loadPasscode(host));
connection.start();

// `?stage=Main` turns this window into that stage; `&monitor` keeps it silent.
const params = new URLSearchParams(window.location.search);
const stage = params.get("stage");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <I18nProvider>
      <AccessGate connection={connection} host={host}>
        {({ access, onChangePasscode }) =>
          stage !== null ? (
            <StagePage connection={connection} name={stage || "Main"} monitor={params.has("monitor")} />
          ) : params.get("view") === "show" ? (
            <ShowView connection={connection} access={access} />
          ) : (
            <App connection={connection} access={access} onChangePasscode={onChangePasscode} />
          )
        }
      </AccessGate>
    </I18nProvider>
  </StrictMode>,
);
