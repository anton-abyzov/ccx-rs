//! Inline terminal rendering — prints styled text directly to stdout.
//!
//! Matches Claude Code's inline rendering: welcome panel, styled chat messages,
//! tool execution display, and prompt — all scrolling naturally.

use std::io::{self, Write};

// ANSI escape sequences.
const ACCENT: &str = "\x1b[38;2;204;120;80m";
const ACCENT_BOLD: &str = "\x1b[1;38;2;204;120;80m";
const GREEN: &str = "\x1b[38;2;80;200;120m";
const DIM: &str = "\x1b[90m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";
const BG_GRAY: &str = "\x1b[48;2;50;50;50m";
const RED: &str = "\x1b[31m";

/// Terminal width, defaulting to 80 if unknown.
pub fn term_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

/// Strip ANSI escape sequences to get plain text.
fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if c == 'm' {
                in_escape = false;
            }
            continue;
        }
        result.push(c);
    }
    result
}

/// Visible character count (ignoring ANSI escapes).
fn visible_len(s: &str) -> usize {
    strip_ansi(s).chars().count()
}

/// Pad string with trailing spaces to reach target visible width.
fn pad_to(s: &str, target: usize) -> String {
    let vis = visible_len(s);
    if vis >= target {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(target - vis))
    }
}

/// Render the welcome panel with two-column layout.
pub fn render_welcome(model: &str, auth_source: &str, cwd: &str, tools: usize) {
    let width = term_width().min(80);

    // Narrow terminal fallback.
    if width < 50 {
        println!("{ACCENT_BOLD}CCX-RS v{}{RESET}", env!("CARGO_PKG_VERSION"));
        println!("{GREEN}{model}{RESET} · {DIM}{auth_source}{RESET}");
        println!("{DIM}{cwd}{RESET}");
        println!();
        return;
    }

    let left_w = width * 55 / 100;
    let right_w = width - left_w - 3; // 3 border columns: │ │ │

    // Header rule.
    let version = env!("CARGO_PKG_VERSION");
    let title = format!(" CCX-RS v{version} ");
    let rule_left = "──────";
    let rule_right_len = width.saturating_sub(rule_left.len() + title.len() + 2);
    println!(
        "{ACCENT}{rule_left}{title}{}──{RESET}",
        "─".repeat(rule_right_len)
    );

    // Top border.
    println!(
        "{ACCENT}┌{}┬{}┐{RESET}",
        "─".repeat(left_w),
        "─".repeat(right_w)
    );

    // Pet artwork (box-drawing characters).
    let pet = [
        "   ╭─────────╮",
        "   │  ◉   ◉  │",
        "   │    ◡    │",
        "   ╰─────────╯",
    ];

    // Left panel content.
    let left_lines: Vec<String> = vec![
        String::new(),
        format!("  {BOLD}Welcome back!{RESET}"),
        String::new(),
        pet[0].to_string(),
        pet[1].to_string(),
        pet[2].to_string(),
        pet[3].to_string(),
        String::new(),
        format!("  {GREEN}{model}{RESET} · {DIM}{auth_source}{RESET}"),
        format!("  {DIM}{cwd}{RESET}"),
        format!("  {DIM}Tools: {tools}{RESET}"),
        String::new(),
    ];

    // Right panel content.
    let sep = "─".repeat(right_w.saturating_sub(2));
    let right_lines: Vec<String> = vec![
        String::new(),
        format!("{ACCENT_BOLD} Tips for getting started{RESET}"),
        " Type a message to start coding".to_string(),
        " Run /init to create a CLAUDE.md".to_string(),
        " Type /help for commands".to_string(),
        " Ctrl+C to quit".to_string(),
        format!(" {DIM}{sep}{RESET}"),
        format!("{ACCENT_BOLD} Recent activity{RESET}"),
        format!(" {DIM}No recent activity{RESET}"),
        String::new(),
        String::new(),
        String::new(),
    ];

    // Print rows with border characters.
    let rows = left_lines.len().max(right_lines.len());
    for i in 0..rows {
        let left = left_lines.get(i).map(|s| s.as_str()).unwrap_or("");
        let right = right_lines.get(i).map(|s| s.as_str()).unwrap_or("");
        let left_padded = pad_to(left, left_w);
        let right_padded = pad_to(right, right_w);
        println!("{ACCENT}│{RESET}{left_padded}{ACCENT}│{RESET}{right_padded}{ACCENT}│{RESET}");
    }

    // Bottom border.
    println!(
        "{ACCENT}└{}┴{}┘{RESET}",
        "─".repeat(left_w),
        "─".repeat(right_w)
    );
    println!();
}

/// Print the input prompt.
pub fn render_prompt() {
    print!("{ACCENT}❯{RESET} ");
    io::stdout().flush().unwrap();
}

/// Print user message with gray background bar.
pub fn render_user_message(text: &str) {
    println!("{BOLD}{BG_GRAY} ❯ {text} {RESET}");
}

/// Print tool start indicator with green dot.
pub fn render_tool_start(name: &str, detail: &str) {
    if detail.is_empty() {
        println!("{GREEN}●{RESET} {BOLD}{name}{RESET}");
    } else {
        println!("{GREEN}●{RESET} {BOLD}{name}({detail}){RESET}");
    }
}

/// Print tool output/result.
pub fn render_tool_end(success: bool, preview: &str) {
    if preview.is_empty() {
        if success {
            println!("  {DIM}└ done{RESET}");
        } else {
            println!("  {RED}└ error{RESET}");
        }
        return;
    }

    let color = if success { DIM } else { RED };
    for line in preview.lines().take(10) {
        let display = if line.chars().count() > 120 {
            let truncated: String = line.chars().take(117).collect();
            format!("{truncated}...")
        } else {
            line.to_string()
        };
        println!("  {color}└ {display}{RESET}");
    }
}

/// Print streaming assistant text (no trailing newline).
pub fn render_text(text: &str) {
    print!("{text}");
    io::stdout().flush().unwrap();
}

/// Print a separator line.
pub fn render_separator() {
    let width = term_width().min(80);
    println!("{DIM}{}{RESET}", "─".repeat(width));
}

/// Print the session footer with model info.
pub fn render_footer(model: &str) {
    let width = term_width().min(80);
    println!("\n{DIM}{}{RESET}", "─".repeat(width));
    let left = "? for shortcuts";
    let gap = width.saturating_sub(left.len() + 2 + model.len());
    println!(
        "{DIM}{left}{RESET}{}{GREEN}●{RESET} {DIM}{model}{RESET}",
        " ".repeat(gap)
    );
}

/// Print an error message.
pub fn render_error(msg: &str) {
    println!("{RED}✗ {msg}{RESET}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[32mhello\x1b[0m"), "hello");
        assert_eq!(strip_ansi("no escapes"), "no escapes");
        assert_eq!(strip_ansi("\x1b[1;38;2;204;120;80mtext\x1b[0m"), "text");
    }

    #[test]
    fn test_visible_len() {
        assert_eq!(visible_len("hello"), 5);
        assert_eq!(visible_len("\x1b[32mhello\x1b[0m"), 5);
        assert_eq!(visible_len(""), 0);
    }

    #[test]
    fn test_pad_to() {
        assert_eq!(pad_to("hi", 5), "hi   ");
        assert_eq!(pad_to("hello", 3), "hello");
        assert_eq!(pad_to("\x1b[32mhi\x1b[0m", 5), "\x1b[32mhi\x1b[0m   ");
    }

    #[test]
    fn test_term_width() {
        let w = term_width();
        assert!(w > 0);
    }
}
