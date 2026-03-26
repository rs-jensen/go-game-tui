use std::time::Instant;
use crate::ai::{best_move, Difficulty};
use crate::game::{Game, Stone};

#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Menu,
    Setup,
    Playing,
    GameOver,
}

#[derive(Clone, Copy)]
pub struct Setup {
    pub board_size: usize,       // 9, 13, or 19
    pub difficulty: Difficulty,
    pub human_color: Stone,      // Black or White
    pub time_limit: Option<u64>, // seconds per player, None = no limit
    // cursor positions for setup navigation
    pub selected: usize,         // which row is highlighted
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            board_size: 19,
            difficulty: Difficulty::Medium,
            human_color: Stone::Black,
            time_limit: None,
            selected: 0,
        }
    }
}

pub struct Session {
    pub game: Game,
    pub human: Stone,
    pub ai: Stone,
    pub difficulty: Difficulty,
    pub cursor: (usize, usize),  // (row, col)
    pub status: String,
    pub ai_thinking: bool,
    // Clocks: time remaining in seconds for [black, white]
    pub clock: [Option<f64>; 2],
    pub last_tick: Instant,
}

impl Session {
    pub fn new(setup: &Setup) -> Self {
        let ai_color = setup.human_color.opponent();
        let game = Game::new(setup.board_size);

        // Give white the komi advantage
        let center = setup.board_size / 2;
        let clock = [
            setup.time_limit.map(|t| t as f64),
            setup.time_limit.map(|t| t as f64),
        ];

        let mut s = Self {
            game,
            human: setup.human_color,
            ai: ai_color,
            difficulty: setup.difficulty,
            cursor: (center, center),
            status: String::new(),
            ai_thinking: false,
            clock,
            last_tick: Instant::now(),
        };

        // If human plays white, AI (black) goes first
        if setup.human_color == Stone::White {
            s.ai_thinking = true;
            s.do_ai_move();
        }

        s
    }

    fn clock_index(color: Stone) -> usize {
        if color == Stone::Black { 0 } else { 1 }
    }

    pub fn tick(&mut self) -> bool {
        let elapsed = self.last_tick.elapsed().as_secs_f64();
        self.last_tick = Instant::now();

        // Deduct time from the current player
        if !self.game.game_over {
            let idx = Self::clock_index(self.game.current);
            if let Some(ref mut t) = self.clock[idx] {
                *t -= elapsed;
                if *t <= 0.0 {
                    *t = 0.0;
                    let loser = self.game.current;
                    self.game.resign(loser); // time out = loss
                    self.status = format!("{} ran out of time!", color_name(loser));
                    return true; // game over signal
                }
            }
        }
        false
    }

    pub fn place_stone(&mut self) -> bool {
        let (r, c) = self.cursor;
        if self.game.place(r, c) {
            self.status.clear();
            if !self.game.game_over {
                self.ai_thinking = true;
                self.do_ai_move();
            }
            true
        } else {
            self.status = "Invalid move".to_string();
            false
        }
    }

    pub fn pass_turn(&mut self) {
        self.game.pass();
        self.status.clear();
        if !self.game.game_over {
            self.ai_thinking = true;
            self.do_ai_move();
        }
    }

    pub fn resign(&mut self) {
        self.game.resign(self.human);
        self.status = "You resigned.".to_string();
    }

    fn do_ai_move(&mut self) {
        if self.game.game_over {
            self.ai_thinking = false;
            return;
        }
        match best_move(&self.game, self.ai, self.difficulty) {
            Some((r, c)) => { self.game.place(r, c); }
            None => { self.game.pass(); }
        }
        self.ai_thinking = false;
    }

    pub fn result_text(&self) -> String {
        if let Some(winner) = self.game.winner {
            return format!("{} wins by resignation!", color_name(winner));
        }
        let (b, w) = self.game.score();
        if b > w {
            format!("Black wins! ({:.1} - {:.1})", b, w)
        } else if w > b {
            format!("White wins! ({:.1} - {:.1})", w, b)
        } else {
            format!("Draw! ({:.1} - {:.1})", b, w)
        }
    }

    pub fn format_clock(&self, color: Stone) -> String {
        let idx = Self::clock_index(color);
        match self.clock[idx] {
            None => "∞".to_string(),
            Some(secs) => {
                let m = secs as u64 / 60;
                let s = secs as u64 % 60;
                format!("{:02}:{:02}", m, s)
            }
        }
    }
}

pub struct App {
    pub screen: Screen,
    pub setup: Setup,
    pub session: Option<Session>,
    pub menu_selected: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Menu,
            setup: Setup::default(),
            session: None,
            menu_selected: 0,
        }
    }

    pub fn start_game(&mut self) {
        self.session = Some(Session::new(&self.setup));
        self.screen = Screen::Playing;
    }

    pub fn tick(&mut self) -> bool {
        if self.screen == Screen::Playing {
            if let Some(ref mut sess) = self.session {
                if sess.tick() {
                    self.screen = Screen::GameOver;
                    return true;
                }
                if sess.game.game_over {
                    self.screen = Screen::GameOver;
                    return true;
                }
            }
        }
        false
    }
}

pub fn color_name(color: Stone) -> &'static str {
    match color {
        Stone::Black => "Black",
        Stone::White => "White",
        Stone::Empty => "Nobody",
    }
}
