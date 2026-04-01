//! Inline terminal rendering — prints styled text directly to stdout.
//!
//! Matches Claude Code's inline rendering: welcome panel, styled chat messages,
//! tool execution display, and prompt — all scrolling naturally.

use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;

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
pub fn render_welcome(model: &str, auth_source: &str, cwd: &str, tools: usize, email: Option<&str>) {
    let width = term_width().min(80);

    // Narrow terminal fallback.
    if width < 50 {
        println!("{ACCENT_BOLD}CCX-RS v{}{RESET}", env!("CARGO_PKG_VERSION"));
        println!("{GREEN}{model}{RESET} · {DIM}{auth_source}{RESET}");
        if let Some(email) = email {
            println!("{DIM}{email}{RESET}");
        }
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
    let mut left_lines: Vec<String> = vec![
        String::new(),
        format!("  {BOLD}Welcome back!{RESET}"),
        String::new(),
        pet[0].to_string(),
        pet[1].to_string(),
        pet[2].to_string(),
        pet[3].to_string(),
        String::new(),
        format!("  {GREEN}{model}{RESET} · {DIM}{auth_source}{RESET}"),
    ];
    if let Some(email) = email {
        left_lines.push(format!("  {DIM}{email}{RESET}"));
    }
    left_lines.push(format!("  {DIM}{cwd}{RESET}"));
    left_lines.push(format!("  {DIM}Tools: {tools}{RESET}"));
    left_lines.push(String::new());

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

/// Print tool output/result with collapsible long output.
/// Shows first 3 lines + "[+N more lines]" when output exceeds 5 lines.
/// Uses `✗` marker for failed tools.
pub fn render_tool_end(success: bool, preview: &str) {
    if preview.is_empty() {
        if success {
            println!("  {DIM}└ done{RESET}");
        } else {
            println!("  {RED}✗ error{RESET}");
        }
        return;
    }

    let color = if success { DIM } else { RED };
    let prefix = if success { "└" } else { "✗" };
    let lines: Vec<&str> = preview.lines().collect();
    let total = lines.len();
    let max_visible = 3;

    let display_lines = if total > 5 { &lines[..max_visible] } else { &lines[..] };

    for (i, line) in display_lines.iter().enumerate() {
        let display = if line.chars().count() > 120 {
            let truncated: String = line.chars().take(117).collect();
            format!("{truncated}...")
        } else {
            line.to_string()
        };
        let marker = if i == 0 { prefix } else { "└" };
        println!("  {color}{marker} {display}{RESET}");
    }

    if total > 5 {
        let remaining = total - max_visible;
        println!("  {DIM}  [+{remaining} more lines]{RESET}");
    }
}

/// Print streaming assistant text (no trailing newline).
pub fn render_text(text: &str) {
    print!("{text}");
    io::stdout().flush().unwrap();
}

/// Clear the previous line (moves cursor up and erases).
/// Used after rustyline echo to replace with styled user message.
pub fn clear_previous_line() {
    print!("\x1b[A\x1b[2K\r");
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
    render_footer_line(model);
}

/// Print just the footer info line (used after welcome panel and at session end).
pub fn render_footer_line(model: &str) {
    let width = term_width().min(80);
    let left = "? for shortcuts";
    let right = format!("● {model} · /effort");
    let gap = width.saturating_sub(left.len() + right.len());
    println!(
        "{DIM}{left}{RESET}{}{GREEN}●{RESET} {DIM}{model} · /effort{RESET}",
        " ".repeat(gap)
    );
}

/// Print the "Working..." spinner indicator.
pub fn render_spinner() {
    print!("{GREEN}●{RESET} Working...");
    io::stdout().flush().unwrap();
}

/// Clear the spinner line (carriage return + clear to end).
pub fn clear_spinner() {
    print!("\r\x1b[K");
    io::stdout().flush().unwrap();
}

/// Result of a permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionChoice {
    Allow,
    Deny,
    AlwaysAllow,
}

/// Show a permission prompt for tool execution and read a single keypress.
pub fn prompt_permission(tool_name: &str, detail: &str) -> PermissionChoice {
    // Print prompt (before raw mode so newlines work normally).
    if detail.is_empty() {
        println!("  {ACCENT}⚙{RESET} {BOLD}{tool_name}{RESET}");
    } else {
        println!("  {ACCENT}⚙{RESET} {BOLD}{tool_name}{RESET}: {detail}");
    }
    print!("  Allow? [{GREEN}y{RESET}]es / [{RED}n{RESET}]o / [{ACCENT}a{RESET}]lways > ");
    io::stdout().flush().unwrap();

    // Enable raw mode to read single keypress.
    terminal::enable_raw_mode().ok();
    let choice = loop {
        if let Ok(Event::Key(key)) = event::read() {
            match key.code {
                KeyCode::Char('y') | KeyCode::Enter => break PermissionChoice::Allow,
                KeyCode::Char('n') => break PermissionChoice::Deny,
                KeyCode::Char('a') => break PermissionChoice::AlwaysAllow,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break PermissionChoice::Deny;
                }
                _ => continue,
            }
        }
    };
    terminal::disable_raw_mode().ok();

    // Print result (after raw mode is disabled).
    println!();
    match choice {
        PermissionChoice::Allow => println!("  {GREEN}✓{RESET} Allowed"),
        PermissionChoice::AlwaysAllow => println!("  {GREEN}✓{RESET} Always allowed"),
        PermissionChoice::Deny => println!("  {RED}✗{RESET} Denied"),
    }

    choice
}

/// Render markdown-formatted text with ANSI styling.
pub fn render_markdown(text: &str) {
    let mut in_code_block = false;

    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block {
                println!("  {DIM}┌────────────────────────────{RESET}");
            } else {
                println!("  {DIM}└────────────────────────────{RESET}");
            }
            continue;
        }

        if in_code_block {
            println!("  {DIM}│{RESET} {ACCENT}{line}{RESET}");
            continue;
        }

        let trimmed = line.trim();

        // Headings.
        if let Some(heading) = trimmed.strip_prefix("### ") {
            println!("\n{BOLD}{heading}{RESET}");
            continue;
        }
        if let Some(heading) = trimmed.strip_prefix("## ") {
            println!("\n{ACCENT_BOLD}{heading}{RESET}");
            continue;
        }
        if let Some(heading) = trimmed.strip_prefix("# ") {
            println!("\n{ACCENT_BOLD}{heading}{RESET}");
            println!("{DIM}{}{RESET}", "─".repeat(heading.len().min(40)));
            continue;
        }

        // Unordered lists.
        if let Some(item) = trimmed.strip_prefix("- ") {
            println!("  • {}", render_inline_md(item));
            continue;
        }
        if let Some(item) = trimmed.strip_prefix("* ") {
            println!("  • {}", render_inline_md(item));
            continue;
        }

        // Numbered lists.
        if let Some(dot_pos) = trimmed.find(". ") {
            if dot_pos <= 3 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
                let num = &trimmed[..dot_pos];
                let item = &trimmed[dot_pos + 2..];
                println!("  {DIM}{num}.{RESET} {}", render_inline_md(item));
                continue;
            }
        }

        // Regular line.
        if trimmed.is_empty() {
            println!();
        } else {
            println!("{}", render_inline_md(line));
        }
    }
    println!();
}

/// Render inline markdown formatting: **bold**, `code`, [links](url).
fn render_inline_md(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Inline code: `code`
        if chars[i] == '`' {
            let start = i + 1;
            let mut end = start;
            while end < len && chars[end] != '`' {
                end += 1;
            }
            if end < len {
                let code: String = chars[start..end].iter().collect();
                result.push_str(ACCENT);
                result.push_str(&code);
                result.push_str(RESET);
                i = end + 1;
                continue;
            }
        }

        // Bold: **text**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            let start = i + 2;
            let mut end = start;
            while end + 1 < len && !(chars[end] == '*' && chars[end + 1] == '*') {
                end += 1;
            }
            if end + 1 < len {
                let bold: String = chars[start..end].iter().collect();
                result.push_str(BOLD);
                result.push_str(&bold);
                result.push_str(RESET);
                i = end + 2;
                continue;
            }
        }

        // Link: [text](url)
        if chars[i] == '[' {
            let text_start = i + 1;
            let mut text_end = text_start;
            while text_end < len && chars[text_end] != ']' {
                text_end += 1;
            }
            if text_end + 1 < len && chars[text_end + 1] == '(' {
                let url_start = text_end + 2;
                let mut url_end = url_start;
                while url_end < len && chars[url_end] != ')' {
                    url_end += 1;
                }
                if url_end < len {
                    let link_text: String = chars[text_start..text_end].iter().collect();
                    let url: String = chars[url_start..url_end].iter().collect();
                    result.push_str(BOLD);
                    result.push_str(&link_text);
                    result.push_str(RESET);
                    result.push_str(" ");
                    result.push_str(DIM);
                    result.push_str("(");
                    result.push_str(&url);
                    result.push_str(")");
                    result.push_str(RESET);
                    i = url_end + 1;
                    continue;
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
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
