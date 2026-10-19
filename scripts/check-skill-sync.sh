#!/usr/bin/env bash
# Drift guards tying SKILL.md, the CLI audit lists, and the MCP tool
# surface to the real registries.
#
# 1. Skill ↔ CLI surface — every command path mentioned in
#    crates/oxidean-cli/skill/SKILL.md (code-fence `ox …` lines and inline
#    backtick spans, including `group sub|sub` alternation cells and
#    `word(s)` plurals) must resolve against `ox completion commands`
#    (COMMAND_TREE — itself parser-verified by args.rs tests).
# 2. Flags — every `--flag`/`-f` on a skill `ox` line must appear in
#    `ox <path> --help` or the `ox --help` GLOBAL FLAGS section.
# 3. Proc registries — every proc-shaped literal in commands.rs's
#    required_procedures + CLI_PROCS/MCP_PROCS audit lists must be a proc
#    rpc.rs actually dispatches, and the MCP audit list must equal the
#    procs the MCP surface really routes (tool `procedure:` fields,
#    build_rpc_input picks, resource dispatch — all captured as proc-shaped
#    string literals intersected with the rpc.rs registry, so error codes
#    like `mcp.internal` exclude themselves). The `vcs.*` tool descriptors
#    live in oxidean-vcs's client/mcp.rs, so it is scanned alongside
#    oxidean-api's mcp.rs.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SKILL=crates/oxidean-cli/skill/SKILL.md
MCP_SRCS=(crates/oxidean-api/src/mcp.rs crates/oxidean-vcs/src/client/mcp.rs)
CMDS_SRC=crates/oxidean-cli/src/commands.rs
RPC_SRC=crates/oxidean-api/src/rpc.rs
OX=${OX_BIN:-target/debug/ox}
if [ ! -x "$OX" ]; then
  cargo build -q -p oxidean-cli
fi

fail=0
say() { printf 'skill-sync-check: %s\n' "$*"; fail=1; }

TREE="$(mktemp)"; GLOBAL_HELP="$(mktemp)"
REGISTERED="$(mktemp)"; MCP_ACTUAL="$(mktemp)"; MCP_DECLARED="$(mktemp)"
CLI_DECLARED="$(mktemp)"; REQPROCS="$(mktemp)"; SUSPECT="$(mktemp)"
trap 'rm -f "$TREE" "$GLOBAL_HELP" "$REGISTERED" "$MCP_ACTUAL" "$MCP_DECLARED" "$CLI_DECLARED" "$REQPROCS" "$SUSPECT"' EXIT

"$OX" completion commands >"$TREE"
"$OX" --help | sed -n '/GLOBAL FLAGS/,/^$/p' >"$GLOBAL_HELP"

# Every procedure the RPC dispatcher serves — `"proc.name" =>` match arms.
grep -oE '^\s+"[a-zA-Z_.]+" =>' "$RPC_SRC" | tr -d ' "=>' | sort -u >"$REGISTERED"

# Proc-shaped literals inside mcp.rs; membership in the registry filters
# out error-code strings (`mcp.internal`, `repo.not_found`, …).
MCP_RAW="$(mktemp)"; trap 'rm -f "$TREE" "$GLOBAL_HELP" "$REGISTERED" "$MCP_ACTUAL" "$MCP_DECLARED" "$CLI_DECLARED" "$REQPROCS" "$SUSPECT" "$MCP_RAW"' EXIT
grep -hoE '"[a-z][a-z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)+"' "${MCP_SRCS[@]}" |
  tr -d '"' | sort -u >"$MCP_RAW"
comm -12 "$MCP_RAW" "$REGISTERED" >"$MCP_ACTUAL"
# Anything left over that isn't an error-code suffix is a suspicious
# proc-shaped literal (renamed/dropped proc still referenced).
comm -23 "$MCP_RAW" "$REGISTERED" |
  grep -vE '\.(internal|not_found|forbidden|unauthorized|unauthenticated|invalid_[a-z_]+|insufficient_scope|conflict|gone|busy|bad_request|unsupported)$' >"$SUSPECT" || true

sed -n '/^const MCP_PROCS/,/^];/p' "$CMDS_SRC" |
  grep -oE '"[a-zA-Z_.]+"' | tr -d '"' | sort -u >"$MCP_DECLARED"
sed -n '/^const CLI_PROCS/,/^];/p' "$CMDS_SRC" |
  grep -oE '"[a-zA-Z_.]+"' | tr -d '"' | sort -u >"$CLI_DECLARED"
sed -n '/fn required_procedures/,/^}/p' "$CMDS_SRC" |
  grep -oE '"[a-z][a-z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)+"' | tr -d '"' | sort -u >"$REQPROCS"

# --- part 1+2: command paths and flags in SKILL.md -------------------------
# Emit `lineno<TAB>text` for every code-fence `ox …` line and every inline
# backtick span, then validate each candidate. Process substitution (not a
# pipe) keeps `fail` writes inside this shell.
while IFS=$'\t' read -r lineno text; do
  read -ra toks <<<"$text"
  [ "${#toks[@]}" -eq 0 ] && continue

  path=""
  i=0
  if [ "${toks[0]}" = "ox" ]; then
    # `ox --version`, `ox <group> …` placeholders, bare `ox` — nothing to
    # check unless a real group word follows.
    i=1
    if [ $i -lt ${#toks[@]} ] && [[ "${toks[$i]}" =~ ^[a-z][a-z0-9-]*$ ]]; then
      if grep -qxF -e "${toks[$i]}" "$TREE"; then
        path="${toks[$i]}"; i=$((i + 1))
      else
        say "$SKILL:$lineno mentions \`ox ${toks[$i]}\` — no such command"
        continue
      fi
    fi
  elif grep -qxF -e "${toks[0]}" "$TREE"; then
    # Cheat-sheet/prose span leading with a group word (`repo list|view`).
    path="${toks[0]}"; i=1
  else
    continue # `oxidean`, `--flag`, `<placeholder>`, env vars — not commands
  fi
  [ -z "$path" ] && continue

  # Greedy extension: consume sub/op tokens while `path tok` stays valid.
  while [ $i -lt ${#toks[@]} ]; do
    tok="${toks[$i]}"
    case "$tok" in
      *\|*)
        # Alternation cell: every member must form a valid path.
        IFS='|' read -ra alts <<<"${tok//\\|/|}"
        for alt in "${alts[@]}"; do
          case "$alt" in
            \[* | -* | "" | …*) continue ;;
          esac
          base="${alt%(s)}" # `collaborator(s)` → check singular and plural
          if ! grep -qxF -e "$path $base" "$TREE" &&
            { [ "$base" = "$alt" ] || ! grep -qxF -e "$path ${base}s" "$TREE"; }; then
            say "$SKILL:$lineno — \`$path $alt\` does not parse"
          fi
        done
        ;;
      \(s\)) : ;; # stray plural marker
      *\(s\))
        base="${tok%(s)}"
        if ! grep -qxF -e "$path $base" "$TREE" &&
          ! grep -qxF -e "$path ${base}s" "$TREE"; then
          say "$SKILL:$lineno — \`$path $tok\` does not parse"
        fi
        ;;
      [a-z]*)
        if grep -qxF -e "$path $tok" "$TREE"; then
          path="$path $tok"
        else
          break # positional arg or prose word — path is done
        fi
        ;;
      *) break ;;
    esac
    i=$((i + 1))
  done

  # Flag references on the same line must exist in the command's --help or
  # the global flag table.
  helptext=""
  for tok in "${toks[@]}"; do
    case "$tok" in
      --[a-z]* | -[a-zA-Z]) ;;
      *) continue ;;
    esac
    flag="${tok%%=*}"
    if [ -z "$helptext" ]; then
      helptext="$("$OX" $path --help 2>&1 || true)"
    fi
    if ! printf '%s' "$helptext" | grep -qF -e "$flag" &&
      ! grep -qF -e "$flag" "$GLOBAL_HELP"; then
      say "$SKILL:$lineno — \`$flag\` not in \`ox $path --help\` or global flags"
    fi
  done
done < <(
  {
    grep -nE '^[[:space:]]*ox[[:space:]]' "$SKILL" | sed 's/:/\t/'
    awk '{
      line = $0
      while (match(line, /`[^`]+`/)) {
        print NR "\t" substr(line, RSTART + 1, RLENGTH - 2)
        line = substr(line, RSTART + RLENGTH)
      }
    }' "$SKILL"
  }
)

# --- part 3: proc registries ------------------------------------------------
# Every proc the audit lists and required_procedures name must be a proc the
# server actually dispatches (renames on the server side fail here).
for p in $(cat "$CLI_DECLARED" "$MCP_DECLARED" "$REQPROCS" | sort -u); do
  if ! grep -qxF -e "$p" "$REGISTERED"; then
    say "proc \`$p\` is referenced in commands.rs but not registered in rpc.rs"
  fi
done

# MCP ↔ CLI parity: the audit list must equal what mcp.rs really routes.
while IFS= read -r p; do
  [ -z "$p" ] && continue
  say "mcp.rs dispatches \`$p\` but MCP_PROCS does not declare it — update the audit list"
done < <(comm -23 "$MCP_ACTUAL" "$MCP_DECLARED")
while IFS= read -r p; do
  [ -z "$p" ] && continue
  say "MCP_PROCS lists \`$p\` but mcp.rs does not dispatch it"
done < <(comm -13 "$MCP_ACTUAL" "$MCP_DECLARED")

# Proc-shaped literals in mcp.rs that dispatch to nothing (renamed procs).
while IFS= read -r p; do
  [ -z "$p" ] && continue
  say "mcp.rs contains proc-shaped literal \`$p\` that rpc.rs does not dispatch"
done <"$SUSPECT"

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "skill-sync-check: ok (skill↔cli surface + flags, MCP_PROCS ↔ mcp.rs, procs ↔ rpc registry)"
