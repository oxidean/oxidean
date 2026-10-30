# Phase 10 deferred items

## Deferred Items

- Signup/rename shared slug dual-check (D-ORG-01): `auth.signup` uses `login_slug_taken` (users.username + organizations.slug); profile and bootstrap username rename check the same namespace; covered by `signup_rejects_existing_org_slug` (and profile rename org-slug collision test).
  status: resolved

- 10-07 / 10-08: `git_smart_collaborator_classic_pat_push` — PAT ∩ ACL collaborator push shipped in plan 10-08; test passes under `test(collab)` filter (no longer a Wave-0 stub).
  status: resolved
