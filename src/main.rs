use std::io::{self, Stdout};
use std::panic;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use trackfolio::store;
use trackfolio::ui::{self, App};

fn main() {
    let Some(path) = store::data_path() else {
        eprintln!("trackfolio: unable to determine data path");
        std::process::exit(1);
    };
    let book = match store::load(&path) {
        Ok(book) => book,
        Err(msg) => {
            eprintln!("trackfolio: {msg}");
            std::process::exit(1);
        }
    };
    let mut app = App::new(book);
    match trackfolio::fx::eur_board() {
        Ok(board) => app.fx_board = Some(board),
        Err(_) => app.message = Some("FX unavailable".to_string()),
    }

    let mut terminal = match setup_terminal() {
        Ok(terminal) => terminal,
        Err(e) => {
            eprintln!("trackfolio: cannot initialize terminal: {e}");
            std::process::exit(1);
        }
    };
    install_terminal_guard();

    let result = run_loop(&mut terminal, &mut app, &path);

    if let Err(e) = restore_terminal(&mut terminal) {
        eprintln!("trackfolio: cannot restore terminal: {e}");
    }

    if let Err(msg) = result {
        eprintln!("trackfolio: {msg}");
        std::process::exit(1);
    }
}

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> io::Result<Tui> {
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Tui) -> io::Result<()> {
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()
}

fn install_terminal_guard() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        original_hook(info);
    }));
}

fn run_loop(terminal: &mut Tui, app: &mut App, path: &PathBuf) -> Result<(), String> {
    loop {
        terminal
            .draw(|frame| ui::draw(frame, app))
            .map_err(|e| format!("draw failed: {e}"))?;

        if crossterm::event::poll(Duration::from_millis(250))
            .map_err(|e| format!("event poll failed: {e}"))?
        {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()
                .map_err(|e| format!("event read failed: {e}"))?
            {
                if key.kind == crossterm::event::KeyEventKind::Press && ui::handle_key(app, key) {
                    break;
                }
            }
        }

        if app.dirty {
            store::save(path, &app.book)?;
            app.dirty = false;
        }
    }
    Ok(())
}
