use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use crate::app::{App, Screen, color_name};
use crate::game::Stone;

const COLS: &str = "ABCDEFGHJKLMNOPQRST"; // Go column labels (no I)

pub fn render(f: &mut Frame, app: &App) {
    match app.screen {
        Screen::Menu => render_menu(f, app),
        Screen::Setup => render_setup(f, app),
        Screen::Playing => render_game(f, app),
        Screen::GameOver => render_game_over(f, app),
    }
}

fn render_menu(f: &mut Frame, app: &App) {
    let area = f.area();
    let block = Block::default()
        .title("  Go TUI  ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(block, area);

    let inner = centered_rect(40, 50, area);
    let items = ["New Game", "Quit"];
    let lines: Vec<Line> = items
        .iter()
        .enumerate()
        .map(|(i, label)| {
            if i == app.menu_selected {
                Line::from(Span::styled(
                    format!(" > {} ", label),
                    Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::raw(format!("   {}   ", label)))
            }
        })
        .collect();

    let para = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(para, inner);

    let hint = Paragraph::new("↑↓ navigate   Enter select   q quit")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    let hint_area = Rect { y: area.bottom().saturating_sub(2), height: 1, ..area };
    f.render_widget(hint, hint_area);
}

fn render_setup(f: &mut Frame, app: &App) {
    let area = f.area();
    let block = Block::default()
        .title("  Game Setup  ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(block, area);

    let inner = centered_rect(50, 70, area);
    let s = &app.setup;

    let time_label = match s.time_limit {
        None => "None".to_string(),
        Some(secs) => format!("{} min", secs / 60),
    };

    let rows = [
        ("Board size", format!("{}×{}", s.board_size, s.board_size)),
        ("Difficulty", s.difficulty.label().to_string()),
        ("Your color", if s.human_color == Stone::Black { "Black (●)".to_string() } else { "White (○)".to_string() }),
        ("Time limit", time_label),
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    for (i, (label, value)) in rows.iter().enumerate() {
        let selected = i == s.selected;
        let style = if selected {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let prefix = if selected { " ► " } else { "   " };
        lines.push(Line::from(Span::styled(
            format!("{}{:<14} {}", prefix, label, value),
            style,
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Enter] Start Game",
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
    )));

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);

    let hint = Paragraph::new("↑↓ select row   ←→ change value   Enter start   Esc back")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    let hint_area = Rect { y: area.bottom().saturating_sub(2), height: 1, ..area };
    f.render_widget(hint, hint_area);
}

fn render_game(f: &mut Frame, app: &App) {
    let area = f.area();
    let sess = match &app.session { Some(s) => s, None => return };

    // Split: board on left, info panel on right
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(44), Constraint::Length(28)])
        .split(area);

    render_board(f, sess, chunks[0]);
    render_info(f, app, chunks[1]);
}

fn render_board(f: &mut Frame, sess: &crate::app::Session, area: Rect) {
    let block = Block::default()
        .title("  Board  ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(block, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let game = &sess.game;
    let size = game.size();
    let stars: std::collections::HashSet<_> = game.star_points().into_iter().collect();
    let (cur_r, cur_c) = sess.cursor;

    // Column header
    let col_header: String = (0..size)
        .map(|c| format!(" {}", COLS.chars().nth(c).unwrap_or('?')))
        .collect::<Vec<_>>()
        .join("");
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(format!("    {}", col_header)));

    for display_r in 0..size {
        // Go notation: row 1 at bottom, so flip
        let board_r = size - 1 - display_r;
        let row_num = board_r + 1;
        let mut spans = vec![
            Span::styled(
                format!("{:2} ", row_num),
                Style::default().fg(Color::DarkGray),
            ),
        ];

        for c in 0..size {
            let stone = game.board.get(board_r, c);
            let is_cursor = board_r == cur_r && c == cur_c;
            let is_star = stars.contains(&(board_r, c));

            let (ch, style) = match stone {
                Stone::Black => {
                    let s = if is_cursor {
                        Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)
                    };
                    ("●", s)
                }
                Stone::White => {
                    let s = if is_cursor {
                        Style::default().fg(Color::White).bg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                    };
                    ("○", s)
                }
                Stone::Empty => {
                    let sym = if is_star { "+" } else { "·" };
                    let s = if is_cursor {
                        Style::default().fg(Color::Black).bg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    (sym, s)
                }
            };

            spans.push(Span::styled(format!(" {}", ch), style));
        }

        spans.push(Span::styled(
            format!(" {:2}", row_num),
            Style::default().fg(Color::DarkGray),
        ));
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(format!("    {}", col_header)));

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);
}

fn render_info(f: &mut Frame, app: &App, area: Rect) {
    let sess = match &app.session { Some(s) => s, None => return };
    let game = &sess.game;

    let block = Block::default()
        .title("  Info  ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let turn_color = if game.current == Stone::Black {
        Color::Gray
    } else {
        Color::White
    };
    let turn_sym = if game.current == Stone::Black { "●" } else { "○" };

    let (black_clock, white_clock) = (
        sess.format_clock(Stone::Black),
        sess.format_clock(Stone::White),
    );

    let difficulty_label = sess.difficulty.label();
    let board_label = format!("{}×{}", game.size(), game.size());

    let human_sym = if sess.human == Stone::Black { "●" } else { "○" };
    let ai_sym = if sess.ai == Stone::Black { "●" } else { "○" };

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(" TURN", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(
            format!(" {} {}", turn_sym, color_name(game.current)),
            Style::default().fg(turn_color).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(" CLOCKS", Style::default().fg(Color::Yellow))),
        Line::from(format!(" ● Black  {}", black_clock)),
        Line::from(format!(" ○ White  {}", white_clock)),
        Line::from(""),
        Line::from(Span::styled(" CAPTURES", Style::default().fg(Color::Yellow))),
        Line::from(format!(" ● Black  {}", game.captures[0])),
        Line::from(format!(" ○ White  {}", game.captures[1])),
        Line::from(""),
        Line::from(Span::styled(" GAME", Style::default().fg(Color::Yellow))),
        Line::from(format!(" Board    {}", board_label)),
        Line::from(format!(" Mode     {}", difficulty_label)),
        Line::from(format!(" Moves    {}", game.move_count)),
        Line::from(format!(" You      {}", human_sym)),
        Line::from(format!(" AI       {}", ai_sym)),
        Line::from(""),
        Line::from(Span::styled(" CONTROLS", Style::default().fg(Color::Yellow))),
        Line::from(" ↑↓←→ / hjkl  Move"),
        Line::from(" Enter / Space  Place"),
        Line::from(" p              Pass"),
        Line::from(" r              Resign"),
        Line::from(" q              Quit"),
    ];

    if !sess.status.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(" {}", sess.status),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }

    if sess.ai_thinking {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " AI thinking...",
            Style::default().fg(Color::Magenta),
        )));
    }

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);
}

fn render_game_over(f: &mut Frame, app: &App) {
    let area = f.area();
    let sess = match &app.session { Some(s) => s, None => return };

    // Still show the board in background
    render_game(f, app);

    // Overlay dialog
    let popup = centered_rect(50, 40, area);
    f.render_widget(Clear, popup);

    let result = sess.result_text();
    let (b, w) = sess.game.score();

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            " GAME OVER ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(" {}", result),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(" Black: {:.1}  White: {:.1}", b, w)),
        Line::from(format!(" (komi {:.1})", sess.game.komi)),
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(
            " [n] New game   [q] Quit",
            Style::default().fg(Color::Cyan),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));
    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(para, popup);
}

// Returns a centered rect using a percentage of the parent
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let pop_w = r.width * percent_x / 100;
    let pop_h = r.height * percent_y / 100;
    Rect {
        x: r.x + (r.width - pop_w) / 2,
        y: r.y + (r.height - pop_h) / 2,
        width: pop_w,
        height: pop_h,
    }
}
