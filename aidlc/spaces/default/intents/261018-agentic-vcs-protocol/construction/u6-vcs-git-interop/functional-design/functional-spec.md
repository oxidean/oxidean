# Functional Specification — u6-vcs-git-interop

Unit: `u6-vcs-git-interop` — deterministic Git→changeset import (explicit + lazy, converging), changeset→Git export (object-mint + exactly one hook-gated ref move), `ProtectionChecker` trait port, actor-capability propagation. *(Revision 2 — dev pass + architecture review pass-1 integrated.)*

## Workflow table

| ID | Workflow | In | Out |
|----|----------|----|-----|
| WF0 | Import claim/cursor state | derived `active`/`cursor` rows | `active: bool`, resumable position |
| WF1 | `import_chain` — explicit entry | `repo_id, repo, from, cursor, limit` | `ImportPage{imported,cursor?,done,heads}` |
| WF2 | `lazy_import` — entry | `repo_id, repo, git_oid` | `ImportPage{done:true,cursor:None}` |
| WF3 | `import_one` — per-commit conversion | `RawCommit` | `ChangesetId` (or prior id) |
| WF4 | `export` — entry | `repo_id, repo, head, branch, force, actor_capability` | `ExportOutcome` |
| WF5 | `export_one` — per-changeset mint | export-set node | `git_oid` |

## State transitions (WF0)

| # | From | On | To |
|---|------|----|----|
| S0.1 | `idle`/none | `claim` `active` ⇒ `ImportState` | `active(nonce)` |
| S0.2 | `active(n)` | terminal + nonce match ⇒ `release` | `idle` |
| S0.3 | `active(n)` | terminal + nonce mismatch ⇒ skip release | `active(n)` |

## WF1 `import_chain` — ordered

```text
1. is_skipped → DbNotConfigured. limit → 1..=cap ⇒ InvalidInput.
   from+cursor ⇒ cursor authoritative (from ignored).
2. claim (S0.1): fresh active ⇒ ImportState; upsert+re-read nonce ⇒ verify.
3. Resolve tips:
     from ⇒ head-preferred verify_commit_oid: heads/{s} → tags/{s} → raw rev.
       All None ⇒ NotFound. dash-led from ⇒ InvalidInput.
       TipSnapshot.name = resolved shortname (or literal from).
     none ⇒ list_refs (heads+tags) each verify_commit_oid-peeled;
       peel None (tag→tree/blob) ⇒ drop + warn.
     tips.len() > 512 ⇒ TooLarge.
4. cursor: present ⇒ parse ⇒ ImportState on malformed; absent ⇒ {v:1,tips,skip:0}.
5. rev_list(tips, 250_000) → parents-first oids (backend reverses).
   truncated ⇒ TooLarge.
   skip>0 ⇒ continuity list[skip-1]==last_oid else ImportState.
6. window = list[skip .. skip+limit] — parents-first order:
     per oid → link_lookup ⇒ skip-count | import_one ⇒ link_insert
     + linked: row.
   NEW skip += consumed window; last_oid = list[new_skip-1];
   imported counts fresh conversions only.
7. upsert cursor (nonce re-verify; covering_changeset = last cs or None);
   release (nonce-verified);
   return ImportPage{imported, cursor iff !done, done,
                     heads[{name,git_oid,changeset_id:Option}]}.
8. error ⇒ release (nonce-verified) still runs.
```

## WF2 `lazy_import` — specialization

```text
1. is_skipped; is_git_oid else InvalidInput; link_lookup ⇒ early {done:true}.
2. verify_commit_oid(oid) ⇒ None ⇒ NotFound.
3. import_chain(from=oid-as-tip, cursor=None, limit=cap) — same traversal,
   claims, early-skip, create, sidecars, link pair ⇒ identical cs ids.
   Lazy exits when the target oid is linked (done=true, cursor=None).
```

## WF3 `import_one` — per-commit

```text
1. cat_commit(oid) → RawCommit (malformed ident ⇒ InvalidInput).
2. parents → link_lookup each ⇒ miss ⇒ Internal.
3. rows = commit_name_status(base, sha); flat = tree_flat(sha)
   (flat.len() > import_tree_max ⇒ TooLarge);
   parent flat lazily when D/R rows exist.
4. rows → FileChange[] (A⇒add M/T⇒modify D⇒delete R⇒rename);
   either-side gitlink ⇒ skip+warn; size>blob_max ⇒ TooLarge;
   cat_blob → objects.put → VCS sha256.
5. validate message UTF-8; refusal inventory (BR6.23); author empty
   after sanitize ⇒ refuse; rfc3339(a.unix) unrepresentable ⇒ InvalidInput.
6. store.create{parents,author,message,committed_at} ⇒ id.
7. ident:{oid}+ident-cs:{cs} sidecars (Some(cs) covering).
8. caller: link_insert(oid→cs) + linked:{cs}→oid.
```

## WF4 `export` — ordered

```text
1. is_skipped → DbNotConfigured; parse head ⇒ NotFound;
   check_refname(refs/heads/{branch}) ⇒ InvalidInput.
2. Chain walk: BFS over vcs_changeset_parents from head; linked_lookup hit
   ⇒ candidate anchor → verify_commit_oid(git_oid) probe — missing
   (gc'd) ⇒ treat as UNLINKED (re-mint heals). Set ≤ export_chain_max;
   topo-order parents-first.
3. Empty-set fast-path: head already anchored ⇒ old==anchored ⇒ early
   {git_oid:old, commits_count:0}; else continue ff gate w/ anchored tip.
4. FF gate: old = verify_commit_oid(refs/heads/{branch}):
   None ⇒ create path.
   Some ⇒ old_cs = link_lookup(old_oid) [FORWARD direction]:
     miss ⇒ git-only tip ⇒ RefConflict{actual:old}.
     hit ⇒ is_ancestor_cs(old_cs, head) OR old_cs ∈ set;
       non-ff ⇒ force ? continue : RefConflict.
5. Write chain (pre-check — checker needs minted tip): per changeset
   topo-order ⇒ export_one ⇒ link_insert + linked: row.
   link_insert false+stored≠ours (same-oid alias) ⇒ still linked:{ours},
   proceed.
6. checker.check(repo, refname, old, minted_tip, actor, force)
   → deny ⇒ Forbidden{verdicts} (surviving link rows point at
   unreferenced objects — healed by step-2 probe on next run).
7. Move: !force ⇒ fast_forward_ref(…, actor); force ⇒ force_update_ref.
   Classified stderr: Denied ⇒ Forbidden{[push_hook]};
   Conflict ⇒ RefConflict{actual:re-read}.
8. Return {branch, git_oid:tip, commits_count, before_oid:old}.
   gc --prune may reap minted objects pre-move — same exposure as
   commit_files; re-export heals.
```

## WF5 `export_one` — per-changeset

```text
1. parent oids: cs.parents → linked_lookup each ⇒ Internal on miss.
2. Build tree: fold manifest_diff(parent-tree, cs-tree) over base_tree;
   nested writes via write_commit recursion (mktree -z, {:06o} modes).
   Unseen blobs ⇒ objects.get (take blob_max+1) → write_object;
   seen-map keyed by VCS blob hash (dedup — same content one write).
3. Idents: sidecar hit ⇒ raw bytes; miss ⇒ author→both
   committer.date=committed_at unix UTC (round-trip note: normalized
   timestamps cannot recover original tz — deterministic fallback).
4. write_commit(entries,parents,author,committer,message) ⇒ git_oid —
   deterministic mint.
5. Return oid; caller links.
```

## Derived entity relationship view

```mermaid
erDiagram
    GitInterop ||--|| GitBackend : "extends (u6-authored)"
    GitInterop ||--|| ProtectionChecker : "in-crate port (u7 impl)"
    GitInterop ||--o{ ImportLink : "writes (insert-if-absent)"
    GitInterop ||--o{ DerivedIndexRow : "linked/ident/state rows"
    ImportPage ||--o{ ImportHead : "1..512"
    ExportOutcome ||--o{ ProtectionVerdict : "denials surface"
    Changeset ||--o{ Changeset : "parents (max 64)"
```

## Derived rules summary

- BR6.1 DbNotConfigured eager; BR6.2-5 import shape/convergence/ordering;
  BR6.6-12 changesets/sidecars/link pair; BR6.13-15 cursor semantics;
  BR6.16-18 export chain/ff/TOCTOU; BR6.19-23 mint fidelity/refusals;
  BR6.24-28 actor capability, force semantics, `linked:` reverse rows,
  nullable covering (u2 amendment), existence probes, error mapping —
  source of truth in `rules.md`.

## Rejected alternatives

- `update_ref` for moves (hook bypass — ADR-004).
- `--reverse` on rev-list (truncates wrong end — backend in-crate reverse instead).
- `""` covering sentinel (FK violation — u2 amendment taken).
- Persistent from-scratch manifests (u5-style fold-forward retained).
- `commit_files` per-commit pushes (N hook invocations — mint+single-move instead).
