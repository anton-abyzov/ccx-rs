pub mod app;
pub mod chat;
pub mod inline;
pub mod input;
pub mod style;
pub mod welcome;

pub use app::{App, Screen, render};
pub use chat::{ChatMessage, ChatRole};
pub use input::InputState;
pub use welcome::WelcomeInfo;

use std::io;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// Events that can be pushed into the TUI from the agent loop.
#[derive(Debug, Clone)]
pub enum TuiEvent {
    /// A new chat message to display.
    NewMessage(ChatMessage),
    /// Streaming text to append to the current assistant message.
    StreamText(String),
    /// A tool started executing.
    ToolStart { name: String, detail: String },
    /// A tool finished executing.
    ToolEnd {
        name: String,
        success: bool,
        preview: String,
    },
    /// Update the status bar.
    SetStatus(String),
    /// Signal the TUI to shut down.
    Quit,
}

/// Result from the TUI event loop — either user input or quit.
#[derive(Debug)]
pub enum TuiInput {
    /// User submitted a message.
    Message(String),
    /// User requested to quit.
    Quit,
}

/// Run the TUI event loop with default welcome info.
pub fn run_tui(
    events_rx: mpsc::Receiver<TuiEvent>,
    input_tx: mpsc::Sender<TuiInput>,
) -> io::Result<()> {
    run_tui_configured(events_rx, input_tx, WelcomeInfo::default())
}

/// Run the TUI event loop with custom welcome info. Blocks the calling thread.
///
/// - `events_rx`: Receives events from the agent (messages, tool status, etc.)
/// - `input_tx`: Sends user input back to the agent loop
/// - `welcome`: Configuration for the welcome screen
pub fn run_tui_configured(
    events_rx: mpsc::Receiver<TuiEvent>,
    input_tx: mpsc::Sender<TuiInput>,
    welcome: WelcomeInfo,
) -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = App::new();
    app_state.welcome = welcome;

    let result = run_event_loop(&mut terminal, &mut app_state, &events_rx, &input_tx);

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    events_rx: &mpsc::Receiver<TuiEvent>,
    input_tx: &mpsc::Sender<TuiInput>,
) -> io::Result<()> {
    loop {
        // Render current state.
        terminal.draw(|f| render(f, app))?;

        // Process all pending TUI events (non-blocking).
        while let Ok(event) = events_rx.try_recv() {
            match event {
                TuiEvent::NewMessage(msg) => {
                    // Transition to chat on first message.
                    if app.screen == Screen::Welcome {
                        app.enter_chat();
                    }
                    app.messages.push(msg);
                    auto_scroll(app);
                }
                TuiEvent::StreamText(text) => {
                    if app.screen == Screen::Welcome {
                        app.enter_chat();
                    }
                    // Append to the last assistant message, or create one.
                    if let Some(last) = app.messages.last_mut()
                        && last.role == ChatRole::Assistant
                    {
                        last.content.push_str(&text);
                        auto_scroll(app);
                        continue;
                    }
                    app.messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: text,
                    });
                    auto_scroll(app);
                }
                TuiEvent::ToolStart { name, detail } => {
                    let msg = if detail.is_empty() {
                        format!("[{name}]")
                    } else {
                        format!("[{name}: {detail}]")
                    };
                    app.messages.push(ChatMessage {
                        role: ChatRole::Tool,
                        content: msg,
                    });
                    auto_scroll(app);
                }
                TuiEvent::ToolEnd {
                    name,
                    success,
                    preview,
                } => {
                    let status_str = if success { "ok" } else { "error" };
                    let msg = if preview.is_empty() {
                        format!("[{name}: {status_str}]")
                    } else {
                        format!("[{name}: {status_str}] {preview}")
                    };
                    app.messages.push(ChatMessage {
                        role: if success {
                            ChatRole::Tool
                        } else {
                            ChatRole::Error
                        },
                        content: msg,
                    });
                    auto_scroll(app);
                }
                TuiEvent::SetStatus(s) => {
                    // Status is shown via footer now; could extend later.
                    let _ = s;
                }
                TuiEvent::Quit => {
                    app.should_quit = true;
                }
            }
        }

        if app.should_quit {
            let _ = input_tx.send(TuiInput::Quit);
            break;
        }

        // Poll for keyboard events with short timeout for responsive UI.
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                // Ctrl+C: quit.
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let _ = input_tx.send(TuiInput::Quit);
                    app.should_quit = true;
                }

                // Ctrl+D: quit if input is empty.
                KeyCode::Char('d')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && app.input.text.is_empty() =>
                {
                    let _ = input_tx.send(TuiInput::Quit);
                    app.should_quit = true;
                }

                // Enter: submit input.
                KeyCode::Enter => {
                    let text = app.input.clear();
                    if !text.is_empty() {
                        // Transition to chat on first user input.
                        if app.screen == Screen::Welcome {
                            app.enter_chat();
                        }
                        app.messages.push(ChatMessage {
                            role: ChatRole::User,
                            content: text.clone(),
                        });
                        auto_scroll(app);
                        let _ = input_tx.send(TuiInput::Message(text));
                    }
                }

                // Backspace.
                KeyCode::Backspace => {
                    app.input.backspace();
                }

                // Delete: remove char at cursor.
                KeyCode::Delete => {
                    if app.input.cursor_pos < app.input.text.len() {
                        app.input.text.remove(app.input.cursor_pos);
                    }
                }

                // Arrow keys.
                KeyCode::Left => app.input.move_left(),
                KeyCode::Right => app.input.move_right(),
                KeyCode::Up => app.scroll_up(),
                KeyCode::Down => app.scroll_down(),

                // Home/End for input cursor.
                KeyCode::Home => app.input.cursor_pos = 0,
                KeyCode::End => app.input.cursor_pos = app.input.text.len(),

                // PageUp/PageDown for scrolling.
                KeyCode::PageUp => {
                    for _ in 0..10 {
                        app.scroll_up();
                    }
                }
                KeyCode::PageDown => {
                    for _ in 0..10 {
                        app.scroll_down();
                    }
                }

                // Ctrl+L: clear screen (re-render).
                KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    terminal.clear()?;
                }

                // Ctrl+U: clear input line.
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.input.clear();
                }

                // Regular character input.
                KeyCode::Char(c) => {
                    app.input.insert(c);
                }

                // Tab: could be used for completion in the future.
                KeyCode::Tab => {}

                _ => {}
            }
        }
    }

    Ok(())
}

/// Auto-scroll to bottom when new content is added.
fn auto_scroll(app: &mut App) {
    let total_lines: u16 = app
        .messages
        .iter()
        .map(|m| m.content.lines().count().max(1) as u16 + 1) // +1 for separator
        .sum();
    if total_lines > 10 {
        app.scroll = total_lines.saturating_sub(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_event_variants() {
        let _msg = TuiEvent::NewMessage(ChatMessage {
            role: ChatRole::User,
            content: "hello".into(),
        });
        let _stream = TuiEvent::StreamText("chunk".into());
        let _tool_start = TuiEvent::ToolStart {
            name: "Bash".into(),
            detail: "echo hi".into(),
        };
        let _tool_end = TuiEvent::ToolEnd {
            name: "Bash".into(),
            success: true,
            preview: String::new(),
        };
        let _status = TuiEvent::SetStatus("Ready".into());
        let _quit = TuiEvent::Quit;
    }

    #[test]
    fn test_tui_input_variants() {
        let _msg = TuiInput::Message("hello".into());
        let _quit = TuiInput::Quit;
    }

    #[test]
    fn test_auto_scroll() {
        let mut app = App::new();
        for i in 0..20 {
            app.messages.push(ChatMessage {
                role: ChatRole::Assistant,
                content: format!("Message {i} with some content"),
            });
        }
        auto_scroll(&mut app);
        assert!(app.scroll > 0, "Should have scrolled down");
    }
}
