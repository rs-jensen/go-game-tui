use crate::game::{Game, Stone};
use rand::prelude::IndexedRandom;
use rand::{Rng, RngExt};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Difficulty {
    Easy,   // Random moves
    Medium, // Heuristic-based
    Hard,   // Time-bounded MCTS
}

impl Difficulty {
    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
        }
    }
}

// Returns the best move for `color` given the current game state, or None to pass
pub fn best_move(game: &Game, color: Stone, difficulty: Difficulty) -> Option<(usize, usize)> {
    let moves = game.valid_moves();
    if moves.is_empty() {
        return None;
    }
    match difficulty {
        Difficulty::Easy => random_move(&moves),
        Difficulty::Medium => heuristic_move(game, color, &moves),
        Difficulty::Hard => mcts_move(game, color, &moves),
    }
}

fn random_move(moves: &[(usize, usize)]) -> Option<(usize, usize)> {
    let mut rng = rand::rng();
    moves.choose(&mut rng).copied()
}

// Score a candidate move for the heuristic AI
fn score_move(game: &Game, color: Stone, r: usize, c: usize) -> i32 {
    let size = game.size();
    let center = (size / 2) as i32;
    let mut score = 0i32;

    // Prefer center area
    let dr = (r as i32 - center).abs();
    let dc = (c as i32 - center).abs();
    score -= dr + dc;

    // Check what happens if we play here
    let mut trial_game = clone_game(game);
    if !trial_game.place(r, c) {
        return i32::MIN;
    }

    // Heavily reward capturing opponent stones
    let opponent = color.opponent();
    for (nr, nc) in game.board.neighbors(r, c) {
        if game.board.get(nr, nc) == opponent {
            let grp = game.board.find_group(nr, nc);
            if game.board.liberties(&grp) == 1 {
                score += 50 * grp.len() as i32; // capture!
            } else if game.board.liberties(&grp) == 2 {
                score += 10; // threatening
            }
        }
    }

    // Reward connecting to own stones
    for (nr, nc) in game.board.neighbors(r, c) {
        if game.board.get(nr, nc) == color {
            score += 5;
        }
    }

    score
}

fn heuristic_move(game: &Game, color: Stone, moves: &[(usize, usize)]) -> Option<(usize, usize)> {
    let mut rng = rand::rng();
    let best_score = moves.iter().map(|&(r, c)| score_move(game, color, r, c)).max()?;
    let best: Vec<_> = moves
        .iter()
        .copied()
        .filter(|&(r, c)| score_move(game, color, r, c) == best_score)
        .collect();
    best.choose(&mut rng).copied()
}

// Monte Carlo Tree Search with a 1.5-second time budget
fn mcts_move(game: &Game, color: Stone, moves: &[(usize, usize)]) -> Option<(usize, usize)> {
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut wins = vec![0u32; moves.len()];
    let mut visits = vec![0u32; moves.len()];
    let mut rng = rand::rng();

    while Instant::now() < deadline {
        for (i, &(r, c)) in moves.iter().enumerate() {
            let mut trial = clone_game(game);
            if !trial.place(r, c) {
                continue;
            }
            let result = random_playout(&mut trial, color, &mut rng);
            visits[i] += 1;
            if result { wins[i] += 1; }
        }
    }

    // Pick the move with best win rate (at least 1 visit)
    moves
        .iter()
        .enumerate()
        .filter(|(i, _)| visits[*i] > 0)
        .max_by(|(a, _), (b, _)| {
            let rate_a = wins[*a] as f64 / visits[*a] as f64;
            let rate_b = wins[*b] as f64 / visits[*b] as f64;
            rate_a.partial_cmp(&rate_b).unwrap()
        })
        .map(|(_, &m)| m)
        .or_else(|| random_move(moves))
}

// Play random moves until game over, return true if `color` wins
fn random_playout(game: &mut Game, color: Stone, rng: &mut impl Rng) -> bool {
    let max_depth = game.size() * game.size() * 2;
    for _ in 0..max_depth {
        if game.game_over { break; }
        let moves = game.valid_moves();
        if moves.is_empty() {
            game.pass();
            continue;
        }
        // 10% chance to pass to speed up playouts
        if rng.random_bool(0.1) {
            game.pass();
            continue;
        }
        let &(r, c) = moves.choose(rng).unwrap();
        game.place(r, c);
    }
    let (black, white) = game.score();
    if color == Stone::Black { black > white } else { white > black }
}

fn clone_game(game: &Game) -> Game {
    game.clone_state()
}

