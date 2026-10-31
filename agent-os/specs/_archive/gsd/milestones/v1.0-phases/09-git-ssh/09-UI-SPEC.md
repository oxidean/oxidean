---
phase: 9
slug: git-ssh
status: draft
shadcn_initialized: true
preset: base-nova
created: 2026-09-14
---

# Phase 9 — UI Design Contract (concise)

> Visual/interaction contract for Git SSH keys + CloneBox SSH. **EXTENDS** Phase 8 (`08-UI-SPEC.md`) and Phase 7 — reuse mark, fonts, semantic tokens, settings chrome, AlertDialog revoke pattern. Locked: `09-CONTEXT.md` D-SSH-02, D-SSH-05, D-SSH-06. Authoring: Octane `.tsrx` (`.agents/skills/octane/SKILL.md`).

---

## Design System

Unchanged from Phase 8: base-nova, Sora / Source Sans 3, ShadCN + Base UI, CVA Button, existing AlertDialog / Input / Label / Skeleton / Badge. **No new tokens or fonts.**

---

## Screens in scope

| Surface | Purpose | Focal point |
|---------|---------|-------------|
| `/settings/ssh-keys` | Add / list / revoke SSH public keys (D-SSH-05, D-SSH-06) | Heading + **Add SSH key** + list |
| Settings secondary nav | Profile \| Personal access tokens \| **SSH keys** | Active underline on SSH keys |
| Account menu | Link **SSH keys** → `/settings/ssh-keys` (sibling under tokens) | Existing Account dropdown |
| CloneBox SSH tab/panel | Copyable scp-style URL + Port hint + add-key CTA (D-SSH-02, D-SSH-06) | SSH URL input + compact how-to |

**Out of scope UI:** SSH CA, deploy keys, interactive shell docs, `ssh://` as primary clone string, PAT scopes on keys.

---

## `/settings/ssh-keys` (D-SSH-05, D-SSH-06)

| Rule | Contract |
|------|----------|
| Layout | Same as tokens: max-w-2xl, SettingsNav with `active: "ssh-keys"` |
| Auth | Unauthenticated → `/login?returnTo=/settings/ssh-keys` |
| Title | **SSH keys** |
| Support | One muted Body: Keys authenticate git over SSH as user `git`. Identity comes from the key fingerprint, not the SSH username. |
| Add | Primary **Add SSH key**. Disabled when `!email_verified` with copy **Verify your email to add an SSH key.** (mirror tokens Generate gate) |
| Form fields | **Title** (required), **Key** (textarea, OpenSSH one-line `ssh-ed25519` / RSA). Submit **Add key**. No one-time secret reveal. |
| Empty | Hero **No SSH keys** + Body + Add CTA |
| Rows | Title (Label 600), fingerprint `SHA256:…` (mono/muted), algorithm Badge, **Last used** or **Never**, created relative, **Delete** / **Revoke** destructive ghost |
| Revoke | AlertDialog: **Revoke SSH key?** / Keep key / Revoke key — confirm only (D-SSH-05) |
| Cap | At 25 keys: disable Add + inline hint **Maximum of 25 SSH keys** |
| Errors | Inline destructive for invalid key / duplicate fingerprint / `auth.email_unverified` |

---

## CloneBox SSH (D-SSH-02, D-SSH-06)

| Rule | Contract |
|------|----------|
| Primary string | Always scp-style `git@{host}:{owner}/{repo}.git` via `sshCloneUrl` — **never** prefer `ssh://` as the copyable primary |
| Port ≠ 22 | One-line muted hint: add `Port {n}` (or Host alias) under `Host {host}` in `~/.ssh/config` |
| Copy | Same copy-input pattern as HTTPS |
| How-to | Compact PatHowTo-style panel: register a key → **Add an SSH key** CTA → `/settings/ssh-keys`; login user is **`git`** |
| Placeholder | Remove Phase 7/8 muted “SSH cloning arrives in a later phase.” |

---

## Copy locks (UI strings)

| Surface | Exact / near-exact |
|---------|-------------------|
| Nav | **SSH keys** |
| Page title | **SSH keys** |
| Empty | **No SSH keys** |
| Add CTA | **Add SSH key** |
| Verify wall | **Verify your email to add an SSH key.** |
| Revoke dialog | **Revoke SSH key?** · **Keep key** · **Revoke key** |
| CloneBox CTA | **Add an SSH key** (links `/settings/ssh-keys`) |
| CloneBox user | Document SSH login user **`git`** |

---

## A11y / motion

- Revoke dialog focus trap via existing AlertDialog
- Copy buttons announce Copied (same as CloneBox HTTPS)
- No new motion; respect `prefers-reduced-motion`

---

## Executor checklist

- [ ] SettingsNav union includes `"ssh-keys"`; chrome Account link present
- [ ] Octane Rivet only — no JSX `return (` mixed with `@{`
- [ ] Primary clone string is scp-style; Port hint when advertised port ≠ 22
- [ ] Unverified cannot add; revoke confirms; no secret reveal on add
