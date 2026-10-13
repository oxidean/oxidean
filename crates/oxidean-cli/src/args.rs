//! Hand-rolled argv parsing — the workspace's small binaries don't pull in
//! clap (see crates/oxidean-api/src/bin/protection_hook.rs and
//! crates/oxidean-runner), so `ox` keeps the same minimal dependency shape.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// `ox` command summary, printed for `ox`, `ox --help`, and usage errors.
/// Per-command details live in `HELP` — `ox <group> <sub> --help`.
pub const USAGE: &str = "\
ox — companion CLI for Oxidean forge instances

USAGE
  ox <command> [flags]

GETTING AROUND
  ox auth login|status|switch|logout|token         sign-in state and tokens
  ox config list|get|set                           local config file (tokens redacted)
  ox completion <bash|zsh|fish|pwsh>               shell completion script
  ox browse [<o/n>]                                open the repo (or instance) in a browser
  ox search <query>                                global search — --type repos|issues|pulls|…

REPOSITORIES
  ox repo list|view|create|fork|sync|clone
  ox repo edit|rename|archive|unarchive|delete|transfer
  ox repo star|unstar|watch|unwatch
  ox repo deploy-key|collaborator|invites          access management
  ox repo branch-protection|tag-protection         protection rules
  ox repo mirror|merge-settings|lfs|quota          repo plumbing

ISSUES & PULL REQUESTS
  ox issue list|view|create|edit|close|reopen|delete|comment
  ox pr list|view|create|merge|diff|files|review|comment
  ox pr ready|update-branch|checks|close|reopen|checkout

ACTIONS
  ox run list|view|watch|rerun|cancel              --job/--log for logs
  ox workflow list|view|run
  ox secret list|set|delete
  ox status create <o/n> --sha <s> --context <c> --state <st>

RELEASES & PACKAGES
  ox release list|view|create|edit|delete|upload|download|delete-asset
  ox pkg list [--owner <o>]

PEOPLE & ORGS
  ox org list|view|create|members|invites
  ox user view|lookup|follow|unfollow|starred|watched
  ox notification list|read|unread
  ox label list|create|edit|delete
  ox webhook list|create|edit|delete|ping|deliveries|redeliver

ACCOUNT
  ox ssh-key|gpg-key list|add|delete
  ox email list|add|remove|set-primary|resend

MAINTENANCE
  ox self-update     replace this binary with the build the instance serves
  ox skill print|install                             agent skill: emit or install SKILL.md
  ox mcp print|install|status|remove                 wire the MCP endpoint into harnesses

ESCAPE HATCH
  ox api <procedure> [-f field=value]... [-F field=<json>]...
     -f sends the value as a string; -F parses it as JSON.

GLOBAL FLAGS
  --instance <url>   instance base URL (else $OXIDEAN_INSTANCE, else login default)
  --json             print the raw {ok, data|error} RPC envelope
  --jq <filter>      run a jq expression over the envelope's data field
  -R, --repo <o/n>   repository in owner/name form (alternative to positional)
  -h, --help         help for `ox` or the command it's attached to
  --version          show version

ENVIRONMENT
  OXIDEAN_INSTANCE    default instance URL
  OXIDEAN_TOKEN       token override for any command
  OXIDEAN_CONFIG_DIR  config directory override (default: ~/.config/ox,
                      %APPDATA%\\ox on Windows)
  OXIDEAN_PAGER       pager for long output (else $PAGER, else `less -FRX`)
  OXIDEAN_FORCE_TTY   force interactive output (color, pager, prompts)
  NO_COLOR            disable color; CLICOLOR_FORCE forces it when piped

EXIT CODES
  0 ok · 1 api error · 2 usage · 3 auth";

/// Per-command help text. `ox <topic> --help` resolves the longest known
/// prefix, so `ox issue create --help` and `ox issue --help` both land.
pub fn help_topic(group: &str, sub: &str) -> Option<&'static str> {
    let key = format!("{group} {sub}").trim().to_string();
    HELP.iter()
        .find(|(k, _)| *k == key)
        .or_else(|| HELP.iter().find(|(k, _)| *k == group))
        .map(|(_, t)| *t)
}

/// `(command-path, help text)`. Every leaf lists its own flags.
const HELP: &[(&str, &str)] = &[
    ("auth", "ox auth — sign-in state\n\n  ox auth login --instance <url> [--web [--scopes <list>] | --token <pat> | --token-stdin]\n  ox auth status      show who you're logged in as + token scopes/expiry\n  ox auth refresh     mint a fresh CLI token (device grant), self-revoke the old one\n  ox auth revoke      revoke the current token on the server and log out\n  ox auth switch      change the default instance (picker, or pass the URL)\n  ox auth setup-git   register ox as a git credential helper\n  ox auth logout      remove the stored token for the instance\n  ox auth token       print the resolved token (for scripts)"),
    ("auth switch", "ox auth switch [<instance-url>]\n\nChanges the default instance used when neither --instance nor\nOXIDEAN_INSTANCE is set. With no argument, lists logged-in instances and\nprompts for one (TTY only)."),
    ("auth login", "ox auth login --instance <url> [--web [--scopes <list>] | --token <pat> | --token-stdin]\n\n--web runs the RFC 8628 device flow: prints a one-time code, opens the\ninstance's /oauth/device page, and polls until you approve or deny.\n--scopes <list> (comma or space separated) picks the scopes the minted\ntoken carries; the default covers the full CLI surface. With no token\nflag and no --web, a piped stdin is used as the token; on a TTY an\ninteractive picker offers browser or token sign-in. The token lives in\nthe OS keyring when one is available (config.json fallback), and the\ninstance becomes the default."),
    ("auth status", "ox auth status\n\nCalls auth.me on the resolved instance and prints the login; when the\ncredential is a PAT it also shows the token's name, scopes, expiry,\nand where it's stored (keyring or config file)."),
    ("auth refresh", "ox auth refresh [--scopes <list>] [--reset-scopes]\n\nMints a fresh CLI token through the browser device grant and self-\nrevokes the old one. With no flags the new token keeps the current\nscopes; --reset-scopes returns to the CLI default set. Only refreshes\nstored logins — a token supplied via OXIDEAN_TOKEN is env-owned and\ncannot be refreshed here."),
    ("auth revoke", "ox auth revoke\n\nRevokes the current PAT on the server (self-revoke) and removes the\nlocal credential. Equivalent to deleting the token at\n{instance}/settings/tokens for the token you're logged in with."),
    ("auth setup-git", "ox auth setup-git\n\nRegisters `ox` as the git credential helper for the resolved instance\nhost: git config credential.https://<host>.helper '!ox auth\ngit-credential'. After this, `git clone`/`fetch`/`push` over HTTPS\nauthenticate with your stored ox token."),
    ("auth logout", "ox auth logout\n\nDrops the stored token for the resolved instance (config file edit —\nno network call)."),
    ("auth token", "ox auth token\n\nPrints the token that would be sent (OXIDEAN_TOKEN wins over the\nstored credential). For scripts: TOKEN=$(ox auth token)."),
    ("config", "ox config — local config file\n\n  list|get|set\n\nKeys: default_instance · instances.<url>.token\nStored tokens are never printed — they report as set/unset."),
    ("config get", "ox config get <key>\n\n  default_instance            prints the configured default instance URL\n  instances.<url>.token       prints 'set' or 'unset' (never the secret)"),
    ("config set", "ox config set <key> <value>\n\n  default_instance <url>      must name a logged-in instance\n  instances.<url>.token <t>   stores the token (600 config file)"),
    ("completion", "ox completion <bash|zsh|fish|pwsh>\n\nPrints a static completion script for the shell — command and\nsubcommand names (per-leaf flags are not completed).\n\n  bash:  ox completion bash | sudo tee /etc/bash_completion.d/ox\n         or per-shell: source <(ox completion bash)\n  zsh:   save as _ox in a directory on $fpath, then compinit\n  fish:  ox completion fish > ~/.config/fish/completions/ox.fish\n  pwsh:  add to $PROFILE: ox completion pwsh | Out-String | Invoke-Expression"),
    ("repo", "ox repo — repositories\n\n  list|view|create|fork|sync|clone|templates|edit|rename|archive|unarchive\n  delete|transfer|star|unstar|watch|unwatch\n  deploy-key list|add|delete · collaborator list|add|set-role|remove\n  invites list|create|link|revoke\n  branch-protection list|set|delete · tag-protection list|set|delete\n  mirror get|set|sync|delete|gen-key|rotate-secret\n  merge-settings [get|set] · lfs [--usage|--objects|--enable|--disable]\n  quota [--set <bytes>]"),
    ("repo list", "ox repo list [--owner <user|org>]\n\n`--owner` → repo.listByOwner; bare → repo.listMine."),
    ("repo view", "ox repo view <owner/name>"),
    ("repo create", "ox repo create [<name>] [--org <slug>] [--description <d>]\n                [--visibility public|private] [--license <spdx>]\n                [--gitignore <id>] [--template <ref>]\n\nOwner defaults to your user; --org creates under that org.\n--template accepts a template ref from `ox repo templates` — a builtin\nstack id, an instance pack slug, or owner/name of a template repo.\nWith no name on a TTY, an interactive wizard walks name, owner,\ntemplate, and visibility; non-interactive use still requires <name>."),
    ("repo templates", "ox repo templates\n\nList the repository templates the connected instance offers — builtin\nstack presets, admin-provided instance packs, and template repos you\ncan read — plus the gitignore catalog and default visibility."),
    ("repo fork", "ox repo fork <owner/name> [--into-owner <o>] [--into-name <n>]"),
    ("repo sync", "ox repo sync <owner/name>\n\nSync a fork with its upstream (repo.forkStatus + repo.syncFork)."),
    ("repo clone", "ox repo clone <owner/name> [<dir>]\n\nRuns `git clone <instance>/<owner>/<name>.git` locally."),
    ("repo edit", "ox repo edit <o/n> [--description <d>] [--homepage <u>]\n              [--topics a,b,c] [--visibility public|private]"),
    ("repo rename", "ox repo rename <o/n> <new-name>"),
    ("repo archive", "ox repo archive <o/n>   /   ox repo unarchive <o/n>"),
    ("repo delete", "ox repo delete <o/n> --confirm <name>\n\nSoft-deletes the repo; the server requires the repo name echoed back\nas confirmation."),
    ("repo transfer", "ox repo transfer <o/n> <dest-owner> [--type user|org] --confirm <name>"),
    ("repo star", "ox repo star <o/n>   /   ox repo unstar <o/n>"),
    ("repo watch", "ox repo watch <o/n>   /   ox repo unwatch <o/n>"),
    ("repo deploy-key", "ox repo deploy-key list|add|delete <o/n>\n  add --title <t> --key-file <path> [--write]   (key-file '-' = stdin)\n  delete --id <id>"),
    ("repo collaborator", "ox repo collaborator list|add|set-role|remove <o/n>\n  add <user> --permission read|write|admin\n  set-role <user> --permission read|write|admin\n  remove <user>"),
    ("repo invites", "ox repo invites list|create|link|revoke <o/n>\n  create --emails a@x,b@y --permission read|write|admin\n  link --permission <p> [--expires <iso>] [--max-uses <n>]\n  revoke --id <invite-id>"),
    ("repo branch-protection", "ox repo branch-protection list|set|delete <o/n>\n  set --pattern <glob> [--reviews] [--approvals <n>] [--dismiss-stale]\n      [--conversation-resolution] [--last-push-approval]\n      [--status-contexts a,b] [--strict-status] [--force-pushes]\n      [--deletions] [--enforce-admins] [--linear-history] [--lock]\n      [--signed-commits]\n  delete --id <id>"),
    ("repo tag-protection", "ox repo tag-protection list|set|delete <o/n>\n  set --pattern <glob> [--no-create] [--no-update] [--no-delete]\n      [--enforce-admins]\n  delete --id <id>"),
    ("repo mirror", "ox repo mirror <o/n>                    show mirror config\n  ox repo mirror set <o/n> --remote-url <u> [--auth-kind <k>]\n      [--username <u>] [--secret <s>] [--ssh-key <k>] [--known-hosts <h>]\n      [--interval <secs>] [--disabled] [--sync-mode merge|exact]\n  ox repo mirror sync|delete|gen-key|rotate-secret <o/n>"),
    ("repo merge-settings", "ox repo merge-settings <o/n>            show allowed strategies\n  ox repo merge-settings <o/n> --merge on|off --squash on|off --rebase on|off"),
    ("repo lfs", "ox repo lfs <o/n> [--usage] [--objects] [--enable|--disable]"),
    ("repo quota", "ox repo quota <o/n> [--set <bytes>]"),
    ("issue", "ox issue — issues\n\n  list|view|create|edit|close|reopen|delete|comment"),
    ("issue list", "ox issue list <o/n> [--state <s>] [--author <u>] [--assignee <u>]\n               [--label <l>] [--search <q>] [--limit <n>]"),
    ("issue view", "ox issue view <o/n> <number>"),
    ("issue create", "ox issue create <o/n> --title <t> [--body <b> | --body-file -]"),
    ("issue edit", "ox issue edit <o/n> <number> [--title <t>] [--body <b>]\n              [--label <name>]... [--assignee <user>]...\n\nLabels/assignees are the FULL replacement set (gh-style set semantics):\nnames are resolved to ids via label.listForRepo / issue.assigneeCandidates."),
    ("issue close", "ox issue close <o/n> <number>   /   reopen   /   delete"),
    ("issue reopen", "ox issue reopen <o/n> <number>"),
    ("issue delete", "ox issue delete <o/n> <number>"),
    ("issue comment", "ox issue comment <o/n> <number> [--body <b> | --body-file -]\n\nWith no --body, a piped stdin is used as the comment text."),
    ("pr", "ox pr — pull requests\n\n  list|view|create|merge|diff|files|review|comment\n  ready|update-branch|checks|close|reopen|checkout"),
    ("pr list", "ox pr list <o/n> [--state <s>] [--author <u>] [--assignee <u>]\n            [--label <l>] [--search <q>] [--limit <n>]"),
    ("pr view", "ox pr view <o/n> <number>"),
    ("pr create", "ox pr create <o/n> --title <t> --base <branch> --head <branch>\n              [--body <b>] [--draft] [--head-owner <o> --head-name <n>]"),
    ("pr merge", "ox pr merge <o/n> <number> [--method merge|squash|rebase]\n            [--commit-title <t>] [--commit-message <m>] [--delete-branch]\n\nWith no --method the first strategy the repo allows is used\n(merge-commit → squash → rebase, via repo.mergeSettings.get)."),
    ("pr diff", "ox pr diff <o/n> <number>     print unified patches for changed files"),
    ("pr files", "ox pr files <o/n> <number>    list changed files + status"),
    ("pr review", "ox pr review <o/n> <number> --approve|--request-changes|--comment [--body <b>]\n               --list | --dismiss <review-id> [--reason <r>]\n               --request <user> | --unrequest <user>"),
    ("pr comment", "ox pr comment <o/n> <number> [--body <b> | --body-file -]"),
    ("pr ready", "ox pr ready <o/n> <number>           mark ready for review"),
    ("pr update-branch", "ox pr update-branch <o/n> <number>   merge base into the head branch"),
    ("pr checks", "ox pr checks <o/n> <number>          commit statuses on the head"),
    ("pr close", "ox pr close <o/n> <number>   /   reopen"),
    ("pr reopen", "ox pr reopen <o/n> <number>"),
    ("pr checkout", "ox pr checkout <o/n> <number>  fetch the head into a local branch"),
    ("run", "ox run — actions runs\n\n  list|view|watch|rerun|cancel"),
    ("run watch", "ox run watch <o/n> <run-id> [--interval <secs>] [--timeout <secs>]\n                  [--exit-status]\n\nPolls the run until it completes (success|failure|cancelled), printing\nstatus changes as they happen and the final run on completion. Default\ninterval is 5s; --exit-status exits nonzero on failure/cancelled."),
    ("run list", "ox run list <o/n> [--limit n] [--page n] [--status s] [--event e]\n            [--branch b] [--workflow w] [--actor u] [--query q]"),
    ("run view", "ox run view <o/n> <run-id> [--log] [--job <job-id>]\n\n--log streams the job log (first job unless --job)."),
    ("run rerun", "ox run rerun <o/n> <run-id> [--failed-only]"),
    ("run cancel", "ox run cancel <o/n> <run-id>"),
    ("workflow", "ox workflow — workflow files\n\n  list|view|run"),
    ("workflow list", "ox workflow list <o/n> [--ref <r>]"),
    ("workflow view", "ox workflow view <o/n> <path-or-name> [--ref <r>]\n\nPrints the workflow file (repo.blob)."),
    ("workflow run", "ox workflow run <o/n> <workflow-id> [--ref <r>]\n\nDispatches repo.actions.dispatchWorkflow; --ref defaults to the repo's\ndefault branch."),
    ("secret", "ox secret — actions secrets\n\n  list|set|delete"),
    ("secret list", "ox secret list <o/n>"),
    ("secret set", "ox secret set <o/n> <NAME> [--body <v> | --file <path>]\n\nWith no --body/--file, a piped stdin supplies the value."),
    ("secret delete", "ox secret delete <o/n> <NAME>"),
    ("release", "ox release — releases\n\n  list|view|create|edit|delete|upload|download|delete-asset"),
    ("release list", "ox release list <o/n>"),
    ("release view", "ox release view <o/n> <tag>"),
    ("release create", "ox release create <o/n> <tag> [--title <t>] [--body <b>]\n                 [--draft] [--prerelease]"),
    ("release edit", "ox release edit <o/n> <tag> [--title <t>] [--body <b>]\n               [--draft|--publish] [--prerelease|--no-prerelease]"),
    ("release delete", "ox release delete <o/n> <tag>"),
    ("release upload", "ox release upload <o/n> <tag> <file>...\n\nUploads each file as a release asset (multipart POST)."),
    ("release download", "ox release download <o/n> <tag> [--pattern <glob>] [--dir <d>] [--clobber]"),
    ("release delete-asset", "ox release delete-asset <o/n> <tag> <filename>"),
    ("search", "ox search <query> [--type repos|issues|pulls|users|orgs|commits|code]...\n           [--limit <n>]"),
    ("browse", "ox browse [<o/n>] [--print]\n\nOpens {instance}/{owner}/{name} (or the instance root) in a browser."),
    ("status", "ox status create <o/n> --sha <s> --context <c>\n                --state pending|success|failure|error\n                [--description <d>] [--target-url <u>]"),
    ("org", "ox org — organizations\n\n  list|view|create|members|invites"),
    ("org list", "ox org list"),
    ("org view", "ox org view <slug>"),
    ("org create", "ox org create <slug> [--name <display-name>]"),
    ("org members", "ox org members <slug>                      list\n  ox org members add|remove|set-role <slug> <user> [--role owner|admin|member]"),
    ("org invites", "ox org invites <slug>                        list\n  ox org invites create <slug> --emails a@x,b@y [--role <r>]\n  ox org invites link <slug> [--role <r>] [--expires <iso>] [--max-uses <n>]\n  ox org invites revoke <slug> --id <invite-id>"),
    ("notification", "ox notification — inbox\n\n  list [--all] [--limit n] · read [<id>...|--all] · unread (count)"),
    ("user", "ox user — profiles\n\n  view <user> · lookup <prefix> · follow|unfollow <user> · starred · watched"),
    ("ssh-key", "ox ssh-key list · add --title <t> --key-file <path> [--no-auth|--no-sign]\n           delete --id <id>      (key-file '-' = stdin)"),
    ("gpg-key", "ox gpg-key list · add --title <t> --key-file <path> · delete --id <id>"),
    ("email", "ox email list · add <addr> · remove --id <id> · set-primary --id <id>\n         resend --id <id>"),
    ("label", "ox label list <o/n>|--org <slug>\n  ox label create <o/n>|--org <slug> --name <n> --color <hex> [--description <d>]\n  ox label edit <o/n>|--org <slug> --id <id> [--name][--color][--description][--hidden]\n  ox label delete <o/n>|--org <slug> --id <id>"),
    ("webhook", "ox webhook list|create|edit|delete|ping <o/n>\n  create --url <u> --secret <s> --events push,pull_request [--name <n>]\n         [--description <d>] [--inactive]\n  edit --id <id> [--url][--secret][--events][--name][--description][--inactive|--active]\n  ping --id <id> · deliveries <o/n> --id <id> [--limit n]\n  redeliver <o/n> --id <id> --delivery <delivery-id>"),
    ("self-update", "ox self-update [--no-skill-refresh] [--no-mcp-refresh]\n\nDownloads the ox build for this platform the configured instance\nserves, verifies the published sha256, and swaps this executable.\nAfter a successful swap, existing `ox skill` installs and `oxidean`\nMCP config entries are refreshed to the new binary's shape (same\nsweeps as `ox skill install --upgrade` / `ox mcp install --upgrade`,\npreserving each entry's endpoint and credential);\n--no-skill-refresh / --no-mcp-refresh skip those steps."),
    ("skill", "ox skill — the consumer-facing agent SKILL.md\n\n  ox skill print     write SKILL.md to stdout\n  ox skill install   write SKILL.md into harness skill dirs\n\nThe document is embedded in this binary and also served by the\ninstance at /cli/skill.md."),
    ("skill print", "ox skill print\n\nPrints the embedded SKILL.md verbatim — `ox skill print > SKILL.md`\nor pipe into `ox skill install`-style tooling."),
    ("skill install", "ox skill install [--harness <id,...> | --harness all] [--detected]\n                 [--upgrade] [--project | --global]\n\nWrites the embedded SKILL.md to <skills-dir>/ox/SKILL.md for each\nharness. --detected selects only harnesses found installed on this\nmachine (config dirs, PATH binaries); combined with --harness it\nintersects. On a TTY it prompts for harnesses (detected ones marked,\n'd' selects them all) and scope. Non-interactive requires --harness\nor --detected, plus a scope flag.\n\n--upgrade rewrites only ox/SKILL.md files that already exist — it\ncreates no new installs and never prompts. Bare --upgrade sweeps all\nharnesses at both scopes; --harness/--detected/scope flags narrow it.\n\nHarness ids follow the vercel-labs/skills registry (~70 rows: claude,\ncursor, codex, copilot, gemini, opencode, amp, devin, windsurf, kiro,\nroo, goose, cline, openclaw, continue, trae, qwen, grok, …) — `generic`\ncovers any other `.agents/skills` consumer"),
    ("mcp", "ox mcp — wire the instance MCP endpoint into agent harnesses\n\n  ox mcp print     emit the config block to paste by hand\n  ox mcp install   merge an `oxidean` entry into harness MCP configs\n  ox mcp status    show what's configured + probe the endpoint\n  ox mcp remove    strip the `oxidean` entry again\n\nThe endpoint is POST {instance}/api/mcp (streamable HTTP, JSON-RPC 2.0).\nHarnesses: claude, cursor, vscode, windsurf, gemini, codex, kiro,\nopencode, zed — `generic` covers any `mcpServers`-shaped config."),
    ("mcp print", "ox mcp print [--harness <id>]\n\nPrints the JSON block (or Codex TOML section) for manual pasting.\nThe bearer is a `<token>` placeholder — substitute a PAT from\n{instance}/settings/tokens."),
    ("mcp install", "ox mcp install [--harness <id,...> | --harness all] [--detected]\n               [--project | --global] [--upgrade]\n               [--token <pat> | --token-stdin | --web | --anonymous]\n\nMerges an `oxidean` server entry into each harness's MCP config file\n— existing servers and unrelated keys are preserved, and the\n`oxidean` entry is replaced in place on re-runs (idempotent update).\nFiles are written 0600 since they may embed the bearer.\n\n--upgrade refreshes only files that already carry an `oxidean` entry\n— never prompts, never creates files; bare invocation sweeps every\nharness at both scopes. Entries keep their existing credential\nunless an explicit auth flag is passed, so\n`ox mcp install --upgrade --token-stdin` rotates tokens everywhere.\n\nAuth resolution: --token/--token-stdin use that PAT; --web runs the\nRFC 8628 device grant (browser sign-in, stored like `ox auth login`);\nwith no auth flag the stored `ox` credential for the instance is\nused; --anonymous writes no auth header (public repos only).\n\n--detected selects only harnesses found installed (config dirs, PATH\nbinaries); combined with --harness it intersects. On a TTY it prompts\nfor harnesses and scope; non-interactive requires --harness or\n--detected plus a scope flag.\n\nCodex writes `bearer_token_env_var = \"OXIDEAN_TOKEN\"` instead of the\ntoken itself — export that env var for the harness to pick it up."),
    ("mcp status", "ox mcp status\n\nLists every harness MCP config file found on this machine — whether\nit has an `oxidean` entry, which endpoint it points at, and its auth\nkind (never the token itself) — then POSTs `initialize` + `tools/list`\nat the resolved endpoint to report reachable/disabled/401."),
    ("mcp remove", "ox mcp remove [--harness <id,...> | --harness all] [--detected]\n              [--project | --global]\n\nDeletes only the `oxidean` server entry from each config file — other\nservers and keys are untouched, and files without an entry report\n`skipped (no entry)`."),
    ("api", "ox api <procedure> [-f field=value]... [-F field=<json>]...\n\nRaw RPC escape hatch. -f values are strings; -F parses JSON."),
];

/// argv parse failure — process exit code 2.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UsageError(pub String);

fn usage(msg: impl Into<String>) -> UsageError {
    UsageError(msg.into())
}

/// Flags shared by every command (extracted before subcommand parsing).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Global {
    /// `--instance` — highest-priority instance URL override.
    pub instance: Option<String>,
    /// `--json` — emit the raw RPC response envelope.
    pub json: bool,
    /// `--jq <filter>` — run a jq expression over the envelope `data`.
    pub jq: Option<String>,
    /// `-h` / `--help`.
    pub help: bool,
    /// `--version`.
    pub version: bool,
}

/// List filters that map 1:1 onto `issue.list` / `pull.list` request fields.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ListFilters {
    pub state: Option<String>,
    pub author: Option<String>,
    pub assignee: Option<String>,
    pub label: Option<String>,
    pub q: Option<String>,
    pub limit: Option<u64>,
}

/// `[--body <b> | --body-file <path>]` — `-` reads stdin.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct BodyArg {
    pub body: Option<String>,
    pub body_file: Option<String>,
}

/// Run mutation input shared by `run rerun` / `run cancel`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RunMutation {
    pub job_id: Option<String>,
    pub failed_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    AuthLogin {
        token: Option<String>,
        token_stdin: bool,
        /// `--web` — RFC 8628 device authorization grant (browser sign-in).
        web: bool,
        /// `--scopes <list>` — requested scopes for `--web` (comma/space
        /// separated classic scope names).
        scopes: Option<String>,
    },
    AuthStatus,
    AuthLogout,
    AuthToken,
    /// `auth refresh` — mint a fresh CLI token through the device grant and
    /// self-revoke the old one (gh `auth refresh` analog).
    AuthRefresh {
        /// `--scopes <list>` — scopes for the new token; default keeps the
        /// current token's scopes.
        scopes: Option<String>,
        /// `--reset-scopes` — drop stored scopes back to the CLI default set.
        reset_scopes: bool,
    },
    /// `auth revoke` — revoke the current stored token server-side and drop
    /// the local credential.
    AuthRevoke,
    /// `auth setup-git` — register `ox` as a git credential helper for the
    /// resolved instance host.
    AuthSetupGit,
    /// `auth git-credential <get|store|erase>` — git credential-helper
    /// protocol on stdin/stdout (wired by `auth setup-git`).
    AuthGitCredential {
        op: String,
    },
    AuthSwitch {
        instance: Option<String>,
    },
    ConfigList,
    ConfigGet {
        key: String,
    },
    ConfigSet {
        key: String,
        value: String,
    },
    Completion {
        shell: String,
    },
    RepoList {
        owner: Option<String>,
    },
    RepoView {
        repo: String,
    },
    RepoCreate {
        name: Option<String>,
        owner: Option<String>,
        description: Option<String>,
        visibility: Option<String>,
        license: Option<String>,
        gitignore: Option<String>,
        template: Option<String>,
    },
    RepoTemplates,
    RepoFork {
        repo: String,
        into_owner: Option<String>,
        into_name: Option<String>,
    },
    RepoSync {
        repo: String,
    },
    RepoClone {
        repo: String,
        dir: Option<String>,
    },
    RepoEdit {
        repo: String,
        description: Option<String>,
        homepage: Option<String>,
        topics: Option<Vec<String>>,
        visibility: Option<String>,
    },
    RepoRename {
        repo: String,
        new_name: String,
    },
    RepoSetArchived {
        repo: String,
        archived: bool,
    },
    RepoDelete {
        repo: String,
        confirm: String,
    },
    RepoTransfer {
        repo: String,
        dest_owner: String,
        dest_owner_type: String,
        confirm: String,
    },
    RepoStar {
        repo: String,
        star: bool,
    },
    RepoWatch {
        repo: String,
        watch: bool,
    },
    DeployKeyList {
        repo: String,
    },
    DeployKeyAdd {
        repo: String,
        title: String,
        key_file: String,
        can_write: bool,
    },
    DeployKeyDelete {
        repo: String,
        id: String,
    },
    CollaboratorList {
        repo: String,
    },
    CollaboratorAdd {
        repo: String,
        username: String,
        permission: String,
    },
    CollaboratorRemove {
        repo: String,
        username: String,
    },
    CollaboratorSetRole {
        repo: String,
        username: String,
        permission: String,
    },
    RepoInviteList {
        repo: String,
    },
    RepoInviteCreate {
        repo: String,
        emails: Vec<String>,
        permission: String,
    },
    RepoInviteLink {
        repo: String,
        permission: String,
        expires_at: Option<String>,
        max_uses: Option<u64>,
    },
    RepoInviteRevoke {
        repo: String,
        id: String,
    },
    BranchProtectionList {
        repo: String,
    },
    BranchProtectionSet {
        repo: String,
        pattern: String,
        opts: Value,
    },
    BranchProtectionDelete {
        repo: String,
        id: String,
    },
    TagProtectionList {
        repo: String,
    },
    TagProtectionSet {
        repo: String,
        pattern: String,
        opts: Value,
    },
    TagProtectionDelete {
        repo: String,
        id: String,
    },
    MirrorGet {
        repo: String,
    },
    MirrorSet {
        repo: String,
        opts: Value,
    },
    MirrorSync {
        repo: String,
    },
    MirrorDelete {
        repo: String,
    },
    MirrorGenKey {
        repo: String,
    },
    MirrorRotateSecret {
        repo: String,
    },
    MergeSettings {
        repo: String,
        merge: Option<bool>,
        squash: Option<bool>,
        rebase: Option<bool>,
    },
    RepoLfs {
        repo: String,
        usage: bool,
        objects: bool,
        enable: Option<bool>,
    },
    RepoQuota {
        repo: String,
        set_bytes: Option<u64>,
    },
    IssueList {
        repo: String,
        filters: ListFilters,
    },
    IssueView {
        repo: String,
        number: u64,
    },
    IssueCreate {
        repo: String,
        title: String,
        body: BodyArg,
    },
    IssueEdit {
        repo: String,
        number: u64,
        title: Option<String>,
        body: BodyArg,
        labels: Vec<String>,
        assignees: Vec<String>,
    },
    IssueClose {
        repo: String,
        number: u64,
    },
    IssueReopen {
        repo: String,
        number: u64,
    },
    IssueDelete {
        repo: String,
        number: u64,
    },
    IssueComment {
        repo: String,
        number: u64,
        body: BodyArg,
    },
    PrList {
        repo: String,
        filters: ListFilters,
    },
    PrView {
        repo: String,
        number: u64,
    },
    PrChecks {
        repo: String,
        number: u64,
    },
    PrCreate {
        repo: String,
        title: String,
        body: BodyArg,
        base: String,
        head: String,
        head_owner: Option<String>,
        head_name: Option<String>,
        draft: bool,
    },
    PrMerge {
        repo: String,
        number: u64,
        method: Option<String>,
        commit_title: Option<String>,
        commit_message: Option<String>,
        delete_branch: bool,
    },
    PrDiff {
        repo: String,
        number: u64,
    },
    PrFiles {
        repo: String,
        number: u64,
    },
    PrReview {
        repo: String,
        number: u64,
        action: ReviewAction,
        body: BodyArg,
        dismiss_id: Option<String>,
        reason: Option<String>,
        request: Option<String>,
        unrequest: Option<String>,
        list: bool,
    },
    PrComment {
        repo: String,
        number: u64,
        body: BodyArg,
    },
    PrReady {
        repo: String,
        number: u64,
    },
    PrUpdateBranch {
        repo: String,
        number: u64,
    },
    PrClose {
        repo: String,
        number: u64,
    },
    PrReopen {
        repo: String,
        number: u64,
    },
    PrCheckout {
        repo: String,
        number: u64,
    },
    RunList {
        repo: String,
        page: Option<u64>,
        per_page: Option<u64>,
        status: Option<String>,
        event: Option<String>,
        branch: Option<String>,
        workflow: Option<String>,
        actor: Option<String>,
        query: Option<String>,
    },
    RunView {
        repo: String,
        run_id: String,
        job: Option<String>,
        log: bool,
    },
    RunWatch {
        repo: String,
        run_id: String,
        interval: Option<u64>,
        timeout: Option<u64>,
        exit_status: bool,
    },
    RunRerun {
        repo: String,
        run_id: String,
        m: RunMutation,
    },
    RunCancel {
        repo: String,
        run_id: String,
        m: RunMutation,
    },
    WorkflowList {
        repo: String,
        git_ref: Option<String>,
    },
    WorkflowView {
        repo: String,
        path: String,
        git_ref: Option<String>,
    },
    WorkflowRun {
        repo: String,
        workflow_id: String,
        git_ref: Option<String>,
    },
    SecretList {
        repo: String,
    },
    SecretSet {
        repo: String,
        name: String,
        body: BodyArg,
    },
    SecretDelete {
        repo: String,
        name: String,
    },
    ReleaseList {
        repo: String,
    },
    ReleaseView {
        repo: String,
        tag: String,
    },
    ReleaseCreate {
        repo: String,
        tag: String,
        title: Option<String>,
        body: BodyArg,
        draft: bool,
        prerelease: bool,
    },
    ReleaseEdit {
        repo: String,
        tag: String,
        title: Option<String>,
        body: BodyArg,
        draft: Option<bool>,
        prerelease: Option<bool>,
    },
    ReleaseDelete {
        repo: String,
        tag: String,
    },
    ReleaseUpload {
        repo: String,
        tag: String,
        files: Vec<String>,
    },
    ReleaseDownload {
        repo: String,
        tag: String,
        pattern: Option<String>,
        dir: Option<String>,
        clobber: bool,
    },
    ReleaseDeleteAsset {
        repo: String,
        tag: String,
        filename: String,
    },
    Search {
        q: String,
        types: Vec<String>,
        limit: Option<u64>,
    },
    Browse {
        repo: Option<String>,
        print: bool,
    },
    StatusCreate {
        repo: String,
        sha: String,
        context: String,
        state: String,
        description: Option<String>,
        target_url: Option<String>,
    },
    OrgList,
    OrgView {
        slug: String,
    },
    OrgCreate {
        slug: String,
        name: Option<String>,
    },
    OrgMembers {
        slug: String,
    },
    OrgMemberAdd {
        slug: String,
        username: String,
        role: String,
    },
    OrgMemberRemove {
        slug: String,
        username: String,
    },
    OrgMemberRole {
        slug: String,
        username: String,
        role: String,
    },
    OrgInviteList {
        slug: String,
    },
    OrgInviteCreate {
        slug: String,
        emails: Vec<String>,
        role: String,
    },
    OrgInviteLink {
        slug: String,
        role: String,
        expires_at: Option<String>,
        max_uses: Option<u64>,
    },
    OrgInviteRevoke {
        slug: String,
        id: String,
    },
    NotificationList {
        all: bool,
        limit: Option<u64>,
        offset: Option<u64>,
    },
    NotificationRead {
        ids: Vec<String>,
        all: bool,
    },
    NotificationUnread,
    UserView {
        username: String,
    },
    UserLookup {
        prefix: String,
    },
    UserFollow {
        username: String,
        follow: bool,
    },
    UserStarred,
    UserWatched,
    SshKeyList,
    SshKeyAdd {
        title: String,
        key_file: String,
        can_auth: bool,
        can_sign: bool,
    },
    SshKeyDelete {
        id: String,
    },
    GpgKeyList,
    GpgKeyAdd {
        title: String,
        key_file: String,
    },
    GpgKeyDelete {
        id: String,
    },
    EmailList,
    EmailAdd {
        email: String,
    },
    EmailRemove {
        id: String,
    },
    EmailSetPrimary {
        id: String,
    },
    EmailResend {
        id: String,
    },
    LabelList {
        repo: Option<String>,
        org: Option<String>,
        include_hidden: bool,
    },
    LabelCreate {
        repo: Option<String>,
        org: Option<String>,
        name: String,
        color: String,
        description: Option<String>,
    },
    LabelEdit {
        repo: Option<String>,
        org: Option<String>,
        id: String,
        name: Option<String>,
        color: Option<String>,
        description: Option<String>,
        hidden: Option<bool>,
    },
    LabelDelete {
        repo: Option<String>,
        org: Option<String>,
        id: String,
    },
    WebhookList {
        repo: String,
    },
    WebhookCreate {
        repo: String,
        url: String,
        secret: String,
        events: Vec<String>,
        active: bool,
        description: Option<String>,
    },
    WebhookEdit {
        repo: String,
        id: String,
        url: Option<String>,
        secret: Option<String>,
        events: Option<Vec<String>>,
        active: Option<bool>,
        description: Option<String>,
    },
    WebhookDelete {
        repo: String,
        id: String,
    },
    WebhookPing {
        repo: String,
        id: String,
    },
    WebhookDeliveries {
        repo: String,
        hook_id: String,
        limit: Option<u64>,
    },
    WebhookDelivery {
        repo: String,
        hook_id: String,
        delivery_id: String,
    },
    WebhookRedeliver {
        repo: String,
        hook_id: String,
        delivery_id: String,
    },
    PkgList {
        owner: Option<String>,
    },
    SkillPrint,
    SkillInstall {
        harnesses: Vec<String>,
        project: bool,
        global: bool,
        upgrade: bool,
        detected: bool,
    },
    McpPrint {
        harness: Option<String>,
    },
    McpInstall {
        harnesses: Vec<String>,
        project: bool,
        global: bool,
        detected: bool,
        token: Option<String>,
        token_stdin: bool,
        web: bool,
        anonymous: bool,
        /// Refresh existing `oxidean` entries only — never prompts or creates.
        upgrade: bool,
    },
    McpStatus,
    McpRemove {
        harnesses: Vec<String>,
        project: bool,
        global: bool,
        detected: bool,
    },
    SelfUpdate {
        /// Skip the post-update `skill install --upgrade` sweep.
        no_skill_refresh: bool,
        /// Skip the post-update `mcp install --upgrade` sweep.
        no_mcp_refresh: bool,
    },
    Api {
        procedure: String,
        input: Value,
    },
    /// `--help` attached to a command path — prints that topic's help.
    HelpTopic(String),
    Help,
    Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewAction {
    /// `pull.reviews.submit` state arg: approved | changes_requested | commented.
    Submit(&'static str),
    /// No flag — default to a plain comment review.
    Comment,
}

#[derive(Debug)]
pub struct Invocation {
    pub global: Global,
    pub command: Command,
}

/// Scanned subcommand tail: positionals plus recognized flags.
#[derive(Debug, Default)]
struct Flags {
    positionals: Vec<String>,
    values: HashMap<String, String>,
    multi: Vec<(String, String)>,
    bools: HashSet<String>,
}

/// Scan a subcommand tail. `value_flags` take a following (or `=`-attached)
/// value, `bool_flags` are bare switches, `multi_flags` are repeatable
/// value flags collected in order (`-f` / `-F`). `--` ends flag parsing.
fn scan(
    args: &[String],
    value_flags: &[&str],
    bool_flags: &[&str],
    multi_flags: &[&str],
) -> Result<Flags, UsageError> {
    let mut out = Flags::default();
    let mut positional_only = false;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if positional_only {
            out.positionals.push(arg.to_string());
            i += 1;
            continue;
        }
        if arg == "--" {
            positional_only = true;
            i += 1;
            continue;
        }
        if let Some(eq) = arg.find('=') {
            let (name, inline) = (&arg[..eq], &arg[eq + 1..]);
            if value_flags.contains(&name) {
                out.values.insert(name.to_string(), inline.to_string());
                i += 1;
                continue;
            }
            if multi_flags.contains(&name) {
                out.multi.push((name.to_string(), inline.to_string()));
                i += 1;
                continue;
            }
            if name.starts_with('-') {
                return Err(usage(format!("unknown flag: {name}")));
            }
            // `=` in a bare positional — keep it as a positional.
        }
        if bool_flags.contains(&arg) {
            out.bools.insert(arg.to_string());
            i += 1;
            continue;
        }
        if value_flags.contains(&arg) {
            let value = args
                .get(i + 1)
                .ok_or_else(|| usage(format!("{arg} requires a value")))?;
            out.values.insert(arg.to_string(), value.clone());
            i += 2;
            continue;
        }
        if multi_flags.contains(&arg) {
            let value = args
                .get(i + 1)
                .ok_or_else(|| usage(format!("{arg} requires a value")))?;
            out.multi.push((arg.to_string(), value.clone()));
            i += 2;
            continue;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            return Err(usage(format!("unknown flag: {arg}")));
        }
        out.positionals.push(arg.to_string());
        i += 1;
    }
    Ok(out)
}

/// Validate `owner/name`, returning the split parts.
pub fn parse_repo(spec: &str) -> Result<(String, String), UsageError> {
    let bad = || usage(format!("invalid repo '{spec}' — expected <owner>/<name>"));
    let (owner, name) = spec.split_once('/').ok_or_else(bad)?;
    if owner.is_empty()
        || name.is_empty()
        || name.contains('/')
        || owner.trim() != owner
        || name.trim() != name
    {
        return Err(bad());
    }
    Ok((owner.to_string(), name.to_string()))
}

/// `<owner/name>` from the first positional, `-R`/`--repo`, `OXIDEAN_REPO`,
/// or the git remote in the current directory (`upstream`, then `origin`,
/// then `fork` — gh's remote priority).
fn repo_spec(f: &Flags, usage_str: &str) -> Result<String, UsageError> {
    let raw = f
        .positionals
        .first()
        .or_else(|| f.values.get("-R"))
        .or_else(|| f.values.get("--repo"))
        .cloned()
        .or_else(detected_repo)
        .ok_or_else(|| usage(format!("usage: ox {usage_str} <owner/name>")))?;
    parse_repo(&raw)?;
    Ok(raw)
}

/// `OXIDEAN_REPO` → git remote URL (`upstream`/`origin`/`fork`).
fn detected_repo() -> Option<String> {
    if let Ok(v) = std::env::var("OXIDEAN_REPO") {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    repo_from_git_remote()
}

fn repo_from_git_remote() -> Option<String> {
    for remote in ["upstream", "origin", "fork"] {
        let out = std::process::Command::new("git")
            .args(["remote", "get-url", remote])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if out.status.success() {
            let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(spec) = remote_url_spec(&url) {
                return Some(spec);
            }
        }
    }
    None
}

/// `https://host/o/n(.git)` | `git@host:o/n(.git)` | `ssh://git@host/o/n`
/// → `owner/name`. Non-repo-shaped URLs return `None`.
pub fn remote_url_spec(url: &str) -> Option<String> {
    let path = if let Some((_, rest)) = url.split_once("://") {
        // scheme://authority/path — strip the authority.
        match rest.split_once('/') {
            Some((_, p)) => p.to_string(),
            None => return None,
        }
    } else if let Some((_, p)) = url.split_once(':') {
        // scp-like `git@host:o/n`.
        p.to_string()
    } else {
        url.to_string()
    };
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let mut it = path.split('/');
    match (it.next(), it.next(), it.next()) {
        (Some(o), Some(n), None) if !o.is_empty() && !n.is_empty() => Some(format!("{o}/{n}")),
        _ => None,
    }
}

/// Repo spec plus one trailing positional (issue number, run id, …). When
/// `-R`/`--repo` supplies the repo, positionals[0] is the trailing arg; a
/// single positional with a detected repo is likewise the trailing arg.
fn repo_and_arg(
    f: &Flags,
    usage_str: &str,
    arg_name: &str,
) -> Result<(String, String), UsageError> {
    let want = || usage(format!("usage: ox {usage_str} <owner/name> <{arg_name}>"));
    if f.values.contains_key("-R") || f.values.contains_key("--repo") {
        let repo = repo_spec(f, usage_str)?;
        let arg = f.positionals.first().ok_or_else(want)?;
        return Ok((repo, arg.clone()));
    }
    match f.positionals.len() {
        0 => Err(want()),
        1 => {
            let arg = f.positionals[0].clone();
            let repo = detected_repo().ok_or_else(want)?;
            parse_repo(&repo)?;
            Ok((repo, arg))
        }
        _ => {
            let repo = f.positionals.first().ok_or_else(want)?;
            parse_repo(repo)?;
            let arg = f.positionals.get(1).ok_or_else(want)?;
            Ok((repo.clone(), arg.clone()))
        }
    }
}

fn repo_and_number(f: &Flags, usage_str: &str) -> Result<(String, u64), UsageError> {
    let (repo, raw) = repo_and_arg(f, usage_str, "number")?;
    let number = raw
        .parse::<u64>()
        .map_err(|_| usage(format!("invalid {usage_str} number: {raw}")))?;
    Ok((repo, number))
}

/// Reject stray positionals on commands that take none.
fn expect_no_positionals(f: &Flags, usage_str: &str) -> Result<(), UsageError> {
    if let Some(extra) = f.positionals.first() {
        return Err(usage(format!(
            "ox {usage_str}: unexpected argument '{extra}'"
        )));
    }
    Ok(())
}

fn list_filters(f: &Flags) -> Result<ListFilters, UsageError> {
    let limit = match f.values.get("--limit") {
        Some(v) => Some(
            v.parse::<u64>()
                .map_err(|_| usage(format!("invalid --limit: {v}")))?,
        ),
        None => None,
    };
    Ok(ListFilters {
        state: f.values.get("--state").cloned(),
        author: f.values.get("--author").cloned(),
        assignee: f.values.get("--assignee").cloned(),
        label: f.values.get("--label").cloned(),
        q: f.values.get("--search").cloned(),
        limit,
    })
}

fn opt_u64(f: &Flags, name: &str) -> Result<Option<u64>, UsageError> {
    match f.values.get(name) {
        Some(v) => Ok(Some(
            v.parse::<u64>()
                .map_err(|_| usage(format!("invalid {name}: {v}")))?,
        )),
        None => Ok(None),
    }
}

fn req(f: &Flags, name: &str, usage_str: &str) -> Result<String, UsageError> {
    f.values
        .get(name)
        .cloned()
        .ok_or_else(|| usage(format!("ox {usage_str}: {name} is required")))
}

/// `--id <id>` required flag shorthand.
fn req_id(f: &Flags, usage_str: &str) -> Result<String, UsageError> {
    req(f, "--id", usage_str)
}

fn csv(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn body_arg(f: &Flags) -> Result<BodyArg, UsageError> {
    Ok(BodyArg {
        body: f.values.get("--body").cloned(),
        body_file: f.values.get("--body-file").cloned(),
    })
}

/// `-f` values are strings; `-F` values are parsed as JSON. Flat keys only.
fn api_input(f: &Flags) -> Result<Value, UsageError> {
    let mut input = serde_json::Map::new();
    for (flag, kv) in &f.multi {
        let (key, raw) = kv
            .split_once('=')
            .ok_or_else(|| usage(format!("{flag} expects field=value, got '{kv}'")))?;
        if key.is_empty() {
            return Err(usage(format!("{flag} expects field=value, got '{kv}'")));
        }
        let value = if flag == "-F" {
            serde_json::from_str(raw)
                .map_err(|e| usage(format!("{flag} {key}: invalid JSON value: {e}")))?
        } else {
            Value::String(raw.to_string())
        };
        input.insert(key.to_string(), value);
    }
    Ok(Value::Object(input))
}

/// on|off flag value → bool.
fn onoff(f: &Flags, name: &str) -> Result<Option<bool>, UsageError> {
    match f.values.get(name).map(String::as_str) {
        Some("on" | "true" | "yes" | "1") => Ok(Some(true)),
        Some("off" | "false" | "no" | "0") => Ok(Some(false)),
        Some(other) => Err(usage(format!("invalid {name}: {other} — want on|off"))),
        None => Ok(None),
    }
}

const REPO_FLAGS: &[&str] = &["-R", "--repo"];
const LIST_FLAGS: &[&str] = &[
    "-R",
    "--repo",
    "--state",
    "--author",
    "--assignee",
    "--label",
    "--search",
    "--limit",
];
const BODY_FLAGS: &[&str] = &["--body", "--body-file"];
const PERMS: &[&str] = &["read", "write", "admin"];
const ROLES: &[&str] = &["owner", "admin", "member"];

fn check_choice(
    f: &Flags,
    name: &str,
    choices: &[&str],
    usage_str: &str,
) -> Result<(), UsageError> {
    if let Some(v) = f.values.get(name) {
        if !choices.contains(&v.as_str()) {
            return Err(usage(format!(
                "ox {usage_str}: {name} must be one of {}",
                choices.join("|")
            )));
        }
    }
    Ok(())
}

/// Third-level command groups: `ox <group> <sub> <op>`. The op word becomes
/// `leaf` only when rest[2] names a known op — otherwise it's data (a slug,
/// a repo spec) and `default` decides the leaf op.
const NESTED: &[(&str, &str, &[&str], Option<&str>)] = &[
    ("repo", "deploy-key", &["list", "add", "delete"], None),
    (
        "repo",
        "collaborator",
        &["list", "add", "set-role", "remove"],
        Some("list"),
    ),
    (
        "repo",
        "collaborators",
        &["list", "add", "set-role", "remove"],
        Some("list"),
    ),
    (
        "repo",
        "invites",
        &["list", "create", "link", "revoke"],
        Some("list"),
    ),
    (
        "repo",
        "branch-protection",
        &["list", "set", "delete"],
        None,
    ),
    ("repo", "tag-protection", &["list", "set", "delete"], None),
    (
        "repo",
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
        Some("get"),
    ),
    (
        "org",
        "members",
        &["list", "add", "remove", "set-role"],
        Some("list"),
    ),
    (
        "org",
        "invites",
        &["list", "create", "link", "revoke"],
        Some("list"),
    ),
    (
        "webhook",
        "deliveries",
        &["list", "get", "view", "redeliver"],
        Some("list"),
    ),
];

/// Split argv into globals + command. Global flags may appear anywhere;
/// everything else keeps its order and is parsed per-subcommand.
pub fn parse(argv: &[String]) -> Result<Invocation, UsageError> {
    let mut global = Global::default();
    let mut rest: Vec<String> = Vec::new();
    let mut positional_only = false;
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        if !positional_only {
            match arg {
                // Fall through to `rest` so the subcommand scanner sees the
                // end-of-flags marker too.
                "--" => positional_only = true,
                "--json" => {
                    global.json = true;
                    i += 1;
                    continue;
                }
                "-h" | "--help" => {
                    global.help = true;
                    i += 1;
                    continue;
                }
                "--version" => {
                    global.version = true;
                    i += 1;
                    continue;
                }
                "--instance" => {
                    let v = argv
                        .get(i + 1)
                        .ok_or_else(|| usage("--instance requires a value"))?;
                    global.instance = Some(v.clone());
                    i += 2;
                    continue;
                }
                "--jq" => {
                    let v = argv
                        .get(i + 1)
                        .ok_or_else(|| usage("--jq requires a filter"))?;
                    global.jq = Some(v.clone());
                    i += 2;
                    continue;
                }
                _ => {
                    if let Some(v) = arg.strip_prefix("--instance=") {
                        if v.is_empty() {
                            return Err(usage("--instance requires a value"));
                        }
                        global.instance = Some(v.to_string());
                        i += 1;
                        continue;
                    }
                    if let Some(v) = arg.strip_prefix("--jq=") {
                        if v.is_empty() {
                            return Err(usage("--jq requires a filter"));
                        }
                        global.jq = Some(v.to_string());
                        i += 1;
                        continue;
                    }
                }
            }
        }
        rest.push(argv[i].clone());
        i += 1;
    }

    if global.version {
        return Ok(Invocation {
            global,
            command: Command::Version,
        });
    }
    let Some(group) = rest.first().map(String::as_str) else {
        return Ok(Invocation {
            global,
            command: Command::Help,
        });
    };
    let sub = rest.get(1).map(String::as_str).unwrap_or("");

    // `ox <cmd> --help` — resolve a help topic without validating flags.
    if global.help {
        let topic = format!("{group} {sub}").trim().to_string();
        if help_topic(group, sub).is_some() {
            global.help = false; // resolved to a topic — don't trip the main() help path
            return Ok(Invocation {
                global,
                command: Command::HelpTopic(topic),
            });
        }
        return Err(usage(format!("unknown command: {topic} — see `ox --help`")));
    }

    // leaf is "" for two-level commands (`ox repo view <spec>`); the NESTED
    // groups above replace it with the resolved op word.
    let mut leaf = "";
    let mut tail: &[String] = &rest[rest.len().min(2)..];
    if let Some((_, _, ops, default)) = NESTED.iter().find(|(g, s, ..)| *g == group && *s == sub) {
        match rest.get(2).map(String::as_str) {
            Some(op) if ops.contains(&op) => {
                leaf = op;
                tail = &rest[3..];
            }
            _ => {
                leaf = default.unwrap_or("");
                // tail stays rest[2..] — rest[2] is data for the default op.
            }
        }
    }

    let command = match (group, sub, leaf) {
        ("auth", "login", "") => {
            let f = scan(
                tail,
                &["--token", "--scopes"],
                &["--token-stdin", "--web"],
                &[],
            )?;
            expect_no_positionals(&f, "auth login")?;
            if f.bools.contains("--web")
                && (f.values.contains_key("--token") || f.bools.contains("--token-stdin"))
            {
                return Err(usage("--web cannot be combined with --token/--token-stdin"));
            }
            if f.values.contains_key("--scopes") && !f.bools.contains("--web") {
                return Err(usage("--scopes only applies to --web sign-in"));
            }
            Command::AuthLogin {
                token: f.values.get("--token").cloned(),
                token_stdin: f.bools.contains("--token-stdin"),
                web: f.bools.contains("--web"),
                scopes: f.values.get("--scopes").cloned(),
            }
        }
        ("auth", "refresh", "") => {
            let f = scan(tail, &["--scopes"], &["--reset-scopes"], &[])?;
            expect_no_positionals(&f, "auth refresh")?;
            if f.values.contains_key("--scopes") && f.bools.contains("--reset-scopes") {
                return Err(usage("--scopes and --reset-scopes are exclusive"));
            }
            Command::AuthRefresh {
                scopes: f.values.get("--scopes").cloned(),
                reset_scopes: f.bools.contains("--reset-scopes"),
            }
        }
        ("auth", "revoke", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "auth revoke")?;
            Command::AuthRevoke
        }
        ("auth", "setup-git", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "auth setup-git")?;
            Command::AuthSetupGit
        }
        ("auth", "git-credential", "") => {
            let f = scan(tail, &[], &[], &[])?;
            let op = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox auth git-credential <get|store|erase>"))?;
            if f.positionals.len() > 1 {
                return Err(usage("usage: ox auth git-credential <get|store|erase>"));
            }
            if !["get", "store", "erase"].contains(&op.as_str()) {
                return Err(usage(format!(
                    "unknown git-credential operation '{op}' — get|store|erase"
                )));
            }
            Command::AuthGitCredential { op }
        }
        ("auth", "status", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "auth status")?;
            Command::AuthStatus
        }
        ("auth", "logout", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "auth logout")?;
            Command::AuthLogout
        }
        ("auth", "token", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "auth token")?;
            Command::AuthToken
        }
        ("auth", "switch", "") => {
            let f = scan(tail, &[], &[], &[])?;
            if f.positionals.len() > 1 {
                return Err(usage("usage: ox auth switch [<instance-url>]"));
            }
            Command::AuthSwitch {
                instance: f.positionals.first().cloned(),
            }
        }

        ("config", "", "") | ("config", "list", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "config list")?;
            Command::ConfigList
        }
        ("config", "get", "") => {
            let f = scan(tail, &[], &[], &[])?;
            if f.positionals.len() != 1 {
                return Err(usage("usage: ox config get <key>"));
            }
            Command::ConfigGet {
                key: f.positionals[0].clone(),
            }
        }
        ("config", "set", "") => {
            let f = scan(tail, &[], &[], &[])?;
            if f.positionals.len() != 2 {
                return Err(usage("usage: ox config set <key> <value>"));
            }
            Command::ConfigSet {
                key: f.positionals[0].clone(),
                value: f.positionals[1].clone(),
            }
        }

        ("completion", sh, "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "completion")?;
            match sh {
                "bash" | "zsh" | "fish" | "pwsh" | "powershell" => Command::Completion {
                    shell: sh.to_string(),
                },
                _ => return Err(usage("usage: ox completion <bash|zsh|fish|pwsh>")),
            }
        }

        ("repo", "list", "") => {
            let f = scan(tail, &["--owner"], &[], &[])?;
            expect_no_positionals(&f, "repo list")?;
            Command::RepoList {
                owner: f.values.get("--owner").cloned(),
            }
        }
        ("repo", "view", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoView {
                repo: repo_spec(&f, "repo view")?,
            }
        }
        ("repo", "create", "") => {
            let f = scan(
                tail,
                &[
                    "--org",
                    "--owner",
                    "--description",
                    "--visibility",
                    "--license",
                    "--gitignore",
                    "--template",
                ],
                &[],
                &[],
            )?;
            let name = f.positionals.first().cloned();
            if f.positionals.len() > 1 {
                return Err(usage(format!(
                    "ox repo create: unexpected argument '{}'",
                    f.positionals[1]
                )));
            }
            check_choice(&f, "--visibility", &["public", "private"], "repo create")?;
            Command::RepoCreate {
                name,
                owner: f
                    .values
                    .get("--org")
                    .or_else(|| f.values.get("--owner"))
                    .cloned(),
                description: f.values.get("--description").cloned(),
                visibility: f.values.get("--visibility").cloned(),
                license: f.values.get("--license").cloned(),
                gitignore: f.values.get("--gitignore").cloned(),
                template: f.values.get("--template").cloned(),
            }
        }
        ("repo", "templates", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "repo templates")?;
            Command::RepoTemplates
        }
        ("repo", "fork", "") => {
            let f = scan(
                tail,
                &["-R", "--repo", "--into-owner", "--into-name"],
                &[],
                &[],
            )?;
            Command::RepoFork {
                repo: repo_spec(&f, "repo fork")?,
                into_owner: f.values.get("--into-owner").cloned(),
                into_name: f.values.get("--into-name").cloned(),
            }
        }
        ("repo", "sync", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoSync {
                repo: repo_spec(&f, "repo sync")?,
            }
        }
        ("repo", "clone", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, dir) = if f.values.contains_key("-R") || f.values.contains_key("--repo") {
                (repo_spec(&f, "repo clone")?, f.positionals.first().cloned())
            } else {
                let repo = f
                    .positionals
                    .first()
                    .ok_or_else(|| usage("usage: ox repo clone <owner/name> [<dir>]"))?;
                parse_repo(repo)?;
                (repo.clone(), f.positionals.get(1).cloned())
            };
            Command::RepoClone { repo, dir }
        }
        ("repo", "edit", "") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--description",
                    "--homepage",
                    "--topics",
                    "--visibility",
                ],
                &[],
                &[],
            )?;
            check_choice(&f, "--visibility", &["public", "private"], "repo edit")?;
            Command::RepoEdit {
                repo: repo_spec(&f, "repo edit")?,
                description: f.values.get("--description").cloned(),
                homepage: f.values.get("--homepage").cloned(),
                topics: f.values.get("--topics").map(|t| csv(t)),
                visibility: f.values.get("--visibility").cloned(),
            }
        }
        ("repo", "rename", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, new_name) = repo_and_arg(&f, "repo rename", "new-name")?;
            Command::RepoRename { repo, new_name }
        }
        ("repo", "archive", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoSetArchived {
                repo: repo_spec(&f, "repo archive")?,
                archived: true,
            }
        }
        ("repo", "unarchive", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoSetArchived {
                repo: repo_spec(&f, "repo unarchive")?,
                archived: false,
            }
        }
        ("repo", "delete", "") => {
            let f = scan(tail, &["-R", "--repo", "--confirm"], &[], &[])?;
            Command::RepoDelete {
                repo: repo_spec(&f, "repo delete")?,
                confirm: req(&f, "--confirm", "repo delete")?,
            }
        }
        ("repo", "transfer", "") => {
            let f = scan(tail, &["-R", "--repo", "--type", "--confirm"], &[], &[])?;
            let (repo, dest_owner) = repo_and_arg(&f, "repo transfer", "dest-owner")?;
            check_choice(&f, "--type", &["user", "org"], "repo transfer")?;
            Command::RepoTransfer {
                repo,
                dest_owner,
                dest_owner_type: f
                    .values
                    .get("--type")
                    .cloned()
                    .unwrap_or_else(|| "user".to_string()),
                confirm: req(&f, "--confirm", "repo transfer")?,
            }
        }
        ("repo", "star", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoStar {
                repo: repo_spec(&f, "repo star")?,
                star: true,
            }
        }
        ("repo", "unstar", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoStar {
                repo: repo_spec(&f, "repo unstar")?,
                star: false,
            }
        }
        ("repo", "watch", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoWatch {
                repo: repo_spec(&f, "repo watch")?,
                watch: true,
            }
        }
        ("repo", "unwatch", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoWatch {
                repo: repo_spec(&f, "repo unwatch")?,
                watch: false,
            }
        }
        ("repo", "deploy-key", "list") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::DeployKeyList {
                repo: repo_spec(&f, "repo deploy-key list")?,
            }
        }
        ("repo", "deploy-key", "add") => {
            let f = scan(
                tail,
                &["-R", "--repo", "--title", "--key-file", "--key"],
                &["--write"],
                &[],
            )?;
            Command::DeployKeyAdd {
                repo: repo_spec(&f, "repo deploy-key add")?,
                title: req(&f, "--title", "repo deploy-key add")?,
                key_file: f
                    .values
                    .get("--key-file")
                    .or_else(|| f.values.get("--key"))
                    .cloned()
                    .ok_or_else(|| usage("repo deploy-key add requires --key-file <path>"))?,
                can_write: f.bools.contains("--write"),
            }
        }
        ("repo", "deploy-key", "delete") => {
            let f = scan(tail, &["-R", "--repo", "--id"], &[], &[])?;
            Command::DeployKeyDelete {
                repo: repo_spec(&f, "repo deploy-key delete")?,
                id: req_id(&f, "repo deploy-key delete")?,
            }
        }
        ("repo", "collaborator", "list") | ("repo", "collaborators", "list") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::CollaboratorList {
                repo: repo_spec(&f, "repo collaborator list")?,
            }
        }
        ("repo", "collaborator", "add") | ("repo", "collaborators", "add") => {
            let f = scan(tail, &["-R", "--repo", "--permission"], &[], &[])?;
            check_choice(&f, "--permission", PERMS, "repo collaborator add")?;
            let (repo, username) = repo_and_arg(&f, "repo collaborator add", "user")?;
            Command::CollaboratorAdd {
                repo,
                username,
                permission: f
                    .values
                    .get("--permission")
                    .cloned()
                    .unwrap_or_else(|| "write".to_string()),
            }
        }
        ("repo", "collaborator", "set-role") | ("repo", "collaborators", "set-role") => {
            let f = scan(tail, &["-R", "--repo", "--permission"], &[], &[])?;
            check_choice(&f, "--permission", PERMS, "repo collaborator set-role")?;
            let (repo, username) = repo_and_arg(&f, "repo collaborator set-role", "user")?;
            Command::CollaboratorSetRole {
                repo,
                username,
                permission: req(&f, "--permission", "repo collaborator set-role")?,
            }
        }
        ("repo", "collaborator", "remove") | ("repo", "collaborators", "remove") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, username) = repo_and_arg(&f, "repo collaborator remove", "user")?;
            Command::CollaboratorRemove { repo, username }
        }
        ("repo", "invites", "list") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::RepoInviteList {
                repo: repo_spec(&f, "repo invites list")?,
            }
        }
        ("repo", "invites", "create") => {
            let f = scan(
                tail,
                &["-R", "--repo", "--emails", "--permission"],
                &[],
                &[],
            )?;
            check_choice(&f, "--permission", PERMS, "repo invites create")?;
            Command::RepoInviteCreate {
                repo: repo_spec(&f, "repo invites create")?,
                emails: csv(&req(&f, "--emails", "repo invites create")?),
                permission: f
                    .values
                    .get("--permission")
                    .cloned()
                    .unwrap_or_else(|| "write".to_string()),
            }
        }
        ("repo", "invites", "link") => {
            let f = scan(
                tail,
                &["-R", "--repo", "--permission", "--expires", "--max-uses"],
                &[],
                &[],
            )?;
            check_choice(&f, "--permission", PERMS, "repo invites link")?;
            Command::RepoInviteLink {
                repo: repo_spec(&f, "repo invites link")?,
                permission: f
                    .values
                    .get("--permission")
                    .cloned()
                    .unwrap_or_else(|| "write".to_string()),
                expires_at: f.values.get("--expires").cloned(),
                max_uses: opt_u64(&f, "--max-uses")?,
            }
        }
        ("repo", "invites", "revoke") => {
            let f = scan(tail, &["-R", "--repo", "--id"], &[], &[])?;
            Command::RepoInviteRevoke {
                repo: repo_spec(&f, "repo invites revoke")?,
                id: req_id(&f, "repo invites revoke")?,
            }
        }
        ("repo", "branch-protection", "list") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::BranchProtectionList {
                repo: repo_spec(&f, "repo branch-protection list")?,
            }
        }
        ("repo", "branch-protection", "set") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--pattern",
                    "--approvals",
                    "--status-contexts",
                    "--require",
                ],
                &[
                    "--reviews",
                    "--dismiss-stale",
                    "--conversation-resolution",
                    "--last-push-approval",
                    "--strict-status",
                    "--force-pushes",
                    "--deletions",
                    "--enforce-admins",
                    "--linear-history",
                    "--lock",
                    "--signed-commits",
                ],
                &[],
            )?;
            let mut opts = serde_json::Map::new();
            let bool_map = [
                ("--reviews", "require_reviews"),
                ("--dismiss-stale", "dismiss_stale_reviews"),
                (
                    "--conversation-resolution",
                    "require_conversation_resolution",
                ),
                ("--last-push-approval", "require_last_push_approval"),
                ("--strict-status", "strict_status_checks"),
                ("--force-pushes", "allow_force_pushes"),
                ("--deletions", "allow_deletions"),
                ("--enforce-admins", "enforce_admins"),
                ("--linear-history", "required_linear_history"),
                ("--lock", "lock_branch"),
                ("--signed-commits", "require_signed_commits"),
            ];
            for (flag, key) in bool_map {
                if f.bools.contains(flag) {
                    opts.insert(key.to_string(), Value::Bool(true));
                }
            }
            if let Some(n) = opt_u64(&f, "--approvals")? {
                opts.insert("required_approving_review_count".into(), json_u64(n));
            }
            if let Some(cs) = f.values.get("--status-contexts") {
                opts.insert(
                    "required_status_contexts".into(),
                    Value::Array(csv(cs).into_iter().map(Value::String).collect()),
                );
            }
            Command::BranchProtectionSet {
                repo: repo_spec(&f, "repo branch-protection set")?,
                pattern: req(&f, "--pattern", "repo branch-protection set")?,
                opts: Value::Object(opts),
            }
        }
        ("repo", "branch-protection", "delete") => {
            let f = scan(tail, &["-R", "--repo", "--id"], &[], &[])?;
            Command::BranchProtectionDelete {
                repo: repo_spec(&f, "repo branch-protection delete")?,
                id: req_id(&f, "repo branch-protection delete")?,
            }
        }
        ("repo", "tag-protection", "list") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::TagProtectionList {
                repo: repo_spec(&f, "repo tag-protection list")?,
            }
        }
        ("repo", "tag-protection", "set") => {
            let f = scan(
                tail,
                &["-R", "--repo", "--pattern"],
                &[
                    "--no-create",
                    "--no-update",
                    "--no-delete",
                    "--enforce-admins",
                ],
                &[],
            )?;
            let mut opts = serde_json::Map::new();
            for (flag, key) in [
                ("--no-create", "allow_create"),
                ("--no-update", "allow_update"),
                ("--no-delete", "allow_delete"),
                ("--enforce-admins", "enforce_admins"),
            ] {
                if f.bools.contains(flag) {
                    // --no-* deny the operation; --enforce-admins enables.
                    let v = flag != "--enforce-admins";
                    opts.insert(key.to_string(), Value::Bool(!v));
                }
            }
            Command::TagProtectionSet {
                repo: repo_spec(&f, "repo tag-protection set")?,
                pattern: req(&f, "--pattern", "repo tag-protection set")?,
                opts: Value::Object(opts),
            }
        }
        ("repo", "tag-protection", "delete") => {
            let f = scan(tail, &["-R", "--repo", "--id"], &[], &[])?;
            Command::TagProtectionDelete {
                repo: repo_spec(&f, "repo tag-protection delete")?,
                id: req_id(&f, "repo tag-protection delete")?,
            }
        }
        ("repo", "mirror", "get") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::MirrorGet {
                repo: repo_spec(&f, "repo mirror get")?,
            }
        }
        ("repo", "mirror", "set") | ("repo", "mirror", "upsert") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--remote-url",
                    "--auth-kind",
                    "--username",
                    "--secret",
                    "--ssh-key",
                    "--known-hosts",
                    "--interval",
                    "--sync-mode",
                ],
                &["--disabled"],
                &[],
            )?;
            check_choice(&f, "--sync-mode", &["merge", "exact"], "repo mirror set")?;
            let mut opts = serde_json::Map::new();
            for (flag, key) in [
                ("--remote-url", "remote_url"),
                ("--auth-kind", "auth_kind"),
                ("--username", "username"),
                ("--secret", "secret"),
                ("--ssh-key", "ssh_public_key"),
                ("--known-hosts", "known_hosts"),
                ("--sync-mode", "sync_mode"),
            ] {
                if let Some(v) = f.values.get(flag) {
                    opts.insert(key.to_string(), Value::String(v.clone()));
                }
            }
            if let Some(n) = opt_u64(&f, "--interval")? {
                opts.insert("poll_interval_secs".into(), json_u64(n));
            }
            if f.bools.contains("--disabled") {
                opts.insert("enabled".into(), Value::Bool(false));
            }
            Command::MirrorSet {
                repo: repo_spec(&f, "repo mirror set")?,
                opts: Value::Object(opts),
            }
        }
        ("repo", "mirror", "sync") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::MirrorSync {
                repo: repo_spec(&f, "repo mirror sync")?,
            }
        }
        ("repo", "mirror", "delete") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::MirrorDelete {
                repo: repo_spec(&f, "repo mirror delete")?,
            }
        }
        ("repo", "mirror", "gen-key") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::MirrorGenKey {
                repo: repo_spec(&f, "repo mirror gen-key")?,
            }
        }
        ("repo", "mirror", "rotate-secret") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::MirrorRotateSecret {
                repo: repo_spec(&f, "repo mirror rotate-secret")?,
            }
        }
        ("repo", "merge-settings", "") => {
            let f = scan(
                tail,
                &["-R", "--repo", "--merge", "--squash", "--rebase"],
                &[],
                &[],
            )?;
            Command::MergeSettings {
                repo: repo_spec(&f, "repo merge-settings")?,
                merge: onoff(&f, "--merge")?,
                squash: onoff(&f, "--squash")?,
                rebase: onoff(&f, "--rebase")?,
            }
        }
        ("repo", "lfs", "") => {
            let f = scan(
                tail,
                REPO_FLAGS,
                &["--usage", "--objects", "--enable", "--disable"],
                &[],
            )?;
            let enable = match (f.bools.contains("--enable"), f.bools.contains("--disable")) {
                (true, true) => return Err(usage("repo lfs: --enable and --disable conflict")),
                (true, false) => Some(true),
                (false, true) => Some(false),
                (false, false) => None,
            };
            Command::RepoLfs {
                repo: repo_spec(&f, "repo lfs")?,
                usage: f.bools.contains("--usage"),
                objects: f.bools.contains("--objects"),
                enable,
            }
        }
        ("repo", "quota", "") => {
            let f = scan(tail, &["-R", "--repo", "--set"], &[], &[])?;
            Command::RepoQuota {
                repo: repo_spec(&f, "repo quota")?,
                set_bytes: opt_u64(&f, "--set")?,
            }
        }

        ("issue", "list", "") => {
            let f = scan(tail, LIST_FLAGS, &[], &[])?;
            Command::IssueList {
                repo: repo_spec(&f, "issue list")?,
                filters: list_filters(&f)?,
            }
        }
        ("issue", "view", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "issue view")?;
            Command::IssueView { repo, number }
        }
        ("issue", "create", "") => {
            let mut vf = vec!["-R", "--repo", "--title"];
            vf.extend(BODY_FLAGS);
            let f = scan(tail, &vf, &[], &[])?;
            let repo = repo_spec(&f, "issue create")?;
            let title = req(&f, "--title", "issue create")?;
            Command::IssueCreate {
                repo,
                title,
                body: body_arg(&f)?,
            }
        }
        ("issue", "edit", "") => {
            let mut vf = vec!["-R", "--repo", "--title"];
            vf.extend(BODY_FLAGS);
            let f = scan(tail, &vf, &[], &["--label", "--assignee"])?;
            let (repo, number) = repo_and_number(&f, "issue edit")?;
            Command::IssueEdit {
                repo,
                number,
                title: f.values.get("--title").cloned(),
                body: body_arg(&f)?,
                labels: f
                    .multi
                    .iter()
                    .filter(|(k, _)| k == "--label")
                    .flat_map(|(_, v)| csv(v))
                    .collect(),
                assignees: f
                    .multi
                    .iter()
                    .filter(|(k, _)| k == "--assignee")
                    .flat_map(|(_, v)| csv(v))
                    .collect(),
            }
        }
        ("issue", "close", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "issue close")?;
            Command::IssueClose { repo, number }
        }
        ("issue", "reopen", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "issue reopen")?;
            Command::IssueReopen { repo, number }
        }
        ("issue", "delete", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "issue delete")?;
            Command::IssueDelete { repo, number }
        }
        ("issue", "comment", "") => {
            let mut vf = vec!["-R", "--repo"];
            vf.extend(BODY_FLAGS);
            let f = scan(tail, &vf, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "issue comment")?;
            Command::IssueComment {
                repo,
                number,
                body: body_arg(&f)?,
            }
        }

        ("pr", "list", "") => {
            let f = scan(tail, LIST_FLAGS, &[], &[])?;
            Command::PrList {
                repo: repo_spec(&f, "pr list")?,
                filters: list_filters(&f)?,
            }
        }
        ("pr", "view", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr view")?;
            Command::PrView { repo, number }
        }
        ("pr", "checks", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr checks")?;
            Command::PrChecks { repo, number }
        }
        ("pr", "create", "") => {
            let mut vf = vec![
                "-R",
                "--repo",
                "--title",
                "--base",
                "--head",
                "--head-owner",
                "--head-name",
            ];
            vf.extend(BODY_FLAGS);
            let f = scan(tail, &vf, &["--draft"], &[])?;
            Command::PrCreate {
                repo: repo_spec(&f, "pr create")?,
                title: req(&f, "--title", "pr create")?,
                body: body_arg(&f)?,
                base: req(&f, "--base", "pr create")?,
                head: req(&f, "--head", "pr create")?,
                head_owner: f.values.get("--head-owner").cloned(),
                head_name: f.values.get("--head-name").cloned(),
                draft: f.bools.contains("--draft"),
            }
        }
        ("pr", "merge", "") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--method",
                    "--commit-title",
                    "--commit-message",
                ],
                &["--delete-branch", "--merge", "--squash", "--rebase"],
                &[],
            )?;
            let (repo, number) = repo_and_number(&f, "pr merge")?;
            let method = if f.bools.contains("--merge") {
                Some("merge".to_string())
            } else if f.bools.contains("--squash") {
                Some("squash".to_string())
            } else if f.bools.contains("--rebase") {
                Some("rebase".to_string())
            } else {
                f.values.get("--method").cloned()
            };
            if let Some(m) = &method {
                if !["merge", "squash", "rebase"].contains(&m.as_str()) {
                    return Err(usage(format!(
                        "pr merge: --method must be merge|squash|rebase, got '{m}'"
                    )));
                }
            }
            Command::PrMerge {
                repo,
                number,
                method,
                commit_title: f.values.get("--commit-title").cloned(),
                commit_message: f.values.get("--commit-message").cloned(),
                delete_branch: f.bools.contains("--delete-branch"),
            }
        }
        ("pr", "diff", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr diff")?;
            Command::PrDiff { repo, number }
        }
        ("pr", "files", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr files")?;
            Command::PrFiles { repo, number }
        }
        ("pr", "review", "") => {
            let mut vf = vec![
                "-R",
                "--repo",
                "--dismiss",
                "--reason",
                "--request",
                "--unrequest",
            ];
            vf.extend(BODY_FLAGS);
            let f = scan(
                tail,
                &vf,
                &["--approve", "--request-changes", "--comment", "--list"],
                &[],
            )?;
            let (repo, number) = repo_and_number(&f, "pr review")?;
            let action = if f.bools.contains("--approve") {
                ReviewAction::Submit("approved")
            } else if f.bools.contains("--request-changes") {
                ReviewAction::Submit("changes_requested")
            } else {
                ReviewAction::Comment
            };
            Command::PrReview {
                repo,
                number,
                action,
                body: body_arg(&f)?,
                dismiss_id: f.values.get("--dismiss").cloned(),
                reason: f.values.get("--reason").cloned(),
                request: f.values.get("--request").cloned(),
                unrequest: f.values.get("--unrequest").cloned(),
                list: f.bools.contains("--list"),
            }
        }
        ("pr", "comment", "") => {
            let mut vf = vec!["-R", "--repo"];
            vf.extend(BODY_FLAGS);
            let f = scan(tail, &vf, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr comment")?;
            Command::PrComment {
                repo,
                number,
                body: body_arg(&f)?,
            }
        }
        ("pr", "ready", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr ready")?;
            Command::PrReady { repo, number }
        }
        ("pr", "update-branch", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr update-branch")?;
            Command::PrUpdateBranch { repo, number }
        }
        ("pr", "close", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr close")?;
            Command::PrClose { repo, number }
        }
        ("pr", "reopen", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr reopen")?;
            Command::PrReopen { repo, number }
        }
        ("pr", "checkout", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, number) = repo_and_number(&f, "pr checkout")?;
            Command::PrCheckout { repo, number }
        }

        ("run", "list", "") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--limit",
                    "--page",
                    "--status",
                    "--event",
                    "--branch",
                    "--workflow",
                    "--actor",
                    "--query",
                ],
                &[],
                &[],
            )?;
            Command::RunList {
                repo: repo_spec(&f, "run list")?,
                page: opt_u64(&f, "--page")?,
                per_page: opt_u64(&f, "--limit")?,
                status: f.values.get("--status").cloned(),
                event: f.values.get("--event").cloned(),
                branch: f.values.get("--branch").cloned(),
                workflow: f.values.get("--workflow").cloned(),
                actor: f.values.get("--actor").cloned(),
                query: f.values.get("--query").cloned(),
            }
        }
        ("run", "view", "") => {
            let f = scan(tail, &["-R", "--repo", "--job"], &["--log"], &[])?;
            let (repo, run_id) = repo_and_arg(&f, "run view", "run-id")?;
            Command::RunView {
                repo,
                run_id,
                job: f.values.get("--job").cloned(),
                log: f.bools.contains("--log"),
            }
        }
        ("run", "rerun", "") => {
            let f = scan(tail, &["-R", "--repo", "--job"], &["--failed-only"], &[])?;
            let (repo, run_id) = repo_and_arg(&f, "run rerun", "run-id")?;
            Command::RunRerun {
                repo,
                run_id,
                m: RunMutation {
                    job_id: f.values.get("--job").cloned(),
                    failed_only: f.bools.contains("--failed-only"),
                },
            }
        }
        ("run", "cancel", "") => {
            let f = scan(tail, &["-R", "--repo", "--job"], &[], &[])?;
            let (repo, run_id) = repo_and_arg(&f, "run cancel", "run-id")?;
            Command::RunCancel {
                repo,
                run_id,
                m: RunMutation {
                    job_id: f.values.get("--job").cloned(),
                    failed_only: false,
                },
            }
        }
        ("run", "watch", "") => {
            let f = scan(
                tail,
                &["-R", "--repo", "--interval", "--timeout"],
                &["--exit-status"],
                &[],
            )?;
            let (repo, run_id) = repo_and_arg(&f, "run watch", "run-id")?;
            Command::RunWatch {
                repo,
                run_id,
                interval: opt_u64(&f, "--interval")?,
                timeout: opt_u64(&f, "--timeout")?,
                exit_status: f.bools.contains("--exit-status"),
            }
        }
        ("workflow", "list", "") => {
            let f = scan(tail, &["-R", "--repo", "--ref"], &[], &[])?;
            Command::WorkflowList {
                repo: repo_spec(&f, "workflow list")?,
                git_ref: f.values.get("--ref").cloned(),
            }
        }
        ("workflow", "view", "") => {
            let f = scan(tail, &["-R", "--repo", "--ref"], &[], &[])?;
            let (repo, path) = repo_and_arg(&f, "workflow view", "path-or-name")?;
            Command::WorkflowView {
                repo,
                path,
                git_ref: f.values.get("--ref").cloned(),
            }
        }
        ("workflow", "run", "") => {
            let f = scan(tail, &["-R", "--repo", "--ref"], &[], &[])?;
            let (repo, workflow_id) = repo_and_arg(&f, "workflow run", "workflow-id")?;
            Command::WorkflowRun {
                repo,
                workflow_id,
                git_ref: f.values.get("--ref").cloned(),
            }
        }
        ("secret", "list", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::SecretList {
                repo: repo_spec(&f, "secret list")?,
            }
        }
        ("secret", "set", "") => {
            let mut vf = vec!["-R", "--repo", "--file"];
            vf.extend(BODY_FLAGS);
            let f = scan(tail, &vf, &[], &[])?;
            let (repo, name) = repo_and_arg(&f, "secret set", "NAME")?;
            Command::SecretSet {
                repo,
                name,
                body: BodyArg {
                    body: f.values.get("--body").cloned(),
                    body_file: f
                        .values
                        .get("--body-file")
                        .or_else(|| f.values.get("--file"))
                        .cloned(),
                },
            }
        }
        ("secret", "delete", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, name) = repo_and_arg(&f, "secret delete", "NAME")?;
            Command::SecretDelete { repo, name }
        }

        ("release", "list", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::ReleaseList {
                repo: repo_spec(&f, "release list")?,
            }
        }
        ("release", "view", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, tag) = repo_and_arg(&f, "release view", "tag")?;
            Command::ReleaseView { repo, tag }
        }
        ("release", "create", "") => {
            let mut vf = vec!["-R", "--repo", "--title"];
            vf.extend(BODY_FLAGS);
            let f = scan(tail, &vf, &["--draft", "--prerelease"], &[])?;
            let (repo, tag) = repo_and_arg(&f, "release create", "tag")?;
            Command::ReleaseCreate {
                repo,
                tag,
                title: f.values.get("--title").cloned(),
                body: body_arg(&f)?,
                draft: f.bools.contains("--draft"),
                prerelease: f.bools.contains("--prerelease"),
            }
        }
        ("release", "edit", "") => {
            let mut vf = vec!["-R", "--repo", "--title"];
            vf.extend(BODY_FLAGS);
            let f = scan(
                tail,
                &vf,
                &["--draft", "--publish", "--prerelease", "--no-prerelease"],
                &[],
            )?;
            let (repo, tag) = repo_and_arg(&f, "release edit", "tag")?;
            let tri = |yes: &str, no: &str| match (f.bools.contains(yes), f.bools.contains(no)) {
                (true, _) => Some(true),
                (false, true) => Some(false),
                _ => None,
            };
            Command::ReleaseEdit {
                repo,
                tag,
                title: f.values.get("--title").cloned(),
                body: body_arg(&f)?,
                draft: tri("--draft", "--publish"),
                prerelease: tri("--prerelease", "--no-prerelease"),
            }
        }
        ("release", "delete", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, tag) = repo_and_arg(&f, "release delete", "tag")?;
            Command::ReleaseDelete { repo, tag }
        }
        ("release", "upload", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, tag) = repo_and_arg(&f, "release upload", "tag")?;
            // files are the positionals after the tag — or all positionals
            // when -R carried the repo spec.
            let files: Vec<String> =
                if f.values.contains_key("-R") || f.values.contains_key("--repo") {
                    f.positionals[1..].to_vec()
                } else {
                    f.positionals[2..].to_vec()
                };
            if files.is_empty() {
                return Err(usage("usage: ox release upload <o/n> <tag> <file>..."));
            }
            Command::ReleaseUpload { repo, tag, files }
        }
        ("release", "download", "") => {
            let f = scan(
                tail,
                &["-R", "--repo", "--dir", "--pattern"],
                &["--clobber"],
                &[],
            )?;
            let (repo, tag) = repo_and_arg(&f, "release download", "tag")?;
            Command::ReleaseDownload {
                repo,
                tag,
                pattern: f.values.get("--pattern").cloned(),
                dir: f.values.get("--dir").cloned(),
                clobber: f.bools.contains("--clobber"),
            }
        }
        ("release", "delete-asset", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            let (repo, tag) = repo_and_arg(&f, "release delete-asset", "tag")?;
            let filename = if f.values.contains_key("-R") || f.values.contains_key("--repo") {
                f.positionals.get(1)
            } else {
                f.positionals.get(2)
            }
            .cloned()
            .ok_or_else(|| usage("usage: ox release delete-asset <o/n> <tag> <filename>"))?;
            Command::ReleaseDeleteAsset {
                repo,
                tag,
                filename,
            }
        }

        ("search", _, _) => {
            // `ox search <q> ...` — the query is a positional, not a subcommand.
            let f = scan(&rest[1..], &["--limit"], &[], &["--type"])?;
            let q = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox search <query> [--type <t>]..."))?;
            Command::Search {
                q,
                types: f
                    .multi
                    .iter()
                    .filter(|(k, _)| k == "--type")
                    .flat_map(|(_, v)| csv(v))
                    .collect(),
                limit: opt_u64(&f, "--limit")?,
            }
        }
        ("browse", _, _) => {
            let f = scan(&rest[1..], REPO_FLAGS, &["--print"], &[])?;
            let repo = match f
                .positionals
                .first()
                .or_else(|| f.values.get("-R"))
                .or_else(|| f.values.get("--repo"))
            {
                Some(r) => {
                    parse_repo(r)?;
                    Some(r.clone())
                }
                None => None,
            };
            Command::Browse {
                repo,
                print: f.bools.contains("--print"),
            }
        }
        ("status", "create", "") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--sha",
                    "--context",
                    "--state",
                    "--description",
                    "--target-url",
                ],
                &[],
                &[],
            )?;
            check_choice(
                &f,
                "--state",
                &["pending", "success", "failure", "error"],
                "status create",
            )?;
            Command::StatusCreate {
                repo: repo_spec(&f, "status create")?,
                sha: req(&f, "--sha", "status create")?,
                context: req(&f, "--context", "status create")?,
                state: req(&f, "--state", "status create")?,
                description: f.values.get("--description").cloned(),
                target_url: f.values.get("--target-url").cloned(),
            }
        }

        ("org", "list", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "org list")?;
            Command::OrgList
        }
        ("org", "view", "") => {
            let f = scan(tail, &[], &[], &[])?;
            let slug = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox org view <slug>"))?;
            Command::OrgView { slug }
        }
        ("org", "create", "") => {
            let f = scan(tail, &["--name"], &[], &[])?;
            let slug = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox org create <slug>"))?;
            Command::OrgCreate {
                slug,
                name: f.values.get("--name").cloned(),
            }
        }
        ("org", "members", "list") | ("org", "members", "") => {
            let f = scan(tail, &["--role"], &[], &[])?;
            let slug = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox org members <slug>"))?;
            Command::OrgMembers { slug }
        }
        ("org", "members", "add") => {
            let f = scan(tail, &["--role"], &[], &[])?;
            check_choice(&f, "--role", ROLES, "org members add")?;
            let slug = f.positionals.first().cloned().unwrap_or_default();
            let username = f.positionals.get(1).cloned().unwrap_or_default();
            if slug.is_empty() || username.is_empty() {
                return Err(usage("usage: ox org members add <slug> <user> [--role r]"));
            }
            Command::OrgMemberAdd {
                slug,
                username,
                role: f
                    .values
                    .get("--role")
                    .cloned()
                    .unwrap_or_else(|| "member".to_string()),
            }
        }
        ("org", "members", "remove") => {
            let f = scan(tail, &[], &[], &[])?;
            let (slug, username) = two_positionals(&f, "org members remove", "slug", "user")?;
            Command::OrgMemberRemove { slug, username }
        }
        ("org", "members", "set-role") => {
            let f = scan(tail, &["--role"], &[], &[])?;
            check_choice(&f, "--role", ROLES, "org members set-role")?;
            let (slug, username) = two_positionals(&f, "org members set-role", "slug", "user")?;
            Command::OrgMemberRole {
                slug,
                username,
                role: req(&f, "--role", "org members set-role")?,
            }
        }
        ("org", "invites", "list") | ("org", "invites", "") => {
            let f = scan(tail, &[], &[], &[])?;
            let slug = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox org invites <slug>"))?;
            Command::OrgInviteList { slug }
        }
        ("org", "invites", "create") => {
            let f = scan(tail, &["--emails", "--role"], &[], &[])?;
            check_choice(&f, "--role", ROLES, "org invites create")?;
            let slug = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox org invites create <slug> --emails a@b"))?;
            Command::OrgInviteCreate {
                slug,
                emails: csv(&req(&f, "--emails", "org invites create")?),
                role: f
                    .values
                    .get("--role")
                    .cloned()
                    .unwrap_or_else(|| "member".to_string()),
            }
        }
        ("org", "invites", "link") => {
            let f = scan(tail, &["--role", "--expires", "--max-uses"], &[], &[])?;
            check_choice(&f, "--role", ROLES, "org invites link")?;
            let slug = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox org invites link <slug>"))?;
            Command::OrgInviteLink {
                slug,
                role: f
                    .values
                    .get("--role")
                    .cloned()
                    .unwrap_or_else(|| "member".to_string()),
                expires_at: f.values.get("--expires").cloned(),
                max_uses: opt_u64(&f, "--max-uses")?,
            }
        }
        ("org", "invites", "revoke") => {
            let f = scan(tail, &["--id"], &[], &[])?;
            let slug = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox org invites revoke <slug> --id <id>"))?;
            Command::OrgInviteRevoke {
                slug,
                id: req_id(&f, "org invites revoke")?,
            }
        }

        ("notification", "list", "") => {
            let f = scan(tail, &["--limit", "--offset"], &["--all"], &[])?;
            expect_no_positionals(&f, "notification list")?;
            Command::NotificationList {
                all: f.bools.contains("--all"),
                limit: opt_u64(&f, "--limit")?,
                offset: opt_u64(&f, "--offset")?,
            }
        }
        ("notification", "read", "") => {
            let f = scan(tail, &[], &["--all"], &[])?;
            Command::NotificationRead {
                ids: f.positionals.clone(),
                all: f.bools.contains("--all"),
            }
        }
        ("notification", "unread", "") | ("notification", "unread-count", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "notification unread")?;
            Command::NotificationUnread
        }

        ("user", "view", "") => {
            let f = scan(tail, &[], &[], &[])?;
            let username = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox user view <username>"))?;
            Command::UserView { username }
        }
        ("user", "lookup", "") => {
            let f = scan(tail, &[], &[], &[])?;
            let prefix = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox user lookup <prefix>"))?;
            Command::UserLookup { prefix }
        }
        ("user", "follow", "") => {
            let f = scan(tail, &[], &[], &[])?;
            let username = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox user follow <username>"))?;
            Command::UserFollow {
                username,
                follow: true,
            }
        }
        ("user", "unfollow", "") => {
            let f = scan(tail, &[], &[], &[])?;
            let username = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox user unfollow <username>"))?;
            Command::UserFollow {
                username,
                follow: false,
            }
        }
        ("user", "starred", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "user starred")?;
            Command::UserStarred
        }
        ("user", "watched", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "user watched")?;
            Command::UserWatched
        }

        ("ssh-key", "list", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "ssh-key list")?;
            Command::SshKeyList
        }
        ("ssh-key", "add", "") => {
            let f = scan(
                tail,
                &["--title", "--key-file", "--key"],
                &["--no-auth", "--no-sign"],
                &[],
            )?;
            expect_no_positionals(&f, "ssh-key add")?;
            Command::SshKeyAdd {
                title: req(&f, "--title", "ssh-key add")?,
                key_file: f
                    .values
                    .get("--key-file")
                    .or_else(|| f.values.get("--key"))
                    .cloned()
                    .ok_or_else(|| usage("ssh-key add requires --key-file <path>"))?,
                can_auth: !f.bools.contains("--no-auth"),
                can_sign: !f.bools.contains("--no-sign"),
            }
        }
        ("ssh-key", "delete", "") => {
            let f = scan(tail, &["--id"], &[], &[])?;
            expect_no_positionals(&f, "ssh-key delete")?;
            Command::SshKeyDelete {
                id: req_id(&f, "ssh-key delete")?,
            }
        }
        ("gpg-key", "list", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "gpg-key list")?;
            Command::GpgKeyList
        }
        ("gpg-key", "add", "") => {
            let f = scan(tail, &["--title", "--key-file", "--key"], &[], &[])?;
            expect_no_positionals(&f, "gpg-key add")?;
            Command::GpgKeyAdd {
                title: req(&f, "--title", "gpg-key add")?,
                key_file: f
                    .values
                    .get("--key-file")
                    .or_else(|| f.values.get("--key"))
                    .cloned()
                    .ok_or_else(|| usage("gpg-key add requires --key-file <path>"))?,
            }
        }
        ("gpg-key", "delete", "") => {
            let f = scan(tail, &["--id"], &[], &[])?;
            expect_no_positionals(&f, "gpg-key delete")?;
            Command::GpgKeyDelete {
                id: req_id(&f, "gpg-key delete")?,
            }
        }
        ("email", "list", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "email list")?;
            Command::EmailList
        }
        ("email", "add", "") => {
            let f = scan(tail, &[], &[], &[])?;
            let email = f
                .positionals
                .first()
                .cloned()
                .ok_or_else(|| usage("usage: ox email add <addr>"))?;
            Command::EmailAdd { email }
        }
        ("email", "remove", "") => {
            let f = scan(tail, &["--id"], &[], &[])?;
            expect_no_positionals(&f, "email remove")?;
            Command::EmailRemove {
                id: req_id(&f, "email remove")?,
            }
        }
        ("email", "set-primary", "") => {
            let f = scan(tail, &["--id"], &[], &[])?;
            expect_no_positionals(&f, "email set-primary")?;
            Command::EmailSetPrimary {
                id: req_id(&f, "email set-primary")?,
            }
        }
        ("email", "resend", "") => {
            let f = scan(tail, &["--id"], &[], &[])?;
            expect_no_positionals(&f, "email resend")?;
            Command::EmailResend {
                id: req_id(&f, "email resend")?,
            }
        }

        ("label", "list", "") => {
            let f = scan(tail, &["-R", "--repo", "--org"], &["--hidden"], &[])?;
            Command::LabelList {
                repo: repo_or_none(&f)?,
                org: f.values.get("--org").cloned(),
                include_hidden: f.bools.contains("--hidden"),
            }
        }
        ("label", "create", "") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--org",
                    "--name",
                    "--color",
                    "--description",
                ],
                &[],
                &[],
            )?;
            Command::LabelCreate {
                repo: repo_or_none(&f)?,
                org: f.values.get("--org").cloned(),
                name: req(&f, "--name", "label create")?,
                color: req(&f, "--color", "label create")?,
                description: f.values.get("--description").cloned(),
            }
        }
        ("label", "edit", "") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--org",
                    "--id",
                    "--name",
                    "--color",
                    "--description",
                ],
                &["--hidden", "--visible"],
                &[],
            )?;
            Command::LabelEdit {
                repo: repo_or_none(&f)?,
                org: f.values.get("--org").cloned(),
                id: req_id(&f, "label edit")?,
                name: f.values.get("--name").cloned(),
                color: f.values.get("--color").cloned(),
                description: f.values.get("--description").cloned(),
                hidden: match (f.bools.contains("--hidden"), f.bools.contains("--visible")) {
                    (true, _) => Some(true),
                    (false, true) => Some(false),
                    _ => None,
                },
            }
        }
        ("label", "delete", "") => {
            let f = scan(tail, &["-R", "--repo", "--org", "--id"], &[], &[])?;
            Command::LabelDelete {
                repo: repo_or_none(&f)?,
                org: f.values.get("--org").cloned(),
                id: req_id(&f, "label delete")?,
            }
        }

        ("webhook", "list", "") => {
            let f = scan(tail, REPO_FLAGS, &[], &[])?;
            Command::WebhookList {
                repo: repo_spec(&f, "webhook list")?,
            }
        }
        ("webhook", "create", "") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--url",
                    "--secret",
                    "--events",
                    "--description",
                ],
                &["--inactive"],
                &[],
            )?;
            Command::WebhookCreate {
                repo: repo_spec(&f, "webhook create")?,
                url: req(&f, "--url", "webhook create")?,
                secret: req(&f, "--secret", "webhook create")?,
                events: csv(&req(&f, "--events", "webhook create")?),
                active: !f.bools.contains("--inactive"),
                description: f.values.get("--description").cloned(),
            }
        }
        ("webhook", "edit", "") => {
            let f = scan(
                tail,
                &[
                    "-R",
                    "--repo",
                    "--id",
                    "--url",
                    "--secret",
                    "--events",
                    "--description",
                ],
                &["--inactive", "--active"],
                &[],
            )?;
            Command::WebhookEdit {
                repo: repo_spec(&f, "webhook edit")?,
                id: req_id(&f, "webhook edit")?,
                url: f.values.get("--url").cloned(),
                secret: f.values.get("--secret").cloned(),
                events: f.values.get("--events").map(|e| csv(e)),
                active: match (f.bools.contains("--active"), f.bools.contains("--inactive")) {
                    (true, _) => Some(true),
                    (false, true) => Some(false),
                    _ => None,
                },
                description: f.values.get("--description").cloned(),
            }
        }
        ("webhook", "delete", "") => {
            let f = scan(tail, &["-R", "--repo", "--id"], &[], &[])?;
            Command::WebhookDelete {
                repo: repo_spec(&f, "webhook delete")?,
                id: req_id(&f, "webhook delete")?,
            }
        }
        ("webhook", "ping", "") => {
            let f = scan(tail, &["-R", "--repo", "--id"], &[], &[])?;
            Command::WebhookPing {
                repo: repo_spec(&f, "webhook ping")?,
                id: req_id(&f, "webhook ping")?,
            }
        }
        ("webhook", "deliveries", "list") | ("webhook", "deliveries", "") => {
            let f = scan(tail, &["-R", "--repo", "--id", "--limit"], &[], &[])?;
            Command::WebhookDeliveries {
                repo: repo_spec(&f, "webhook deliveries")?,
                hook_id: req_id(&f, "webhook deliveries")?,
                limit: opt_u64(&f, "--limit")?,
            }
        }
        ("webhook", "deliveries", "get") | ("webhook", "deliveries", "view") => {
            let f = scan(tail, &["-R", "--repo", "--id", "--delivery"], &[], &[])?;
            Command::WebhookDelivery {
                repo: repo_spec(&f, "webhook deliveries get")?,
                hook_id: req_id(&f, "webhook deliveries get")?,
                delivery_id: req(&f, "--delivery", "webhook deliveries get")?,
            }
        }
        ("webhook", "deliveries", "redeliver") | ("webhook", "redeliver", "") => {
            let f = scan(tail, &["-R", "--repo", "--id", "--delivery"], &[], &[])?;
            Command::WebhookRedeliver {
                repo: repo_spec(&f, "webhook redeliver")?,
                hook_id: req_id(&f, "webhook redeliver")?,
                delivery_id: req(&f, "--delivery", "webhook redeliver")?,
            }
        }

        ("pkg", "list", "") => {
            let f = scan(tail, &["--owner"], &[], &[])?;
            expect_no_positionals(&f, "pkg list")?;
            Command::PkgList {
                owner: f.values.get("--owner").cloned(),
            }
        }
        ("skill", "print", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "skill print")?;
            Command::SkillPrint
        }
        ("skill", "install", "") => {
            let f = scan(
                tail,
                &[],
                &["--project", "--global", "--upgrade", "--detected"],
                &["--harness"],
            )?;
            expect_no_positionals(&f, "skill install")?;
            Command::SkillInstall {
                harnesses: f
                    .multi
                    .iter()
                    .filter(|(k, _)| k == "--harness")
                    .map(|(_, v)| v.clone())
                    .collect(),
                project: f.bools.contains("--project"),
                global: f.bools.contains("--global"),
                upgrade: f.bools.contains("--upgrade"),
                detected: f.bools.contains("--detected"),
            }
        }
        ("mcp", "print", "") => {
            let f = scan(tail, &["--harness"], &[], &[])?;
            expect_no_positionals(&f, "mcp print")?;
            Command::McpPrint {
                harness: f.values.get("--harness").cloned(),
            }
        }
        ("mcp", "install", "") => {
            let f = scan(
                tail,
                &["--token"],
                &[
                    "--token-stdin",
                    "--web",
                    "--anonymous",
                    "--upgrade",
                    "--project",
                    "--global",
                    "--detected",
                ],
                &["--harness"],
            )?;
            expect_no_positionals(&f, "mcp install")?;
            let auth_flags = f.values.contains_key("--token") as usize
                + f.bools.contains("--token-stdin") as usize
                + f.bools.contains("--web") as usize
                + f.bools.contains("--anonymous") as usize;
            if auth_flags > 1 {
                return Err(usage(
                    "--token, --token-stdin, --web, and --anonymous are exclusive",
                ));
            }
            Command::McpInstall {
                harnesses: f
                    .multi
                    .iter()
                    .filter(|(k, _)| k == "--harness")
                    .map(|(_, v)| v.clone())
                    .collect(),
                project: f.bools.contains("--project"),
                global: f.bools.contains("--global"),
                detected: f.bools.contains("--detected"),
                token: f.values.get("--token").cloned(),
                token_stdin: f.bools.contains("--token-stdin"),
                web: f.bools.contains("--web"),
                anonymous: f.bools.contains("--anonymous"),
                upgrade: f.bools.contains("--upgrade"),
            }
        }
        ("mcp", "status", "") => {
            let f = scan(tail, &[], &[], &[])?;
            expect_no_positionals(&f, "mcp status")?;
            Command::McpStatus
        }
        ("mcp", "remove", "") => {
            let f = scan(
                tail,
                &[],
                &["--project", "--global", "--detected"],
                &["--harness"],
            )?;
            expect_no_positionals(&f, "mcp remove")?;
            Command::McpRemove {
                harnesses: f
                    .multi
                    .iter()
                    .filter(|(k, _)| k == "--harness")
                    .map(|(_, v)| v.clone())
                    .collect(),
                project: f.bools.contains("--project"),
                global: f.bools.contains("--global"),
                detected: f.bools.contains("--detected"),
            }
        }
        ("self-update", _, _) => {
            // flags sit at the `sub` position — rescan rest[1..] like browse.
            let f = scan(
                &rest[1..],
                &[],
                &["--no-skill-refresh", "--no-mcp-refresh"],
                &[],
            )?;
            expect_no_positionals(&f, "self-update")?;
            Command::SelfUpdate {
                no_skill_refresh: f.bools.contains("--no-skill-refresh"),
                no_mcp_refresh: f.bools.contains("--no-mcp-refresh"),
            }
        }
        ("api", proc, _) if !proc.is_empty() => {
            // procedure is `sub`; everything after it is flags.
            let f = scan(&rest[2..], &[], &[], &["-f", "-F"])?;
            expect_no_positionals(&f, "api")?;
            Command::Api {
                procedure: proc.to_string(),
                input: api_input(&f)?,
            }
        }
        ("api", _, _) => {
            return Err(usage("usage: ox api <procedure> [-f field=value]..."));
        }
        (g, s, "") if NESTED.iter().any(|(g2, s2, ..)| *g2 == g && *s2 == s) => {
            let ops = NESTED
                .iter()
                .find(|(g2, s2, ..)| *g2 == g && *s2 == s)
                .map(|(.., ops, _)| ops.join("|"))
                .unwrap_or_default();
            return Err(usage(format!("ox {g} {s}: expected subcommand ({ops})")));
        }
        (_, "", "") => {
            const GROUPS: &[&str] = &[
                "auth",
                "config",
                "repo",
                "issue",
                "pr",
                "run",
                "workflow",
                "secret",
                "release",
                "pkg",
                "search",
                "browse",
                "status",
                "org",
                "notification",
                "user",
                "ssh-key",
                "gpg-key",
                "email",
                "label",
                "webhook",
                "self-update",
                "skill",
                "mcp",
                "api",
                "completion",
            ];
            if GROUPS.contains(&group) {
                return Err(usage(format!(
                    "ox {group}: missing subcommand — see `ox {group} --help`"
                )));
            }
            return Err(usage(format!("unknown command: {group} — see `ox --help`")));
        }
        (g, s, _) => {
            let what = if s.is_empty() {
                g.to_string()
            } else {
                format!("{g} {s}")
            };
            return Err(usage(format!("unknown command: {what} — see `ox --help`")));
        }
    };
    Ok(Invocation { global, command })
}

/// `<owner/name>` optional — returns None when neither positional nor -R given.
fn repo_or_none(f: &Flags) -> Result<Option<String>, UsageError> {
    match f
        .positionals
        .first()
        .or_else(|| f.values.get("-R"))
        .or_else(|| f.values.get("--repo"))
    {
        Some(r) => {
            parse_repo(r)?;
            Ok(Some(r.clone()))
        }
        None => Ok(None),
    }
}

fn two_positionals(
    f: &Flags,
    usage_str: &str,
    a: &str,
    b: &str,
) -> Result<(String, String), UsageError> {
    let first = f
        .positionals
        .first()
        .ok_or_else(|| usage(format!("usage: ox {usage_str} <{a}> <{b}>")))?;
    let second = f
        .positionals
        .get(1)
        .ok_or_else(|| usage(format!("usage: ox {usage_str} <{a}> <{b}>")))?;
    Ok((first.clone(), second.clone()))
}

fn json_u64(n: u64) -> Value {
    Value::Number(serde_json::Number::from(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn self_update_parses() {
        let inv = parse(&argv(&["self-update"])).unwrap();
        assert_eq!(
            inv.command,
            Command::SelfUpdate {
                no_skill_refresh: false,
                no_mcp_refresh: false,
            }
        );
        let inv = parse(&argv(&["self-update", "--no-skill-refresh"])).unwrap();
        assert_eq!(
            inv.command,
            Command::SelfUpdate {
                no_skill_refresh: true,
                no_mcp_refresh: false,
            }
        );
        let inv = parse(&argv(&[
            "self-update",
            "--no-skill-refresh",
            "--no-mcp-refresh",
        ]))
        .unwrap();
        assert_eq!(
            inv.command,
            Command::SelfUpdate {
                no_skill_refresh: true,
                no_mcp_refresh: true,
            }
        );
        let err = parse(&argv(&["self-update", "extra"])).unwrap_err();
        assert!(err.0.contains("unexpected argument"), "{err}");
    }

    #[test]
    fn mcp_subcommands_parse() {
        let inv = parse(&argv(&["mcp", "print"])).unwrap();
        assert_eq!(inv.command, Command::McpPrint { harness: None });
        let inv = parse(&argv(&["mcp", "print", "--harness", "codex"])).unwrap();
        assert_eq!(
            inv.command,
            Command::McpPrint {
                harness: Some("codex".into())
            }
        );

        let inv = parse(&argv(&["mcp", "status"])).unwrap();
        assert_eq!(inv.command, Command::McpStatus);
        let err = parse(&argv(&["mcp", "status", "extra"])).unwrap_err();
        assert!(err.0.contains("unexpected argument"), "{err}");

        let inv = parse(&argv(&["mcp", "install", "--detected", "--project"])).unwrap();
        assert_eq!(
            inv.command,
            Command::McpInstall {
                harnesses: vec![],
                project: true,
                global: false,
                detected: true,
                token: None,
                token_stdin: false,
                web: false,
                anonymous: false,
                upgrade: false,
            }
        );
        let inv = parse(&argv(&[
            "mcp",
            "install",
            "--upgrade",
            "--harness",
            "cursor",
        ]))
        .unwrap();
        assert_eq!(
            inv.command,
            Command::McpInstall {
                harnesses: vec!["cursor".into()],
                project: false,
                global: false,
                detected: false,
                token: None,
                token_stdin: false,
                web: false,
                anonymous: false,
                upgrade: true,
            }
        );
        let inv = parse(&argv(&[
            "mcp",
            "install",
            "--harness",
            "cursor",
            "--harness",
            "claude",
            "--global",
            "--token",
            "pat_x",
        ]))
        .unwrap();
        assert_eq!(
            inv.command,
            Command::McpInstall {
                harnesses: vec!["cursor".into(), "claude".into()],
                project: false,
                global: true,
                detected: false,
                token: Some("pat_x".into()),
                token_stdin: false,
                web: false,
                anonymous: false,
                upgrade: false,
            }
        );
        // auth modes are exclusive.
        for args in [
            vec!["mcp", "install", "--token", "x", "--web"],
            vec!["mcp", "install", "--token-stdin", "--anonymous"],
            vec!["mcp", "install", "--web", "--anonymous"],
        ] {
            let err = parse(&argv(&args)).unwrap_err();
            assert!(err.0.contains("are exclusive"), "{err}");
        }

        let inv = parse(&argv(&["mcp", "remove", "--harness", "all", "--global"])).unwrap();
        assert_eq!(
            inv.command,
            Command::McpRemove {
                harnesses: vec!["all".into()],
                project: false,
                global: true,
                detected: false,
            }
        );

        // bare `ox mcp` is a group — missing-subcommand hint, not unknown.
        let err = parse(&argv(&["mcp"])).unwrap_err();
        assert!(err.0.contains("missing subcommand"), "{err}");
    }

    #[test]
    fn empty_argv_is_help() {
        let inv = parse(&argv(&[])).unwrap();
        assert!(matches!(inv.command, Command::Help));
    }

    #[test]
    fn help_attaches_to_command_path() {
        let inv = parse(&argv(&["issue", "create", "--help"])).unwrap();
        assert_eq!(inv.command, Command::HelpTopic("issue create".into()));
        let inv = parse(&argv(&["issue", "--help"])).unwrap();
        assert_eq!(inv.command, Command::HelpTopic("issue".into()));
        assert!(parse(&argv(&["frob", "--help"])).is_err());
    }

    #[test]
    fn repo_view_positional() {
        let inv = parse(&argv(&["repo", "view", "octo/demo"])).unwrap();
        assert_eq!(
            inv.command,
            Command::RepoView {
                repo: "octo/demo".into()
            }
        );
    }

    #[test]
    fn repo_view_dash_r_flag() {
        let inv = parse(&argv(&["repo", "view", "-R", "octo/demo"])).unwrap();
        assert_eq!(
            inv.command,
            Command::RepoView {
                repo: "octo/demo".into()
            }
        );
    }

    #[test]
    fn repo_view_rejects_missing_slash() {
        let err = parse(&argv(&["repo", "view", "demo"])).unwrap_err();
        assert!(err.0.contains("invalid repo"), "{err}");
    }

    #[test]
    fn globals_extracted_anywhere() {
        let inv = parse(&argv(&[
            "issue",
            "list",
            "o/n",
            "--json",
            "--instance",
            "https://f.example.com",
        ]))
        .unwrap();
        assert!(inv.global.json);
        assert_eq!(
            inv.global.instance.as_deref(),
            Some("https://f.example.com")
        );
        match inv.command {
            Command::IssueList { repo, .. } => assert_eq!(repo, "o/n"),
            other => panic!("expected issue list, got {other:?}"),
        }
    }

    #[test]
    fn instance_equals_form() {
        let inv = parse(&argv(&[
            "--instance=https://f.example.com/",
            "repo",
            "list",
        ]))
        .unwrap();
        assert_eq!(
            inv.global.instance.as_deref(),
            Some("https://f.example.com/")
        );
    }

    #[test]
    fn issue_edit_collects_multi() {
        let inv = parse(&argv(&[
            "issue",
            "edit",
            "o/n",
            "4",
            "--label",
            "bug,ui",
            "--assignee",
            "octo",
        ]))
        .unwrap();
        match inv.command {
            Command::IssueEdit {
                labels, assignees, ..
            } => {
                assert_eq!(labels, vec!["bug", "ui"]);
                assert_eq!(assignees, vec!["octo"]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pr_review_submit_flags() {
        let inv = parse(&argv(&["pr", "review", "o/n", "2", "--approve"])).unwrap();
        match inv.command {
            Command::PrReview { action, .. } => {
                assert_eq!(action, ReviewAction::Submit("approved"))
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nested_third_level() {
        let inv = parse(&argv(&["repo", "deploy-key", "list", "o/n"])).unwrap();
        assert!(matches!(inv.command, Command::DeployKeyList { .. }));
        let inv = parse(&argv(&["org", "members", "add", "eng", "alice"])).unwrap();
        assert!(matches!(inv.command, Command::OrgMemberAdd { .. }));
    }

    #[test]
    fn search_query_positional() {
        let inv = parse(&argv(&["search", "crash", "--type", "issues"])).unwrap();
        match inv.command {
            Command::Search { q, types, .. } => {
                assert_eq!(q, "crash");
                assert_eq!(types, vec!["issues"]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn api_input_flag_modes() {
        let inv = parse(&argv(&[
            "api",
            "issue.list",
            "-f",
            "owner=o",
            "-F",
            "limit=3",
        ]))
        .unwrap();
        match inv.command {
            Command::Api { procedure, input } => {
                assert_eq!(procedure, "issue.list");
                assert_eq!(input, json!({"owner": "o", "limit": 3}));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn missing_subcommand_named() {
        let err = parse(&argv(&["issue"])).unwrap_err();
        assert!(err.0.contains("missing subcommand"), "{err}");
    }

    #[test]
    fn run_watch_parses() {
        let inv = parse(&argv(&[
            "run",
            "watch",
            "o/n",
            "r1",
            "--interval",
            "2",
            "--timeout",
            "60",
            "--exit-status",
        ]))
        .unwrap();
        match inv.command {
            Command::RunWatch {
                repo,
                run_id,
                interval,
                timeout,
                exit_status,
            } => {
                assert_eq!(repo, "o/n");
                assert_eq!(run_id, "r1");
                assert_eq!(interval, Some(2));
                assert_eq!(timeout, Some(60));
                assert!(exit_status);
            }
            other => panic!("{other:?}"),
        }
        assert!(parse(&argv(&["run", "watch"])).is_err());
    }

    #[test]
    fn config_and_completion_parse() {
        match parse(&argv(&["config", "get", "default_instance"]))
            .unwrap()
            .command
        {
            Command::ConfigGet { key } => assert_eq!(key, "default_instance"),
            other => panic!("{other:?}"),
        }
        match parse(&argv(&["config"])).unwrap().command {
            Command::ConfigList => {}
            other => panic!("{other:?}"),
        }
        match parse(&argv(&["config", "set", "k", "v"])).unwrap().command {
            Command::ConfigSet { key, value } => {
                assert_eq!((key.as_str(), value.as_str()), ("k", "v"));
            }
            other => panic!("{other:?}"),
        }
        assert!(parse(&argv(&["config", "set", "only-key"])).is_err());
        match parse(&argv(&["completion", "zsh"])).unwrap().command {
            Command::Completion { shell } => assert_eq!(shell, "zsh"),
            other => panic!("{other:?}"),
        }
        assert!(parse(&argv(&["completion", "tcsh"])).is_err());
        assert!(parse(&argv(&["completion"])).is_err());
    }

    #[test]
    fn auth_switch_parses() {
        match parse(&argv(&["auth", "switch"])).unwrap().command {
            Command::AuthSwitch { instance } => assert!(instance.is_none()),
            other => panic!("{other:?}"),
        }
        match parse(&argv(&["auth", "switch", "https://f.example.com"]))
            .unwrap()
            .command
        {
            Command::AuthSwitch { instance } => {
                assert_eq!(instance.as_deref(), Some("https://f.example.com"));
            }
            other => panic!("{other:?}"),
        }
    }

    /// Every verb/op in `completion::COMMAND_TREE` must resolve through the
    /// parser — a missing parse arm or NESTED row shows up as an
    /// "unknown command"/"expected subcommand" usage error.
    #[test]
    fn completion_tree_parses() {
        for (group, subs) in crate::completion::COMMAND_TREE {
            for (sub, ops) in *subs {
                // NESTED groups without a default legitimately reject bare
                // `ox <group> <sub>` — their ops are checked individually.
                if ops.is_empty() {
                    if let Err(e) = parse(&argv(&[group, sub])) {
                        assert!(
                            !e.0.contains("unknown command")
                                && !e.0.contains("expected subcommand"),
                            "tree entry `ox {group} {sub}` does not parse: {e}"
                        );
                    }
                }
                for op in *ops {
                    if let Err(e) = parse(&argv(&[group, sub, op])) {
                        assert!(
                            !e.0.contains("unknown command")
                                && !e.0.contains("expected subcommand"),
                            "tree entry `ox {group} {sub} {op}` does not parse: {e}"
                        );
                    }
                }
            }
        }
    }

    /// Inverse direction: every NESTED group must appear in the completion
    /// tree with the same op list, so adding a NESTED row without updating
    /// COMMAND_TREE fails the test.
    #[test]
    fn completion_tree_covers_nested() {
        for (group, sub, ops, _) in NESTED {
            let subs = crate::completion::COMMAND_TREE
                .iter()
                .find(|(g, _)| g == group)
                .unwrap_or_else(|| panic!("completion tree missing group {group}"))
                .1;
            let entry = subs
                .iter()
                .find(|(s, _)| s == sub)
                .unwrap_or_else(|| panic!("completion tree missing {group} {sub}"));
            let mut want: Vec<&str> = ops.to_vec();
            want.sort_unstable();
            let mut got: Vec<&str> = entry.1.to_vec();
            got.sort_unstable();
            assert_eq!(want, got, "ops differ for {group} {sub}");
        }
    }
}
