//! AccessKit display implementation for the Blackjack [`BlackjackStateView`].

use accesskit::Role as AkRole;
use elicit_accesskit::{NodeId, NodeJson, Orientation, Role};
use strictly_blackjack::BlackjackDisplayMode;
use tracing::{debug, instrument};

use crate::assets::bj_card_ascii;
use crate::games::blackjack::BlackjackStateView;
use crate::games::display::GameDisplay;

// ── GameDisplay impl ──────────────────────────────────────────────────────────

impl GameDisplay for BlackjackStateView {
    type Mode = BlackjackDisplayMode;

    #[instrument(skip(self), fields(phase = %self.phase, player_hands = self.player_hands.len(), dealer_cards = self.dealer_hand.len()))]
    fn to_ak_nodes(
        &self,
        mode: &BlackjackDisplayMode,
        id_base: u64,
    ) -> (NodeId, Vec<(NodeId, NodeJson)>) {
        let mut nodes: Vec<(NodeId, NodeJson)> = Vec::new();
        let root_id = NodeId::from(id_base);
        let mut ctr = id_base + 1;

        match mode {
            BlackjackDisplayMode::Table => {
                // ── Status bar: phase + bankroll ─────────────────────────────
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
                    NodeJson::new(Role(AkRole::Paragraph))
                        .with_label("Dealer:".to_string()),
                ));

                let dealer_row_id = hand_card_nodes(&self.dealer_hand, &mut ctr, &mut nodes);

                // ── Player hands ──────────────────────────────────────────────
                let player_label_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    player_label_id,
                    NodeJson::new(Role(AkRole::Paragraph))
                        .with_label("Your hand:".to_string()),
                ));

                let mut player_section_ids = vec![player_label_id];
                for hand in self.player_hands.iter() {
                    let hand_cards: Vec<Option<_>> = hand.iter().map(|&c| Some(c)).collect();
                    let row_id = hand_card_nodes(&hand_cards, &mut ctr, &mut nodes);
                    player_section_ids.push(row_id);
                }

                // Fallback description when no hand data is available.
                if self.player_hands.is_empty() {
                    let desc_id = NodeId::from(ctr);
                    ctr += 1;
                    nodes.push((
                        desc_id,
                        NodeJson::new(Role(AkRole::Paragraph))
                            .with_label(self.description.clone()),
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
            }
            BlackjackDisplayMode::Scorecard => {
                // Compact single article: bankroll + terminal flag.
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
            }
        }

        let _ = ctr;
        (root_id, nodes)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a horizontal row of card art nodes for one hand.
///
/// Returns the ID of the row container.  Each card becomes a `Role::Paragraph`
/// leaf (multi-line ASCII art renders correctly there); the row container is a
/// `Role::Group` with `Orientation::Horizontal` so the bridge produces a
/// horizontal `TuiNode::Layout` instead of a list widget.
fn hand_card_nodes(
    hand: &[Option<(strictly_blackjack::Rank, strictly_blackjack::Suit)>],
    ctr: &mut u64,
    nodes: &mut Vec<(NodeId, NodeJson)>,
) -> NodeId {
    let cards: Vec<_> = if hand.is_empty() {
        // No cards yet — show a single face-down placeholder.
        vec![None]
    } else {
        hand.to_vec()
    };

    let mut card_ids = Vec::with_capacity(cards.len());
    for card in &cards {
        let id = NodeId::from(*ctr);
        *ctr += 1;
        let art = bj_card_ascii(*card).to_string();
        debug!(
            card = ?card,
            art_lines = art.lines().count(),
            art_max_w = art.lines().map(|l| l.len()).max().unwrap_or(0),
            art_empty = art.is_empty(),
            "hand_card_nodes: card art"
        );
        nodes.push((
            id,
            NodeJson::new(Role(AkRole::Paragraph)).with_label(art),
        ));
        card_ids.push(id);
    }

    // Horizontal container — children → TuiNode::Layout(Horizontal).
    let row_id = NodeId::from(*ctr);
    *ctr += 1;
    nodes.push((
        row_id,
        NodeJson::new(Role(AkRole::Group))
            .with_orientation(Orientation(accesskit::Orientation::Horizontal))
            .with_children(card_ids),
    ));
    row_id
}
