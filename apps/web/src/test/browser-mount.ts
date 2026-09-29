/**
 * Minimal Octane mount for Vitest browser mode — avoids @testing-library/dom
 * (aria-query CJS named-export break under Chromium ESM).
 */
import { createRoot, act, type Root } from "octane";

type Mounted = {
  container: HTMLElement;
  unmount: () => Promise<void>;
};

let active: Mounted | null = null;

export async function mountComponent(
  Component: unknown,
  props: Record<string, unknown> = {},
): Promise<Mounted> {
  await cleanupBrowserMount();

  const container = document.createElement("div");
  document.body.appendChild(container);

  let root!: Root;
  try {
    await act(() => {
      root = createRoot(container);
      // Body + props form — same as Octane's preferred createRoot API.
      root.render(Component as never, props);
    });
  } catch (e) {
    container.remove();
    throw e;
  }

  active = {
    container,
    unmount: async () => {
      await act(() => {
        root.unmount();
      });
      container.remove();
    },
  };
  return active;
}

export async function cleanupBrowserMount(): Promise<void> {
  if (!active) return;
  const current = active;
  active = null;
  await current.unmount();
}

export async function clickTestId(testId: string): Promise<void> {
  const el = document.querySelector(`[data-testid="${testId}"]`);
  if (!el) throw new Error(`clickTestId: no element [data-testid="${testId}"]`);
  await act(async () => {
    (el as HTMLElement).click();
  });
}

export function expectNoOctaneOverlayInDocument(): void {
  const html = document.body.innerHTML;
  if (
    html.includes("vite-error-overlay") ||
    html.includes("Something went wrong!") ||
    /is not defined|ReferenceError|Octane error|@else if/i.test(html)
  ) {
    throw new Error(`Octane/Vite overlay in document. body=${html.slice(0, 1200)}`);
  }
}

export function debugBody(label = "body"): string {
  const html = document.body.innerHTML;
  return `${label}=${html.slice(0, 2000)}`;
}
