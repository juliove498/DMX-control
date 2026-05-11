import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { LanguageProvider } from "./i18n";
import { installDocStubs } from "./lib/docMode";

// Install Tauri IPC stubs *before* React mounts so the first render's
// effects (which call invoke) don't throw when running inside a plain
// browser for documentation captures. No-op outside doc mode.
installDocStubs();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LanguageProvider>
      <App />
    </LanguageProvider>
  </React.StrictMode>,
);
