/**
 * Minimal Octane mount for Vitest browser mode — avoids @testing-library/dom
 * (aria-query CJS named-export break under Chromium ESM).
 */
import { QueryClient, QueryClientProvider } from "@octanejs/tanstack-query";
import { createRoot, act, createElement, type Root } from "octane";

type Mounted = {
  container: HTMLElement;
  unmount: () => Promise<void>;
};

let active: Mounted | null = null;

async function mountRoot(renderBody: (root: Root) => void): Promise<Mounted> {
  await cleanupBrowserMount();

  const container = document.createElement("div");
  document.body.appendChild(container);

  let root!: Root;
  try {
    await act(() => {
      root = createRoot(container);
      renderBody(root);
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

export async function mountComponent(
  Component: unknown,
  props: Record<string, unknown> = {},
): Promise<Mounted> {
  return mountRoot((root) => {
    // Body + props form — same as Octane's preferred createRoot API.
    root.render(Component as never, props);
  });
}

/** Mount with QueryClientProvider (mutations / useQuery surfaces). */
export async function mountWithQueryClient(
  Component: unknown,
  props: Record<string, unknown> = {},
): Promise<Mounted> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  function Harness() {
    return createElement(
      QueryClientProvider as never,
      { client } as never,
      createElement(Component as never, props as never),
    );
  }
  return mountComponent(Harness);
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

export async function clickAriaLabel(label: string): Promise<void> {
  const el = document.querySelector(`[aria-label="${label}"]`);
  if (!el) throw new Error(`clickAriaLabel: no element [aria-label="${label}"]`);
  await act(async () => {
    (el as HTMLElement).click();
  });
}

async function waitForTestIdAttached(testId: string, ms = 2_000): Promise<Element | null> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 25));
  }
  return document.querySelector(`[data-testid="${testId}"]`);
}

/**
 * Open a Base UI Select and commit an option.
 *
 * Base UI ignores mouse `click` on Select.Item unless `pointerdown` first set
 * `allowMouseSelectionRef` (see admin/auth.integration.test.ts).
 */
export async function pickSelectOptionByTestId(
  triggerTestId: string,
  optionTestId: string,
): Promise<void> {
  const trigger = document.querySelector(`[data-testid="${triggerTestId}"]`);
  if (!trigger) {
    throw new Error(`pickSelectOption: no trigger [data-testid="${triggerTestId}"]`);
  }

  await act(async () => {
    (trigger as HTMLElement).click();
  });

  let option = await waitForTestIdAttached(optionTestId, 2_000);
  if (!option) {
    // Retry open once — portal open can miss the first click under Chromium.
    await act(async () => {
      (trigger as HTMLElement).click();
    });
    option = await waitForTestIdAttached(optionTestId, 2_000);
  }
  if (!option) {
    throw new Error(
      `pickSelectOption: option [data-testid="${optionTestId}"] never attached. ${debugBody()}`,
    );
  }

  await act(async () => {
    option.dispatchEvent(
      new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerType: "mouse" }),
    );
    (option as HTMLElement).click();
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
