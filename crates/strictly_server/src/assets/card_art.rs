//! Procedural ASCII card art renderer.
//!
//! Generates playing card art from rank and suit data without image conversion.
//! Cards adapt to any requested size via the `width` and `height` parameters.
//!
//! The pip layout for numbered cards (2–10) follows the standard arrangement
//! used on every manufactured deck.  Face cards (J/Q/K) render a centred letter
//! in a box; the Ace renders a single large centred pip.

use crate::assets::{Card, JokerColor, Rank, Suit};
use tracing::instrument;

// ── Public API ────────────────────────────────────────────────────────────────

/// Render a playing card as an ASCII string.
///
/// `width` × `height` are the **outer** dimensions including the card border.
/// Returns a string with exactly `height` newline-terminated rows, each
/// `width` display columns wide (assuming 1-column-wide suit symbols).
#[instrument(fields(?card, width, height))]
pub fn render_card(card: Card, width: usize, height: usize) -> String {
    let iw = width - 2; // interior width
    let ip = height - 4; // pip rows (between the two rank lines)

    match card {
        Card::Playing(rank, suit) => render_playing(rank, suit, iw, ip),
        Card::Joker(color) => render_joker_card(color, iw, ip),
    }
}

/// Render a face-down card placeholder the same dimensions as [`render_card`].
#[instrument(fields(width, height))]
pub fn render_card_back(width: usize, height: usize) -> String {
    let iw = width - 2;
    let ip = height - 4;

    let mut out = String::new();
    push_top_border(&mut out, iw);
    push_row(&mut out, iw, |_| '?');
    for r in 0..ip {
        out.push('|');
        for c in 0..iw {
            out.push(if (r + c) % 2 == 0 { ':' } else { '.' });
        }
        out.push('|');
        out.push('\n');
    }
    push_row(&mut out, iw, |_| '?');
    push_bottom_border(&mut out, iw);
    out
}

// ── Suit / rank helpers ───────────────────────────────────────────────────────

fn suit_char(suit: Suit) -> char {
    match suit {
        Suit::Clubs => '♣',
        Suit::Diamonds => '♦',
        Suit::Hearts => '♥',
        Suit::Spades => '♠',
    }
}

fn rank_str(rank: Rank) -> &'static str {
    match rank {
        Rank::Ace => "A",
        Rank::Two => "2",
        Rank::Three => "3",
        Rank::Four => "4",
        Rank::Five => "5",
        Rank::Six => "6",
        Rank::Seven => "7",
        Rank::Eight => "8",
        Rank::Nine => "9",
        Rank::Ten => "10",
        Rank::Jack => "J",
        Rank::Queen => "Q",
        Rank::King => "K",
    }
}

// ── Pip grid ──────────────────────────────────────────────────────────────────

/// Standard pip positions in a 5-row × 3-col normalised grid.
///
/// Row 0 = top, row 4 = bottom.  Col 0 = left, col 1 = centre, col 2 = right.
/// Face cards and Ace are handled separately.
fn pip_positions(rank: Rank) -> &'static [(usize, usize)] {
    match rank {
        Rank::Ace => &[(2, 1)],
        Rank::Two => &[(0, 1), (4, 1)],
        Rank::Three => &[(0, 1), (2, 1), (4, 1)],
        Rank::Four => &[(0, 0), (0, 2), (4, 0), (4, 2)],
        Rank::Five => &[(0, 0), (0, 2), (2, 1), (4, 0), (4, 2)],
        Rank::Six => &[(0, 0), (0, 2), (2, 0), (2, 2), (4, 0), (4, 2)],
        Rank::Seven => &[(0, 0), (0, 2), (1, 1), (2, 0), (2, 2), (4, 0), (4, 2)],
        Rank::Eight => &[
            (0, 0),
            (0, 2),
            (1, 1),
            (2, 0),
            (2, 2),
            (3, 1),
            (4, 0),
            (4, 2),
        ],
        Rank::Nine => &[
            (0, 0),
            (0, 2),
            (1, 0),
            (1, 2),
            (2, 1),
            (3, 0),
            (3, 2),
            (4, 0),
            (4, 2),
        ],
        Rank::Ten => &[
            (0, 0),
            (0, 2),
            (1, 0),
            (1, 1),
            (1, 2),
            (3, 0),
            (3, 1),
            (3, 2),
            (4, 0),
            (4, 2),
        ],
        _ => &[],
    }
}

/// Map a normalised grid column (0=L, 1=C, 2=R) to an interior column index.
fn grid_col(gc: usize, iw: usize) -> usize {
    match gc {
        0 => iw / 4,
        1 => iw / 2,
        _ => iw - iw / 4 - 1,
    }
}

/// Map a normalised grid row (0–4) to an interior row index within `ip` rows.
fn grid_row(gr: usize, ip: usize) -> usize {
    match gr {
        0 => 0,
        1 => ip / 4,
        2 => ip / 2,
        3 => ip - ip / 4 - 1,
        _ => ip - 1,
    }
}

// ── Card renderers ────────────────────────────────────────────────────────────

fn render_playing(rank: Rank, suit: Suit, iw: usize, ip: usize) -> String {
    let sym = suit_char(suit);
    let rs = rank_str(rank);

    // Build the pip grid (interior rows × interior cols, all spaces initially).
    let mut grid: Vec<Vec<char>> = vec![vec![' '; iw]; ip];

    match rank {
        Rank::Jack | Rank::Queen | Rank::King => {
            render_face_pips(&mut grid, rank, sym, iw, ip);
        }
        _ => {
            for &(gr, gc) in pip_positions(rank) {
                let r = grid_row(gr, ip);
                let c = grid_col(gc, iw);
                grid[r][c] = sym;
            }
        }
    }

    assemble(rs, sym, &grid, iw)
}

fn render_joker_card(color: JokerColor, iw: usize, ip: usize) -> String {
    let sym = match color {
        JokerColor::Black => '*',
        JokerColor::Red => '#',
    };
    let mut grid: Vec<Vec<char>> = vec![vec![' '; iw]; ip];

    // Star pattern around centre.
    let cr = ip / 2;
    let cc = iw / 2;
    grid[cr][cc] = sym;
    if cr > 0 {
        grid[cr - 1][cc] = sym;
    }
    if cr + 1 < ip {
        grid[cr + 1][cc] = sym;
    }
    if cc > 0 {
        grid[cr][cc - 1] = sym;
    }
    if cc + 1 < iw {
        grid[cr][cc + 1] = sym;
    }

    assemble("Jkr", sym, &grid, iw)
}

/// Render the pip area for J / Q / K: a centred letter in a small box.
fn render_face_pips(grid: &mut [Vec<char>], rank: Rank, sym: char, iw: usize, ip: usize) {
    let face = match rank {
        Rank::Jack => 'J',
        Rank::Queen => 'Q',
        Rank::King => 'K',
        _ => '?',
    };
    let cr = ip / 2;
    let cc = iw / 2;

    // Draw a 5-wide box centred on (cr, cc) when space allows.
    if iw >= 5 && ip >= 3 && cr >= 1 && cc >= 2 {
        let bl = cc - 2; // box left
        let br = cc + 2; // box right

        // Top of box
        let top = cr - 1;
        grid[top][bl] = '.';
        grid[top][(bl + 1)..br].fill('-');
        grid[top][br] = '.';

        // Face letter row
        grid[cr][bl] = '|';
        grid[cr][cc] = face;
        grid[cr][br] = '|';

        // Bottom of box
        if cr + 1 < ip {
            let bot = cr + 1;
            grid[bot][bl] = '`';
            grid[bot][(bl + 1)..br].fill('-');
            grid[bot][br] = '\'';
        }

        // Suit symbol below the box
        if cr + 2 < ip {
            grid[cr + 2][cc] = sym;
        }
    } else {
        // Fallback: just the face letter centred
        grid[cr][cc] = face;
    }
}

// ── String assembly ───────────────────────────────────────────────────────────

/// Assemble the full card string from the pip grid and rank/suit strings.
fn assemble(rank_s: &str, sym: char, grid: &[Vec<char>], iw: usize) -> String {
    let used = rank_s.len() + 1; // rank chars + 1 display col for suit symbol
    let mut out = String::new();

    push_top_border(&mut out, iw);

    // Top rank line: |{rank}{suit}{spaces}|
    out.push('|');
    out.push_str(rank_s);
    out.push(sym);
    for _ in used..iw {
        out.push(' ');
    }
    out.push('|');
    out.push('\n');

    // Pip rows
    for row in grid {
        out.push('|');
        for &ch in row {
            out.push(ch);
        }
        out.push('|');
        out.push('\n');
    }

    // Bottom rank line: |{spaces}{suit}{rank}|
    out.push('|');
    for _ in used..iw {
        out.push(' ');
    }
    out.push(sym);
    out.push_str(rank_s);
    out.push('|');
    out.push('\n');

    push_bottom_border(&mut out, iw);
    out
}

fn push_top_border(out: &mut String, iw: usize) {
    out.push('.');
    for _ in 0..iw {
        out.push('-');
    }
    out.push('.');
    out.push('\n');
}

fn push_bottom_border(out: &mut String, iw: usize) {
    out.push('`');
    for _ in 0..iw {
        out.push('-');
    }
    out.push('\'');
    out.push('\n');
}

fn push_row(out: &mut String, iw: usize, fill: impl Fn(usize) -> char) {
    out.push('|');
    for c in 0..iw {
        out.push(fill(c));
    }
    out.push('|');
    out.push('\n');
}
