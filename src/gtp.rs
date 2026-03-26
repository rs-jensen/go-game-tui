use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, Command, Stdio};
use crate::game::Stone;

// Go column labels used in GTP notation (no I)
const GTP_COLS: &str = "ABCDEFGHJKLMNOPQRST";

pub struct GtpEngine {
    process: Child,
    writer: BufWriter<std::process::ChildStdin>,
    reader: BufReader<std::process::ChildStdout>,
    pub name: String,
    pub board_size: usize,
}

impl GtpEngine {
    pub fn launch_gnugo(level: u8, size: usize, komi: f32) -> Option<Self> {
        let mut child = Command::new("gnugo")
            .args(["--mode", "gtp", "--level", &level.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;

        let mut engine = GtpEngine {
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout),
            process: child,
            name: format!("GNU Go  Lv.{}", level),
            board_size: size,
        };

        // Set up the board
        engine.cmd(&format!("boardsize {}", size));
        engine.cmd("clear_board");
        engine.cmd(&format!("komi {}", komi));

        Some(engine)
    }

    fn cmd(&mut self, command: &str) -> Option<String> {
        writeln!(self.writer, "{}", command).ok()?;
        self.writer.flush().ok()?;

        // Read lines until a blank line (GTP response terminator)
        let mut result = String::new();
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) | Err(_) => return None,
                Ok(_) => {}
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            result.push_str(trimmed);
            result.push('\n');
        }

        let result = result.trim().to_string();
        if result.starts_with('=') {
            Some(result[1..].trim().to_string())
        } else {
            None // engine returned an error
        }
    }

    // Inform engine of a move that was just played (human move)
    pub fn play(&mut self, color: Stone, r: usize, c: usize) {
        let coord = to_gtp(r, c, self.board_size);
        self.cmd(&format!("play {} {}", color_char(color), coord));
    }

    pub fn play_pass(&mut self, color: Stone) {
        self.cmd(&format!("play {} pass", color_char(color)));
    }

    // Ask engine to generate and play a move; returns None for pass/resign
    pub fn genmove(&mut self, color: Stone) -> Option<(usize, usize)> {
        let resp = self.cmd(&format!("genmove {}", color_char(color)))?;
        let resp = resp.trim().to_uppercase();
        if resp == "PASS" || resp == "RESIGN" {
            None
        } else {
            from_gtp(&resp, self.board_size)
        }
    }

    // Suggest a move without playing it on the engine's internal board (used for hints)
    pub fn reg_genmove(&mut self, color: Stone) -> Option<(usize, usize)> {
        let resp = self.cmd(&format!("reg_genmove {}", color_char(color)))?;
        let resp = resp.trim().to_uppercase();
        if resp == "PASS" || resp == "RESIGN" {
            None
        } else {
            from_gtp(&resp, self.board_size)
        }
    }

    // Roll back the last move on the engine's internal board
    pub fn undo(&mut self) {
        self.cmd("undo");
    }
}

impl Drop for GtpEngine {
    fn drop(&mut self) {
        let _ = self.cmd("quit");
        let _ = self.process.kill();
    }
}

// Check if gnugo binary is available on this system
pub fn gnugo_available() -> bool {
    Command::new("gnugo")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

// Internal (row, col) -> GTP coordinate string like "D4"
pub fn to_gtp(r: usize, c: usize, size: usize) -> String {
    let col = GTP_COLS.chars().nth(c).unwrap_or('A');
    format!("{}{}", col, size - r)
}

// GTP coordinate string "D4" -> internal (row, col)
pub fn from_gtp(s: &str, size: usize) -> Option<(usize, usize)> {
    let s = s.trim().to_uppercase();
    let col_char = s.chars().next()?;
    let row_str: String = s.chars().skip(1).collect();
    let c = GTP_COLS.find(col_char)?;
    let gtp_row: usize = row_str.parse().ok()?;
    if gtp_row == 0 || gtp_row > size {
        return None;
    }
    Some((size - gtp_row, c))
}

fn color_char(color: Stone) -> char {
    match color {
        Stone::Black => 'B',
        Stone::White | Stone::Empty => 'W',
    }
}
