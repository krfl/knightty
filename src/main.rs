use std::io::{self, BufWriter};

use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{
    EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

mod app;
mod audio;
mod fx;
mod game_loop;
mod input;
mod physics;
mod render;
mod ui;
mod world;

#[cfg(test)]
mod render_tests;

/// Output buffer size. A full repaint of a big terminal at a small font is
/// a few hundred kilobytes of escape codes, so this keeps it to a few writes.
const STDOUT_BUFFER: usize = 256 * 1024;

fn main() -> io::Result<()> {
    install_panic_hook();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let keyboard_enhanced = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
    )
    .is_ok();

    let backend = CrosstermBackend::new(BufWriter::with_capacity(STDOUT_BUFFER, stdout));
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = app::App::new();

    let result = game_loop::run(&mut terminal, &mut app);

    // Run every restore step even if an earlier one fails, then report the
    // first error.
    let raw = disable_raw_mode();
    if keyboard_enhanced {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    let leave = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let cursor = terminal.show_cursor();

    result.and(raw).and(leave).and(cursor)
}

/// Restore the terminal on panic so the user isn't left in alt-screen + raw
/// mode if anything blows up mid-frame (kitty font-size events, rodio glitches,
/// out-of-bounds in a renderer, etc.). Without this hook, a panic kills our
/// cleanup path and the terminal looks frozen.
fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            EndSynchronizedUpdate,
            PopKeyboardEnhancementFlags,
            LeaveAlternateScreen,
            Show,
        );
        original(info);
    }));
}
