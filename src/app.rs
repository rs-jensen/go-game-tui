use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;
use std::time::Instant;
use crate::ai::{best_move, Difficulty};
use crate::game::{Game, Stone};
use crate::gtp::{GtpEngine, to_gtp};

// ─── Screens ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Menu,
    Setup,
    Playing,
    Review,
    GameOver,
}

// ─── Setup options ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameMode {
    VsAI,
    TwoPlayer,
}

impl GameMode {
    pub fn label(self) -> &'static str {
        match self { GameMode::VsAI => "vs AI", GameMode::TwoPlayer => "2 Players (local)" }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineKind {
    BuiltIn,
    GnuGo,
}

impl EngineKind {
    pub fn label(self) -> &'static str {
        match self { EngineKind::BuiltIn => "Built-in", EngineKind::GnuGo => "GNU Go" }
    }
}

/// How many hints the player gets per game
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HintAllowance {
    Off,
    Three,
    Five,
    Ten,
    Unlimited,
}

impl HintAllowance {
    pub fn label(self) -> &'static str {
        match self {
            HintAllowance::Off => "Off",
            HintAllowance::Three => "3",
            HintAllowance::Five => "5",
            HintAllowance::Ten => "10",
            HintAllowance::Unlimited => "Unlimited",
        }
    }
    pub fn starting_count(self) -> Option<u8> {
        match self {
            HintAllowance::Off => None,
            HintAllowance::Three => Some(3),
            HintAllowance::Five => Some(5),
            HintAllowance::Ten => Some(10),
            HintAllowance::Unlimited => Some(255), // sentinel for unlimited
        }
    }
    pub fn next(self) -> Self {
        match self {
            HintAllowance::Off => HintAllowance::Three,
            HintAllowance::Three => HintAllowance::Five,
            HintAllowance::Five => HintAllowance::Ten,
            HintAllowance::Ten => HintAllowance::Unlimited,
            HintAllowance::Unlimited => HintAllowance::Off,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            HintAllowance::Off => HintAllowance::Unlimited,
            HintAllowance::Three => HintAllowance::Off,
            HintAllowance::Five => HintAllowance::Three,
            HintAllowance::Ten => HintAllowance::Five,
            HintAllowance::Unlimited => HintAllowance::Ten,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Setup {
    pub board_size: usize,
    pub game_mode: GameMode,
    pub engine: EngineKind,
    pub engine_level: u8,       // 1-10
    pub human_color: Stone,
    pub hints: HintAllowance,
    pub undo_enabled: bool,
    pub handicap: u8,           // 0, 2, 3, 4, 5, 6, 9
    pub time_limit: Option<u64>,
    pub selected: usize,        // row cursor in setup screen
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            board_size: 19,
            game_mode: GameMode::VsAI,
            engine: EngineKind::GnuGo,
            engine_level: 5,
            human_color: Stone::Black,
            hints: HintAllowance::Three,
            undo_enabled: true,
            handicap: 0,
            time_limit: None,
            selected: 0,
        }
    }
}

impl Setup {
    pub fn difficulty(&self) -> Difficulty {
        match self.engine_level {
            1..=3 => Difficulty::Easy,
            4..=7 => Difficulty::Medium,
            _ => Difficulty::Hard,
        }
    }

    pub fn level_label(&self) -> String {
        match self.engine {
            EngineKind::GnuGo => format!("{} / 10", self.engine_level),
            EngineKind::BuiltIn => match self.engine_level {
                1..=3 => "Easy".to_string(),
                4..=7 => "Medium".to_string(),
                _ => "Hard".to_string(),
            },
        }
    }

    pub fn handicap_label(&self) -> String {
        if self.handicap == 0 { "None".to_string() } else { format!("{} stones", self.handicap) }
    }
}

// ─── Move records ─────────────────────────────────────────────────────────────

pub struct MoveRecord {
    pub color: Stone,
    pub coord: Option<(usize, usize)>,
    pub num: usize,
}

// ─── Undo snapshot ────────────────────────────────────────────────────────────

struct UndoSnapshot {
    game: Game,
    last_move: Option<(usize, usize)>,
    history_len: usize,
    snapshots_len: usize,
}

// ─── Review state ─────────────────────────────────────────────────────────────

pub struct ReviewState {
    pub snapshots: Vec<Game>,
    pub history: Vec<MoveRecord>,
    pub pos: usize, // 0 = empty board, N = after move N
}

impl ReviewState {
    pub fn current_board(&self) -> &Game {
        if self.pos == 0 || self.snapshots.is_empty() {
            &self.snapshots[0]
        } else {
            let idx = (self.pos - 1).min(self.snapshots.len() - 1);
            &self.snapshots[idx]
        }
    }
    pub fn total(&self) -> usize { self.snapshots.len() }
    pub fn step_forward(&mut self) { if self.pos < self.total() { self.pos += 1; } }
    pub fn step_back(&mut self) { if self.pos > 0 { self.pos -= 1; } }
}

// ─── Session ─────────────────────────────────────────────────────────────────

pub struct Session {
    pub game: Game,
    pub game_mode: GameMode,
    pub human: Stone,
    pub ai: Stone,
    pub cursor: (usize, usize),
    pub status: String,

    // AI background thread
    pub ai_thinking: bool,
    ai_rx: Option<Receiver<Option<(usize, usize)>>>,

    // GTP engine
    gtp_engine: Option<Arc<Mutex<GtpEngine>>>,

    // Hints
    pub hints_remaining: Option<u8>, // None = hints off; Some(255) = unlimited
    pub hint_move: Option<(usize, usize)>,
    pub hint_thinking: bool,
    hint_rx: Option<Receiver<Option<(usize, usize)>>>,

    // Territory overlay
    pub show_territory: bool,
    pub territory: Option<Vec<Vec<Option<Stone>>>>,

    // Undo
    pub undo_enabled: bool,
    undo_stack: Vec<UndoSnapshot>,

    // Move tracking
    pub last_move: Option<(usize, usize)>,
    pub history: Vec<MoveRecord>,
    pub snapshots: Vec<Game>, // board state AFTER each move (for review)

    // Clocks
    pub clock: [Option<f64>; 2],
    last_tick: Instant,

    // Setup echo for display
    pub engine_kind: EngineKind,
    pub engine_level: u8,
}

impl Session {
    pub fn new(setup: &Setup, gnugo_ok: bool) -> Self {
        let ai_color = setup.human_color.opponent();
        let mut game = Game::new(setup.board_size);

        // Place handicap stones if requested
        if setup.handicap > 0 {
            game.place_handicap(setup.handicap);
        }

        let center = setup.board_size / 2;
        let clock = [
            setup.time_limit.map(|t| t as f64),
            setup.time_limit.map(|t| t as f64),
        ];

        let gtp_engine = if setup.game_mode == GameMode::VsAI
            && setup.engine == EngineKind::GnuGo
            && gnugo_ok
        {
            GtpEngine::launch_gnugo(setup.engine_level, setup.board_size, game.komi)
                .map(|e| Arc::new(Mutex::new(e)))
        } else {
            None
        };

        // Sync handicap stones to GTP engine
        if setup.handicap > 0 {
            if let Some(ref gtp) = gtp_engine {
                if let Ok(mut e) = gtp.lock() {
                    for &(r, c) in &game.handicap_positions(setup.handicap) {
                        e.play(Stone::Black, r, c);
                    }
                }
            }
        }

        let mut sess = Session {
            game,
            game_mode: setup.game_mode,
            human: setup.human_color,
            ai: ai_color,
            cursor: (center, center),
            status: String::new(),
            ai_thinking: false,
            ai_rx: None,
            gtp_engine,
            hints_remaining: setup.hints.starting_count(),
            hint_move: None,
            hint_thinking: false,
            hint_rx: None,
            show_territory: false,
            territory: None,
            undo_enabled: setup.undo_enabled,
            undo_stack: Vec::new(),
            last_move: None,
            history: Vec::new(),
            snapshots: Vec::new(),
            clock,
            last_tick: Instant::now(),
            engine_kind: setup.engine,
            engine_level: setup.engine_level,
        };

        // If human plays White in vs AI mode, AI (Black) goes first
        if setup.game_mode == GameMode::VsAI && setup.human_color == Stone::White {
            sess.trigger_ai_move();
        }

        sess
    }

    // ── Tick (called ~10x per second) ──────────────────────────────────────

    pub fn tick(&mut self) -> bool {
        let elapsed = self.last_tick.elapsed().as_secs_f64();
        self.last_tick = Instant::now();

        // Check hint thread
        let hint_result = self.hint_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(mv) = hint_result {
            self.hint_thinking = false;
            self.hint_rx = None;
            self.hint_move = mv;
            if mv.is_none() {
                self.status = "AI suggests: pass".to_string();
            } else if let Some((r, c)) = mv {
                self.status = format!("Hint: try {}", to_gtp(r, c, self.game.size()));
            }
        }

        // Check AI move thread
        let ai_result = self.ai_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(mv) = ai_result {
            self.ai_thinking = false;
            self.ai_rx = None;
            self.apply_ai_move(mv);
            if self.game.game_over { return true; }
        }

        // Deduct clock time from current player (only when it's human's turn)
        if !self.game.game_over && !self.ai_thinking && !self.hint_thinking {
            let idx = clock_idx(self.game.current);
            if let Some(ref mut t) = self.clock[idx] {
                *t -= elapsed;
                if *t <= 0.0 {
                    *t = 0.0;
                    let loser = self.game.current;
                    self.game.resign(loser);
                    self.status = format!("{} ran out of time!", color_name(loser));
                    return true;
                }
            }
        }
        false
    }

    // ── Place stone (human action) ─────────────────────────────────────────

    pub fn place_stone(&mut self) -> bool {
        let (r, c) = self.cursor;

        if self.undo_enabled {
            self.push_undo();
        }

        if !self.game.place(r, c) {
            self.undo_stack.pop(); // discard the snapshot we just pushed
            self.status = "Invalid move".to_string();
            return false;
        }

        self.hint_move = None;
        self.last_move = Some((r, c));
        self.record(self.game.current.opponent(), Some((r, c)));
        self.snapshots.push(self.game.clone_state());
        self.status.clear();
        self.invalidate_territory();

        // Sync GTP engine with human's move
        if let Some(ref gtp) = self.gtp_engine {
            if let Ok(mut e) = gtp.lock() {
                e.play(self.human, r, c);
            }
        }

        if self.game_mode == GameMode::VsAI && !self.game.game_over {
            self.trigger_ai_move();
        }
        true
    }

    pub fn pass_turn(&mut self) {
        if self.undo_enabled {
            self.push_undo();
        }
        let passer = self.game.current;
        self.game.pass();
        self.record(passer, None);
        self.snapshots.push(self.game.clone_state());
        self.status.clear();
        self.hint_move = None;
        self.invalidate_territory();

        if let Some(ref gtp) = self.gtp_engine {
            if let Ok(mut e) = gtp.lock() {
                e.play_pass(self.human);
            }
        }

        if self.game_mode == GameMode::VsAI && !self.game.game_over {
            self.trigger_ai_move();
        }
    }

    pub fn resign(&mut self) {
        self.game.resign(self.human);
        self.status = "You resigned.".to_string();
    }

    // ── Undo ──────────────────────────────────────────────────────────────

    pub fn undo(&mut self) {
        if self.ai_thinking || self.hint_thinking { return; }
        if let Some(snap) = self.undo_stack.pop() {
            // Count how many moves to roll back in GTP (1 per ply undone)
            let moves_undone = self.history.len() - snap.history_len;
            self.game = snap.game;
            self.last_move = snap.last_move;
            self.history.truncate(snap.history_len);
            self.snapshots.truncate(snap.snapshots_len);
            self.hint_move = None;
            self.status = "Move undone.".to_string();
            self.invalidate_territory();

            if let Some(ref gtp) = self.gtp_engine {
                if let Ok(mut e) = gtp.lock() {
                    for _ in 0..moves_undone {
                        e.undo();
                    }
                }
            }
        } else {
            self.status = "Nothing to undo.".to_string();
        }
    }

    // ── Hints ─────────────────────────────────────────────────────────────

    pub fn request_hint(&mut self) {
        if self.game_mode == GameMode::TwoPlayer {
            self.status = "Hints not available in 2-player mode.".to_string();
            return;
        }
        if self.ai_thinking || self.hint_thinking { return; }

        // Check if hints are available
        match self.hints_remaining {
            None => {
                self.status = "Hints are off.".to_string();
                return;
            }
            Some(0) => {
                self.status = "No hints remaining!".to_string();
                return;
            }
            Some(n) if n != 255 => self.hints_remaining = Some(n - 1),
            _ => {}
        }

        self.hint_thinking = true;
        self.status.clear();
        let (tx, rx) = std::sync::mpsc::channel();
        self.hint_rx = Some(rx);

        if let Some(ref gtp_arc) = self.gtp_engine {
            let gtp = Arc::clone(gtp_arc);
            let color = self.human;
            std::thread::spawn(move || {
                let mv = gtp.lock().unwrap().reg_genmove(color);
                let _ = tx.send(mv);
            });
        } else {
            let game = self.game.clone_state();
            let difficulty = Difficulty::Hard; // hints always use strongest
            let color = self.human;
            std::thread::spawn(move || {
                let mv = best_move(&game, color, difficulty);
                let _ = tx.send(mv);
            });
        }
    }

    pub fn hints_label(&self) -> String {
        match self.hints_remaining {
            None => "Off".to_string(),
            Some(255) => "Unlimited".to_string(),
            Some(0) => "0 left".to_string(),
            Some(n) => format!("{} left", n),
        }
    }

    // ── Territory overlay ─────────────────────────────────────────────────

    pub fn toggle_territory(&mut self) {
        self.show_territory = !self.show_territory;
        if self.show_territory && self.territory.is_none() {
            self.territory = Some(self.game.territory_map());
        }
    }

    fn invalidate_territory(&mut self) {
        if self.show_territory {
            self.territory = Some(self.game.territory_map());
        } else {
            self.territory = None;
        }
    }

    // ── AI move (internal) ────────────────────────────────────────────────

    fn trigger_ai_move(&mut self) {
        self.ai_thinking = true;
        let (tx, rx) = std::sync::mpsc::channel();
        self.ai_rx = Some(rx);

        if let Some(ref gtp_arc) = self.gtp_engine {
            let gtp = Arc::clone(gtp_arc);
            let ai_color = self.ai;
            std::thread::spawn(move || {
                let mv = gtp.lock().unwrap().genmove(ai_color);
                let _ = tx.send(mv);
            });
        } else {
            let game = self.game.clone_state();
            let difficulty = self.difficulty();
            let ai_color = self.ai;
            std::thread::spawn(move || {
                let mv = best_move(&game, ai_color, difficulty);
                let _ = tx.send(mv);
            });
        }
    }

    fn apply_ai_move(&mut self, mv: Option<(usize, usize)>) {
        let mover = self.game.current;
        match mv {
            Some((r, c)) => {
                if self.game.place(r, c) {
                    self.last_move = Some((r, c));
                    self.record(mover, Some((r, c)));
                } else {
                    self.game.pass();
                    self.record(mover, None);
                }
            }
            None => {
                self.game.pass();
                self.record(mover, None);
            }
        }
        self.snapshots.push(self.game.clone_state());
        self.invalidate_territory();
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(UndoSnapshot {
            game: self.game.clone_state(),
            last_move: self.last_move,
            history_len: self.history.len(),
            snapshots_len: self.snapshots.len(),
        });
    }

    fn record(&mut self, color: Stone, coord: Option<(usize, usize)>) {
        self.history.push(MoveRecord { color, coord, num: self.history.len() + 1 });
    }

    fn difficulty(&self) -> Difficulty {
        match self.engine_level {
            1..=3 => Difficulty::Easy,
            4..=7 => Difficulty::Medium,
            _ => Difficulty::Hard,
        }
    }

    // ── Display helpers ───────────────────────────────────────────────────

    pub fn result_text(&self) -> String {
        if let Some(winner) = self.game.winner {
            return format!("{} wins by resignation!", color_name(winner));
        }
        let (b, w) = self.game.score();
        if b > w {
            format!("Black wins!  B {:.1}  W {:.1}", b, w)
        } else if w > b {
            format!("White wins!  B {:.1}  W {:.1}", b, w)
        } else {
            "Draw!".to_string()
        }
    }

    pub fn score_estimate(&self) -> String {
        let (b, w) = self.game.score();
        if b > w { format!("~B+{:.1}", b - w) }
        else if w > b { format!("~W+{:.1}", w - b) }
        else { "~Even".to_string() }
    }

    pub fn format_clock(&self, color: Stone) -> String {
        match self.clock[clock_idx(color)] {
            None => "--:--".to_string(),
            Some(secs) => format!("{:02}:{:02}", secs as u64 / 60, secs as u64 % 60),
        }
    }

    pub fn cursor_coord(&self) -> String {
        to_gtp(self.cursor.0, self.cursor.1, self.game.size())
    }

    pub fn engine_name(&self) -> String {
        if let Some(ref gtp) = self.gtp_engine {
            if let Ok(e) = gtp.lock() { return e.name.clone(); }
        }
        match self.engine_kind {
            EngineKind::GnuGo => "GNU Go (offline)".to_string(),
            EngineKind::BuiltIn => format!("Built-in ({})", self.difficulty().label()),
        }
    }

    pub fn to_sgf(&self) -> String {
        let size = self.game.size();
        let mut s = format!(
            "(;GM[1]FF[4]SZ[{}]KM[{:.1}]PB[Human]PW[{}]\n",
            size, self.game.komi, self.engine_name()
        );
        for mv in &self.history {
            let cc = if mv.color == Stone::Black { 'B' } else { 'W' };
            let coord = match mv.coord {
                Some((r, c)) => format!("[{}{}]", (b'a' + c as u8) as char, (b'a' + r as u8) as char),
                None => "[tt]".to_string(),
            };
            s.push_str(&format!(";{}{}", cc, coord));
        }
        s.push(')');
        s
    }

    pub fn save_sgf(&self) -> std::io::Result<String> {
        let filename = format!(
            "go_{}.sgf",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        std::fs::write(&filename, self.to_sgf())?;
        Ok(filename)
    }

    pub fn into_review(self) -> ReviewState {
        ReviewState {
            pos: self.snapshots.len(), // start at end of game
            history: self.history,
            snapshots: self.snapshots,
        }
    }
}

// ─── App ─────────────────────────────────────────────────────────────────────

pub struct App {
    pub screen: Screen,
    pub setup: Setup,
    pub session: Option<Session>,
    pub menu_selected: usize,
    pub gnugo_available: bool,
    pub review: Option<ReviewState>,
}

impl App {
    pub fn new(gnugo_available: bool) -> Self {
        Self {
            screen: Screen::Menu,
            setup: Setup::default(),
            session: None,
            menu_selected: 0,
            gnugo_available,
            review: None,
        }
    }

    pub fn start_game(&mut self) {
        self.review = None;
        self.session = Some(Session::new(&self.setup, self.gnugo_available));
        self.screen = Screen::Playing;
    }

    pub fn enter_review(&mut self) {
        if let Some(sess) = self.session.take() {
            self.review = Some(sess.into_review());
            self.screen = Screen::Review;
        }
    }

    pub fn tick(&mut self) -> bool {
        if self.screen == Screen::Playing {
            if let Some(ref mut sess) = self.session {
                if sess.tick() || sess.game.game_over {
                    self.screen = Screen::GameOver;
                    return true;
                }
            }
        }
        false
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

pub fn color_name(color: Stone) -> &'static str {
    match color { Stone::Black => "Black", Stone::White => "White", Stone::Empty => "Nobody" }
}

fn clock_idx(color: Stone) -> usize {
    if color == Stone::Black { 0 } else { 1 }
}
