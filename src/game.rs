use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Stone {
    Empty,
    Black,
    White,
}

impl Stone {
    pub fn opponent(self) -> Stone {
        match self {
            Stone::Black => Stone::White,
            Stone::White => Stone::Black,
            Stone::Empty => Stone::Empty,
        }
    }
}

#[derive(Clone)]
pub struct Board {
    pub size: usize,
    pub grid: Vec<Vec<Stone>>,
}

impl Board {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            grid: vec![vec![Stone::Empty; size]; size],
        }
    }

    pub fn get(&self, r: usize, c: usize) -> Stone {
        self.grid[r][c]
    }

    pub fn set(&mut self, r: usize, c: usize, s: Stone) {
        self.grid[r][c] = s;
    }

    pub fn neighbors(&self, r: usize, c: usize) -> Vec<(usize, usize)> {
        let mut n = Vec::new();
        if r > 0 { n.push((r - 1, c)); }
        if r + 1 < self.size { n.push((r + 1, c)); }
        if c > 0 { n.push((r, c - 1)); }
        if c + 1 < self.size { n.push((r, c + 1)); }
        n
    }

    // Find all stones in the same connected group
    pub fn find_group(&self, r: usize, c: usize) -> HashSet<(usize, usize)> {
        let color = self.get(r, c);
        let mut group = HashSet::new();
        let mut stack = vec![(r, c)];
        while let Some((cr, cc)) = stack.pop() {
            if group.contains(&(cr, cc)) { continue; }
            if self.get(cr, cc) == color {
                group.insert((cr, cc));
                for nb in self.neighbors(cr, cc) {
                    if !group.contains(&nb) {
                        stack.push(nb);
                    }
                }
            }
        }
        group
    }

    // Count liberties (empty adjacent intersections) for a group
    pub fn liberties(&self, group: &HashSet<(usize, usize)>) -> usize {
        let mut libs = HashSet::new();
        for &(r, c) in group {
            for (nr, nc) in self.neighbors(r, c) {
                if self.get(nr, nc) == Stone::Empty {
                    libs.insert((nr, nc));
                }
            }
        }
        libs.len()
    }

    // Remove a group from the board, returns the count removed
    pub fn remove_group(&mut self, group: &HashSet<(usize, usize)>) -> usize {
        let count = group.len();
        for &(r, c) in group {
            self.set(r, c, Stone::Empty);
        }
        count
    }

    pub fn grid_eq(&self, other: &Board) -> bool {
        self.grid == other.grid
    }
}

pub struct Game {
    pub board: Board,
    pub current: Stone,       // whose turn it is
    pub captures: [usize; 2], // [black_captures, white_captures]
    pub consecutive_passes: usize,
    pub game_over: bool,
    pub winner: Option<Stone>,
    pub komi: f32,
    prev_board: Option<Board>, // for ko rule
    pub move_count: usize,
}

impl Game {
    pub fn new(size: usize) -> Self {
        Self {
            board: Board::new(size),
            current: Stone::Black,
            captures: [0, 0],
            consecutive_passes: 0,
            game_over: false,
            winner: None,
            komi: 6.5,
            prev_board: None,
            move_count: 0,
        }
    }

    pub fn size(&self) -> usize {
        self.board.size
    }

    // Try to place a stone; returns true if valid
    pub fn place(&mut self, r: usize, c: usize) -> bool {
        if self.game_over { return false; }
        if self.board.get(r, c) != Stone::Empty { return false; }

        let mut trial = self.board.clone();
        trial.set(r, c, self.current);

        // Capture opponent stones with no liberties
        let opponent = self.current.opponent();
        let mut captured = 0;
        for (nr, nc) in trial.neighbors(r, c) {
            if trial.get(nr, nc) == opponent {
                let grp = trial.find_group(nr, nc);
                if trial.liberties(&grp) == 0 {
                    captured += trial.remove_group(&grp);
                }
            }
        }

        // Suicide rule: if the placed stone's group has no liberties, invalid
        let placed_group = trial.find_group(r, c);
        if trial.liberties(&placed_group) == 0 {
            return false;
        }

        // Ko rule: new board state must differ from the state before last move
        if let Some(ref prev) = self.prev_board {
            if trial.grid_eq(prev) {
                return false;
            }
        }

        // Commit move
        self.prev_board = Some(self.board.clone());
        self.board = trial;
        if self.current == Stone::Black {
            self.captures[0] += captured;
        } else {
            self.captures[1] += captured;
        }
        self.consecutive_passes = 0;
        self.move_count += 1;
        self.current = opponent;
        true
    }

    pub fn pass(&mut self) {
        if self.game_over { return; }
        self.consecutive_passes += 1;
        self.move_count += 1;
        if self.consecutive_passes >= 2 {
            self.game_over = true;
        }
        self.current = self.current.opponent();
    }

    pub fn resign(&mut self, resigning: Stone) {
        self.game_over = true;
        self.winner = Some(resigning.opponent());
    }

    // Chinese scoring: stones + territory + komi for white
    pub fn score(&self) -> (f32, f32) {
        let size = self.board.size;
        let mut black_stones = 0usize;
        let mut white_stones = 0usize;

        for r in 0..size {
            for c in 0..size {
                match self.board.get(r, c) {
                    Stone::Black => black_stones += 1,
                    Stone::White => white_stones += 1,
                    Stone::Empty => {}
                }
            }
        }

        let (black_territory, white_territory) = self.count_territory();

        let black = (black_stones + black_territory) as f32;
        let white = (white_stones + white_territory) as f32 + self.komi;
        (black, white)
    }

    fn count_territory(&self) -> (usize, usize) {
        let size = self.board.size;
        let mut visited = vec![vec![false; size]; size];
        let mut black_territory = 0;
        let mut white_territory = 0;

        for r in 0..size {
            for c in 0..size {
                if self.board.get(r, c) == Stone::Empty && !visited[r][c] {
                    // Flood fill this empty region
                    let mut region = Vec::new();
                    let mut borders = HashSet::new();
                    let mut stack = vec![(r, c)];
                    while let Some((cr, cc)) = stack.pop() {
                        if visited[cr][cc] { continue; }
                        visited[cr][cc] = true;
                        region.push((cr, cc));
                        for (nr, nc) in self.board.neighbors(cr, cc) {
                            match self.board.get(nr, nc) {
                                Stone::Empty => {
                                    if !visited[nr][nc] {
                                        stack.push((nr, nc));
                                    }
                                }
                                s => { borders.insert(s); }
                            }
                        }
                    }
                    // Assign territory if only one color borders the region
                    if borders.len() == 1 {
                        let owner = *borders.iter().next().unwrap();
                        match owner {
                            Stone::Black => black_territory += region.len(),
                            Stone::White => white_territory += region.len(),
                            Stone::Empty => {}
                        }
                    }
                }
            }
        }

        (black_territory, white_territory)
    }

    pub fn valid_moves(&self) -> Vec<(usize, usize)> {
        let size = self.board.size;
        let mut moves = Vec::new();
        for r in 0..size {
            for c in 0..size {
                if self.board.get(r, c) == Stone::Empty {
                    let mut trial = self.clone_for_trial();
                    if trial.place(r, c) {
                        moves.push((r, c));
                    }
                }
            }
        }
        moves
    }

    fn clone_for_trial(&self) -> Game {
        self.clone_state()
    }

    pub fn clone_state(&self) -> Game {
        Game {
            board: self.board.clone(),
            current: self.current,
            captures: self.captures,
            consecutive_passes: self.consecutive_passes,
            game_over: self.game_over,
            winner: self.winner,
            komi: self.komi,
            prev_board: self.prev_board.clone(),
            move_count: self.move_count,
        }
    }

    // Star points for visual reference on the board
    pub fn star_points(&self) -> Vec<(usize, usize)> {
        match self.board.size {
            19 => vec![
                (3, 3), (3, 9), (3, 15),
                (9, 3), (9, 9), (9, 15),
                (15, 3), (15, 9), (15, 15),
            ],
            13 => vec![
                (3, 3), (3, 9),
                (6, 6),
                (9, 3), (9, 9),
            ],
            9 => vec![
                (2, 2), (2, 6),
                (4, 4),
                (6, 2), (6, 6),
            ],
            _ => vec![],
        }
    }
}
