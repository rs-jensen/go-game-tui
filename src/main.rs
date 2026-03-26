mod game;
mod ai;
mod app;
mod ui;

use std::io;
use std::time::Duration;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use app::{App, Screen};
use ai::Difficulty;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();

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
        Screen::Menu => handle_menu(app, key),
        Screen::Setup => handle_setup(app, key),
        Screen::Playing => handle_game(app, key),
        Screen::GameOver => handle_game_over(app, key),
    }
}

fn handle_menu(app: &mut App, key: KeyCode) -> bool {
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.menu_selected > 0 { app.menu_selected -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.menu_selected < 1 { app.menu_selected += 1; }
        }
        KeyCode::Enter => {
            if app.menu_selected == 0 {
                app.screen = Screen::Setup;
            } else {
                return true;
            }
        }
        KeyCode::Char('q') => return true,
        _ => {}
    }
    false
}

fn handle_setup(app: &mut App, key: KeyCode) -> bool {
    let s = &mut app.setup;
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            if s.selected > 0 { s.selected -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if s.selected < 3 { s.selected += 1; }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            match s.selected {
                0 => s.board_size = prev_board_size(s.board_size),
                1 => s.difficulty = prev_difficulty(s.difficulty),
                2 => s.human_color = s.human_color.opponent(),
                3 => s.time_limit = prev_time(s.time_limit),
                _ => {}
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            match s.selected {
                0 => s.board_size = next_board_size(s.board_size),
                1 => s.difficulty = next_difficulty(s.difficulty),
                2 => s.human_color = s.human_color.opponent(),
                3 => s.time_limit = next_time(s.time_limit),
                _ => {}
            }
        }
        KeyCode::Enter => app.start_game(),
        KeyCode::Esc => app.screen = Screen::Menu,
        KeyCode::Char('q') => return true,
        _ => {}
    }
    false
}

fn handle_game(app: &mut App, key: KeyCode) -> bool {
    let sess = match &mut app.session {
        Some(s) => s,
        None => return false,
    };

    if sess.game.game_over {
        if key == KeyCode::Char('q') { return true; }
        return false;
    }

    // Block input when it's the AI's turn
    if sess.game.current != sess.human {
        return false;
    }

    let size = sess.game.size();
    let mut game_ended = false;

    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            if sess.cursor.0 + 1 < size { sess.cursor.0 += 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if sess.cursor.0 > 0 { sess.cursor.0 -= 1; }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if sess.cursor.1 > 0 { sess.cursor.1 -= 1; }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if sess.cursor.1 + 1 < size { sess.cursor.1 += 1; }
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            sess.place_stone();
            game_ended = sess.game.game_over;
        }
        KeyCode::Char('p') => {
            sess.pass_turn();
            game_ended = sess.game.game_over;
        }
        KeyCode::Char('r') => {
            sess.resign();
            game_ended = true;
        }
        KeyCode::Char('q') => return true,
        _ => {}
    }

    if game_ended {
        app.screen = Screen::GameOver;
    }
    false
}

fn handle_game_over(app: &mut App, key: KeyCode) -> bool {
    match key {
        KeyCode::Char('n') => {
            app.screen = Screen::Setup;
            app.session = None;
        }
        KeyCode::Char('q') | KeyCode::Esc => return true,
        _ => {}
    }
    false
}

fn next_board_size(cur: usize) -> usize {
    match cur { 9 => 13, 13 => 19, _ => 9 }
}
fn prev_board_size(cur: usize) -> usize {
    match cur { 19 => 13, 13 => 9, _ => 19 }
}

fn next_difficulty(cur: Difficulty) -> Difficulty {
    match cur {
        Difficulty::Easy => Difficulty::Medium,
        Difficulty::Medium => Difficulty::Hard,
        Difficulty::Hard => Difficulty::Easy,
    }
}
fn prev_difficulty(cur: Difficulty) -> Difficulty {
    match cur {
        Difficulty::Easy => Difficulty::Hard,
        Difficulty::Medium => Difficulty::Easy,
        Difficulty::Hard => Difficulty::Medium,
    }
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
