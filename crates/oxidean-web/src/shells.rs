//! Request-path → shell dispatch table.
//!
//! The Astro build emits every dynamic route at a placeholder path where each
//! parameter segment is a literal `_` (e.g. `/{owner}/{repo}/issues/{n}` ships
//! as `_/_/issues/_/index.html`). This table maps incoming request paths onto
//! those shells — the contract that keeps every route served without a JS
//! runtime. Parameters bind their names for title interpolation.
//!
//! Ordering matters: entries are tried in order; put more-specific patterns
//! (more literal segments) before generic ones.

/// One path template segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seg {
    /// Literal text match.
    Lit(&'static str),
    /// `{name}` — binds a single segment.
    Param(&'static str),
    /// `{name...}` — binds one or more trailing segments (Astro rest param).
    Splat(&'static str),
}

/// A dispatchable route pattern.
struct Shell {
    /// URL segments, e.g. `["{owner}", "{repo}", "issues", "{n}"]`.
    pattern: &'static [Seg],
    /// Shell HTML relative to the dist root (`_/_/issues/_/index.html`).
    file: &'static str,
    /// `<title>` template — `{name}` placeholders interpolate bound params.
    /// `None` keeps the title the shell was built with.
    title: Option<&'static str>,
    /// Anonymous browsers get 302'd to `/login?returnTo=<path>` (presence is
    /// checked from `oxidean_session`/`oxidean_signed_in` cookies — the real
    /// ACL check still happens in the API and client shell).
    protected: bool,
    /// Server-side redirect target template for legacy aliases — `{param}`
    /// placeholders interpolate bound segments verbatim (already percent-
    /// encoded in the request path). Answers a real 302 instead of serving
    /// the stub page's client-side redirect, which bots/no-JS never follow.
    /// The emitted shell file stays in the table as a fallback for direct
    /// file access and to keep `check-route-sync` parity.
    redirect: Option<&'static str>,
}

use Seg::{Lit as L, Param as P, Splat as S};

/// The route corpus — one entry per URL shape the SPA serves.
static SHELLS: &[Shell] = &[
    // ── Public + auth-adjacent statics ────────────────────────────────────
    Shell { pattern: &[L("login")], file: "login/index.html", title: None, protected: false, redirect: None },
    Shell { pattern: &[L("signup")], file: "signup/index.html", title: None, protected: false, redirect: None },
    Shell { pattern: &[L("reset-password")], file: "reset-password/index.html", title: None, protected: false, redirect: None },
    Shell { pattern: &[L("verify")], file: "verify/index.html", title: None, protected: false, redirect: None },
    Shell { pattern: &[L("status")], file: "status/index.html", title: None, protected: false, redirect: None },
    Shell { pattern: &[L("explore")], file: "explore/index.html", title: None, protected: false, redirect: None },
    Shell { pattern: &[L("search")], file: "search/index.html", title: None, protected: false, redirect: None },
    Shell { pattern: &[L("oauth"), L("consent")], file: "oauth/consent/index.html", title: None, protected: false, redirect: None },
    Shell { pattern: &[L("invites"), P("token")], file: "invites/_/index.html", title: Some("Accept invitation · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[L("setup")], file: "setup/index.html", title: None, protected: false, redirect: None },
    Shell { pattern: &[L("setup"), L("credentials")], file: "setup/credentials/index.html", title: None, protected: false, redirect: None },
    // ── Session-required statics ──────────────────────────────────────────
    Shell { pattern: &[L("notifications")], file: "notifications/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("new")], file: "new/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("orgs"), L("new")], file: "orgs/new/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings")], file: "settings/general/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("general")], file: "settings/general/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("profile")], file: "settings/profile/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("notifications")], file: "settings/notifications/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("emails")], file: "settings/emails/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("ssh-keys")], file: "settings/ssh-keys/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("cli")], file: "settings/cli/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("applications")], file: "settings/applications/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("tokens")], file: "settings/tokens/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("tokens"), L("new")], file: "settings/tokens/new/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("settings"), L("tokens"), L("new"), L("fine-grained")], file: "settings/tokens/new/fine-grained/index.html", title: None, protected: true, redirect: None },
    // ── Admin (concrete shells; dispatch still applies the session gate) ───
    Shell { pattern: &[L("admin"), L("auth")], file: "admin/auth/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("admin"), L("lfs")], file: "admin/lfs/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("admin"), L("mcp")], file: "admin/mcp/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("admin"), L("packages")], file: "admin/packages/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("admin"), L("runners")], file: "admin/runners/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("admin"), L("templates")], file: "admin/templates/index.html", title: None, protected: true, redirect: None },
    Shell { pattern: &[L("admin"), L("users")], file: "admin/users/index.html", title: None, protected: true, redirect: None },
    // ── Owner (user/org) profiles ─────────────────────────────────────────
    Shell { pattern: &[P("owner")], file: "_/index.html", title: Some("{owner} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), L("packages")], file: "_/packages/index.html", title: Some("{owner} · Packages · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), L("settings")], file: "_/settings/index.html", title: Some("Settings · {owner} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), L("settings"), L("labels")], file: "_/settings/labels/index.html", title: Some("Labels · {owner} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), L("settings"), L("members")], file: "_/settings/members/index.html", title: Some("Members · {owner} · Oxidean"), protected: true, redirect: None },
    // ── Repository leaves — static second segments first, then params/splats
    Shell { pattern: &[P("owner"), P("repo"), L("actions")], file: "_/_/actions/index.html", title: Some("Actions · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("actions"), P("run")], file: "_/_/actions/_/index.html", title: Some("Run {run} · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("activity")], file: "_/_/activity/index.html", title: Some("Activity · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("branches")], file: "_/_/branches/index.html", title: Some("Branches · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("forks")], file: "_/_/forks/index.html", title: Some("Forks · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("insights")], file: "_/_/insights/index.html", title: Some("Insights · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("packages")], file: "_/_/packages/index.html", title: Some("Packages · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("search")], file: "_/_/search/index.html", title: Some("Search · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("stargazers")], file: "_/_/stargazers/index.html", title: Some("Stargazers · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("tags")], file: "_/_/tags/index.html", title: Some("Tags · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("watchers")], file: "_/_/watchers/index.html", title: Some("Watchers · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("issues")], file: "_/_/issues/index.html", title: Some("Issues · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("issues"), L("labels")], file: "_/_/issues/labels/index.html", title: Some("Labels · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("issues"), L("new")], file: "_/_/issues/new/index.html", title: Some("New issue · {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("issues"), P("n")], file: "_/_/issues/_/index.html", title: Some("Issue #{n} · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("pulls")], file: "_/_/pulls/index.html", title: Some("Pulls · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("pulls"), L("new")], file: "_/_/pulls/new/index.html", title: Some("New pull request · {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("pull"), P("n")], file: "_/_/pull/_/index.html", title: Some("Pull #{n} · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("releases")], file: "_/_/releases/index.html", title: Some("Releases · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("releases"), L("new")], file: "_/_/releases/new/index.html", title: Some("New release · {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("releases"), P("tag")], file: "_/_/releases/_/index.html", title: Some("Release {tag} · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("commit"), P("sha")], file: "_/_/commit/_/index.html", title: Some("{sha} · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("settings")], file: "_/_/settings/index.html", title: Some("Settings · {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    // Legacy alias — Actions settings moved to /{o}/{r}/settings#repo-settings-actions.
    // Real 302 (param-interpolated) so bots/no-JS follow too; the emitted shell
    // file remains as the no-JS fallback document.
    Shell { pattern: &[P("owner"), P("repo"), L("settings"), L("actions")], file: "_/_/settings/actions/index.html", title: Some("Settings · {owner}/{repo} · Oxidean"), protected: true, redirect: Some("/{owner}/{repo}/settings#repo-settings-actions") },
    // Auth-gated mutating leaves (splat tails are file paths, not params).
    Shell { pattern: &[P("owner"), P("repo"), L("fork")], file: "_/_/fork/index.html", title: Some("Fork {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("new"), S("path")], file: "_/_/new/_/index.html", title: Some("New file · {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("edit"), S("path")], file: "_/_/edit/_/index.html", title: Some("Edit · {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("mkdir"), S("path")], file: "_/_/mkdir/_/index.html", title: Some("New directory · {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("delete"), S("path")], file: "_/_/delete/_/index.html", title: Some("Delete · {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("upload"), S("path")], file: "_/_/upload/_/index.html", title: Some("Upload files · {owner}/{repo} · Oxidean"), protected: true, redirect: None },
    // Read-only splat leaves.
    Shell { pattern: &[P("owner"), P("repo"), L("blame"), S("path")], file: "_/_/blame/_/index.html", title: Some("Blame · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("blob"), S("path")], file: "_/_/blob/_/index.html", title: Some("{owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("tree"), S("path")], file: "_/_/tree/_/index.html", title: Some("{owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("commits"), S("ref")], file: "_/_/commits/_/index.html", title: Some("Commits · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    Shell { pattern: &[P("owner"), P("repo"), L("compare"), S("range")], file: "_/_/compare/_/index.html", title: Some("Compare · {owner}/{repo} · Oxidean"), protected: false, redirect: None },
    // The repo index is the catch-all two-segment route — keep it last.
    Shell { pattern: &[P("owner"), P("repo")], file: "_/_/index.html", title: Some("{owner}/{repo} · Oxidean"), protected: false, redirect: None },
];

/// The outcome of matching a request path against the table.
pub struct Match {
    /// Shell HTML relative to the dist root.
    pub file: &'static str,
    /// Interpolated `<title>` (params substituted), if the pattern sets one.
    pub title: Option<String>,
    /// Whether anonymous requests redirect to login.
    pub protected: bool,
    /// The pattern is all params (`/{owner}`, `/{owner}/{repo}`), so a dotted
    /// final segment can also be a real file under dist/ (`/brand/logo.png`).
    /// Literal-prefix patterns never overlap asset space — their dotted
    /// params are route data (`releases/v1.0`, `blob/README.md`).
    pub asset_overlap: bool,
    /// Interpolated 302 target (params substituted, percent-encoded form
    /// preserved) for legacy alias patterns, if the pattern sets one.
    pub redirect: Option<String>,
}

/// Match `path` (no query string, no trailing slash besides root) to a shell.
/// Params bind percent-decoded raw segments — shells only echo them into
/// `<title>` after HTML-escaping, so arbitrary values are safe.
pub fn dispatch(path: &str) -> Option<Match> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Some(Match { file: "index.html", title: None, protected: false, asset_overlap: false, redirect: None });
    }
    let segs: Vec<&str> = trimmed.split('/').collect();

    // Score patterns: more literal segments win, earlier entries break ties.
    let mut best: Option<(&Shell, Vec<(usize, usize)>)> = None;
    let mut best_lit = usize::MAX;
    'outer: for shell in SHELLS {
        let mut bindings: Vec<(usize, usize)> = Vec::new(); // (param id, seg idx)
        let mut i = 0;
        for (pid, seg) in shell.pattern.iter().enumerate() {
            match seg {
                L(lit) => {
                    if segs.get(i).copied() != Some(*lit) {
                        continue 'outer;
                    }
                    i += 1;
                }
                P(_) => {
                    if i >= segs.len() {
                        continue 'outer;
                    }
                    bindings.push((pid, i));
                    i += 1;
                }
                S(_) => {
                    if i >= segs.len() {
                        continue 'outer;
                    }
                    bindings.push((pid, i));
                    i = segs.len();
                }
            }
        }
        if i != segs.len() {
            continue;
        }
        let lit = shell
            .pattern
            .iter()
            .filter(|s| matches!(s, L(_)))
            .count();
        let score = usize::MAX - lit; // smaller = better
        if best.is_none() || score < best_lit {
            best_lit = score;
            best = Some((shell, bindings));
        }
    }

    let (shell, bindings) = best?;
    let asset_overlap = !shell.pattern.iter().any(|s| matches!(s, L(_)));
    let title = shell
        .title
        .map(|tpl| interpolate(tpl, &segs, &bindings, shell.pattern, true));
    // Redirect targets keep raw (still percent-encoded) segments — they're
    // URLs, not markup.
    let redirect = shell
        .redirect
        .map(|tpl| interpolate(tpl, &segs, &bindings, shell.pattern, false));
    Some(Match {
        file: shell.file,
        title,
        protected: shell.protected,
        asset_overlap,
        redirect,
    })
}

/// Substitute `{param}`/`{param…}` placeholders in `tpl` with bound segments.
/// `escape` HTML-escapes values (for `<title>`); redirect targets pass false.
fn interpolate(
    tpl: &str,
    segs: &[&str],
    bindings: &[(usize, usize)],
    pattern: &[Seg],
    escape: bool,
) -> String {
    let mut out = String::from(tpl);
    for &(pid, seg_idx) in bindings {
        let map = |v: String| if escape { html_escape(&v) } else { v };
        match pattern[pid] {
            P(name) => {
                let v = map(segs[seg_idx].to_string());
                out = out.replace(&format!("{{{name}}}"), &v);
            }
            S(name) => {
                let v = map(segs[seg_idx..].join("/"));
                out = out.replace(&format!("{{{name}}}"), &v);
            }
            L(_) => {}
        }
    }
    out
}

/// Detect Astro's static redirect stub: `Astro.redirect()` in a prerendered
/// page emits `<meta http-equiv="refresh" content="N;url=/target">` HTML since
/// static hosts can't send 302s. We can — return the target so the server
/// answers with a real `Location` redirect instead of a visible "Redirecting"
/// interstitial.
pub(crate) fn redirect_stub_target(html: &str) -> Option<&str> {
    // Stubs are single-line stubs a few hundred bytes long; real shells are
    // far larger and contain scripts/markup.
    if html.len() > 2048 || !html.contains("http-equiv=\"refresh\"") {
        return None;
    }
    let start = html.find("http-equiv=\"refresh\"")?;
    // Parse the `content` attribute of the refresh tag itself — other meta
    // tags on the stub (robots, canonical) also carry `content=` values.
    let tag = html[start..].split('>').next()?;
    let content = tag.split("content=\"").nth(1)?.split('"').next()?;
    let url = content.split("url=").nth(1)?.trim();
    // Same-origin paths only — never redirect to a scheme or protocol-relative URL.
    (url.starts_with('/') && !url.starts_with("//")).then_some(url)
}

/// Minimal escaping for values interpolated into `<title>`/meta text.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_of(path: &str) -> &'static str {
        dispatch(path).expect(path).file
    }

    #[test]
    fn statics() {
        assert_eq!(file_of("/"), "index.html");
        assert_eq!(file_of("/login"), "login/index.html");
        assert_eq!(file_of("/login/"), "login/index.html");
        assert_eq!(file_of("/settings/tokens/new"), "settings/tokens/new/index.html");
    }

    #[test]
    fn dynamic() {
        assert_eq!(file_of("/jesse"), "_/index.html");
        assert_eq!(file_of("/jesse/app"), "_/_/index.html");
        assert_eq!(file_of("/jesse/app/issues/42"), "_/_/issues/_/index.html");
        assert_eq!(file_of("/jesse/app/tree/main/src"), "_/_/tree/_/index.html");
        assert_eq!(file_of("/jesse/settings/members"), "_/settings/members/index.html");
        assert_eq!(file_of("/invites/abc123"), "invites/_/index.html");
    }

    #[test]
    fn precedence() {
        // Literal beats param: /o/r/issues/new → new-issue shell, not {n}.
        assert_eq!(file_of("/o/r/issues/new"), "_/_/issues/new/index.html");
        assert_eq!(file_of("/o/r/pulls/new"), "_/_/pulls/new/index.html");
        // Owner "settings" beats repo named "settings".
        assert_eq!(file_of("/jesse/settings"), "_/settings/index.html");
    }

    #[test]
    fn protection() {
        assert!(!dispatch("/jesse/app").unwrap().protected);
        assert!(dispatch("/settings").unwrap().protected);
        assert!(dispatch("/admin/users").unwrap().protected);
        assert!(dispatch("/o/r/fork").unwrap().protected);
        assert!(dispatch("/o/r/issues/new").unwrap().protected);
        assert!(!dispatch("/o/r/issues/5").unwrap().protected);
    }

    #[test]
    fn titles() {
        assert_eq!(
            dispatch("/jesse/app/issues/42").unwrap().title.as_deref(),
            Some("Issue #42 · jesse/app · Oxidean")
        );
        assert_eq!(dispatch("/login").unwrap().title, None);
        // Injection-safe: angle brackets in params are escaped.
        assert_eq!(
            dispatch("/<b>/app").unwrap().title.as_deref(),
            Some("&lt;b&gt;/app · Oxidean")
        );
    }

    #[test]
    fn unmatched() {
        assert!(dispatch("/no/such/route/shape/here/xyz/123/456").is_none());
    }

    #[test]
    fn legacy_redirect() {
        let m = dispatch("/jesse/app/settings/actions").unwrap();
        assert_eq!(
            m.redirect.as_deref(),
            Some("/jesse/app/settings#repo-settings-actions")
        );
        // Percent-encoded segments survive verbatim into the target.
        let m = dispatch("/jesse/a%20b/settings/actions").unwrap();
        assert_eq!(
            m.redirect.as_deref(),
            Some("/jesse/a%20b/settings#repo-settings-actions")
        );
        assert!(dispatch("/o/r/settings").unwrap().redirect.is_none());
    }

    #[test]
    fn redirect_stubs() {
        let stub = r#"<html><head><title>Redirecting</title><meta http-equiv="refresh" content="2;url=/settings/profile"><meta name="robots" content="noindex"></head><body><a href="/settings/profile">Redirecting</a></body></html>"#;
        assert_eq!(redirect_stub_target(stub), Some("/settings/profile"));
        // Real shells are not stubs (too large / no refresh marker).
        assert_eq!(redirect_stub_target("<html><body>app</body></html>"), None);
        let big = format!("<html>{}</html>", "x".repeat(3000));
        assert_eq!(redirect_stub_target(&big), None);
        // Protocol-relative / external targets are refused.
        let evil = r#"<html><meta http-equiv="refresh" content="0;url=//evil.test/x"></html>"#;
        assert_eq!(redirect_stub_target(evil), None);
        let ext = r#"<html><meta http-equiv="refresh" content="0;url=https://evil.test"></html>"#;
        assert_eq!(redirect_stub_target(ext), None);
    }

    #[test]
    fn asset_overlap_only_on_param_only_patterns() {
        // `/{owner}` and `/{owner}/{repo}` overlap real files under dist/.
        assert!(dispatch("/favicon.ico").unwrap().asset_overlap);
        assert!(dispatch("/brand/logo.png").unwrap().asset_overlap);
        // Dotted route params are route data on literal-prefix patterns —
        // they must never take the file-or-404 branch.
        let rel = dispatch("/jesse/app/releases/v1.2.3").unwrap();
        assert_eq!(rel.file, "_/_/releases/_/index.html");
        assert!(!rel.asset_overlap);
        let blob = dispatch("/jesse/app/blob/main/README.md").unwrap();
        assert_eq!(blob.file, "_/_/blob/_/index.html");
        assert!(!blob.asset_overlap);
        let edit = dispatch("/jesse/app/edit/src/main.rs").unwrap();
        assert_eq!(edit.file, "_/_/edit/_/index.html");
        assert!(edit.protected);
        assert!(!edit.asset_overlap);
    }
}
