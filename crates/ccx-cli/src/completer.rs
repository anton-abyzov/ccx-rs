use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper, Result};

use crate::commands;

pub struct CcxCompleter;

impl Completer for CcxCompleter {
    type Candidate = Pair;

    fn complete(&self, line: &str, _pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        if !line.starts_with('/') {
            return Ok((0, vec![]));
        }

        let matches: Vec<Pair> = commands::COMMANDS
            .iter()
            .filter(|cmd| cmd.name.starts_with(line))
            .map(|cmd| Pair {
                display: format!("{} — {}", cmd.name, cmd.description),
                replacement: format!("{} ", cmd.name),
            })
            .collect();

        Ok((0, matches))
    }
}

impl Hinter for CcxCompleter {
    type Hint = String;

    fn hint(&self, line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        if line.starts_with('/') && line.len() > 1 {
            for cmd in commands::COMMANDS {
                if cmd.name.starts_with(line) && cmd.name.len() > line.len() {
                    return Some(cmd.name[line.len()..].to_string());
                }
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
