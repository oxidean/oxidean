import type { ClassicPatScope, FgPerms, FgPermLevel } from "@oxidean/api-client";

/** Three-way permission picker value — `none` omits the family from `perms`. */
export type PermChoice = "none" | "read" | "write";

type ClassicScopeRow = {
  scope: ClassicPatScope;
  testid: string;
  label: string;
  hint: string;
};

export type ClassicScopeGroup = {
  title: string;
  scopes: ClassicScopeRow[];
};

/** Classic scope catalog, grouped like GitHub's token page. */
export const CLASSIC_SCOPE_GROUPS: ClassicScopeGroup[] = [
  {
    title: "Repository",
    scopes: [
      {
        scope: "repo",
        testid: "scope-repo",
        label: "repo — Full control of private repositories",
        hint: "Required for git clone, fetch, and push over HTTPS.",
      },
      {
        scope: "public_repo",
        testid: "scope-public-repo",
        label: "public_repo — Public repositories only",
        hint: "Read and collaborate on public repositories; no private access.",
      },
      {
        scope: "repo:status",
        testid: "scope-repo-status",
        label: "repo:status — Commit statuses",
        hint: "Read and create commit status checks.",
      },
      {
        scope: "repo:invite",
        testid: "scope-repo-invite",
        label: "repo:invite — Repository invitations",
        hint: "Invite collaborators and manage repository invites.",
      },
      {
        scope: "workflow",
        testid: "scope-workflow",
        label: "workflow — Actions runs",
        hint: "Dispatch, rerun, and cancel actions workflows.",
      },
      {
        scope: "security_events",
        testid: "scope-security-events",
        label: "security_events — Security events",
        hint: "Reserved for the security-events surface.",
      },
      {
        scope: "delete_repo",
        testid: "scope-delete-repo",
        label: "delete_repo — Delete repositories",
        hint: "Not implied by repo; required to soft-delete a repository.",
      },
    ],
  },
  {
    title: "Packages",
    scopes: [
      {
        scope: "package:read",
        testid: "scope-package-read",
        label: "package:read — Download packages",
        hint: "Registry download only; does not include repository access by itself.",
      },
      {
        scope: "package:write",
        testid: "scope-package-write",
        label: "package:write — Publish packages",
        hint: "Implies package:read. Publish and overwrite package versions.",
      },
      {
        scope: "package:delete",
        testid: "scope-package-delete",
        label: "package:delete — Delete package versions",
        hint: "Not implied by package:write.",
      },
    ],
  },
  {
    title: "Account",
    scopes: [
      {
        scope: "notifications",
        testid: "scope-notifications",
        label: "notifications — Notifications",
        hint: "Read and manage your notifications.",
      },
      {
        scope: "user",
        testid: "scope-user",
        label: "user — Full profile access",
        hint: "Implies read:user, user:email, and user:follow.",
      },
      {
        scope: "read:user",
        testid: "scope-read-user",
        label: "read:user — Read profile data",
        hint: "Read your profile and starred/watched lists.",
      },
      {
        scope: "user:email",
        testid: "scope-user-email",
        label: "user:email — Read email addresses",
        hint: "List your account email addresses.",
      },
      {
        scope: "user:follow",
        testid: "scope-user-follow",
        label: "user:follow — Follow users",
        hint: "Follow and unfollow other users.",
      },
    ],
  },
  {
    title: "Organizations",
    scopes: [
      {
        scope: "read:org",
        testid: "scope-read-org",
        label: "read:org — Read org data",
        hint: "Read org profiles and membership.",
      },
      {
        scope: "write:org",
        testid: "scope-write-org",
        label: "write:org — Administer orgs",
        hint: "Implies read:org. Create orgs and manage membership.",
      },
    ],
  },
  {
    title: "Webhooks and keys",
    scopes: [
      {
        scope: "admin:repo_hook",
        testid: "scope-admin-repo-hook",
        label: "admin:repo_hook — Repository webhooks",
        hint: "Full control of repository webhooks.",
      },
      {
        scope: "read:public_key",
        testid: "scope-read-public-key",
        label: "read:public_key — List SSH keys",
        hint: "Read your SSH public keys.",
      },
      {
        scope: "write:public_key",
        testid: "scope-write-public-key",
        label: "write:public_key — Manage SSH keys",
        hint: "Implies read:public_key. Add and revoke SSH keys.",
      },
      {
        scope: "admin:public_key",
        testid: "scope-admin-public-key",
        label: "admin:public_key — Full SSH key control",
        hint: "Implies write:public_key.",
      },
      {
        scope: "read:gpg_key",
        testid: "scope-read-gpg-key",
        label: "read:gpg_key — List GPG keys",
        hint: "Read your GPG public keys.",
      },
      {
        scope: "write:gpg_key",
        testid: "scope-write-gpg-key",
        label: "write:gpg_key — Manage GPG keys",
        hint: "Implies read:gpg_key. Add and revoke GPG keys.",
      },
      {
        scope: "admin:gpg_key",
        testid: "scope-admin-gpg-key",
        label: "admin:gpg_key — Full GPG key control",
        hint: "Implies write:gpg_key.",
      },
    ],
  },
  {
    title: "Projects",
    scopes: [
      {
        scope: "project",
        testid: "scope-project",
        label: "project — Projects",
        hint: "Reserved for project boards.",
      },
      {
        scope: "read:project",
        testid: "scope-read-project",
        label: "read:project — Read projects",
        hint: "Reserved for project boards.",
      },
    ],
  },
];

/** Broader scope ⇒ scopes it implies (UI greys implied boxes; the API enforces). */
const CLASSIC_SCOPE_IMPLIES: Partial<Record<ClassicPatScope, ClassicPatScope[]>> = {
  user: ["read:user", "user:email", "user:follow"],
  "write:org": ["read:org"],
  "write:public_key": ["read:public_key"],
  "admin:public_key": ["write:public_key", "read:public_key"],
  "write:gpg_key": ["read:gpg_key"],
  "admin:gpg_key": ["write:gpg_key", "read:gpg_key"],
  "package:write": ["package:read"],
};

export type ClassicScopeValues = Record<string, boolean>;

/** Every scope `scope` implies — transitively flattened. */
function impliedScopes(scope: ClassicPatScope): ClassicPatScope[] {
  const seen = new Set<ClassicPatScope>();
  const walk = (s: ClassicPatScope) => {
    for (const next of CLASSIC_SCOPE_IMPLIES[s] ?? []) {
      if (seen.has(next)) continue;
      seen.add(next);
      walk(next);
    }
  };
  walk(scope);
  return [...seen];
}

/** True when `scope` is implied by another checked scope (checked + disabled). */
export function isImplied(values: ClassicScopeValues, scope: ClassicPatScope): boolean {
  return (Object.keys(values) as ClassicPatScope[]).some(
    (held) => values[held] === true && impliedScopes(held).includes(scope),
  );
}

/** Scopes to send — checked scopes only (the server applies implications). */
export function selectedScopes(values: ClassicScopeValues): ClassicPatScope[] {
  return (Object.keys(values) as ClassicPatScope[]).filter((s) => values[s] === true);
}

export type FgPermRow = {
  key: keyof FgPerms;
  label: string;
  hint: string;
};

/** Repo-bound FG families — apply to covered repositories. */
export const FG_REPO_PERMS: FgPermRow[] = [
  { key: "issues", label: "Issues", hint: "Read and write issues, comments, and labels." },
  {
    key: "pull_requests",
    label: "Pull requests",
    hint: "Read and write pull requests, reviews, and merges.",
  },
  { key: "actions", label: "Actions", hint: "List, dispatch, rerun, and cancel workflow runs." },
  {
    key: "commit_statuses",
    label: "Commit statuses",
    hint: "Read and create commit status checks.",
  },
  { key: "secrets", label: "Secrets", hint: "Manage actions secrets on repositories." },
  { key: "webhooks", label: "Webhooks", hint: "Manage repository webhooks and deliveries." },
  {
    key: "administration",
    label: "Administration",
    hint: "Collaborators, protection rules, mirrors, settings, deletion.",
  },
  {
    key: "releases",
    label: "Releases",
    hint: "Manage releases and assets. Read also rides the contents permission.",
  },
  {
    key: "metadata",
    label: "Metadata",
    hint: "Repository metadata reads; an alternative to contents for read-only tokens.",
  },
];

/** Account-level FG families — act on the token owner, no repo binding. */
export const FG_ACCOUNT_PERMS: FgPermRow[] = [
  { key: "emails", label: "Email addresses", hint: "Read and manage account emails." },
  { key: "notifications", label: "Notifications", hint: "Read and manage notifications." },
  { key: "profile", label: "Profile", hint: "Update your public profile." },
  { key: "ssh_keys", label: "SSH keys", hint: "Read and manage SSH public keys." },
  { key: "gpg_keys", label: "GPG keys", hint: "Read and manage GPG public keys." },
  { key: "starring", label: "Starring", hint: "Star and unstar repositories." },
  { key: "watching", label: "Watching", hint: "Watch and unwatch repositories." },
  { key: "followers", label: "Followers", hint: "Follow users and list followers." },
];

export type FgPermValues = Record<string, PermChoice>;

/** `perms` payload for `pat.createFineGrained` — `none` entries are dropped. */
export function buildPerms(values: FgPermValues): FgPerms | undefined {
  const out: Record<string, FgPermLevel> = {};
  for (const [key, level] of Object.entries(values)) {
    if (level === "read" || level === "write") out[key] = level;
  }
  return Object.keys(out).length > 0 ? (out as FgPerms) : undefined;
}
