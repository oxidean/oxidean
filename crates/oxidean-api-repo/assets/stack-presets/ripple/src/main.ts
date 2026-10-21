import { mount } from "ripple";
import { App } from "./App.tsrx";
import "./style.css";

const root = document.getElementById("app");
if (!root) {
  throw new Error("#app not found");
}

mount(App, { target: root, props: { title: "Ripple" } });
