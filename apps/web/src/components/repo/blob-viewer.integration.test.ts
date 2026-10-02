import { cleanup, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/highlight", () => ({
  languageIdForPath: () => "javascript",
  clientHighlightTheme: () => "oxidean-dark",
  countCodeLines: (code: string) => {
    if (!code) return 0;
    const parts = code.split("\n");
    return parts[parts.length - 1] === "" ? parts.length - 1 : parts.length;
  },
  stripTrailingNewline: (code: string) => (code.endsWith("\n") ? code.slice(0, -1) : code),
  highlightCode: async (code: string) =>
    `<pre data-language="javascript"><code>${code
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")}</code></pre>`,
}));

vi.mock("@/lib/theme", () => ({
  resolveTheme: () => "dark",
  readThemePreference: () => "dark",
}));

import { BlobViewer } from "./blob-viewer";

afterEach(cleanup);

describe("BlobViewer file chrome", () => {
  it("shows filename, line/size meta, and Raw/Blame/Copy actions", async () => {
    render(BlobViewer, {
      props: {
        owner: "ada",
        repo: "hello",
        highlightedHtml:
          '<pre data-language="javascript"><code>export default function App() {\n  return null;\n}</code></pre>',
        highlightTheme: "oxidean-dark",
        blob: {
          path: "src/App.jsx",
          ref: "main",
          size: 48,
          truncated: false,
          is_binary: false,
          encoding: "utf-8",
          content: "export default function App() {\n  return null;\n}\n",
          soft_max_bytes: 1_048_576,
        },
      },
    });

    expect(await screen.findByText("App.jsx")).toBeInTheDocument();
    expect(screen.getByText(/3 lines/)).toBeInTheDocument();
    expect(screen.getByText(/48 Bytes/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Raw" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Blame" })).toHaveAttribute(
      "href",
      "/ada/hello/blame/main/src/App.jsx",
    );
    expect(screen.getByRole("button", { name: "Copy file contents" })).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByLabelText("Line numbers")).toBeInTheDocument();
    });
  });
});
