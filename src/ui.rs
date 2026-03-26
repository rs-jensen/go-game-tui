use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use crate::app::{App, EngineKind, GameMode, Screen, color_name};
use crate::game::Stone;
use crate::gtp::to_gtp;

const COLS: &str = "ABCDEFGHJKLMNOPQRST";

pub fn render(f: &mut Frame, app: &App) {
    match app.screen {
        Screen::Menu => render_menu(f, app),
        Screen::Setup => render_setup(f, app),
        Screen::Playing => render_game(f, app),
        Screen::Review => render_review(f, app),
        Screen::GameOver => render_game_over(f, app),
    }
}

// ─── Menu ─────────────────────────────────────────────────────────────────────

fn render_menu(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(
        Block::default()
            .title("  Go TUI  ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Yellow)),
        area,
    );

    let inner = centered_rect(38, 50, area);
    let items = ["New Game", "Quit"];
    let lines: Vec<Line> = items
        .iter()
        .enumerate()
        .map(|(i, label)| {
            if i == app.menu_selected {
                Line::from(Span::styled(
                    format!(" > {}  ", label),
                    Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(format!("   {}  ", label))
            }
        })
        .collect();

    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    hint_bar(f, area, "up/down  enter  q quit");
}

// ─── Setup ────────────────────────────────────────────────────────────────────

fn render_setup(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(
        Block::default()
            .title("  Game Setup  ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan)),
        area,
    );

    let inner = centered_rect(58, 95, area);
    let s = &app.setup;

    let engine_label = if s.engine == EngineKind::GnuGo && !app.gnugo_available {
        "GNU Go  (not installed)".to_string()
    } else {
        s.engine.label().to_string()
    };
    let time_label = match s.time_limit {
        None => "No limit".to_string(),
        Some(secs) => format!("{} min", secs / 60),
    };
    let color_label = if s.human_color == Stone::Black { "Black (first)" } else { "White (second)" };
    let first_label = "Black (first)"; // 2P mode always shows this

    let vs_ai = s.game_mode == GameMode::VsAI;

    // Row definitions: (label, value, active)
    let rows: &[(&str, String, bool)] = &[
        ("Board size",  format!("{}x{}", s.board_size, s.board_size), true),
        ("Game mode",   s.game_mode.label().to_string(),              true),
        ("Engine",      engine_label,                                  vs_ai),
        ("Level",       s.level_label(),                               vs_ai),
        ("Your color",  if vs_ai { color_label } else { first_label }.to_string(), true),
        ("Hints",       s.hints.label().to_string(),                   vs_ai),
        ("Undo",        if s.undo_enabled { "On" } else { "Off" }.to_string(), true),
        ("Handicap",    s.handicap_label(),                            true),
        ("Time limit",  time_label,                                    true),
    ];

    let mut lines: Vec<Line> = vec![Line::from("")];
    for (i, (label, value, active)) in rows.iter().enumerate() {
        let selected = i == s.selected;
        let fg = if !active { Color::DarkGray }
                 else if selected { Color::Black }
                 else { Color::Reset };
        let bg = if selected && *active { Color::Cyan } else { Color::Reset };
        let mut style = Style::default().fg(fg).bg(bg);
        if selected && *active { style = style.add_modifier(Modifier::BOLD); }
        let prefix = if selected { " > " } else { "   " };
        lines.push(Line::from(Span::styled(
            format!("{}{:<14} {}", prefix, label, value),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Enter] Start game",
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
    )));

    f.render_widget(Paragraph::new(lines), inner);
    hint_bar(f, area, "up/down  left/right change  enter start  esc back");
}

// ─── Game ─────────────────────────────────────────────────────────────────────

fn render_game(f: &mut Frame, app: &App) {
    let area = f.area();
    let sess = match &app.session { Some(s) => s, None => return };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(46), Constraint::Length(32)])
        .split(area);

    render_board_sess(f, sess, chunks[0]);
    render_info(f, sess, chunks[1]);
}

fn render_board_sess(f: &mut Frame, sess: &crate::app::Session, area: Rect) {
    render_board_game(
        f,
        &sess.game,
        area,
        Some(sess.cursor),
        sess.last_move,
        sess.hint_move,
        if sess.show_territory { sess.territory.as_ref() } else { None },
    );
}

fn render_board_game(
    f: &mut Frame,
    game: &crate::game::Game,
    area: Rect,
    cursor: Option<(usize, usize)>,
    last_move: Option<(usize, usize)>,
    hint_move: Option<(usize, usize)>,
    territory: Option<&Vec<Vec<Option<Stone>>>>,  // None = off
) {
    f.render_widget(
        Block::default()
            .title("  Board  ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Yellow)),
        area,
    );

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let size = game.size();
    let stars: std::collections::HashSet<_> = game.star_points().into_iter().collect();

    let col_header: String = (0..size)
        .map(|c| format!(" {}", COLS.chars().nth(c).unwrap_or('?')))
        .collect::<Vec<_>>()
        .join("");

    let mut lines: Vec<Line> = vec![Line::from(format!("    {}", col_header))];

    for display_r in 0..size {
        let board_r = size - 1 - display_r;
        let row_num = board_r + 1;
        let mut spans = vec![Span::styled(
            format!("{:2} ", row_num),
            Style::default().fg(Color::DarkGray),
        )];

        for c in 0..size {
            let stone = game.board.get(board_r, c);
            let is_cursor = cursor == Some((board_r, c));
            let is_last = last_move == Some((board_r, c));
            let is_hint = hint_move == Some((board_r, c));
            let is_star = stars.contains(&(board_r, c));

            // Territory background color for empty cells
            let terr_bg = if stone == Stone::Empty {
                territory.and_then(|t| t[board_r][c]).map(|owner| {
                    if owner == Stone::Black { Color::Rgb(0, 50, 0) } else { Color::Rgb(50, 0, 0) }
                })
            } else {
                None
            };

            let (ch, mut style) = if is_hint && stone == Stone::Empty {
                // Hint marker: bright green
                ("?", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                match stone {
                    Stone::Black => (
                        "●",
                        Style::default().fg(Color::Rgb(180, 180, 180)).add_modifier(Modifier::BOLD),
                    ),
                    Stone::White => (
                        "○",
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                    Stone::Empty => {
                        let sym = if is_star { "+" } else { "·" };
                        let mut s = Style::default().fg(Color::Rgb(90, 90, 90));
                        if let Some(bg) = terr_bg { s = s.bg(bg); }
                        (sym, s)
                    }
                }
            };

            if is_last && stone != Stone::Empty {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if is_cursor {
                style = style.bg(Color::Rgb(140, 100, 0)).fg(Color::Black);
            }

            spans.push(Span::styled(format!(" {}", ch), style));
        }

        spans.push(Span::styled(format!(" {:2}", row_num), Style::default().fg(Color::DarkGray)));
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(format!("    {}", col_header)));
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_info(f: &mut Frame, sess: &crate::app::Session, area: Rect) {
    f.render_widget(
        Block::default()
            .title("  Info  ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan)),
        area,
    );

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let game = &sess.game;
    let turn_color = if game.current == Stone::Black { Color::Rgb(180, 180, 180) } else { Color::White };
    let turn_sym = if game.current == Stone::Black { "●" } else { "○" };
    let last_str = match sess.last_move {
        Some((r, c)) => to_gtp(r, c, game.size()),
        None => "-".to_string(),
    };

    let is_2p = sess.game_mode == GameMode::TwoPlayer;

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        header(" TURN"),
        Line::from(Span::styled(
            format!(" {} {}", turn_sym, color_name(game.current)),
            Style::default().fg(turn_color).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(" @ {}     last: {}", sess.cursor_coord(), last_str),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        header(" SCORE EST"),
        Line::from(format!(" {}", sess.score_estimate())),
        Line::from(""),
        header(" CLOCKS"),
        Line::from(format!(" ● Black  {}", sess.format_clock(Stone::Black))),
        Line::from(format!(" ○ White  {}", sess.format_clock(Stone::White))),
        Line::from(""),
        header(" CAPTURES"),
        Line::from(format!(" ● Black  {}", game.captures[0])),
        Line::from(format!(" ○ White  {}", game.captures[1])),
        Line::from(""),
    ];

    if !is_2p {
        lines.push(header(" ENGINE"));
        lines.push(Line::from(format!(" {}", sess.engine_name())));
        lines.push(Line::from(format!(" Move #{}  Hints: {}", game.move_count, sess.hints_label())));
        lines.push(Line::from(""));
    } else {
        lines.push(Line::from(format!(" Move #{}", game.move_count)));
        lines.push(Line::from(""));
    }

    // Move history (last 7)
    if !sess.history.is_empty() {
        lines.push(header(" HISTORY"));
        let start = sess.history.len().saturating_sub(7);
        for mv in &sess.history[start..] {
            let sym = if mv.color == Stone::Black { "●" } else { "○" };
            let coord = match mv.coord {
                Some((r, c)) => to_gtp(r, c, game.size()),
                None => "pass".to_string(),
            };
            lines.push(Line::from(Span::styled(
                format!(" {:3}. {} {}", mv.num, sym, coord),
                Style::default().fg(Color::DarkGray),
            )));
        }
        lines.push(Line::from(""));
    }

    lines.push(header(" KEYS"));
    lines.push(key_line("hjkl/arrows", "move cursor"));
    lines.push(key_line("enter/space", "place stone"));
    lines.push(key_line("p", "pass"));
    if !is_2p { lines.push(key_line("?", "hint")); }
    lines.push(key_line("t", "territory overlay"));
    if sess.undo_enabled { lines.push(key_line("u", "undo")); }
    lines.push(key_line("r", "resign"));
    lines.push(key_line("s", "save SGF"));
    lines.push(key_line("q", "quit"));

    if !sess.status.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(" {}", sess.status),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )));
    }

    if sess.hint_thinking {
        lines.push(Line::from(Span::styled(
            " Hint loading...",
            Style::default().fg(Color::Green),
        )));
    }

    if sess.ai_thinking {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " AI thinking...",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ─── Review ───────────────────────────────────────────────────────────────────

fn render_review(f: &mut Frame, app: &App) {
    let area = f.area();
    let rv = match &app.review { Some(r) => r, None => return };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(46), Constraint::Length(32)])
        .split(area);

    let game = rv.current_board();
    render_board_game(f, game, chunks[0], None, None, None, None);

    // Info panel for review
    f.render_widget(
        Block::default()
            .title("  Review  ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan)),
        chunks[1],
    );

    let inner = Rect {
        x: chunks[1].x + 1,
        y: chunks[1].y + 1,
        width: chunks[1].width.saturating_sub(2),
        height: chunks[1].height.saturating_sub(2),
    };

    let total = rv.total();
    let pos = rv.pos;

    // Current move info
    let move_info = if pos == 0 {
        " Start of game".to_string()
    } else {
        let mv = &rv.history[pos - 1];
        let sym = if mv.color == Stone::Black { "●" } else { "○" };
        let coord = match mv.coord {
            Some((r, c)) => to_gtp(r, c, game.size()),
            None => "pass".to_string(),
        };
        format!(" Move {}: {} {}", mv.num, sym, coord)
    };

    let mut lines = vec![
        Line::from(""),
        header(" REVIEWING"),
        Line::from(Span::styled(
            format!(" Move {}/{}", pos, total),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(move_info, Style::default().fg(Color::Yellow))),
        Line::from(""),
        header(" MOVE LIST"),
    ];

    // Show moves around current position
    let start = pos.saturating_sub(5);
    let end = (start + 12).min(rv.history.len());
    for mv in &rv.history[start..end] {
        let sym = if mv.color == Stone::Black { "●" } else { "○" };
        let coord = match mv.coord {
            Some((r, c)) => to_gtp(r, c, game.size()),
            None => "pass".to_string(),
        };
        let style = if mv.num == pos {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        lines.push(Line::from(Span::styled(
            format!(" {:3}. {} {}", mv.num, sym, coord),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(header(" KEYS"));
    lines.push(key_line("left/h", "previous move"));
    lines.push(key_line("right/l", "next move"));
    lines.push(key_line("0", "go to start"));
    lines.push(key_line("$", "go to end"));
    lines.push(key_line("esc/q", "exit review"));

    f.render_widget(Paragraph::new(lines), inner);
}

// ─── Game Over ────────────────────────────────────────────────────────────────

fn render_game_over(f: &mut Frame, app: &App) {
    let area = f.area();
    let sess = match &app.session { Some(s) => s, None => return };

    render_game(f, app);

    let popup = centered_rect(52, 50, area);
    f.render_widget(Clear, popup);

    let result = sess.result_text();
    let (b, w) = sess.game.score();

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  GAME OVER  ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(" {}", result),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(" Black: {:.1}    White: {:.1}", b, w)),
        Line::from(format!(" komi {:.1} applied to White", sess.game.komi)),
        Line::from(""),
        Line::from(Span::styled(" [v] Review game", Style::default().fg(Color::Cyan))),
        Line::from(Span::styled(" [s] Save SGF", Style::default().fg(Color::Cyan))),
        Line::from(""),
        Line::from(Span::styled(" [n] New game     [q] Quit", Style::default().fg(Color::Cyan))),
    ];

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::Yellow)))
            .alignment(Alignment::Center),
        popup,
    );
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let pw = r.width * percent_x / 100;
    let ph = r.height * percent_y / 100;
    Rect {
        x: r.x + (r.width.saturating_sub(pw)) / 2,
        y: r.y + (r.height.saturating_sub(ph)) / 2,
        width: pw,
        height: ph,
    }
}

fn hint_bar(f: &mut Frame, area: Rect, text: &str) {
    let bar = Rect { y: area.bottom().saturating_sub(2), height: 1, ..area };
    f.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        bar,
    );
}

fn header(text: &'static str) -> Line<'static> {
    Line::from(Span::styled(text, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
}

fn key_line(key: &'static str, action: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {:12}", key), Style::default().fg(Color::Cyan)),
        Span::raw(action),
    ])
}
