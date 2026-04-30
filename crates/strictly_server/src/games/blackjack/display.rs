//! AccessKit display implementation for the Blackjack [`BlackjackStateView`].

use accesskit::Role as AkRole;
use elicit_accesskit::{NodeId, NodeJson, Role};
use strictly_blackjack::BlackjackDisplayMode;
use tracing::instrument;

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
                // Phase paragraph.
                let phase_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    phase_id,
                    NodeJson::new(Role(AkRole::Paragraph))
                        .with_label(format!("Phase: {}", self.phase)),
                ));

                // Bankroll paragraph.
                let bankroll_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    bankroll_id,
                    NodeJson::new(Role(AkRole::Paragraph))
                        .with_label(format!("Bankroll: ${}", self.bankroll)),
                ));

                // Description paragraph.
                let desc_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    desc_id,
                    NodeJson::new(Role(AkRole::Paragraph)).with_label(self.description.clone()),
                ));

                nodes.push((
                    root_id,
                    NodeJson::new(Role(AkRole::Main))
                        .with_label("Blackjack — Table".to_string())
                        .with_children(vec![phase_id, bankroll_id, desc_id]),
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
