## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 2

**Scope:** verification of the four iteration-1 fixes across
`construction/oxpilot-tools/nfr-requirements/` (tech-stack-decisions.md
TS-5 + rejected alternatives, security-requirements.md NFR1.2/NFR1.3/
NFR2.4 + threat bullets, performance-requirements.md NFR3.6/NFR3.10/
NFR3.12 + Consumes header, observability-requirements.md NFR5.7 +
NFR6.4, reliability-requirements.md NFR4.2, nfr-requirements-questions.md
Q8-A, traceability.json) plus a fresh defect sweep for
fix-introduced drift, against the READY-iter-2 functional design
(`rules.md` BR1.4/BR3.3/BR4.4/BR5.4/BR7.2, `entities.md` DiffFileOp
:121–144, `functional-spec.md` pin table :367–370) and shipped ground
truth (`oxpilot.rs:127–141` NullTools impl + message, `api-core/src/repo/
files.rs:28` UPLOAD_MAX_FILES).

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Major | TS-5 + rejected-alternatives; Q8-A; NFR1.3; NFR3.10 + NFR3.12 | Pinned five-keyword validator could not express files[] object arrays, integer pagination caps, or path patterns that sibling rows demanded be schema-enforced | Expand the closed keyword set or enumerate schema-enforced vs handler-enforced bounds explicitly | Resolved |
| R-02 | Minor | security-requirements.md threat considerations | Two content-injection classes unowned: indirect prompt injection via tool OUTPUT and markup injection into persisted/public surfaces | Add both hazards; bound injected-content reach by the registry closure; assign markup sanitization to U8/forge renderer | Resolved |
| R-03 | Nit | NFR2.4; NFR5.7 vs NFR5.1 | "ids, counts, and refs ONLY" phrasing contradicted NFR5.1's emitted `title` (bounded redacted free text) | Rephrase as ids, counts, refs, and bounded redacted fields (title ≤ TITLE_MAX, redacted) | Resolved |
| R-04 | Nit | NFR1.2; NFR4.2; NFR3.6 | NullTools cite :127–137 missed the pinned message at :138–139; UPLOAD_MAX_FILES cite ambiguous between the two files.rs | Widen to :127–141; qualify as oxidean-api-core/src/repo/files.rs:28 | Resolved |

**R-01 verification detail.** The expanded closed vocabulary — `type`,
`required`, `enum`, `properties`, `items`, `minLength`/`maxLength`,
`minItems`/`maxItems`, `minimum`/`maximum`, `pattern`,
`additionalProperties: false` — is pinned verbatim in TS-5 (:18), Q8-A
(:252–260), NFR1.3 (:22), and NFR3.12 (:36), and is verified sufficient
for the catalog's own schemas: DiffFileOp element objects ride
`items`+`properties`+`required`+`enum`+closed `additionalProperties`,
integer caps ride `minimum`/`maximum`, path shape rides `pattern`.
NFR3.10 (:29) now states the schema-vs-handler split explicitly —
pagination `maximum` ≤ 100, `files` `maxItems` = 50, `path` `pattern`
schema-side; summed decoded `content_b64` and op-conditional requireds
(rename⇒`new_path`, create/update⇒`content_b64`) handler-side
`invalid_args` — so BR3.3's "unrepresentable, not fixable-by-handler"
pin still holds because those exact bounds remain schema-enforced, and
"the handler NEVER sees unvalidated input" (NFR1.3) is now implementable
as written.

**R-02 verification detail.** Both bullets land at
security-requirements.md:91–110 and are technically sound: (a)
indirect injection via read-tool output names the correct vector —
issue titles/bodies, PR text, CI job logs, file contents are
adversary-authored strings relayed verbatim — and the containment
claim is right: a convinced model can still only attempt what the
closed registry + write gates + gateway consent chain permit, so the
worst case stays a bounded read, a pending proposal, or a gateway
denial; "output-side instruction-safety is the model's" is an honest
scope cut. (b) The markup bullet correctly identifies `redact()` as
credential-pattern masking only (verified redact.rs:77, iter-1) and
assigns sanitization explicitly to U8's diff-card render + the forge
comment renderer as a recorded hand-off, matching the BR5.4 shape
(PR body embeds the already-redacted proposal title + fixed
provenance line).

**R-03 verification detail.** NFR2.4 (:63) and NFR5.7 (:23) both read
"ids, counts, refs, and bounded redacted fields (`title` ≤ TITLE_MAX,
redacted)" verbatim; TITLE_MAX=200 is a real pin
(functional-spec.md:369). Sibling-row residue filed as R-08.

**R-04 verification detail.** NFR1.2 (:21) and NFR4.2 (:15) now cite
`oxpilot.rs:127–141` — verified: the `NullTools` impl spans :127–141
with the pinned "no tool executor installed" message at :138. NFR3.6
(:20) qualifies `oxidean-api-core/src/repo/files.rs:28` — verified:
UPLOAD_MAX_FILES=50 is defined at :28. Same-file/ledger residue filed
as R-06/R-09.

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-05 | Minor | tech-stack-decisions.md:32–34 (rejected alternatives) | Stale vocabulary count survives the R-01 fix: the `jsonschema`-rejection rationale still says "a five-keyword vocabulary U5 emits itself" while TS-5 (:18) two sections up pins the expanded 12-entry closed set — the rejection stays sound but now stakes its rationale on a count that contradicts the sibling row | Update the rationale wording to the expanded vocabulary (e.g. "a closed ~12-keyword vocabulary U5 emits itself") | New |
| R-06 | Minor | traceability.json :265, :32, :240, :190, :26 | Ledger unswept for the fixes: NFR4.2 reverse (:265) still cites `oxpilot.rs:127-137` — the exact drift R-04 corrected in the tables; the NFR3 coverage note (:32) and NFR3.10 reverse (:240) still characterize arg-side bounds as "unrepresentable" — the dropped claim, now contradicted by NFR3.10's explicit schema-vs-handler split; the NFR2.4 reverse (:190) and NFR2 note (:26) still gloss audit detail as "ids/counts/refs only" pre-R-03 | Sweep traceability targets/notes for the widened :127–141 cite, the schema-vs-handler split wording, and the reconciled detail vocabulary | New |
| R-07 | Nit | performance-requirements.md NFR3.10 (:29) | Constant-name drift: "`files` `maxItems` = MAX_PROPOSALS (50)" — the pinned name is `MAX_PROPOSAL_FILES` everywhere else (BR4.4:208, entities.md:138, functional-spec.md:368, NFR-S5:20, Q5-A:171) | Rename to `MAX_PROPOSAL_FILES` | New |
| R-08 | Nit | observability-requirements.md NFR6.4 (:37) vs NFR5.7 (:23) | Unreconciled sibling of R-03: NFR6.4 still pins "Log/audit fields carry ids and counts ONLY," which on the audit-detail half forbids `title`/`branch`/`pr_ref` — fields NFR5.1/NFR5.2 emit and the reconciled NFR5.7 explicitly allows | Apply the same "ids, counts, refs, and bounded redacted fields" phrasing to the audit half (the tracing half may stay ids/counts) | New |
| R-09 | Nit | performance-requirements.md:9 (Consumes header) | Same-class cite residue from R-04: the header still attributes UPLOAD_MAX_FILES to bare `files.rs` while NFR3.6 (:20) was qualified — two files.rs exist (api-core defines at :28, api-repo only imports) | Qualify the header cite as `oxidean-api-core/src/repo/files.rs` | New |

### Verdict rationale

READY: all four prior findings are fixed at substance and verified
against shipped ground truth — the validator vocabulary now covers the
catalog's own schemas with an explicit schema-vs-handler split, the
threat model owns both content-injection classes with sound
containment reasoning, the audit-detail phrasing is reconciled with
the emitted field set, and both cites resolve at line precision. The
five new findings are residual sweep misses — a stale keyword count in
a rejection rationale, an unswept traceability ledger, one constant
name drift, one unreconciled sibling row, one unqualified header cite
— all bounded wording/ledger defects that no longer leave codegen
choosing which clause to violate; the normative pins themselves are
now internally consistent. JSON validity of traceability.json
re-verified.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Major | tech-stack-decisions.md TS-5 + rejected-alternatives; nfr-requirements-questions.md Q8-A; security-requirements.md NFR1.3; performance-requirements.md NFR3.10 + NFR3.12 | **The pinned validator vocabulary cannot express bounds the same artifacts require be schema-enforced.** TS-5/Q8-A/NFR3.12 pin a "closed five-keyword subset" — `type`, `required`, `enum`, `minLength`/`maxLength`, `additionalProperties: false` — but the catalog's own schemas need more: (a) `propose_diff.files[]` is an array of `DiffFileOp` objects (entities.md:121–144), so per-element `op` enum, `path`/`new_path`/`content_b64` bounds, closed `additionalProperties` inside elements, and `files ≤ MAX_PROPOSAL_FILES=50` need `items` + nested `properties`/`required` + `minItems`/`maxItems` — none in the pinned set; (b) NFR3.10/BR3.3 demand `per_page`/`limit`/`skip ≤ 100` and `repo.commits limit ≤ 100` be "unrepresentable, not handler-fixable" — `minLength`/`maxLength` are string bounds only, an integer cap needs `maximum`; (c) `path` "no `..`, no leading `/`" needs `pattern`. Meanwhile NFR1.3 pins "the handler NEVER sees unvalidated input" and NFR3.10 repeats "unrepresentable." As written, codegen must either silently widen the keyword closure TS-5 stakes its `jsonschema`-rejection on, or silently move checks handler-side violating the unrepresentable pin — the artifacts do not say which. Conditional per-op requireds (rename→`new_path`, create/update→`content_b64`) also have no expressible home in the pinned set. | Expand the pinned keyword list to the set the catalog actually emits — add `properties`, `items`, `minItems`/`maxItems`, integer `minimum`/`maximum`, `pattern` as needed (still a closed, hand-rollable vocabulary; the zero-new-deps decision itself is sound and verified: no validation crate exists in Cargo.lock, schemars is generation-only). Or explicitly enumerate which bounds are schema-enforced vs handler-enforced `invalid_args` and drop "unrepresentable" for the latter. Either way, state where conditional per-op requireds live. | New |
| R-02 | Minor | security-requirements.md Threat considerations (:65–90) | **Threat model omits the two content-injection classes on this surface.** It covers arg-side prompt injection, confused deputy, cross-session reach, and the consent re-issue hazard, but never names: (a) indirect prompt injection riding tool OUTPUT — issue titles/bodies, PR text, CI job logs, and file contents are adversary-authored strings the read tools relay verbatim into model context, the canonical second-order vector for exactly this surface; (b) markup injection into persisted and public surfaces — model-authored `title`/`diff_body`/PR-body/`pr_comment` body pass `redact()`, which masks credential patterns only (redact.rs:77), so markup sanitization is an unowned boundary between U5's redaction pins and U8's diff-card render / the forge's public comment renderer. PR-body provenance itself is covered (BR5.4 pins the line server-side). | Add both hazards to Threat considerations: bound the injected-content reach by the same registry-closure/session pins that already bound hostile args (worst case stays a bounded read, a pending proposal, or a gateway denial), and assign markup sanitization explicitly to U8/the forge renderer or record it as an accepted residual. | New |
| R-03 | Nit | security-requirements.md NFR2.4; observability-requirements.md NFR5.7 vs NFR5.1 | **Audit-detail phrasing is stricter than the pinned event set.** "Detail carries ids, counts, and refs ONLY" — but NFR5.1's `oxpilot.proposal_created` emits `title`, a model-authored free-text field (≤200 chars, redacted) that is BR9.1-verbatim yet literally not an id/count/ref; the operative bound is really "beyond summary length." | Reconcile — phrase as "ids, counts, refs, and bounded redacted fields (title ≤ TITLE_MAX, redacted)" or cite `title` as the BR9.1-sanctioned exception. | New |
| R-04 | Nit | security-requirements.md NFR1.2; reliability-requirements.md NFR4.2; performance-requirements.md NFR3.6 | **Cite drift.** NullTools cited as `oxpilot.rs:127–137` but the pinned message "no tool executor installed" lands at :138–139 (impl spans :127–141); `UPLOAD_MAX_FILES` cited as `files.rs:28` — the constant is defined at `crates/oxidean-api-core/src/repo/files.rs:28`, while `crates/oxidean-api-repo/src/files.rs` only imports it (:18). | Widen the NullTools cite to :127–141 and qualify the UPLOAD_MAX_FILES cite as `oxidean-api-core/src/repo/files.rs:28`. | New |

> R-01 Not re-checked this round

> R-02 Not re-checked this round

> R-03 Not re-checked this round

> R-04 Not re-checked this round
