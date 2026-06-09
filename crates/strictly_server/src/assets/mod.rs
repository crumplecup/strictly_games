//! Game asset routing — returns the correct representation for each frontend.
//!
//! Each frontend uses a different format:
//! - **ratatui** — [`GameArt::Ascii`]: a pre-generated `&'static str` embedded
//!   at compile time via [`ascii_art`] constants.
//! - **egui / leptos** — [`GameArt::SvgPath`]: a filesystem path to the SVG.
//!
//! The constants in [`ascii_art`] start empty and are populated by running:
//! ```text
//! strictly_games generate-assets --columns 12
//! ```
//! After regenerating, recompile the workspace to embed the new art.

pub(crate) mod ascii_art;
pub(crate) mod card_art;

use std::path::Path;

// ── Card types ────────────────────────────────────────────────────────────────

/// Playing card rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rank {
    /// Ace
    Ace,
    /// 2
    Two,
    /// 3
    Three,
    /// 4
    Four,
    /// 5
    Five,
    /// 6
    Six,
    /// 7
    Seven,
    /// 8
    Eight,
    /// 9
    Nine,
    /// 10
    Ten,
    /// Jack
    Jack,
    /// Queen
    Queen,
    /// King
    King,
}

/// Playing card suit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Suit {
    /// ♣
    Clubs,
    /// ♦
    Diamonds,
    /// ♥
    Hearts,
    /// ♠
    Spades,
}

/// Joker colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JokerColor {
    /// Black joker
    Black,
    /// Red joker
    Red,
}

/// A single card from the deck — either a standard playing card or a joker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Card {
    /// Standard playing card (rank + suit).
    Playing(Rank, Suit),
    /// Joker.
    Joker(JokerColor),
}

// ── Die type ──────────────────────────────────────────────────────────────────

/// A single die face (1–6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Die {
    /// ⚀
    One,
    /// ⚁
    Two,
    /// ⚂
    Three,
    /// ⚃
    Four,
    /// ⚄
    Five,
    /// ⚅
    Six,
}

// ── Asset representation ──────────────────────────────────────────────────────

/// The appropriate asset representation for the current frontend.
#[derive(Debug, Clone, Copy)]
pub enum GameArt {
    /// Pre-generated ASCII art for terminal rendering (ratatui).
    Ascii(&'static str),
    /// Filesystem path to the SVG source for GPU/browser rendering (egui, leptos).
    SvgPath(&'static Path),
}

// ── Card lookups ──────────────────────────────────────────────────────────────

/// Return the pre-generated ASCII art for a card.
///
/// Empty string until `generate-assets` has been run and the workspace recompiled.
pub fn card_ascii(card: Card) -> &'static str {
    use Rank::*;
    use Suit::*;
    match card {
        Card::Playing(Ace, Clubs) => ascii_art::CARD_ACE_OF_CLUBS,
        Card::Playing(Ace, Diamonds) => ascii_art::CARD_ACE_OF_DIAMONDS,
        Card::Playing(Ace, Hearts) => ascii_art::CARD_ACE_OF_HEARTS,
        Card::Playing(Ace, Spades) => ascii_art::CARD_ACE_OF_SPADES,
        Card::Playing(Two, Clubs) => ascii_art::CARD_2_OF_CLUBS,
        Card::Playing(Two, Diamonds) => ascii_art::CARD_2_OF_DIAMONDS,
        Card::Playing(Two, Hearts) => ascii_art::CARD_2_OF_HEARTS,
        Card::Playing(Two, Spades) => ascii_art::CARD_2_OF_SPADES,
        Card::Playing(Three, Clubs) => ascii_art::CARD_3_OF_CLUBS,
        Card::Playing(Three, Diamonds) => ascii_art::CARD_3_OF_DIAMONDS,
        Card::Playing(Three, Hearts) => ascii_art::CARD_3_OF_HEARTS,
        Card::Playing(Three, Spades) => ascii_art::CARD_3_OF_SPADES,
        Card::Playing(Four, Clubs) => ascii_art::CARD_4_OF_CLUBS,
        Card::Playing(Four, Diamonds) => ascii_art::CARD_4_OF_DIAMONDS,
        Card::Playing(Four, Hearts) => ascii_art::CARD_4_OF_HEARTS,
        Card::Playing(Four, Spades) => ascii_art::CARD_4_OF_SPADES,
        Card::Playing(Five, Clubs) => ascii_art::CARD_5_OF_CLUBS,
        Card::Playing(Five, Diamonds) => ascii_art::CARD_5_OF_DIAMONDS,
        Card::Playing(Five, Hearts) => ascii_art::CARD_5_OF_HEARTS,
        Card::Playing(Five, Spades) => ascii_art::CARD_5_OF_SPADES,
        Card::Playing(Six, Clubs) => ascii_art::CARD_6_OF_CLUBS,
        Card::Playing(Six, Diamonds) => ascii_art::CARD_6_OF_DIAMONDS,
        Card::Playing(Six, Hearts) => ascii_art::CARD_6_OF_HEARTS,
        Card::Playing(Six, Spades) => ascii_art::CARD_6_OF_SPADES,
        Card::Playing(Seven, Clubs) => ascii_art::CARD_7_OF_CLUBS,
        Card::Playing(Seven, Diamonds) => ascii_art::CARD_7_OF_DIAMONDS,
        Card::Playing(Seven, Hearts) => ascii_art::CARD_7_OF_HEARTS,
        Card::Playing(Seven, Spades) => ascii_art::CARD_7_OF_SPADES,
        Card::Playing(Eight, Clubs) => ascii_art::CARD_8_OF_CLUBS,
        Card::Playing(Eight, Diamonds) => ascii_art::CARD_8_OF_DIAMONDS,
        Card::Playing(Eight, Hearts) => ascii_art::CARD_8_OF_HEARTS,
        Card::Playing(Eight, Spades) => ascii_art::CARD_8_OF_SPADES,
        Card::Playing(Nine, Clubs) => ascii_art::CARD_9_OF_CLUBS,
        Card::Playing(Nine, Diamonds) => ascii_art::CARD_9_OF_DIAMONDS,
        Card::Playing(Nine, Hearts) => ascii_art::CARD_9_OF_HEARTS,
        Card::Playing(Nine, Spades) => ascii_art::CARD_9_OF_SPADES,
        Card::Playing(Ten, Clubs) => ascii_art::CARD_10_OF_CLUBS,
        Card::Playing(Ten, Diamonds) => ascii_art::CARD_10_OF_DIAMONDS,
        Card::Playing(Ten, Hearts) => ascii_art::CARD_10_OF_HEARTS,
        Card::Playing(Ten, Spades) => ascii_art::CARD_10_OF_SPADES,
        Card::Playing(Jack, Clubs) => ascii_art::CARD_JACK_OF_CLUBS,
        Card::Playing(Jack, Diamonds) => ascii_art::CARD_JACK_OF_DIAMONDS,
        Card::Playing(Jack, Hearts) => ascii_art::CARD_JACK_OF_HEARTS,
        Card::Playing(Jack, Spades) => ascii_art::CARD_JACK_OF_SPADES,
        Card::Playing(Queen, Clubs) => ascii_art::CARD_QUEEN_OF_CLUBS,
        Card::Playing(Queen, Diamonds) => ascii_art::CARD_QUEEN_OF_DIAMONDS,
        Card::Playing(Queen, Hearts) => ascii_art::CARD_QUEEN_OF_HEARTS,
        Card::Playing(Queen, Spades) => ascii_art::CARD_QUEEN_OF_SPADES,
        Card::Playing(King, Clubs) => ascii_art::CARD_KING_OF_CLUBS,
        Card::Playing(King, Diamonds) => ascii_art::CARD_KING_OF_DIAMONDS,
        Card::Playing(King, Hearts) => ascii_art::CARD_KING_OF_HEARTS,
        Card::Playing(King, Spades) => ascii_art::CARD_KING_OF_SPADES,
        Card::Joker(JokerColor::Black) => ascii_art::CARD_BLACK_JOKER,
        Card::Joker(JokerColor::Red) => ascii_art::CARD_RED_JOKER,
    }
}

/// Return the SVG filesystem path for a card (for egui / leptos frontends).
pub fn card_svg_path(card: Card) -> &'static Path {
    use Rank::*;
    use Suit::*;
    Path::new(match card {
        Card::Playing(Ace, Clubs) => "assets/cards/Ace_of_clubs.svg",
        Card::Playing(Ace, Diamonds) => "assets/cards/Ace_of_diamonds.svg",
        Card::Playing(Ace, Hearts) => "assets/cards/Ace_of_hearts.svg",
        Card::Playing(Ace, Spades) => "assets/cards/Ace_of_spades.svg",
        Card::Playing(Two, Clubs) => "assets/cards/2_of_clubs.svg",
        Card::Playing(Two, Diamonds) => "assets/cards/2_of_diamonds.svg",
        Card::Playing(Two, Hearts) => "assets/cards/2_of_hearts.svg",
        Card::Playing(Two, Spades) => "assets/cards/2_of_spades.svg",
        Card::Playing(Three, Clubs) => "assets/cards/3_of_clubs.svg",
        Card::Playing(Three, Diamonds) => "assets/cards/3_of_diamonds.svg",
        Card::Playing(Three, Hearts) => "assets/cards/3_of_hearts.svg",
        Card::Playing(Three, Spades) => "assets/cards/3_of_spades.svg",
        Card::Playing(Four, Clubs) => "assets/cards/4_of_clubs.svg",
        Card::Playing(Four, Diamonds) => "assets/cards/4_of_diamonds.svg",
        Card::Playing(Four, Hearts) => "assets/cards/4_of_hearts.svg",
        Card::Playing(Four, Spades) => "assets/cards/4_of_spades.svg",
        Card::Playing(Five, Clubs) => "assets/cards/5_of_clubs.svg",
        Card::Playing(Five, Diamonds) => "assets/cards/5_of_diamonds.svg",
        Card::Playing(Five, Hearts) => "assets/cards/5_of_hearts.svg",
        Card::Playing(Five, Spades) => "assets/cards/5_of_spades.svg",
        Card::Playing(Six, Clubs) => "assets/cards/6_of_clubs.svg",
        Card::Playing(Six, Diamonds) => "assets/cards/6_of_diamonds.svg",
        Card::Playing(Six, Hearts) => "assets/cards/6_of_hearts.svg",
        Card::Playing(Six, Spades) => "assets/cards/6_of_spades.svg",
        Card::Playing(Seven, Clubs) => "assets/cards/7_of_clubs.svg",
        Card::Playing(Seven, Diamonds) => "assets/cards/7_of_diamonds.svg",
        Card::Playing(Seven, Hearts) => "assets/cards/7_of_hearts.svg",
        Card::Playing(Seven, Spades) => "assets/cards/7_of_spades.svg",
        Card::Playing(Eight, Clubs) => "assets/cards/8_of_clubs.svg",
        Card::Playing(Eight, Diamonds) => "assets/cards/8_of_diamonds.svg",
        Card::Playing(Eight, Hearts) => "assets/cards/8_of_hearts.svg",
        Card::Playing(Eight, Spades) => "assets/cards/8_of_spades.svg",
        Card::Playing(Nine, Clubs) => "assets/cards/9_of_clubs.svg",
        Card::Playing(Nine, Diamonds) => "assets/cards/9_of_diamonds.svg",
        Card::Playing(Nine, Hearts) => "assets/cards/9_of_hearts.svg",
        Card::Playing(Nine, Spades) => "assets/cards/9_of_spades.svg",
        Card::Playing(Ten, Clubs) => "assets/cards/10_of_clubs.svg",
        Card::Playing(Ten, Diamonds) => "assets/cards/10_of_diamonds.svg",
        Card::Playing(Ten, Hearts) => "assets/cards/10_of_hearts.svg",
        Card::Playing(Ten, Spades) => "assets/cards/10_of_spades.svg",
        Card::Playing(Jack, Clubs) => "assets/cards/Jack_of_clubs2.svg",
        Card::Playing(Jack, Diamonds) => "assets/cards/Jack_of_diamonds2.svg",
        Card::Playing(Jack, Hearts) => "assets/cards/Jack_of_hearts2.svg",
        Card::Playing(Jack, Spades) => "assets/cards/Jack_of_spades2.svg",
        Card::Playing(Queen, Clubs) => "assets/cards/Queen_of_clubs2.svg",
        Card::Playing(Queen, Diamonds) => "assets/cards/Queen_of_diamonds2.svg",
        Card::Playing(Queen, Hearts) => "assets/cards/Queen_of_hearts2.svg",
        Card::Playing(Queen, Spades) => "assets/cards/Queen_of_spades2.svg",
        Card::Playing(King, Clubs) => "assets/cards/King_of_clubs2.svg",
        Card::Playing(King, Diamonds) => "assets/cards/King_of_diamonds2.svg",
        Card::Playing(King, Hearts) => "assets/cards/King_of_hearts2.svg",
        Card::Playing(King, Spades) => "assets/cards/King_of_spades2.svg",
        Card::Joker(JokerColor::Black) => "assets/cards/Black_joker.svg",
        Card::Joker(JokerColor::Red) => "assets/cards/Red_joker.svg",
    })
}

// ── Blackjack card conversion ─────────────────────────────────────────────────

/// Convert a [`strictly_blackjack::Rank`] to the asset [`Rank`].
pub fn bj_rank(rank: strictly_blackjack::Rank) -> Rank {
    use strictly_blackjack::Rank as B;
    match rank {
        B::Ace => Rank::Ace,
        B::Two => Rank::Two,
        B::Three => Rank::Three,
        B::Four => Rank::Four,
        B::Five => Rank::Five,
        B::Six => Rank::Six,
        B::Seven => Rank::Seven,
        B::Eight => Rank::Eight,
        B::Nine => Rank::Nine,
        B::Ten => Rank::Ten,
        B::Jack => Rank::Jack,
        B::Queen => Rank::Queen,
        B::King => Rank::King,
    }
}

/// Convert a [`strictly_blackjack::Suit`] to the asset [`Suit`].
pub fn bj_suit(suit: strictly_blackjack::Suit) -> Suit {
    use strictly_blackjack::Suit as B;
    match suit {
        B::Hearts => Suit::Hearts,
        B::Diamonds => Suit::Diamonds,
        B::Clubs => Suit::Clubs,
        B::Spades => Suit::Spades,
    }
}

// ── Size-adaptive card art ────────────────────────────────────────────────────

/// Size tier for card art, chosen to fit the available column width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardSize {
    /// Large cards: 13 wide × 11 tall.
    Large,
    /// Medium cards: 9 wide × 7 tall (default).
    Medium,
    /// Small cards: 7 wide × 5 tall.
    Small,
}

impl CardSize {
    /// Outer width in columns for this size tier.
    pub fn width(self) -> usize {
        match self {
            Self::Large => 13,
            Self::Medium => 9,
            Self::Small => 7,
        }
    }

    /// Outer height in rows for this size tier.
    pub fn height(self) -> usize {
        match self {
            Self::Large => 11,
            Self::Medium => 7,
            Self::Small => 5,
        }
    }

    /// Choose the largest tier that fits both horizontally and vertically.
    ///
    /// Horizontal condition: `n * size.width() ≤ col_width`
    /// Vertical condition: `n_card_rows * (size.height() + 1) + WINDOW_CHROME ≤ viewport_height`
    ///
    /// Degrades through Large → Medium → Small until both conditions hold.
    /// `n_card_rows` is the number of horizontal card rows in the column
    /// (1 for the dealer row + 1 per player hand).
    /// `WINDOW_CHROME = 2` accounts for the top and bottom window bars.
    #[tracing::instrument]
    pub fn for_hand(
        n: usize,
        n_card_rows: usize,
        col_width: u16,
        viewport_height: u16,
    ) -> (Self, elicitation::contracts::Established<CardSizeFits>, elicitation::contracts::Established<CardHeightFits>) {
        const WINDOW_CHROME: usize = 2;
        let col = col_width as usize;
        let vp = viewport_height as usize;
        let n = n.max(1);
        let rows = n_card_rows.max(1);

        let h_fits = |s: Self| n * s.width() <= col;
        let v_fits = |s: Self| rows * (s.height() + 1) + WINDOW_CHROME <= vp;

        let size = if h_fits(Self::Large) && v_fits(Self::Large) {
            tracing::debug!(n, n_card_rows, col_width, viewport_height, tier = "Large",
                h_needed = n * Self::Large.width(), v_needed = rows * (Self::Large.height() + 1) + WINDOW_CHROME,
                "CardSize::for_hand: Large fits");
            Self::Large
        } else if h_fits(Self::Medium) && v_fits(Self::Medium) {
            tracing::debug!(n, n_card_rows, col_width, viewport_height, tier = "Medium",
                h_needed = n * Self::Medium.width(), v_needed = rows * (Self::Medium.height() + 1) + WINDOW_CHROME,
                "CardSize::for_hand: Medium fits");
            Self::Medium
        } else {
            tracing::debug!(n, n_card_rows, col_width, viewport_height, tier = "Small (fallback)",
                h_needed = n * Self::Small.width(), v_needed = rows * (Self::Small.height() + 1) + WINDOW_CHROME,
                "CardSize::for_hand: using Small");
            Self::Small
        };
        (
            size,
            elicitation::contracts::Established::assert(),
            elicitation::contracts::Established::assert(),
        )
    }
}

/// Proposition: a [`CardSize`] was chosen such that `n * size.width() ≤ col_width`.
#[derive(elicitation::Prop)]
pub struct CardSizeFits;

/// Proposition: a [`CardSize`] was chosen such that
/// `n_card_rows * (size.height() + 1) + window_chrome ≤ viewport_height`.
#[derive(elicitation::Prop)]
pub struct CardHeightFits;

/// Render ASCII art for one blackjack card at the given size, or a face-down
/// placeholder when `card` is `None`.
pub fn bj_card_ascii_sized(
    card: Option<(strictly_blackjack::Rank, strictly_blackjack::Suit)>,
    size: CardSize,
) -> String {
    let w = size.width();
    let h = size.height();
    match card {
        Some((rank, suit)) => {
            crate::assets::card_art::render_card(Card::Playing(bj_rank(rank), bj_suit(suit)), w, h)
        }
        None => crate::assets::card_art::render_card_back(w, h),
    }
}

/// ASCII art for a blackjack card at medium size, or a face-down placeholder.
///
/// Prefer [`bj_card_ascii_sized`] when the column width is known so the tier
/// is chosen to fit the available space.
pub fn bj_card_ascii(
    card: Option<(strictly_blackjack::Rank, strictly_blackjack::Suit)>,
) -> String {
    bj_card_ascii_sized(card, CardSize::Medium)
}

// ── Die lookups ───────────────────────────────────────────────────────────────

/// Return the pre-generated ASCII art for a die face.
///
/// Empty string until `generate-assets` has been run and the workspace recompiled.
pub fn die_ascii(die: Die) -> &'static str {
    match die {
        Die::One => ascii_art::DIE_1,
        Die::Two => ascii_art::DIE_2,
        Die::Three => ascii_art::DIE_3,
        Die::Four => ascii_art::DIE_4,
        Die::Five => ascii_art::DIE_5,
        Die::Six => ascii_art::DIE_6,
    }
}

/// Return the SVG filesystem path for a die face (for egui / leptos frontends).
pub fn die_svg_path(die: Die) -> &'static Path {
    Path::new(match die {
        Die::One => "assets/dice/Dice-1-b.svg",
        Die::Two => "assets/dice/Dice-2-b.svg",
        Die::Three => "assets/dice/Dice-3-b.svg",
        Die::Four => "assets/dice/Dice-4-b.svg",
        Die::Five => "assets/dice/Dice-5-b.svg",
        Die::Six => "assets/dice/Dice-6-b.svg",
    })
}
