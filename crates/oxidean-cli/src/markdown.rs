//! Markdown rendering — `termimad` on TTY, raw text when piped.
//!
//! Issue/PR/release bodies render with structure (headings, emphasis, code
//! blocks) in an interactive terminal; piped output keeps the source so
//! `ox issue view 1 | grep` works on the underlying markdown.

/// Render `md` for display: styled when stdout is a TTY, verbatim otherwise.
pub fn render(md: &str) -> String {
    if !crate::term::stdout_tty() {
        return md.to_string();
    }
    let skin = termimad::MadSkin::default();
    skin.term_text(md).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_tty_passthrough() {
        if !crate::term::stdout_tty() {
            assert_eq!(render("**bold**"), "**bold**");
        }
    }
}
