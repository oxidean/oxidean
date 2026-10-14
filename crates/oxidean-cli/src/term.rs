//! Terminal capability detection — TTY checks, color policy, OSC-8 links.
//!
//! Color policy mirrors `gh`: color when stdout is a TTY unless `NO_COLOR` is
//! set or `CLICOLOR=0`; `CLICOLOR_FORCE` (non-`0`) forces color even when
//! piped. `OXIDEAN_FORCE_TTY` forces TTY behavior for prompts/color (useful in
//! CI and tests) — gh's `GH_FORCE_TTY` analog.

use std::io::IsTerminal;

/// `OXIDEAN_FORCE_TTY` — any non-empty, non-`0` value forces interactive
/// behavior on stdout (prompts stay stdin-gated separately).
pub fn force_tty() -> bool {
    env_on("OXIDEAN_FORCE_TTY")
}

fn env_on(name: &str) -> bool {
    std::env::var(name)
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// Interactive terminal on stdout (or forced).
pub fn stdout_tty() -> bool {
    force_tty() || std::io::stdout().is_terminal()
}

/// Interactive terminal on stderr (spinners, prompt surfaces write here).
pub fn stderr_tty() -> bool {
    force_tty() || std::io::stderr().is_terminal()
}

/// Interactive terminal on stdin — prompts require this AND `stdout_tty`.
pub fn stdin_tty() -> bool {
    std::io::stdin().is_terminal()
}

/// Safe to prompt: gh prompts only when stdin is a TTY and output isn't
/// piped into another program (answers would go nowhere useful).
pub fn can_prompt() -> bool {
    stdin_tty() && stdout_tty()
}

/// Color styling allowed for stdout text.
pub fn color_enabled() -> bool {
    if env_on("CLICOLOR_FORCE") {
        return true;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if let Ok(v) = std::env::var("CLICOLOR") {
        if v == "0" {
            return false;
        }
    }
    stdout_tty()
}

/// Wrap `text` in a style when color is on, pass through otherwise.
pub fn styled(text: &str, style: anstyle::Style) -> String {
    if color_enabled() {
        format!("{style}{text}{}", anstyle::Reset)
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    styled(text, anstyle::Style::new().bold())
}

pub fn dim(text: &str) -> String {
    styled(text, anstyle::Style::new().dimmed())
}

pub fn green(text: &str) -> String {
    styled(
        text,
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green))),
    )
}

pub fn red(text: &str) -> String {
    styled(
        text,
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red))),
    )
}

pub fn yellow(text: &str) -> String {
    styled(
        text,
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow))),
    )
}

pub fn cyan(text: &str) -> String {
    styled(
        text,
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Cyan))),
    )
}

/// OSC-8 clickable hyperlink on TTY; plain `label (url)`-style fallback when
/// piped — `label` unchanged when it already equals `url`.
pub fn link(url: &str, label: &str) -> String {
    if stdout_tty() {
        format!("\x1b]8;;{url}\x1b\\{label}\x1b]8;;\x1b\\")
    } else if label == url {
        url.to_string()
    } else {
        format!("{label} ({url})")
    }
}

#[cfg(test)]
mod tests {
    // Color/link fns read the process env; exercised indirectly through the
    // pure styled/link paths below — env-dependent branches are covered by
    // the harness e2e instead of racing on env mutation here.
    use super::*;

    #[test]
    fn link_plain_when_not_tty() {
        // In the test harness stdout is piped → fallback rendering.
        if !stdout_tty() {
            assert_eq!(link("https://x", "title"), "title (https://x)");
            assert_eq!(link("https://x", "https://x"), "https://x");
        }
    }

    #[test]
    fn styled_passthrough_when_no_color() {
        if !color_enabled() {
            assert_eq!(bold("x"), "x");
            assert_eq!(dim("x"), "x");
        }
    }
}
