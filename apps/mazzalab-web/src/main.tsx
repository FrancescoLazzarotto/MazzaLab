import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ShowConnection, serverUrl } from "./api/connection";
import { I18nProvider } from "./i18n";
import "./styles.css";

const connection = new ShowConnection(serverUrl(window.location));
connection.start();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <I18nProvider>
      <App connection={connection} />
    </I18nProvider>
  </StrictMode>,
);
