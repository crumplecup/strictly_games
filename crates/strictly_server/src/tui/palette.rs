//! WCAG-verified color palette for the TUI.
//!
//! Every foreground/background pair is checked at construction time against
//! WCAG 2.1 contrast ratios using [`elicit_ui::contrast_ratio`].  The palette
//! assumes a dark terminal background (≈ #1e1e1e) and targets AA-normal
//! (4.5:1) for body text and AAA-normal (7:1) for high-emphasis text.

use elicit_ratatui::ColorJson;
use elicit_ui::{SrgbColor, contrast_ratio};
use tracing::instrument;

/// Terminal background assumed for contrast checks (dark theme ≈ #1e1e1e).
const BG: SrgbColor = SrgbColor {
    r: 0.118,
    g: 0.118,
    b: 0.118,
};

/// Minimum contrast ratio for AA-normal text (WCAG 1.4.3).
const AA_NORMAL: f32 = 4.5;

/// Minimum contrast ratio for AAA-normal text (WCAG 1.4.6).
const AAA_NORMAL: f32 = 7.0;

/// Minimum contrast ratio for non-text UI components (WCAG 1.4.11).
const NON_TEXT: f32 = 3.0;

/// A WCAG-verified color entry: its pre-computed [`ColorJson`] for embedding
/// in [`WidgetJson`] trees.  The contrast check happens at construction time
/// inside [`verified`]; only the ready-to-use JSON value is retained.
#[derive(Debug, Clone)]
pub struct VerifiedColor {
    /// Ready-to-embed JSON color for elicit_ratatui widgets.
    pub json: ColorJson,
}

/// Converts an [`SrgbColor`] to a [`ColorJson::Rgb`] value.
#[instrument(skip_all)]
fn srgb_to_json(c: &SrgbColor) -> ColorJson {
    ColorJson::Rgb {
        r: (c.r * 255.0).round() as u8,
        g: (c.g * 255.0).round() as u8,
        b: (c.b * 255.0).round() as u8,
    }
}

/// Creates a [`VerifiedColor`], panicking if it does not meet `min_ratio`.
#[instrument(skip_all, fields(r, g, b, min_ratio))]
fn verified(r: u8, g: u8, b: u8, min_ratio: f32) -> VerifiedColor {
    let srgb = SrgbColor::from_u8(r, g, b);
    let ratio = contrast_ratio(&srgb, &BG);
    assert!(
        ratio >= min_ratio,
        "Color rgb({r},{g},{b}) has contrast {ratio:.2}:1, need {min_ratio:.1}:1"
    );
    VerifiedColor {
        json: srgb_to_json(&srgb),
    }
}

/// The complete WCAG-verified palette for Strictly Games TUI.
///
/// All colors are verified against a dark terminal background at
/// construction time; if any color fails its contrast requirement
/// the palette constructor panics (fail-fast in debug, impossible
/// in release once constants are locked).
#[derive(Debug, Clone)]
pub struct GamePalette {
    // --- High-emphasis text (AAA 7:1) ---
    /// Bright white — titles, status text.
    pub title: VerifiedColor,
    /// Bright cyan — board highlights, active elements.
    pub highlight: VerifiedColor,

    // --- Normal text (AA 4.5:1) ---
    /// Soft green — success, game-over positive.
    pub success: VerifiedColor,
    /// Warm amber — warnings, agent turn indicator.
    pub warning: VerifiedColor,
    /// Coral red — errors, urgent status.
    pub error: VerifiedColor,
    /// Light grey — body text, event log.
    pub body: VerifiedColor,
    /// Muted lavender — proof/verification annotations.
    pub proof: VerifiedColor,
    /// Soft blue — host/server messages in chat.
    pub host: VerifiedColor,
    /// Peach — agent messages in chat.
    pub agent: VerifiedColor,

    // --- Non-text UI (3:1) ---
    /// Medium grey — borders, separators, inactive elements.
    pub border: VerifiedColor,
    /// Dim grey — faded/old event log entries.
    pub muted: VerifiedColor,
}

impl GamePalette {
    /// Builds the verified palette.  Panics if any color fails its
    /// WCAG contrast requirement — this is intentional to catch palette
    /// regressions at startup.
    #[instrument]
    pub fn new() -> Self {
        Self {
            // AAA 7:1 — high emphasis
            title: verified(255, 255, 255, AAA_NORMAL), // #FFFFFF
            highlight: verified(80, 220, 240, AAA_NORMAL), // #50DCF0

            // AA 4.5:1 — normal text
            success: verified(100, 220, 100, AA_NORMAL), // #64DC64
            warning: verified(240, 200, 80, AA_NORMAL),  // #F0C850
            error: verified(255, 110, 110, AA_NORMAL),   // #FF6E6E
            body: verified(200, 200, 200, AA_NORMAL),    // #C8C8C8
            proof: verified(180, 160, 220, AA_NORMAL),   // #B4A0DC
            host: verified(100, 180, 255, AA_NORMAL),    // #64B4FF
            agent: verified(255, 180, 140, AA_NORMAL),   // #FFB48C

            // Non-text 3:1
            border: verified(140, 140, 140, NON_TEXT), // #8C8C8C
            muted: verified(120, 120, 120, NON_TEXT),  // #787878
        }
    }
}

impl Default for GamePalette {
    fn default() -> Self {
        Self::new()
    }
}
