/**
 * Shared conventional-commit parser — contract C2 (`conventional-commit-parse`).
 * Produces raw signals only: type/scope/breaking/exempt. The bump decision is
 * U3's; this module never maps a commit to a bump level. Grammar vocabulary
 * (`allowed_types`, `reserved_scopes`) comes from the registry (U1), so changing
 * types is a registry edit, not a parser edit.
 */
import type { RegistryConfig } from "./registry.ts";

export interface CommitMessage {
  /** First line of the commit message. */
  subject: string;
  /** Remainder of the message (body + footers), if any. */
  body?: string;
  sha?: string;
  changedFiles?: string[];
  source?: "message" | "file" | "range" | "pr_title";
}

export type ScopeStatus = "mapped" | "unmapped" | "reserved" | "absent";

export interface ConventionalCommit {
  sha?: string;
  /** Present unless the commit is exempt or grammar-invalid. */
  type?: string;
  /** Raw scope token as written (single token only). */
  scope?: string;
  /** Component ids resolved via registry scope aliases. */
  resolved: string[];
  scope_status: ScopeStatus;
  breaking: boolean;
  exempt: boolean;
  /** Raw signal — BR/C2 output; U3 maps it to a bump level. */
  valid: boolean;
  /** Grammar violation description when `valid` is false. */
  error?: string;
  warnings: string[];
}

/** Subject prefixes that short-circuit parsing entirely (BR2.3). */
const EXEMPT_SUBJECTS = [/^Merge\b/, /^Revert\s+"/, /^chore\(release\):/];

const HEADER_RE = /^([A-Za-z]+)(?:\(([^)]*)\))?(!)?:\s+(.*)$/;
const BREAKING_FOOTER_RE = /^BREAKING[ -]CHANGE:\s*/m;
const SUBJECT_LENGTH_WARN = 100;

/**
 * Parse one commit message through the shared grammar path (BR2.5 — every
 * enforcement surface uses this exact function).
 */
export function parseCommit(
  msg: CommitMessage,
  registry: Pick<RegistryConfig, "allowed_types" | "reserved_scopes" | "entries">,
): ConventionalCommit {
  const subject = msg.subject.trimEnd();
  const base = {
    sha: msg.sha,
    resolved: [] as string[],
    breaking: false,
    warnings: [] as string[],
  };

  // BR2.3 — exempt subjects swallow everything else: no type required.
  if (EXEMPT_SUBJECTS.some((re) => re.test(subject))) {
    return { ...base, scope_status: "absent", exempt: true, valid: true };
  }

  const m = subject.match(HEADER_RE);
  if (!m) {
    return {
      ...base,
      scope_status: "absent",
      exempt: false,
      valid: false,
      error: `subject does not match "type(scope)!: subject": ${JSON.stringify(subject.slice(0, 72))}`,
    };
  }
  const [, type, rawScope, bang, subjectText] = m;
  const warnings: string[] = [...base.warnings];

  if (!registry.allowed_types.includes(type)) {
    const suggestion = /[A-Z]/.test(type)
      ? ` (type must be lowercase)`
      : "";
    return {
      ...base,
      type,
      scope_status: "absent",
      exempt: false,
      valid: false,
      error: `unknown type "${type}"${suggestion} — allowed: ${registry.allowed_types.join(", ")}`,
    };
  }
  if (subjectText.trim() === "") {
    return {
      ...base,
      type,
      scope_status: "absent",
      exempt: false,
      valid: false,
      error: "empty subject",
    };
  }

  // Scope — single token only; multi-token scopes are grammar violations.
  let scope: string | undefined;
  let scope_status: ScopeStatus = "absent";
  let resolved: string[] = [];
  if (rawScope !== undefined) {
    const s = rawScope.trim();
    if (s === "" || /[\s,]/.test(s)) {
      return {
        ...base,
        type,
        scope: rawScope,
        scope_status: "absent",
        exempt: false,
        valid: false,
        error: `scope must be a single token, got ${JSON.stringify(rawScope)}`,
      };
    }
    scope = s;
    if (registry.reserved_scopes.includes(s)) {
      scope_status = "reserved";
      warnings.push(`scope "${s}" is reserved — attributes to no component`);
    } else {
      const hits = registry.entries.filter((e) => e.scopes.includes(s)).map((e) => e.id);
      if (hits.length > 0) {
        scope_status = "mapped";
        resolved = hits;
      } else {
        scope_status = "unmapped";
        warnings.push(
          `scope "${s}" maps to no registry component — attribution falls back to changed paths`,
        );
      }
    }
  }

  // BR2.2 — `!` on the header or a BREAKING footer in the body.
  const breaking = bang === "!" || BREAKING_FOOTER_RE.test(msg.body ?? "");

  if (subjectText.length > SUBJECT_LENGTH_WARN) {
    warnings.push(`subject is ${subjectText.length} chars (> ${SUBJECT_LENGTH_WARN}) — advisory`);
  }

  return {
    ...base,
    type,
    scope,
    resolved,
    scope_status,
    breaking,
    exempt: false,
    valid: true,
    warnings,
  };
}
