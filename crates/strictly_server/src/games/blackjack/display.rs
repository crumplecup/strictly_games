//! AccessKit display implementation for the Blackjack [`BlackjackStateView`].

use accesskit::Role as AkRole;
use elicit_accesskit::{NodeId, NodeJson, Role};
use strictly_blackjack::BlackjackDisplayMode;
use tracing::instrument;

use crate::assets::{bj_card_ascii, CARD_BACK};
use crate::games::blackjack::BlackjackStateView;
use crate::games::display::GameDisplay;

// ── GameDisplay impl ──────────────────────────────────────────────────────────

impl GameDisplay for BlackjackStateView {
    type Mode = BlackjackDisplayMode;

    #[instrument(skip(self))]
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

                let dealer_card_ids =
                    hand_card_nodes(&self.dealer_hand, &mut ctr, &mut nodes, true);
                let dealer_row_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    dealer_row_id,
                    NodeJson::new(Role(AkRole::List))
                        .with_label("Dealer cards".to_string())
                        .with_children(dealer_card_ids),
                ));

                // ── Player hands ──────────────────────────────────────────────
                let player_label_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    player_label_id,
                    NodeJson::new(Role(AkRole::Paragraph))
                        .with_label("Your hand:".to_string()),
                ));

                let mut player_section_ids = vec![player_label_id];
                for (hand_idx, hand) in self.player_hands.iter().enumerate() {
                    let hand_cards: Vec<Option<_>> = hand.iter().map(|&c| Some(c)).collect();
                    let card_ids =
                        hand_card_nodes(&hand_cards, &mut ctr, &mut nodes, false);
                    let hand_row_id = NodeId::from(ctr);
                    ctr += 1;
                    nodes.push((
                        hand_row_id,
                        NodeJson::new(Role(AkRole::List))
                            .with_label(format!("Hand {}", hand_idx + 1))
                            .with_children(card_ids),
                    ));
                    player_section_ids.push(hand_row_id);
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

/// Build one [`NodeId`] per card in `hand`, pushing `(id, NodeJson)` pairs into
/// `nodes` and advancing `ctr`.  Each node is `Role::ListItem` whose label is
/// the pre-generated ASCII art (or the back-of-card placeholder for `None`).
fn hand_card_nodes(
    hand: &[Option<(strictly_blackjack::Rank, strictly_blackjack::Suit)>],
    ctr: &mut u64,
    nodes: &mut Vec<(NodeId, NodeJson)>,
    _is_dealer: bool,
) -> Vec<NodeId> {
    if hand.is_empty() {
        let placeholder_id = NodeId::from(*ctr);
        *ctr += 1;
        nodes.push((
            placeholder_id,
            NodeJson::new(Role(AkRole::ListItem)).with_label(CARD_BACK.to_string()),
        ));
        return vec![placeholder_id];
    }

    hand.iter()
        .map(|card| {
            let id = NodeId::from(*ctr);
            *ctr += 1;
            let art = bj_card_ascii(*card).to_string();
            nodes.push((
                id,
                NodeJson::new(Role(AkRole::ListItem)).with_label(art),
            ));
            id
        })
        .collect()
}
