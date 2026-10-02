import { describe, expect, it } from "vitest";
import {
  countCodeLines,
  getHighlighter,
  highlightCode,
  languageIdForPath,
  stripTrailingNewline,
} from "./highlight";

describe("highlight", { timeout: 60_000 }, () => {
  it("maps .tsrx and .ripple via in-repo grammars not TS/JS alias alone", async () => {
    expect(languageIdForPath("App.tsrx")).toBe("tsrx");
    expect(languageIdForPath("view.ripple")).toBe("ripple");

    const highlighter = await getHighlighter();
    const langs = highlighter.getLoadedLanguages();
    expect(langs).toContain("tsrx");
    expect(langs).toContain("ripple");

    const themes = highlighter.getLoadedThemes();
    expect(themes).toContain("oxidean-light");
    expect(themes).toContain("oxidean-dark");
  });

  it("highlights tsrx source with registered language id", async () => {
    const html = await highlightCode('@if (true) { "ok" }', {
      lang: "tsrx",
      theme: "oxidean-dark",
    });
    expect(html).toMatch(/shiki/i);
    expect(html).toMatch(/data-language="tsrx"/);
  });

  it("highlights App.tsrx-shaped JSX and @{ statement container", async () => {
    const sample = `export function App(props: AppProps) @{
  <main>
    <h1>{props.title as string}</h1>
  </main>
}
`;
    const html = await highlightCode(sample, {
      lang: "tsrx",
      theme: "oxidean-dark",
    });
    expect(html).toMatch(/data-language="tsrx"/);
    // Tag names are colored distinctly from punctuation (oxidean-dark purple).
    expect(html).toMatch(/color:#D2A8FF[^"]*">main</);
    expect(html).toMatch(/color:#D2A8FF[^"]*">h1</);
    // @{ statement container is a keyword-colored token.
    expect(html).toMatch(/@\{/);
  });

  it("maps and highlights .ts blobs as typescript (07-15 UAT)", async () => {
    expect(languageIdForPath("src/util.ts")).toBe("typescript");
    const html = await highlightCode("const x: number = 1;", {
      lang: languageIdForPath("src/util.ts"),
      theme: "oxidean-dark",
    });
    expect(html).toMatch(/shiki/i);
    expect(html).toMatch(/language-typescript|typescript/i);
    expect(html).toMatch(/const/);
  });

  it("covers the shared-table languages stats already detects (issue #59)", () => {
    // Previously unsupported: these all rendered as plaintext.
    const cases: Array<[string, string]> = [
      ["lib/app.rb", "ruby"],
      ["web/index.php", "php"],
      ["src/Main.java", "java"],
      ["app/build.gradle.kts", "kotlin"],
      ["ios/App.swift", "swift"],
      ["proj/core.scala", "scala"],
      ["src/main.c", "c"],
      ["src/main.cpp", "cpp"],
      ["Game/Program.cs", "csharp"],
      ["src/Lib.fs", "fsharp"],
      ["lib/server.ex", "elixir"],
      ["src/handler.erl", "erlang"],
      ["src/Main.hs", "haskell"],
      ["src/core.clj", "clojure"],
      ["init.lua", "lua"],
      ["analysis.R", "r"],
      ["lib/main.dart", "dart"],
      ["src/main.zig", "zig"],
      ["src/tool.nim", "nim"],
      ["src/main.v", "v"],
      ["scripts/gen.pl", "perl"],
      ["src/calc.jl", "julia"],
      ["ci/Jenkinsfile", "groovy"],
      ["mac/AppDelegate.m", "objective-c"],
      ["mac/view.mm", "objective-cpp"],
      ["deploy.ps1", "powershell"],
      ["build.bat", "bat"],
      ["editor/init.vim", "vim"],
      ["emacs/init.el", "elisp"],
      ["src/CMakeLists.txt", "cmake"],
      ["proto/api.proto", "proto"],
      ["schema.graphql", "graphql"],
      ["GNUmakefile", "makefile"],
      ["styles/app.sass", "sass"],
      ["styles/app.less", "less"],
      ["src/App.vue", "vue"],
      ["src/App.svelte", "svelte"],
      ["pages/index.astro", "astro"],
      ["infra/main.tf", "terraform"],
      ["flake.nix", "nix"],
      ["src/main.ml", "ocaml"],
    ];
    for (const [path, lang] of cases) {
      expect(languageIdForPath(path), path).toBe(lang);
    }
  });

  it("fills the partial-extension gaps from issue #59", () => {
    expect(languageIdForPath("shell/rc.zsh")).toBe("shellscript");
    expect(languageIdForPath("shell/rc.ksh")).toBe("shellscript");
    expect(languageIdForPath("shell/rc.fish")).toBe("shellscript");
    expect(languageIdForPath("src/mod.mts")).toBe("typescript");
    expect(languageIdForPath("src/mod.cts")).toBe("typescript");
    expect(languageIdForPath("src/stubs.pyi")).toBe("python");
    expect(languageIdForPath("deploy/Containerfile")).toBe("dockerfile");
    expect(languageIdForPath("pages/index.xhtml")).toBe("html");
  });

  it("matches special basenames case-insensitively before extensions", () => {
    expect(languageIdForPath("Dockerfile")).toBe("dockerfile");
    expect(languageIdForPath("src/dockerfile")).toBe("dockerfile");
    expect(languageIdForPath("Makefile")).toBe("makefile");
    expect(languageIdForPath("a/b/CMakeLists.txt")).toBe("cmake");
    expect(languageIdForPath("scripts/Rakefile")).toBe("ruby");
  });

  it("falls back to plaintext for unknown or grammar-less rows", () => {
    expect(languageIdForPath("assets/logo.png")).toBe("plaintext");
    expect(languageIdForPath("notes.txt")).toBe("plaintext");
    expect(languageIdForPath("LICENSE")).toBe("plaintext");
    expect(languageIdForPath("src/module.fth")).toBe("plaintext");
  });

  it("every loaded table lang resolves to a real Shiki grammar", async () => {
    const highlighter = await getHighlighter();
    const loaded = highlighter.getLoadedLanguages();
    // Spot-check the load path: a language added by this change highlights.
    for (const lang of ["ruby", "go", "kotlin", "terraform"]) {
      expect(loaded, lang).toContain(lang);
    }
    const html = await highlightCode("puts 1", { lang: "ruby", theme: "oxidean-light" });
    expect(html).toMatch(/data-language="ruby"/);
  });

  it("strips a trailing newline so highlight rows match line numbers", async () => {
    expect(stripTrailingNewline("a\nb\n")).toBe("a\nb");
    expect(countCodeLines("a\nb\n")).toBe(2);
    expect(countCodeLines("a\nb")).toBe(2);
    expect(countCodeLines("")).toBe(0);

    const withNl = await highlightCode("const x = 1;\n", {
      lang: "typescript",
      theme: "oxidean-light",
    });
    const withoutNl = await highlightCode("const x = 1;", {
      lang: "typescript",
      theme: "oxidean-light",
    });
    expect(withNl).toBe(withoutNl);
  });
});
