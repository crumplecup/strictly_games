//! AccessKit display implementation for the craps [`CrapsStateView`].

use accesskit::Role as AkRole;
use elicit_accesskit::{NodeId, NodeJson, Role};
use strictly_craps::CrapsDisplayMode;
use tracing::instrument;

use crate::games::craps::CrapsStateView;
use crate::games::display::GameDisplay;

// ── GameDisplay impl ──────────────────────────────────────────────────────────

impl GameDisplay for CrapsStateView {
    type Mode = CrapsDisplayMode;

    #[instrument(skip(self))]
    fn to_ak_nodes(
        &self,
        mode: &CrapsDisplayMode,
        id_base: u64,
    ) -> (NodeId, Vec<(NodeId, NodeJson)>) {
        let mut nodes: Vec<(NodeId, NodeJson)> = Vec::new();
        let root_id = NodeId::from(id_base);
        let mut ctr = id_base + 1;

        match mode {
            CrapsDisplayMode::Table => {
                // Phase + bankroll header.
                let phase_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    phase_id,
                    NodeJson::new(Role(AkRole::Paragraph)).with_label(format!(
                        "Phase: {} — Bankroll: ${}",
                        self.phase, self.bankroll
                    )),
                ));

                // Description.
                let desc_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    desc_id,
                    NodeJson::new(Role(AkRole::Paragraph)).with_label(self.description.clone()),
                ));

                // Active bets list.
                let mut bet_ids: Vec<NodeId> = Vec::with_capacity(self.active_bets.len());
                for bet in &self.active_bets {
                    let bid = NodeId::from(ctr);
                    ctr += 1;
                    bet_ids.push(bid);
                    nodes.push((
                        bid,
                        NodeJson::new(Role(AkRole::ListItem)).with_label(bet.clone()),
                    ));
                }
                let bets_list_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    bets_list_id,
                    NodeJson::new(Role(AkRole::List))
                        .with_label(format!("Active bets ({})", self.active_bets.len()))
                        .with_children(bet_ids),
                ));

                // Optional dice roll and point.
                let mut table_children = vec![phase_id, desc_id, bets_list_id];

                if let Some(roll) = &self.dice_roll {
                    let roll_id = NodeId::from(ctr);
                    ctr += 1;
                    nodes.push((
                        roll_id,
                        NodeJson::new(Role(AkRole::Paragraph)).with_label(format!("Roll: {roll}")),
                    ));
                    table_children.push(roll_id);
                }

                if let Some(point) = &self.point {
                    let pt_id = NodeId::from(ctr);
                    ctr += 1;
                    nodes.push((
                        pt_id,
                        NodeJson::new(Role(AkRole::Paragraph))
                            .with_label(format!("Point: {point}")),
                    ));
                    table_children.push(pt_id);
                }

                nodes.push((
                    root_id,
                    NodeJson::new(Role(AkRole::Main))
                        .with_label("Craps — Table".to_string())
                        .with_children(table_children),
                ));
            }
            CrapsDisplayMode::Stats => {
                // Minimal stats card.
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
                        .with_label("Stats".to_string())
                        .with_children(vec![bankroll_id, status_id]),
                ));

                nodes.push((
                    root_id,
                    NodeJson::new(Role(AkRole::Main))
                        .with_label("Craps — Stats".to_string())
                        .with_children(vec![card_id]),
                ));
            }
        }

        let _ = ctr;
        (root_id, nodes)
    }
}
