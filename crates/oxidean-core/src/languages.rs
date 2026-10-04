//! Shared extension/filename → language taxonomy (issue #59).
//!
//! One table feeds three consumers so they can't drift:
//! - repo language stats (`repo.languages`, `crates/oxidean-api/src/repo/language_stats.rs`)
//! - syntax highlighting (`languageIdForPath` in `apps/web`, via the generated
//!   `packages/api-client/src/languages.ts` emitted by `make rpc-gen`)
//! - the `language:` repo-search qualifier (`repo.search` code search)
//!
//! Detection is deliberately linguist-lite: exact filename match first
//! (`Dockerfile`, `Makefile`, `Jenkinsfile`, …), then a lowercase extension
//! lookup. Extension-only detection cannot disambiguate shared extensions, so
//! each extension is owned by exactly one row — the linguist winners kept here
//! are `.v` → V, `.m` → Objective-C, `.pl` → Perl, `.fs` → F#, `.pp` → Pascal.
//! Content heuristics for those collisions are a future extension.
//!
//! `stats: false` rows are prose/data formats — detected for highlighting and
//! `language:` search but excluded from the About language bar.

/// One language row in the shared taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageSpec {
    /// Display name (sidebar label; also the `language:` qualifier value).
    pub name: &'static str,
    /// Linguist-style stats group. When set, stats and `language:` group-name
    /// searches roll up under this name (e.g. TSX rolls into "TypeScript").
    pub group: Option<&'static str>,
    /// Shiki grammar id (bundled id or an in-repo grammar like `tsrx`).
    /// `None` → renders as plaintext.
    pub shiki: Option<&'static str>,
    /// Lowercase extensions without the dot. Each extension is unique across rows.
    pub extensions: &'static [&'static str],
    /// Exact basenames matched case-insensitively before extensions
    /// (`makefile`, `cmakelists.txt`, `.vimrc`, …).
    pub filenames: &'static [&'static str],
    /// Extra `language:` qualifier values beyond `name`/`group`/`shiki`.
    pub aliases: &'static [&'static str],
    /// Linguist-style color for the About bar (lives on the group owner row).
    pub color: Option<&'static str>,
    /// Count bytes in the language bar (false for prose/data rows).
    pub stats: bool,
}

impl LanguageSpec {
    /// Language-bar name — `group` when set, else `name`.
    pub fn stat_name(&self) -> &'static str {
        self.group.unwrap_or(self.name)
    }
}

const fn lang(
    name: &'static str,
    group: Option<&'static str>,
    shiki: Option<&'static str>,
    extensions: &'static [&'static str],
    filenames: &'static [&'static str],
    aliases: &'static [&'static str],
    color: Option<&'static str>,
    stats: bool,
) -> LanguageSpec {
    LanguageSpec {
        name,
        group,
        shiki,
        extensions,
        filenames,
        aliases,
        color,
        stats,
    }
}

/// The shared language table. Extensions and filenames are unique across rows —
/// enforced by tests below. Adding a language is one row here: stats,
/// highlighting, and `language:` search all pick it up.
pub const LANGUAGES: &[LanguageSpec] = &[
    // ---- programming (counted in the language bar) ----
    lang("Rust", None, Some("rust"), &["rs"], &[], &[], Some("#dea584"), true),
    lang("TSRX", None, Some("tsrx"), &["tsrx"], &[], &[], Some("#6f00ff"), true),
    lang("Ripple", None, Some("ripple"), &["ripple"], &[], &[], Some("#8b5cf6"), true),
    lang(
        "TypeScript",
        None,
        Some("typescript"),
        &["ts", "mts", "cts"],
        &[],
        &["ts"],
        Some("#3178c6"),
        true,
    ),
    lang(
        "TSX",
        Some("TypeScript"),
        Some("tsx"),
        &["tsx"],
        &[],
        &[],
        None,
        true,
    ),
    lang(
        "JavaScript",
        None,
        Some("javascript"),
        &["js", "mjs", "cjs"],
        &[],
        &["js"],
        Some("#f1e05a"),
        true,
    ),
    lang(
        "JSX",
        Some("JavaScript"),
        Some("jsx"),
        &["jsx"],
        &[],
        &[],
        None,
        true,
    ),
    lang(
        "Python",
        None,
        Some("python"),
        &["py", "pyi", "pyw"],
        &["sconstruct", "sconscript"],
        &[],
        Some("#3572a5"),
        true,
    ),
    lang("Go", None, Some("go"), &["go"], &[], &["golang"], Some("#00add8"), true),
    lang(
        "Ruby",
        None,
        Some("ruby"),
        &["rb", "rake", "gemspec", "ru"],
        &[
            "gemfile",
            "rakefile",
            "guardfile",
            "vagrantfile",
            "podfile",
            "brewfile",
            "fastfile",
            "berksfile",
        ],
        &[],
        Some("#701516"),
        true,
    ),
    lang("PHP", None, Some("php"), &["php", "phtml", "ctp"], &[], &[], Some("#4f5d95"), true),
    lang("Java", None, Some("java"), &["java"], &[], &[], Some("#b07219"), true),
    lang("Kotlin", None, Some("kotlin"), &["kt", "kts"], &[], &[], Some("#a97bff"), true),
    lang("Swift", None, Some("swift"), &["swift"], &[], &[], Some("#f05138"), true),
    lang("Scala", None, Some("scala"), &["scala", "sc"], &[], &[], Some("#c22d40"), true),
    lang("C", None, Some("c"), &["c", "h"], &[], &[], Some("#555555"), true),
    lang(
        "C++",
        None,
        Some("cpp"),
        &["cc", "cpp", "cxx", "hpp", "hxx", "hh", "tpp", "ipp", "inl"],
        &[],
        &["cpp", "cplusplus"],
        Some("#f34b7d"),
        true,
    ),
    lang("C#", None, Some("csharp"), &["cs", "csx"], &[], &["csharp", "cs"], Some("#178600"), true),
    lang(
        "F#",
        None,
        Some("fsharp"),
        &["fs", "fsi", "fsx", "fsscript"],
        &[],
        &["fsharp", "fs"],
        Some("#b845fc"),
        true,
    ),
    lang("Elixir", None, Some("elixir"), &["ex", "exs"], &[], &[], Some("#6e4a7e"), true),
    lang(
        "Erlang",
        None,
        Some("erlang"),
        &["erl", "hrl"],
        &["rebar.config"],
        &[],
        Some("#b83998"),
        true,
    ),
    lang("Haskell", None, Some("haskell"), &["hs", "lhs"], &[], &[], Some("#5e5086"), true),
    lang(
        "Clojure",
        None,
        Some("clojure"),
        &["clj", "cljs", "cljc", "edn"],
        &[],
        &[],
        Some("#db5855"),
        true,
    ),
    lang("Lua", None, Some("lua"), &["lua"], &[], &[], Some("#000080"), true),
    lang("R", None, Some("r"), &["r"], &[".rprofile"], &[], Some("#198ce7"), true),
    lang("Dart", None, Some("dart"), &["dart"], &[], &[], Some("#00b4ab"), true),
    lang("Zig", None, Some("zig"), &["zig"], &[], &[], Some("#ec915c"), true),
    lang("Nim", None, Some("nim"), &["nim", "nims", "nimble"], &[], &[], Some("#ffc200"), true),
    // `.v` is V here — Verilog owns `.veo` only (extension detection cannot
    // disambiguate `.v`; see module docs for the collision policy).
    lang("V", None, Some("v"), &["v", "vsh"], &[], &["vlang"], Some("#4f87c4"), true),
    lang("Perl", None, Some("perl"), &["pl", "pm", "t"], &[], &[], Some("#0298c3"), true),
    lang("Julia", None, Some("julia"), &["jl"], &[], &[], Some("#a270ba"), true),
    lang(
        "Groovy",
        None,
        Some("groovy"),
        &["groovy", "gradle", "gvy"],
        &["jenkinsfile"],
        &[],
        Some("#4298b8"),
        true,
    ),
    lang(
        "Objective-C",
        None,
        Some("objective-c"),
        &["m"],
        &[],
        &["objc", "objectivec"],
        Some("#438eff"),
        true,
    ),
    lang(
        "Objective-C++",
        None,
        Some("objective-cpp"),
        &["mm"],
        &[],
        &["objcpp", "objectivecpp"],
        Some("#6866fb"),
        true,
    ),
    lang(
        "Shell",
        None,
        Some("shellscript"),
        &["sh", "bash", "zsh", "ksh", "fish"],
        &[".bashrc", ".zshrc", ".bash_profile", ".zprofile", ".profile"],
        &["bash", "sh", "zsh", "fish", "ksh", "shellscript", "posix"],
        Some("#89e051"),
        true,
    ),
    lang(
        "PowerShell",
        None,
        Some("powershell"),
        &["ps1", "psm1", "psd1"],
        &[],
        &["pwsh", "ps1", "posh"],
        Some("#012456"),
        true,
    ),
    lang(
        "Batchfile",
        None,
        Some("bat"),
        &["bat", "cmd"],
        &[],
        &["batch", "bat", "cmd", "batchfile"],
        Some("#c1f12e"),
        true,
    ),
    lang(
        "Vim Script",
        None,
        Some("vim"),
        &["vim", "viml"],
        &[".vimrc", "vimrc", "_vimrc", ".gvimrc"],
        &["vim", "viml", "vimscript"],
        Some("#199f4b"),
        true,
    ),
    lang(
        "Emacs Lisp",
        None,
        Some("elisp"),
        &["el"],
        &[".emacs", "_emacs"],
        &["elisp", "el", "emacslisp"],
        Some("#c065db"),
        true,
    ),
    lang(
        "CMake",
        None,
        Some("cmake"),
        &["cmake"],
        &["cmakelists.txt"],
        &[],
        Some("#da3434"),
        true,
    ),
    lang(
        "Protocol Buffer",
        None,
        Some("proto"),
        &["proto"],
        &[],
        &["protobuf", "proto", "protocolbuffer", "protobuffers"],
        Some("#ededed"),
        true,
    ),
    lang(
        "GraphQL",
        None,
        Some("graphql"),
        &["graphql", "gql", "graphqls"],
        &[],
        &["gql"],
        Some("#e10098"),
        true,
    ),
    lang("SQL", None, Some("sql"), &["sql"], &[], &[], Some("#e38c00"), true),
    lang(
        "Dockerfile",
        None,
        Some("dockerfile"),
        &["dockerfile"],
        &["dockerfile", "containerfile"],
        &["docker", "containerfile"],
        Some("#384d54"),
        true,
    ),
    lang(
        "Makefile",
        None,
        Some("makefile"),
        &["mk", "mak"],
        &["makefile", "gnumakefile"],
        &["make", "gnumake"],
        Some("#427819"),
        true,
    ),
    lang(
        "Terraform",
        None,
        Some("terraform"),
        &["tf", "tfvars"],
        &[],
        &["tf"],
        Some("#844fba"),
        true,
    ),
    lang("HCL", None, Some("hcl"), &["hcl"], &[], &[], Some("#844fba"), true),
    lang("Nix", None, Some("nix"), &["nix"], &[], &[], Some("#7e7eff"), true),
    lang("OCaml", None, Some("ocaml"), &["ml", "mli"], &[], &[], Some("#ef7a08"), true),
    lang("D", None, Some("d"), &["d"], &[], &["dlang"], Some("#ba595e"), true),
    lang(
        "Pascal",
        None,
        Some("pascal"),
        &["pas", "pp", "dpr", "lpr"],
        &[],
        &["delphi", "objectpascal"],
        Some("#b0ce4e"),
        true,
    ),
    lang(
        "Fortran",
        None,
        Some("fortran-free-form"),
        &["f", "f77", "f90", "f95", "f03", "f08", "for", "fpp"],
        &[],
        &[],
        Some("#4d41b1"),
        true,
    ),
    lang("Tcl", None, Some("tcl"), &["tcl"], &[], &[], Some("#e4cc98"), true),
    lang(
        "Assembly",
        None,
        Some("asm"),
        &["asm", "s", "nasm"],
        &[],
        &["asm", "nasm"],
        Some("#6e4c13"),
        true,
    ),
    lang(
        "Visual Basic",
        None,
        Some("vb"),
        &["vb", "vbs", "bas"],
        &[],
        &["vb", "vbnet", "visualbasic"],
        Some("#945db7"),
        true,
    ),
    lang("Verilog", None, Some("verilog"), &["veo"], &[], &[], Some("#b2b7f8"), true),
    lang(
        "SystemVerilog",
        None,
        Some("system-verilog"),
        &["sv", "svh"],
        &[],
        &["systemverilog"],
        Some("#dae1c2"),
        true,
    ),
    lang("VHDL", None, Some("vhdl"), &["vhd", "vhdl"], &[], &[], Some("#adb2cb"), true),
    lang("COBOL", None, Some("cobol"), &["cob", "cbl"], &[], &[], None, true),
    lang("Ada", None, Some("ada"), &["adb", "ads"], &[], &[], Some("#02f88c"), true),
    lang("ABAP", None, Some("abap"), &["abap"], &[], &[], Some("#e8274b"), true),
    lang(
        "Common Lisp",
        None,
        Some("common-lisp"),
        &["lisp", "lsp"],
        &[],
        &["lisp", "commonlisp"],
        Some("#3fb68b"),
        true,
    ),
    lang("Scheme", None, Some("scheme"), &["scm", "ss", "sld"], &[], &[], Some("#1e4aec"), true),
    lang("Racket", None, Some("racket"), &["rkt"], &[], &[], Some("#3c5caa"), true),
    // No bundled Shiki grammar — detected + searchable, renders plaintext.
    lang(
        "Standard ML",
        None,
        None,
        &["sml", "sig"],
        &[],
        &["sml", "standardml"],
        Some("#dc566d"),
        true,
    ),
    lang("Prolog", None, Some("prolog"), &["pro"], &[], &[], Some("#74283c"), true),
    lang("Forth", None, None, &["fth", "4th"], &[], &[], Some("#341708"), true),
    lang(
        "TeX",
        None,
        Some("latex"),
        &["tex", "sty", "cls"],
        &[],
        &["tex", "latex"],
        Some("#3d6117"),
        true,
    ),
    lang("BibTeX", None, Some("bibtex"), &["bib"], &[], &[], Some("#778899"), true),
    lang("Haxe", None, Some("haxe"), &["hx"], &[], &[], Some("#df7900"), true),
    lang(
        "ActionScript",
        None,
        Some("actionscript"),
        &["as"],
        &[],
        &["as3", "actionscript3"],
        Some("#882b0f"),
        true,
    ),
    lang("ColdFusion", None, None, &["cfm", "cfc"], &[], &["cfml"], Some("#ed2cd6"), true),
    lang("ERB", None, Some("erb"), &["erb"], &[], &[], Some("#701516"), true),
    lang("Haml", None, Some("haml"), &["haml"], &[], &[], Some("#ece2a9"), true),
    lang("Twig", None, Some("twig"), &["twig"], &[], &[], Some("#c1d026"), true),
    lang("JSP", None, None, &["jsp"], &[], &["javaserverpages"], Some("#2a6277"), true),
    lang("Smarty", None, None, &["tpl"], &[], &[], Some("#f0c040"), true),
    lang(
        "Graphviz DOT",
        None,
        None,
        &["dot", "gv"],
        &[],
        &["dot", "graphviz", "dotlanguage"],
        Some("#2596be"),
        true,
    ),
    lang("AWK", None, Some("awk"), &["awk"], &[], &[], Some("#c30e9b"), true),
    lang(
        "QML",
        None,
        Some("qml"),
        &["qml"],
        &["qmldir"],
        &[],
        Some("#44a51c"),
        true,
    ),
    lang(
        "LLVM IR",
        None,
        Some("llvm"),
        &["ll"],
        &[],
        &["llvm", "llvmir"],
        Some("#185619"),
        true,
    ),
    lang("Stata", None, Some("stata"), &["do", "mata"], &[], &["mata"], Some("#1a5f91"), true),
    lang("Squirrel", None, None, &["nut"], &[], &[], Some("#800000"), true),
    lang("PostScript", None, None, &["ps"], &[], &["ps"], Some("#da291c"), true),
    lang("XSLT", None, Some("xsl"), &["xsl", "xslt"], &[], &["xslt"], Some("#eb8ceb"), true),
    lang("XQuery", None, None, &["xq", "xquery"], &[], &["xquery"], Some("#5232e7"), true),
    // ---- markup / style (counted, like linguist markup types) ----
    lang(
        "HTML",
        None,
        Some("html"),
        &["html", "htm", "xhtml"],
        &[],
        &[],
        Some("#e34c26"),
        true,
    ),
    lang("CSS", None, Some("css"), &["css"], &[], &[], Some("#563d7c"), true),
    lang("SCSS", None, Some("scss"), &["scss"], &[], &[], Some("#c6538c"), true),
    lang("Sass", None, Some("sass"), &["sass"], &[], &[], Some("#a53b70"), true),
    lang("Less", None, Some("less"), &["less"], &[], &[], Some("#1d365d"), true),
    lang("Vue", None, Some("vue"), &["vue"], &[], &[], Some("#41b883"), true),
    lang("Svelte", None, Some("svelte"), &["svelte"], &[], &[], Some("#ff3e00"), true),
    lang("Astro", None, Some("astro"), &["astro"], &[], &[], Some("#ff5a03"), true),
    // ---- prose / data (detected + searchable + highlightable, excluded from the bar) ----
    lang(
        "Markdown",
        None,
        Some("markdown"),
        &["md", "markdown"],
        &[],
        &["md", "gfm"],
        Some("#083fa1"),
        false,
    ),
    lang("MDX", None, Some("mdx"), &["mdx"], &[], &[], Some("#fcb32c"), false),
    lang(
        "reStructuredText",
        None,
        Some("rst"),
        &["rst"],
        &[],
        &["rst"],
        Some("#141414"),
        false,
    ),
    lang(
        "AsciiDoc",
        None,
        Some("adoc"),
        &["adoc", "asciidoc"],
        &[],
        &["adoc"],
        Some("#73a0c5"),
        false,
    ),
    lang("Text", None, None, &["txt"], &[], &["plaintext"], Some("#ededed"), false),
    lang("JSON", None, Some("json"), &["json"], &[], &[], Some("#292929"), false),
    lang(
        "JSON with Comments",
        None,
        Some("jsonc"),
        &["jsonc"],
        &[],
        &["jsonc"],
        Some("#292929"),
        false,
    ),
    lang("JSON5", None, Some("json5"), &["json5"], &[], &[], Some("#267cb9"), false),
    lang(
        "JSON Lines",
        None,
        Some("jsonl"),
        &["jsonl"],
        &[],
        &["jsonl", "ndjson"],
        Some("#292929"),
        false,
    ),
    lang("YAML", None, Some("yaml"), &["yaml", "yml"], &[], &["yml"], Some("#cb171e"), false),
    lang("TOML", None, Some("toml"), &["toml"], &[], &[], Some("#9c4221"), false),
    lang(
        "XML",
        None,
        Some("xml"),
        &["xml", "xsd", "wsdl", "csproj", "vbproj", "fsproj", "props", "targets"],
        &[],
        &[],
        Some("#0060ac"),
        false,
    ),
    lang(
        "INI",
        None,
        Some("ini"),
        &["ini"],
        &[".editorconfig"],
        &["editorconfig"],
        Some("#d1dbe0"),
        false,
    ),
    lang(
        "Properties",
        None,
        Some("properties"),
        &["properties"],
        &[],
        &[],
        Some("#2a6277"),
        false,
    ),
    lang("CSV", None, Some("csv"), &["csv"], &[], &[], Some("#237346"), false),
    lang("TSV", None, Some("tsv"), &["tsv"], &[], &[], Some("#237346"), false),
    lang(
        "Diff",
        None,
        Some("diff"),
        &["diff", "patch"],
        &[],
        &["patch"],
        Some("#88dddd"),
        false,
    ),
];

/// Map a repo path → language row (filename first, then extension), or `None`
/// when unknown/binary. Mirrors the detection rules in the module docs.
pub fn language_for_path(path: &str) -> Option<&'static LanguageSpec> {
    let path = path.replace('\\', "/");
    let file = path.rsplit('/').next().unwrap_or(path.as_str());
    let lower = file.to_ascii_lowercase();

    if let Some(spec) = LANGUAGES.iter().find(|l| l.filenames.contains(&lower.as_str())) {
        return Some(spec);
    }

    let ext = match lower.rsplit_once('.') {
        Some((_, e)) if !e.is_empty() && e != lower => e,
        _ => return None,
    };
    LANGUAGES.iter().find(|l| l.extensions.contains(&ext))
}

/// Exact display-name lookup (case-insensitive) — resolves a stats group name
/// back to its owner row (for colors) or a canonical row by name.
pub fn language_named(name: &str) -> Option<&'static LanguageSpec> {
    LANGUAGES.iter().find(|l| l.name.eq_ignore_ascii_case(name))
}

/// All rows matching a `language:` qualifier value, compared case-insensitively
/// on name, group, aliases, and Shiki id after dropping `-`/`_`/`.`/space.
/// `language:typescript` pulls in the TypeScript + TSX rows (group rollup);
/// `language:tsx` narrows to TSX alone.
pub fn languages_named(value: &str) -> Vec<&'static LanguageSpec> {
    let needle = normalize_language_key(value);
    if needle.is_empty() {
        return Vec::new();
    }
    LANGUAGES
        .iter()
        .filter(|l| {
            normalize_language_key(l.name) == needle
                || l.group
                    .map(normalize_language_key)
                    .is_some_and(|g| g == needle)
                || l.shiki
                    .map(normalize_language_key)
                    .is_some_and(|s| s == needle)
                || l.aliases
                    .iter()
                    .any(|a| normalize_language_key(a) == needle)
        })
        .collect()
}

/// Lowercase and drop separators so `emacs-lisp`, `Emacs Lisp`, `emacslisp`
/// key identically. `#`/`+` are preserved (`c++`, `f#` stay intact).
fn normalize_language_key(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_' | '.'))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn extensions_are_unique() {
        let mut seen = HashSet::new();
        for spec in LANGUAGES {
            for ext in spec.extensions {
                assert!(seen.insert(ext), "duplicate extension .{ext} ({})", spec.name);
            }
        }
    }

    #[test]
    fn filenames_are_unique() {
        let mut seen = HashSet::new();
        for spec in LANGUAGES {
            for f in spec.filenames {
                assert!(seen.insert(f), "duplicate filename {f} ({})", spec.name);
            }
        }
    }

    #[test]
    fn group_names_resolve_to_owner_rows() {
        for spec in LANGUAGES {
            if let Some(g) = spec.group {
                assert!(
                    language_named(g).is_some(),
                    "group {g} on {} has no owner row",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn detects_by_filename_before_extension() {
        assert_eq!(language_for_path("src/Dockerfile").unwrap().name, "Dockerfile");
        assert_eq!(language_for_path("containerfile").unwrap().name, "Dockerfile");
        assert_eq!(language_for_path("GNUmakefile").unwrap().name, "Makefile");
        assert_eq!(language_for_path("a/CMakeLists.txt").unwrap().name, "CMake");
        assert_eq!(language_for_path("ci/Jenkinsfile").unwrap().name, "Groovy");
        assert_eq!(language_for_path("SConstruct").unwrap().name, "Python");
    }

    #[test]
    fn detects_by_extension() {
        assert_eq!(language_for_path("src/main.rs").unwrap().name, "Rust");
        let tsx = language_for_path("web/app.tsx").unwrap();
        assert_eq!(tsx.name, "TSX");
        assert_eq!(tsx.stat_name(), "TypeScript");
        assert_eq!(language_for_path("a.rb").unwrap().name, "Ruby");
        assert_eq!(language_for_path("a.tf").unwrap().name, "Terraform");
        assert_eq!(language_for_path("a.nix").unwrap().name, "Nix");
        assert_eq!(language_for_path("a.vue").unwrap().name, "Vue");
        assert_eq!(language_for_path("a.zsh").unwrap().name, "Shell");
        assert_eq!(language_for_path("a.pyi").unwrap().name, "Python");
        assert_eq!(language_for_path("a.mts").unwrap().name, "TypeScript");
        assert_eq!(language_for_path("a.xhtml").unwrap().name, "HTML");
    }

    #[test]
    fn prose_and_data_detect_but_opt_out_of_stats() {
        let md = language_for_path("README.md").unwrap();
        assert_eq!(md.name, "Markdown");
        assert!(!md.stats);
        let json = language_for_path("data.json").unwrap();
        assert_eq!(json.name, "JSON");
        assert!(!json.stats);
        // Binary/unknown stays undetected.
        assert!(language_for_path("a.png").is_none());
        assert!(language_for_path("a.lock").is_none());
        assert!(language_for_path("LICENSE").is_none());
        // go.mod / go.sum stay excluded (module data, not code).
        assert!(language_for_path("go.mod").is_none());
        assert!(language_for_path("go.sum").is_none());
    }

    #[test]
    fn language_qualifier_matches_name_group_alias_shiki() {
        assert_eq!(languages_named("rust").len(), 1);
        assert_eq!(languages_named("Rust").len(), 1);
        // Group name pulls in every member row.
        let ts = languages_named("typescript");
        assert!(ts.iter().any(|s| s.name == "TypeScript"));
        assert!(ts.iter().any(|s| s.name == "TSX"));
        // Aliases / shiki ids / normalized punctuation.
        assert_eq!(languages_named("csharp")[0].name, "C#");
        assert_eq!(languages_named("c++")[0].name, "C++");
        assert_eq!(languages_named("proto")[0].name, "Protocol Buffer");
        assert_eq!(languages_named("emacs-lisp")[0].name, "Emacs Lisp");
        assert_eq!(languages_named("objc")[0].name, "Objective-C");
        assert_eq!(languages_named("shellscript")[0].name, "Shell");
        // Unknown language resolves to nothing.
        assert!(languages_named("cobolscript-9000").is_empty());
        assert!(languages_named("").is_empty());
    }
}
