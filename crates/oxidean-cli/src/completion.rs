//! `ox completion <shell>` — static completion scripts for bash, zsh, fish,
//! and PowerShell, generated from `COMMAND_TREE` so there's one list to keep
//! in sync with the parser (the conformance test in `args.rs` enforces it).
//!
//! Completes command and subcommand names only — per-leaf flags are a
//! deliberate non-goal (the hand-rolled parser keeps flag tables inline).

use serde_json::json;

use crate::commands::{CliError, Out};

/// sub → nested ops. Empty `ops` means the sub is a leaf verb.
type Subs = &'static [(&'static str, &'static [&'static str])];

/// group → subs. Empty `subs` means the group takes data positionals
/// (search/browse/api/self-update) — nothing to complete.
pub const COMMAND_TREE: &[(&str, Subs)] = &[
    (
        "admin",
        &[
            (
                "users",
                &[
                    "list",
                    "ban",
                    "unban",
                    "delete",
                    "set-role",
                    "sessions",
                    "revoke-sessions",
                    "access",
                    "activity",
                ],
            ),
            ("invites", &["list", "create", "link", "revoke"]),
            ("actions", &["runners", "runner-token"]),
            ("runners", &[]),
            ("runner-token", &[]),
            ("repos", &["gc"]),
            ("lfs", &["get", "set"]),
            ("git", &["get", "set"]),
            ("mcp", &["get", "set"]),
            ("auth", &["get", "set"]),
            ("templates", &["list", "enable", "disable", "delete"]),
        ],
    ),
    ("api", &[]),
    (
        "auth",
        &[
            ("login", &[]),
            ("status", &[]),
            ("switch", &[]),
            ("logout", &[]),
            ("token", &[]),
        ],
    ),
    ("browse", &[]),
    (
        "completion",
        &[("bash", &[]), ("zsh", &[]), ("fish", &[]), ("pwsh", &[])],
    ),
    ("config", &[("list", &[]), ("get", &[]), ("set", &[])]),
    (
        "email",
        &[
            ("list", &[]),
            ("add", &[]),
            ("remove", &[]),
            ("resend", &[]),
            ("set-primary", &[]),
        ],
    ),
    ("gpg-key", &[("list", &[]), ("add", &[]), ("delete", &[])]),
    ("invite", &[("accept", &[])]),
    (
        "issue",
        &[
            ("list", &[]),
            ("view", &[]),
            ("create", &[]),
            ("edit", &[]),
            ("close", &[]),
            ("reopen", &[]),
            ("delete", &[]),
            ("comment", &[]),
        ],
    ),
    (
        "label",
        &[
            ("list", &[]),
            ("create", &[]),
            ("edit", &[]),
            ("delete", &[]),
        ],
    ),
    (
        "notification",
        &[
            ("list", &[]),
            ("read", &[]),
            ("unread", &[]),
            ("unread-count", &[]),
        ],
    ),
    (
        "org",
        &[
            ("list", &[]),
            ("view", &[]),
            ("create", &[]),
            ("members", &["list", "add", "remove", "set-role"]),
            ("invites", &["list", "create", "link", "revoke"]),
        ],
    ),
    ("pat", &[("list", &[]), ("create", &[]), ("revoke", &[])]),
    ("pkg", &[("list", &[])]),
    (
        "pr",
        &[
            ("list", &[]),
            ("view", &[]),
            ("create", &[]),
            ("merge", &[]),
            ("diff", &[]),
            ("files", &[]),
            ("review", &[]),
            ("comment", &[]),
            ("ready", &[]),
            ("update-branch", &[]),
            ("checks", &[]),
            ("close", &[]),
            ("reopen", &[]),
            ("checkout", &[]),
        ],
    ),
    (
        "release",
        &[
            ("list", &[]),
            ("view", &[]),
            ("create", &[]),
            ("edit", &[]),
            ("delete", &[]),
            ("upload", &[]),
            ("download", &[]),
            ("delete-asset", &[]),
        ],
    ),
    (
        "repo",
        &[
            ("list", &[]),
            ("view", &[]),
            ("create", &[]),
            ("fork", &[]),
            ("sync", &[]),
            ("clone", &[]),
            ("edit", &[]),
            ("rename", &[]),
            ("archive", &[]),
            ("unarchive", &[]),
            ("delete", &[]),
            ("transfer", &[]),
            ("star", &[]),
            ("unstar", &[]),
            ("watch", &[]),
            ("unwatch", &[]),
            ("lfs", &[]),
            ("quota", &[]),
            ("merge-settings", &[]),
            ("deploy-key", &["list", "add", "delete"]),
            ("collaborator", &["list", "add", "set-role", "remove"]),
            ("collaborators", &["list", "add", "set-role", "remove"]),
            ("invites", &["list", "create", "link", "revoke"]),
            ("branch-protection", &["list", "set", "delete"]),
            ("tag-protection", &["list", "set", "delete"]),
            (
                "mirror",
                &[
                    "get",
                    "set",
                    "upsert",
                    "sync",
                    "delete",
                    "gen-key",
                    "rotate-secret",
                ],
            ),
        ],
    ),
    (
        "run",
        &[
            ("list", &[]),
            ("view", &[]),
            ("watch", &[]),
            ("rerun", &[]),
            ("cancel", &[]),
        ],
    ),
    ("search", &[]),
    ("secret", &[("list", &[]), ("set", &[]), ("delete", &[])]),
    ("self-update", &[]),
    ("skill", &[("print", &[]), ("install", &[])]),
    (
        "mcp",
        &[
            ("print", &[]),
            ("install", &[]),
            ("status", &[]),
            ("remove", &[]),
        ],
    ),
    ("ssh-key", &[("list", &[]), ("add", &[]), ("delete", &[])]),
    ("status", &[("create", &[])]),
    (
        "user",
        &[
            ("view", &[]),
            ("lookup", &[]),
            ("follow", &[]),
            ("unfollow", &[]),
            ("starred", &[]),
            ("watched", &[]),
        ],
    ),
    (
        "webhook",
        &[
            ("list", &[]),
            ("create", &[]),
            ("edit", &[]),
            ("delete", &[]),
            ("ping", &[]),
            ("redeliver", &[]),
            ("deliveries", &["list", "get", "view", "redeliver"]),
        ],
    ),
    ("workflow", &[("list", &[]), ("view", &[]), ("run", &[])]),
];

pub fn run(shell: &str) -> Result<Out, CliError> {
    let script = match shell {
        "bash" => bash(),
        "zsh" => zsh(),
        "fish" => fish(),
        "pwsh" | "powershell" => pwsh(),
        _ => {
            return Err(CliError::Usage(format!(
                "unknown shell '{shell}' — expected bash|zsh|fish|pwsh"
            )))
        }
    };
    Ok(Out::Report {
        text: script.clone(),
        json: json!({"ok": true, "data": {"shell": shell, "script": script}}),
    })
}

fn word_list(words: &[&str]) -> String {
    words.join(" ")
}

fn bash() -> String {
    let mut top = Vec::new();
    let mut subs_cases = String::new();
    let mut deep_cases = String::new();
    for (group, subs) in COMMAND_TREE {
        top.push(*group);
        if subs.is_empty() {
            continue;
        }
        let words: Vec<&str> = subs.iter().map(|(s, _)| *s).collect();
        subs_cases.push_str(&format!(
            "            {group}) COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") ) ;;\n",
            word_list(&words)
        ));
        for (sub, ops) in *subs {
            if ops.is_empty() {
                continue;
            }
            deep_cases.push_str(&format!(
                "                \"{group} {sub}\") COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") ) ;;\n",
                word_list(ops)
            ));
        }
    }
    format!(
        "# bash completion for ox\n\
         # install:  ox completion bash | sudo tee /etc/bash_completion.d/ox\n\
         # per-shell: source <(ox completion bash)\n\
         _ox() {{\n\
         \x20   local cur\n\
         \x20   cur=\"${{COMP_WORDS[COMP_CWORD]}}\"\n\
         \x20   COMPREPLY=()\n\
         \x20   case $COMP_CWORD in\n\
         \x20       1)\n\
         \x20           COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )\n\
         \x20           ;;\n\
         \x20       2)\n\
         \x20           case \"${{COMP_WORDS[1]}}\" in\n\
         {subs_cases}\
         \x20           esac\n\
         \x20           ;;\n\
         \x20       *)\n\
         \x20           case \"${{COMP_WORDS[1]}} ${{COMP_WORDS[2]}}\" in\n\
         {deep_cases}\
         \x20           esac\n\
         \x20           ;;\n\
         \x20   esac\n\
         \x20   return 0\n\
         }}\n\
         complete -F _ox ox\n",
        word_list(&top)
    )
}

fn zsh() -> String {
    let mut top = Vec::new();
    let mut subs_cases = String::new();
    let mut deep_cases = String::new();
    for (group, subs) in COMMAND_TREE {
        top.push(*group);
        if subs.is_empty() {
            continue;
        }
        let words: Vec<&str> = subs.iter().map(|(s, _)| *s).collect();
        subs_cases.push_str(&format!(
            "                {group}) subs=({}) ;;\n",
            word_list(&words)
        ));
        for (sub, ops) in *subs {
            if ops.is_empty() {
                continue;
            }
            deep_cases.push_str(&format!(
                "                \"{group} {sub}\") ops=({}) ;;\n",
                word_list(ops)
            ));
        }
    }
    format!(
        "#compdef ox\n\
         # zsh completion for ox — save as _ox in a directory on $fpath,\n\
         # then: autoload -Uz compinit && compinit\n\
         _ox() {{\n\
         \x20   local -a top subs ops\n\
         \x20   case $CURRENT in\n\
         \x20       2)\n\
         \x20           top=({})\n\
         \x20           _describe 'command' top\n\
         \x20           ;;\n\
         \x20       3)\n\
         \x20           case $words[2] in\n\
         {subs_cases}\
         \x20           esac\n\
         \x20           (( $#subs )) && _describe 'subcommand' subs\n\
         \x20           ;;\n\
         \x20       *)\n\
         \x20           case \"$words[2] $words[3]\" in\n\
         {deep_cases}\
         \x20           esac\n\
         \x20           (( $#ops )) && _describe 'operation' ops\n\
         \x20           ;;\n\
         \x20   esac\n\
         }}\n\
         _ox \"$@\"\n",
        word_list(&top)
    )
}

fn fish() -> String {
    let mut out = String::from(
        "# fish completion for ox — save to ~/.config/fish/completions/ox.fish\n\
         # words[2]/words[3] of `commandline -poc` are the group and subgroup.\n\
         function __ox_in_group --argument-names grp sub\n\
         \x20   set -l words (commandline -poc)\n\
         \x20   test (count $words) -ge 3; and test $words[2] = $grp; and test $words[3] = $sub\n\
         end\n\
         function __ox_in --argument-names grp\n\
         \x20   test (commandline -poc)[2] = $grp\n\
         end\n\n",
    );
    for (group, subs) in COMMAND_TREE {
        out.push_str(&format!(
            "complete -c ox -f -n '__fish_use_subcommand' -a {group}\n"
        ));
        if subs.is_empty() {
            continue;
        }
        let words: Vec<&str> = subs.iter().map(|(s, _)| *s).collect();
        out.push_str(&format!(
            "complete -c ox -f -n '__ox_in {group}' -a '{}'\n",
            words.join(" ")
        ));
        for (sub, ops) in *subs {
            if ops.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "complete -c ox -f -n '__ox_in_group {group} {sub}' -a '{}'\n",
                ops.join(" ")
            ));
        }
    }
    out
}

fn pwsh() -> String {
    let mut top = Vec::new();
    let mut subs_map = String::new();
    let mut deep_map = String::new();
    for (group, subs) in COMMAND_TREE {
        top.push(*group);
        if subs.is_empty() {
            continue;
        }
        let words: Vec<&str> = subs.iter().map(|(s, _)| *s).collect();
        subs_map.push_str(&format!("    '{group}' = '{}'\n", words.join(" ")));
        for (sub, ops) in *subs {
            if ops.is_empty() {
                continue;
            }
            deep_map.push_str(&format!("    '{group} {sub}' = '{}'\n", ops.join(" ")));
        }
    }
    format!(
        "# PowerShell completion for ox — add to $PROFILE:\n\
         #   ox completion pwsh | Out-String | Invoke-Expression\n\
         Register-ArgumentCompleter -Native -CommandName ox -ScriptBlock {{\n\
         \x20   param($wordToComplete, $commandAst, $cursorPosition)\n\
         \x20   $line = $commandAst.ToString()\n\
         \x20   $prefix = $line.Substring(0, [Math]::Min($cursorPosition, $line.Length))\n\
         \x20   $words = @($prefix -split '\\s+')\n\
         \x20   $pos = $words.Count - 1\n\
         \x20   $subs = @{{\n\
         {subs_map}\
         \x20   }}\n\
         \x20   $nested = @{{\n\
         {deep_map}\
         \x20   }}\n\
         \x20   $cands = @()\n\
         \x20   if ($pos -eq 1) {{\n\
         \x20       $cands = @('{}')\n\
         \x20   }} elseif ($pos -eq 2) {{\n\
         \x20       if ($subs.ContainsKey($words[1])) {{ $cands = $subs[$words[1]] -split ' ' }}\n\
         \x20   }} else {{\n\
         \x20       $k = \"$($words[1]) $($words[2])\"\n\
         \x20       if ($nested.ContainsKey($k)) {{ $cands = $nested[$k] -split ' ' }}\n\
         \x20   }}\n\
         \x20   $cands | Where-Object {{ $_ -like \"$wordToComplete*\" }} | ForEach-Object {{\n\
         \x20       [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)\n\
         \x20   }}\n\
         }}\n",
        top.join("', '")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_script_covers_tree() {
        let s = bash();
        assert!(s.contains("complete -F _ox ox"));
        let top = COMMAND_TREE
            .iter()
            .map(|(g, _)| *g)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(s.contains(&format!("-W \"{top}\"")), "top word list wrong");
        for (group, subs) in COMMAND_TREE {
            if !subs.is_empty() {
                assert!(
                    s.contains(&format!("{group}) COMPREPLY")),
                    "missing case for {group}"
                );
            }
            for (sub, ops) in *subs {
                if !ops.is_empty() {
                    assert!(s.contains(&format!("\"{group} {sub}\")")), "{group} {sub}");
                }
            }
        }
    }

    #[test]
    fn all_shells_emit_nonempty_scripts() {
        for sh in ["bash", "zsh", "fish", "pwsh", "powershell"] {
            let Out::Report { text, .. } = run(sh).unwrap() else {
                panic!("expected report for {sh}");
            };
            assert!(text.contains("ox"), "{sh} script empty");
        }
        assert!(run("tcsh").is_err());
    }

    #[test]
    fn tree_has_no_duplicate_words() {
        for (group, subs) in COMMAND_TREE {
            let mut seen = std::collections::BTreeSet::new();
            for (sub, ops) in *subs {
                assert!(seen.insert(sub), "duplicate sub {group} {sub}");
                let mut opseen = std::collections::BTreeSet::new();
                for op in *ops {
                    assert!(opseen.insert(op), "duplicate op {group} {sub} {op}");
                }
            }
        }
    }
}
