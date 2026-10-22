import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

const app = document.querySelector<HTMLDivElement>("#app")!;
app.innerHTML = `
  <main class="container">
    <h1>Welcome to Tauri</h1>
    <p>Click the button to greet from Rust.</p>
    <form id="greet-form" class="row">
      <input id="greet-input" placeholder="Enter a name..." />
      <button type="submit">Greet</button>
    </form>
    <p id="greet-msg"></p>
  </main>
`;

document.querySelector("#greet-form")!.addEventListener("submit", async (e) => {
  e.preventDefault();
  const name = document.querySelector<HTMLInputElement>("#greet-input")!.value;
  const msg = document.querySelector("#greet-msg")!;
  msg.textContent = await invoke("greet", { name });
});
