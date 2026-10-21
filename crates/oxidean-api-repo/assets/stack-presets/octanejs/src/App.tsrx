import { useState } from "octane";

type AppProps = {
  title: string;
};

export function App(props: AppProps) @{
  const [count, setCount] = useState(0);

  <main class="app">
    <h1>{props.title as string}</h1>
    <p>AOT-compiled UI with Rivet templates (.tsrx).</p>
    <p>{"Button pressed " + count + " times"}</p>
    <button type="button" onClick={() => setCount((c) => c + 1)}>
      Press me
    </button>
    <p class="hint">Edit <code>src/App.tsrx</code> and save to reload.</p>
  </main>
}
