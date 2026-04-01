/// Slash command definitions for the inline interactive mode.

/// A slash command available in interactive mode.
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
}

/// All available slash commands.
pub const COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "/help",
        description: "Show available commands",
    },
    SlashCommand {
        name: "/exit",
        description: "Quit the session",
    },
    SlashCommand {
        name: "/clear",
        description: "Clear the screen",
    },
    SlashCommand {
        name: "/cost",
        description: "Show token usage and cost",
    },
    SlashCommand {
        name: "/model",
        description: "Show or change model",
    },
    SlashCommand {
        name: "/compact",
        description: "Compress conversation context",
    },
    SlashCommand {
        name: "/init",
        description: "Create CLAUDE.md file",
    },
    SlashCommand {
        name: "/version",
        description: "Show version info",
    },
    SlashCommand {
        name: "/tools",
        description: "List available tools",
    },
];

/// Find commands matching a prefix (e.g., "/he" matches "/help").
pub fn find_matches(prefix: &str) -> Vec<&'static SlashCommand> {
    COMMANDS
        .iter()
        .filter(|c| c.name.starts_with(prefix))
        .collect()
}

/// Print the full command list with ANSI styling.
pub fn print_command_list() {
    const ACCENT: &str = "\x1b[38;2;204;120;80m";
    const DIM: &str = "\x1b[90m";
    const BOLD: &str = "\x1b[1m";
    const RESET: &str = "\x1b[0m";

    println!("\n{BOLD}Available commands:{RESET}\n");
    for cmd in COMMANDS {
        println!(
            "  {ACCENT}{:<12}{RESET} {DIM}— {}{RESET}",
            cmd.name, cmd.description
        );
    }
    println!();
}

/// Print matching commands for partial input (e.g., "/he").
pub fn print_suggestions(prefix: &str) {
    let matches = find_matches(prefix);
    if matches.is_empty() {
        println!("\x1b[31mNo matching commands for '{prefix}'\x1b[0m");
        return;
    }

    const ACCENT: &str = "\x1b[38;2;204;120;80m";
    const DIM: &str = "\x1b[90m";
    const RESET: &str = "\x1b[0m";

    println!();
    for cmd in matches {
        println!(
            "  {ACCENT}{:<12}{RESET} {DIM}— {}{RESET}",
            cmd.name, cmd.description
        );
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_matches_exact() {
        let matches = find_matches("/help");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "/help");
    }

    #[test]
    fn test_find_matches_prefix() {
        let matches = find_matches("/c");
        assert!(matches.len() >= 2); // /clear, /cost, /compact
        assert!(matches.iter().all(|m| m.name.starts_with("/c")));
    }

    #[test]
    fn test_find_matches_slash_only() {
        let matches = find_matches("/");
        assert_eq!(matches.len(), COMMANDS.len());
    }

    #[test]
    fn test_find_matches_no_match() {
        let matches = find_matches("/xyz");
        assert!(matches.is_empty());
    }
}
