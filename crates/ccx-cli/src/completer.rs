use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper, Result};

use crate::commands;

/// Autocomplete helper that knows about built-in commands AND discovered skills.
pub struct CcxCompleter {
    /// Discovered skill commands: (slash_name, description).
    skill_commands: Vec<(String, String)>,
}

impl CcxCompleter {
    pub fn new() -> Self {
        let skills = ccx_skill::discover_all_skills();
        let skill_commands: Vec<(String, String)> = skills
            .into_iter()
            .filter(|s| {
                // Skip skills whose name collides with a built-in command.
                let slash = format!("/{}", s.name);
                !commands::COMMANDS.iter().any(|c| c.name == slash)
            })
            .map(|s| {
                let desc = if s.description.len() > 60 {
                    format!("{}...", &s.description[..57])
                } else if s.description.is_empty() {
                    s.name.clone()
                } else {
                    s.description
                };
                (format!("/{}", s.name), desc)
            })
            .collect();

        Self { skill_commands }
    }
}

impl Completer for CcxCompleter {
    type Candidate = Pair;

    fn complete(&self, line: &str, _pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        if !line.starts_with('/') {
            return Ok((0, vec![]));
        }

        // Built-in commands
        let mut matches: Vec<Pair> = commands::COMMANDS
            .iter()
            .filter(|cmd| cmd.name.starts_with(line))
            .map(|cmd| Pair {
                display: format!("{} — {}", cmd.name, cmd.description),
                replacement: format!("{} ", cmd.name),
            })
            .collect();

        // Discovered skills
        for (name, desc) in &self.skill_commands {
            if name.starts_with(line) {
                matches.push(Pair {
                    display: format!("{name} — {desc}"),
                    replacement: format!("{name} "),
                });
            }
        }

        Ok((0, matches))
    }
}

impl Hinter for CcxCompleter {
    type Hint = String;

    fn hint(&self, line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        if !line.starts_with('/') || line.len() <= 1 {
            return None;
        }

        // Check built-in commands first.
        for cmd in commands::COMMANDS {
            if cmd.name.starts_with(line) && cmd.name.len() > line.len() {
                return Some(cmd.name[line.len()..].to_string());
            }
        }

        // Then check discovered skills.
        for (name, _) in &self.skill_commands {
            if name.starts_with(line) && name.len() > line.len() {
                return Some(name[line.len()..].to_string());
            }
        }

        None
    }
}

impl Highlighter for CcxCompleter {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> std::borrow::Cow<'b, str> {
        std::borrow::Cow::Owned(format!("\x1b[38;2;204;120;80;1m{}\x1b[0m", prompt))
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        std::borrow::Cow::Owned(format!("\x1b[90m{}\x1b[0m", hint))
    }
}

impl Validator for CcxCompleter {}
impl Helper for CcxCompleter {}
