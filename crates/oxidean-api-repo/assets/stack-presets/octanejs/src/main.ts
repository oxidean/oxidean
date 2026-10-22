import { createRoot } from "octane";
import { App } from "./App.tsrx";
import "./style.css";

const el = document.getElementById("app");
if (!el) {
  throw new Error("#app not found");
}

createRoot(el).render(App, { title: "Octane" });
