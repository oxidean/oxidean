# Accessibility Checklist — Side-by-Side Agentic VCS Protocol

Scope: one Octane control (`VcsToggle` on `[owner]/[repo]/settings`) + the terminal/agent surfaces' accessibility analogues. Standard: WCAG 2.1 AA for the web control.

## Web control (`VcsToggle`) — WCAG 2.1 AA

- [ ] **Label**: visible text label "Enable the VCS protocol" associated to the checkbox (not placeholder/aria-only)
- [ ] **Grouping**: inside a `fieldset` with `legend` "Source control protocols"
- [ ] **Keyboard**: Tab reaches it; Space toggles; the section Save activates with Enter — no mouse-only path
- [ ] **Focus visible**: focus indicator preserved (existing settings focus styles; never `outline: none` unstyled)
- [ ] **Disabled states**: archived/unauthorized render `disabled` with muted styling + explanatory text (not color-only)
- [ ] **Error announcement**: save failure renders inline text with `role="alert"` — announced without focus theft; control reverts to the stored value
- [ ] **Saving state**: button disabled + text "Saving…" — not a spinner-only affordance
- [ ] **Contrast**: label/description/error text ≥ 4.5:1; control affordance ≥ 3:1 against background
- [ ] **Reflow**: content readable at 320px width (settings layout already single-column; verify no clipping)
- [ ] **No keyboard trap**: standard checkbox semantics; nothing custom that could trap focus

## Terminal/CLI surfaces — accessibility analogues

- [ ] **No color-only semantics**: every status/conflict/error is spelled in text (`ox: vcs.ref_conflict: …`); color strictly decorative
- [ ] **Deterministic plain-text output**: stable one-line summaries + tables a screen reader can linearize; no box-drawing that scrambles reading order (mockup borders are doc-only, not emitted)
- [ ] **stderr discipline**: errors on stderr, results on stdout — assistive tools and pipelines both read the right stream
- [ ] **Non-zero exit on every typed failure**: state never encoded in exit-adjacent side channels a screen reader can't see
- [ ] **`--json` parity**: every human-visible datum exists in the JSON payload (assistive pipelines are first-class)

## MCP/RPC surfaces

- [ ] Domain denials return `isError` tool content with the typed code — an agent's tooling can perceive denial without parsing prose
- [ ] Error `data` fields carry machine-usable detail (`actual` for ref conflicts, `conflicts[]` for merge) — no message-string scraping required

## Notes

- No ARIA-live regions needed beyond the `role="alert"` error path — the control is a simple form element.
- Responsive behavior is inherited; no breakpoint-specific a11y concerns introduced.
