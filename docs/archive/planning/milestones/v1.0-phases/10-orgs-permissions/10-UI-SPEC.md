---
phase: 10
slug: orgs-permissions
status: draft
shadcn_initialized: true
preset: base-nova
created: 2026-09-14
---

# Phase 10 — UI Design Contract

> Concise visual/interaction contract for Orgs & Permissions (D-ORG-06).
> **EXTENDS** Phase 7–8 UI-SPECs — do not reinvent mark, fonts, semantic tokens, spacing, or chrome.
> Authoring: Octane `.tsrx` (`.agents/skills/octane/SKILL.md`). No JSX `return (` mixed with Rivet `@{`.

---

## Design System (unchanged)

| Property | Value |
|----------|-------|
| Tool | shadcn base-nova (`apps/web/components.json`) |
| Fonts | Sora Variable (display), Source Sans 3 (body) |
| Tokens | `apps/web/src/styles.css` |
| Primitives | Button, Input, Label, Select, Switch, Dialog/AlertDialog, Badge, Skeleton — reuse; no new registries |

---

## Screens in scope

| Route / surface | Purpose | Focal point |
|-----------------|---------|-------------|
| `/orgs/new` | Create organization (slug + display name) | Slug field + Create organization CTA |
| `/{org}` | Minimal org overview: public repos list + member count | Org display name / slug hero |
| `/{org}/settings` | Org profile + `member_base_permission` | Settings form |
| `/{org}/settings/members` | Members list, role changes, remove; Add member by username | Members table + Add member |
| Invites panel (members page section) | Email invite create/list/revoke | Invite email field |
| `/invites/$token` | Accept email invite (account create/link) | Accept CTA / password fields if needed |
| `/new` owner picker | Choose personal user or org (Owner/Admin) | Owner Select/combobox |
| `/{owner}/{repo}/settings` Collaborators | List/add/update/remove collaborators | Collaborators section |

**Out of scope UI:** Teams (D-ORG-07); org delete/transfer UI; org-scoped PATs; branch protection.

---

## Interaction contracts

### `/orgs/new` (D-ORG-01, D-ORG-06)
- Verified session required (same verify wall pattern as `/new`).
- Fields: **Slug** (required, `validate_username` rules), **Display name** (optional).
- Errors: reserved → same copy as signup reserved username; taken → slug already used by a user or org; unverified → wall.
- Success → navigate to `/{slug}` overview.

### Members + live lookup (D-ORG-03)
- Add existing user: username Input with **live autocomplete** via `user.lookup` (prefix ≥2 chars, ≤10 results: username, display name, avatar URL only — never email).
- Role Select: Owner | Admin | Member (default Member).
- Username add works when `allow_signup` is false.
- Last Owner demote/remove → show stable error (`org.last_owner`); do not leave org ownerless.

### Email invites (D-ORG-03)
- Email + role; soft success on create where anti-enumeration requires it.
- List pending invites; Revoke with AlertDialog (Revoke invite / Keep invite).
- Accept route bypasses closed signup for the invited email only; copy explains invite-gated join.

### `member_base_permission` (D-ORG-02b)
- Select: **None** (default) | Read | Write. Helper: applies to Members on **private** org repos; Owner/Admin always admin; public repos remain readable.

### Collaborators (D-ORG-02c, D-ORG-04)
- Personal and org-owned repos. Permission Select: read | write | admin.
- Gate entire settings (and Collaborators section) on API `can_admin` (not `me.id === owner_id`).
- Add by username (reuse lookup). Empty state: short sentence + Add collaborator.

### `/new` owner picker (D-ORG-06)
- Options: `@username` (self) + orgs from `org.listMine` where caller is Owner or Admin.
- Default: self. Pass optional `owner` slug on `repo.create`.
- Remove “Organizations come in a later phase” copy.

### Org overview `/{org}`
- Public: display name, slug, public repo list (links to `/{org}/{repo}`), member count.
- Private-only repos never listed for unauthorized callers.
- Settings link visible to Org Admin+ only.

---

## Copy & a11y

- Primary CTAs ≥44px; form labels associated; autocomplete list keyboard-reachable.
- Prefer existing AuthShell / settings density from `admin/auth` and `settings/profile`.
- Document titles: `New organization · Oxidean`, `{Org} · Oxidean`, `Members · {Org}`, etc.

---

## Executor checklist

- [ ] `/orgs/new` create happy path
- [ ] Members add via lookup; role change; last-owner blocked
- [ ] Email invite create + accept (including closed signup)
- [ ] member_base None/Read/Write reflected in private repo access (API-backed)
- [ ] Collaborators on personal + org repo; settings uses `can_admin`
- [ ] `/new` owner picker creates org-owned repo
- [ ] Octane Rivet-only; Query for server lists; `onInput` for text
