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
    SlashCommand {
        name: "/login",
        description: "Authenticate with Claude (opens browser)",
    },
];

/// Find commands matching a prefix (e.g., "/he" matches "/help").
pub fn find_matches(prefix: &str) -> Vec<&'static SlashCommand> {
    COMMANDS
        .iter()
        .filter(|c| c.name.starts_with(prefix))
        .collect()
}

/// Print the full command list with ANSI styling, including discovered skills.
pub fn print_command_list(skills: &[(String, String)]) {
    const ACCENT: &str = "\x1b[38;2;204;120;80m";
    const DIM: &str = "\x1b[90m";
    const BOLD: &str = "\x1b[1m";
    const RESET: &str = "\x1b[0m";
    const SKILL_ACCENT: &str = "\x1b[38;2;130;170;255m";

    println!("\n{BOLD}Available commands:{RESET}\n");
    for cmd in COMMANDS {
        println!(
            "  {ACCENT}{:<12}{RESET} {DIM}— {}{RESET}",
            cmd.name, cmd.description
        );
    }

    if !skills.is_empty() {
        println!("\n{BOLD}Skills ({}):{RESET}\n", skills.len());
        for (name, desc) in skills {
            println!(
                "  {SKILL_ACCENT}{:<24}{RESET} {DIM}— {}{RESET}",
                name, desc
            );
        }
    }
    println!();
}

/// Print matching commands for partial input (e.g., "/he"), including skills.
pub fn print_suggestions(prefix: &str, skills: &[(String, String)]) {
    let builtin_matches = find_matches(prefix);
    let skill_matches: Vec<&(String, String)> = skills
        .iter()
        .filter(|(name, _)| name.starts_with(prefix))
        .collect();

    if builtin_matches.is_empty() && skill_matches.is_empty() {
        println!("\x1b[31mNo matching commands for '{prefix}'\x1b[0m");
        return;
    }

    const ACCENT: &str = "\x1b[38;2;204;120;80m";
    const DIM: &str = "\x1b[90m";
    const RESET: &str = "\x1b[0m";
    const SKILL_ACCENT: &str = "\x1b[38;2;130;170;255m";

    println!();
    for cmd in builtin_matches {
        println!(
            "  {ACCENT}{:<24}{RESET} {DIM}— {}{RESET}",
            cmd.name, cmd.description
        );
    }
    for (name, desc) in skill_matches {
        println!(
            "  {SKILL_ACCENT}{:<24}{RESET} {DIM}— {}{RESET}",
            name, desc
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
