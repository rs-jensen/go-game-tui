mod ai;
mod app;
mod game;
mod gtp;
mod ui;

use std::io;
use std::time::Duration;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use app::{App, EngineKind, GameMode, Screen};
use gtp::gnugo_available;

fn main() -> io::Result<()> {
    let gnugo = gnugo_available();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, gnugo);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, gnugo: bool) -> io::Result<()> {
    let mut app = App::new(gnugo);
    if !gnugo {
        app.setup.engine = EngineKind::BuiltIn;
    }

    loop {
        terminal.draw(|f| ui::render(f, &app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if handle_input(&mut app, key.code, key.modifiers) {
                    break;
                }
            }
        } else {
            app.tick();
        }
    }
    Ok(())
}

fn handle_input(app: &mut App, key: KeyCode, mods: KeyModifiers) -> bool {
    if mods.contains(KeyModifiers::CONTROL) && key == KeyCode::Char('c') {
        return true;
    }
    match app.screen {
        Screen::Menu => menu(app, key),
        Screen::Setup => setup(app, key),
        Screen::Playing => playing(app, key),
        Screen::Review => review(app, key),
        Screen::GameOver => game_over(app, key),
    }
}

// ─── Menu ─────────────────────────────────────────────────────────────────────

fn menu(app: &mut App, key: KeyCode) -> bool {
    match key {
        KeyCode::Up | KeyCode::Char('k') => { if app.menu_selected > 0 { app.menu_selected -= 1; } }
        KeyCode::Down | KeyCode::Char('j') => { if app.menu_selected < 1 { app.menu_selected += 1; } }
        KeyCode::Enter => {
            if app.menu_selected == 0 { app.screen = Screen::Setup; } else { return true; }
        }
        KeyCode::Char('q') => return true,
        _ => {}
    }
    false
}

// ─── Setup ────────────────────────────────────────────────────────────────────

const SETUP_ROWS: usize = 9; // rows 0-8

fn setup(app: &mut App, key: KeyCode) -> bool {
    let s = &mut app.setup;
    let vs_ai = s.game_mode == GameMode::VsAI;

    match key {
        KeyCode::Up | KeyCode::Char('k') => { if s.selected > 0 { s.selected -= 1; } }
        KeyCode::Down | KeyCode::Char('j') => { if s.selected < SETUP_ROWS - 1 { s.selected += 1; } }
        KeyCode::Left | KeyCode::Char('h') => change_setup(s, false, vs_ai),
        KeyCode::Right | KeyCode::Char('l') => change_setup(s, true, vs_ai),
        KeyCode::Enter => app.start_game(),
        KeyCode::Esc => app.screen = Screen::Menu,
        KeyCode::Char('q') => return true,
        _ => {}
    }
    false
}

fn change_setup(s: &mut app::Setup, forward: bool, vs_ai: bool) {
    match s.selected {
        0 => s.board_size = cycle_board_size(s.board_size, forward),
        1 => s.game_mode = if forward {
            match s.game_mode { GameMode::VsAI => GameMode::TwoPlayer, _ => GameMode::VsAI }
        } else {
            match s.game_mode { GameMode::TwoPlayer => GameMode::VsAI, _ => GameMode::TwoPlayer }
        },
        2 if vs_ai => s.engine = match s.engine { EngineKind::BuiltIn => EngineKind::GnuGo, _ => EngineKind::BuiltIn },
        3 if vs_ai => {
            if forward { if s.engine_level < 10 { s.engine_level += 1; } }
            else       { if s.engine_level > 1  { s.engine_level -= 1; } }
        }
        4 => s.human_color = s.human_color.opponent(),
        5 if vs_ai => s.hints = if forward { s.hints.next() } else { s.hints.prev() },
        6 => s.undo_enabled = !s.undo_enabled,
        7 => s.handicap = cycle_handicap(s.handicap, forward),
        8 => s.time_limit = if forward { next_time(s.time_limit) } else { prev_time(s.time_limit) },
        _ => {}
    }
}

// ─── Playing ──────────────────────────────────────────────────────────────────

fn playing(app: &mut App, key: KeyCode) -> bool {
    let sess = match &mut app.session { Some(s) => s, None => return false };

    // Always allow quit
    if key == KeyCode::Char('q') { return true; }

    // Block most keys while AI computes
    if sess.ai_thinking {
        return false;
    }

    if sess.game.game_over {
        // Redirect to game over screen on any key
        app.screen = Screen::GameOver;
        return false;
    }

    let size = sess.game.size();
    let mut ended = false;

    match key {
        // Cursor movement
        KeyCode::Up    | KeyCode::Char('k') => { if sess.cursor.0 + 1 < size { sess.cursor.0 += 1; } sess.hint_move = None; }
        KeyCode::Down  | KeyCode::Char('j') => { if sess.cursor.0 > 0        { sess.cursor.0 -= 1; } sess.hint_move = None; }
        KeyCode::Left  | KeyCode::Char('h') => { if sess.cursor.1 > 0        { sess.cursor.1 -= 1; } sess.hint_move = None; }
        KeyCode::Right | KeyCode::Char('l') => { if sess.cursor.1 + 1 < size { sess.cursor.1 += 1; } sess.hint_move = None; }
        // Place stone
        KeyCode::Enter | KeyCode::Char(' ') => {
            sess.place_stone();
            ended = sess.game.game_over;
        }
        // Pass
        KeyCode::Char('p') => {
            sess.pass_turn();
            ended = sess.game.game_over;
        }
        // Resign
        KeyCode::Char('r') => { sess.resign(); ended = true; }
        // Hint
        KeyCode::Char('?') => sess.request_hint(),
        // Territory overlay
        KeyCode::Char('t') => sess.toggle_territory(),
        // Undo
        KeyCode::Char('u') => sess.undo(),
        // Save SGF
        KeyCode::Char('s') => {
            match sess.save_sgf() {
                Ok(f) => sess.status = format!("Saved {}", f),
                Err(e) => sess.status = format!("Save error: {}", e),
            }
        }
        _ => {}
    }

    if ended { app.screen = Screen::GameOver; }
    false
}

// ─── Review ───────────────────────────────────────────────────────────────────

fn review(app: &mut App, key: KeyCode) -> bool {
    let rv = match &mut app.review { Some(r) => r, None => return false };
    match key {
        KeyCode::Right | KeyCode::Char('l') => rv.step_forward(),
        KeyCode::Left  | KeyCode::Char('h') => rv.step_back(),
        KeyCode::Char('0') => rv.pos = 0,
        KeyCode::Char('$') => { let t = rv.total(); rv.pos = t; }
        KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::GameOver,
        _ => {}
    }
    false
}

// ─── Game over ────────────────────────────────────────────────────────────────

fn game_over(app: &mut App, key: KeyCode) -> bool {
    match key {
        KeyCode::Char('v') => app.enter_review(),
        KeyCode::Char('s') => {
            if let Some(ref sess) = app.session {
                let _ = sess.save_sgf();
            }
        }
        KeyCode::Char('n') => { app.session = None; app.screen = Screen::Setup; }
        KeyCode::Char('q') | KeyCode::Esc => return true,
        _ => {}
    }
    false
}

// ─── Option cycling helpers ───────────────────────────────────────────────────

fn cycle_board_size(cur: usize, forward: bool) -> usize {
    let opts = [9usize, 13, 19];
    let pos = opts.iter().position(|&x| x == cur).unwrap_or(2);
    if forward { opts[(pos + 1) % 3] } else { opts[(pos + 2) % 3] }
}

const HANDICAP_OPTIONS: [u8; 7] = [0, 2, 3, 4, 5, 6, 9];

fn cycle_handicap(cur: u8, forward: bool) -> u8 {
    let pos = HANDICAP_OPTIONS.iter().position(|&x| x == cur).unwrap_or(0);
    let n = HANDICAP_OPTIONS.len();
    if forward { HANDICAP_OPTIONS[(pos + 1) % n] } else { HANDICAP_OPTIONS[(pos + n - 1) % n] }
}

const TIME_OPTIONS: [Option<u64>; 5] = [None, Some(180), Some(300), Some(600), Some(1200)];

fn next_time(cur: Option<u64>) -> Option<u64> {
    let pos = TIME_OPTIONS.iter().position(|&t| t == cur).unwrap_or(0);
    TIME_OPTIONS[(pos + 1) % TIME_OPTIONS.len()]
}
fn prev_time(cur: Option<u64>) -> Option<u64> {
    let pos = TIME_OPTIONS.iter().position(|&t| t == cur).unwrap_or(0);
    TIME_OPTIONS[(pos + TIME_OPTIONS.len() - 1) % TIME_OPTIONS.len()]
}
