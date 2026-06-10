//! AccessKit display implementation for the Blackjack [`BlackjackStateView`].

use accesskit::Role as AkRole;
use elicit_accesskit::{NodeId, NodeJson, Orientation, Role};
use elicitation::contracts::Established;
use strictly_blackjack::BlackjackDisplayMode;
use tracing::{debug, instrument};

use crate::assets::Card;
use crate::assets::{
    CardHeightFits, CardSize, CardSizeFits, bj_card_ascii_sized, bj_rank, bj_suit,
    card_absolute_path, card_svg_path,
};
use crate::games::blackjack::BlackjackStateView;
use crate::games::display::GameDisplay;
use crate::tui::contracts::CardDisplayBuilt;

// ── GameDisplay impl ──────────────────────────────────────────────────────────

impl GameDisplay for BlackjackStateView {
    type Mode = BlackjackDisplayMode;

    #[instrument(skip(self), fields(mode = ?mode, phase = %self.phase, player_hands = self.player_hands.len(), dealer_cards = self.dealer_hand.len()))]
    fn to_ak_nodes(
        &self,
        mode: &BlackjackDisplayMode,
        id_base: u64,
    ) -> (NodeId, Vec<(NodeId, NodeJson)>) {
        // Conservative fallback — callers with actual layout dimensions should
        // use to_ak_nodes_bj directly.
        let (root, nodes, _proof) = self.to_ak_nodes_bj(mode, id_base, 80, 50);
        (root, nodes)
    }
}

impl BlackjackStateView {
    /// Build the AccessKit IR for this view, choosing card art that fits within
    /// `col_width` columns.  Returns the root id, the node list, and a proof
    /// that the display was built with verified card geometry.
    #[instrument(skip(self), fields(mode = ?mode, id_base, col_width, viewport_height, phase = %self.phase, player_hands = self.player_hands.len(), dealer_cards = self.dealer_hand.len()))]
    pub fn to_ak_nodes_bj(
        &self,
        mode: &BlackjackDisplayMode,
        id_base: u64,
        col_width: u16,
        viewport_height: u16,
    ) -> (
        NodeId,
        Vec<(NodeId, NodeJson)>,
        Established<CardDisplayBuilt>,
    ) {
        let mut nodes: Vec<(NodeId, NodeJson)> = Vec::new();
        let root_id = NodeId::from(id_base);
        let mut ctr = id_base + 1;

        let display_proof = match mode {
            BlackjackDisplayMode::Table => {
                // Determine max cards across all hands for size selection.
                let max_cards = self
                    .player_hands
                    .iter()
                    .map(|h| h.len())
                    .max()
                    .unwrap_or(0)
                    .max(self.dealer_hand.len())
                    .max(1);

                // n_card_rows = 1 dealer row + 1 row per player hand (min 1).
                let n_card_rows = 1 + self.player_hands.len().max(1);

                let (size, size_proof, height_proof) =
                    CardSize::for_hand(max_cards, n_card_rows, col_width, viewport_height);

                // ── Status bar ────────────────────────────────────────────────
                let status_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    status_id,
                    NodeJson::new(Role(AkRole::Paragraph)).with_label(format!(
                        "Phase: {}  │  Bankroll: ${}",
                        self.phase, self.bankroll
                    )),
                ));

                // ── Dealer hand ───────────────────────────────────────────────
                let dealer_label_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    dealer_label_id,
                    NodeJson::new(Role(AkRole::Paragraph)).with_label("Dealer:".to_string()),
                ));
                let dealer_row_id = hand_card_nodes(
                    &self.dealer_hand,
                    size,
                    &size_proof,
                    &height_proof,
                    &mut ctr,
                    &mut nodes,
                );

                // ── Player hands ──────────────────────────────────────────────
                let player_label_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    player_label_id,
                    NodeJson::new(Role(AkRole::Paragraph)).with_label("Your hand:".to_string()),
                ));

                let mut player_section_ids = vec![player_label_id];
                for hand in self.player_hands.iter() {
                    let hand_cards: Vec<Option<_>> = hand.iter().map(|&c| Some(c)).collect();
                    let row_id = hand_card_nodes(
                        &hand_cards,
                        size,
                        &size_proof,
                        &height_proof,
                        &mut ctr,
                        &mut nodes,
                    );
                    player_section_ids.push(row_id);
                }

                // Always show the description — during player_turn it contains
                // hand totals ("Your hand: J♣+4♣ (value: 14)\nDealer shows: J♦")
                // which are the primary way the player reads card values.
                if !self.description.is_empty() {
                    let desc_id = NodeId::from(ctr);
                    ctr += 1;
                    nodes.push((
                        desc_id,
                        NodeJson::new(Role(AkRole::Paragraph)).with_label(self.description.clone()),
                    ));
                    player_section_ids.push(desc_id);
                }

                let mut root_children = vec![status_id, dealer_label_id, dealer_row_id];
                root_children.extend(player_section_ids);
                nodes.push((
                    root_id,
                    NodeJson::new(Role(AkRole::Main))
                        .with_label("Blackjack — Table".to_string())
                        .with_children(root_children),
                ));

                use elicitation::contracts::both;
                Established::prove(&both(size_proof, height_proof))
            }

            BlackjackDisplayMode::Scorecard => {
                let bankroll_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    bankroll_id,
                    NodeJson::new(Role(AkRole::Paragraph))
                        .with_label(format!("Bankroll: ${}", self.bankroll)),
                ));

                let status_id = NodeId::from(ctr);
                ctr += 1;
                let status_text = if self.is_terminal {
                    "Session ended".to_string()
                } else {
                    format!("Phase: {}", self.phase)
                };
                nodes.push((
                    status_id,
                    NodeJson::new(Role(AkRole::Paragraph)).with_label(status_text),
                ));

                let card_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    card_id,
                    NodeJson::new(Role(AkRole::Article))
                        .with_label("Scorecard".to_string())
                        .with_children(vec![bankroll_id, status_id]),
                ));

                nodes.push((
                    root_id,
                    NodeJson::new(Role(AkRole::Main))
                        .with_label("Blackjack — Scorecard".to_string())
                        .with_children(vec![card_id]),
                ));

                // Scorecard has no card art — mint proof via trivial size check.
                let (_, size_proof, height_proof) =
                    CardSize::for_hand(1, 1, col_width, viewport_height);
                use elicitation::contracts::both;
                Established::prove(&both(size_proof, height_proof))
            }
        };

        let _ = ctr;
        (root_id, nodes, display_proof)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a horizontal row of card art nodes for one hand at the given size.
///
/// Requires `&Established<CardSizeFits>` — callers must have verified that
/// `size` fits the column width before building the IR.  Returns the row
/// container ID.
#[instrument(skip(nodes, _size_proof, _height_proof), fields(n_cards = hand.len(), size = ?size))]
fn hand_card_nodes(
    hand: &[Option<(strictly_blackjack::Rank, strictly_blackjack::Suit)>],
    size: CardSize,
    _size_proof: &Established<CardSizeFits>,
    _height_proof: &Established<CardHeightFits>,
    ctr: &mut u64,
    nodes: &mut Vec<(NodeId, NodeJson)>,
) -> NodeId {
    let cards: Vec<_> = if hand.is_empty() {
        vec![None]
    } else {
        hand.to_vec()
    };

    let mut card_ids = Vec::with_capacity(cards.len());
    for card in &cards {
        let id = NodeId::from(*ctr);
        *ctr += 1;
        // ASCII art goes in the label — TUI bridge renders it as monospace.
        // SVG/PNG path goes in the URL — visual bridges (egui, leptos) load it.
        let art = bj_card_ascii_sized(*card, size);
        let svg_url = match card {
            Some((rank, suit)) => {
                let rel = card_svg_path(Card::Playing(bj_rank(*rank), bj_suit(*suit)));
                card_absolute_path(rel.to_str().unwrap_or(""))
                    .to_string_lossy()
                    .into_owned()
            }
            None => card_absolute_path("assets/cards/USPCC_card_backs_Bicycle_Rider.png")
                .to_string_lossy()
                .into_owned(),
        };
        debug!(
            card = ?card,
            url = %svg_url,
            art_lines = art.lines().count(),
            size = ?size,
            "hand_card_nodes card art"
        );
        let font_path =
            crate::assets::card_absolute_path("assets/fonts/LiberationSans-Regular.ttf")
                .to_string_lossy()
                .into_owned();
        nodes.push((
            id,
            NodeJson::new(Role(AkRole::Image))
                .with_url(svg_url)
                .with_label(art)
                .with_font_family("Liberation Sans".to_string())
                .with_font_path(font_path),
        ));
        card_ids.push(id);
    }

    let art_height = size.height() as f64;
    let row_id = NodeId::from(*ctr);
    *ctr += 1;
    nodes.push((
        row_id,
        NodeJson::new(Role(AkRole::Group))
            .with_orientation(Orientation(accesskit::Orientation::Horizontal))
            .with_numeric_value(art_height + 1.0)
            .with_children(card_ids),
    ));
    row_id
}
