import "@vitejs/plugin-react/preamble";
import { createRoot } from "react-dom/client";
import { App } from "./react/app";
import type { PagePayload } from "./react/types";
import "./styles.css";

declare global {
  interface Window {
    dataSSr: PagePayload;
  }
}

const container = document.getElementById("app");

if (!container) {
  throw new Error("Missing #app root");
}

createRoot(container).render(<App initialPayload={window.dataSSr} />);
