type AppProps = {
  title: string;
};

export function App(props: AppProps) @{
  <main class="app">
    <h1>{props.title as string}</h1>
    <p>Ripple UI with TypeScript + Vite.</p>
    <p class="hint">Edit <code>src/App.tsrx</code> and save to reload.</p>
  </main>
}
