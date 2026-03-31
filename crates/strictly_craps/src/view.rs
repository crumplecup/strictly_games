//! Craps game state view for agent exploration.
//!
//! [`CrapsTableView`] snapshots the visible table state during betting.
//! Its [`ElicitSpec`] impl registers categories that map 1:1 to the
//! explore variants in [`CrapsAction`](crate::CrapsAction).

use elicitation::{
    ElicitSpec, SpecCategoryBuilder, SpecEntryBuilder, TypeSpec, TypeSpecBuilder,
    TypeSpecInventoryKey,
};
use tracing::instrument;

use crate::{ActiveBet, DiceRoll, Point};

/// Snapshot of visible craps table state during a betting decision.
#[derive(Debug, Clone)]
pub struct CrapsTableView {
    point: Option<Point>,
    player_bets: Vec<ActiveBet>,
    other_bets: Vec<(String, Vec<ActiveBet>)>,
    roll_history: Vec<DiceRoll>,
    bankroll: u64,
}

impl CrapsTableView {
    /// Builds a view from betting phase state (no point established).
    #[instrument]
    pub fn from_betting(bankroll: u64) -> Self {
        Self {
            point: None,
            player_bets: Vec::new(),
            other_bets: Vec::new(),
            roll_history: Vec::new(),
            bankroll,
        }
    }

    /// Builds a view from point phase state.
    #[instrument(skip(player_bets, other_bets, roll_history))]
    pub fn from_point_phase(
        point: Point,
        player_bets: Vec<ActiveBet>,
        other_bets: Vec<(String, Vec<ActiveBet>)>,
        roll_history: Vec<DiceRoll>,
        bankroll: u64,
    ) -> Self {
        Self {
            point: Some(point),
            player_bets,
            other_bets,
            roll_history,
            bankroll,
        }
    }

    /// Formats the response for a given explore category.
    #[instrument(skip(self))]
    pub fn describe_category(&self, category: &str) -> Option<String> {
        match category {
            "point" => {
                let desc = match self.point {
                    Some(pt) => format!("Point is {}", pt),
                    None => "No point established (come-out roll)".to_string(),
                };
                Some(desc)
            }
            "active_bets" => {
                if self.player_bets.is_empty() {
                    Some("No active bets".to_string())
                } else {
                    let lines: Vec<String> =
                        self.player_bets.iter().map(|b| format!("  {b}")).collect();
                    Some(format!("Your bets:\n{}", lines.join("\n")))
                }
            }
            "other_bets" => {
                if self.other_bets.is_empty() {
                    Some("No other players' bets visible".to_string())
                } else {
                    let lines: Vec<String> = self
                        .other_bets
                        .iter()
                        .map(|(name, bets)| {
                            let bet_strs: Vec<String> =
                                bets.iter().map(|b| format!("{b}")).collect();
                            format!("  {}: {}", name, bet_strs.join(", "))
                        })
                        .collect();
                    Some(format!("Other bets:\n{}", lines.join("\n")))
                }
            }
            "roll_history" => {
                if self.roll_history.is_empty() {
                    Some("No rolls yet this round".to_string())
                } else {
                    let recent: Vec<String> = self
                        .roll_history
                        .iter()
                        .rev()
                        .take(5)
                        .map(|r| format!("{r}"))
                        .collect();
                    Some(format!(
                        "Last {} rolls: {}",
                        recent.len(),
                        recent.join(", ")
                    ))
                }
            }
            "bankroll" => Some(format!("Bankroll: ${}", self.bankroll)),
            _ => None,
        }
    }
}

impl ElicitSpec for CrapsTableView {
    fn type_spec() -> TypeSpec {
        let point = SpecCategoryBuilder::default()
            .name("point".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("value".to_string())
                    .description("Current point number (4-10) or come-out status".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        let active_bets = SpecCategoryBuilder::default()
            .name("active_bets".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("bets".to_string())
                    .description("Your bets on the table with type and amount".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        let other_bets = SpecCategoryBuilder::default()
            .name("other_bets".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("visible".to_string())
                    .description("Other players' bets visible on the table".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        let roll_history = SpecCategoryBuilder::default()
            .name("roll_history".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("recent_rolls".to_string())
                    .description("Last 5 dice rolls with sums".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        let bankroll = SpecCategoryBuilder::default()
            .name("bankroll".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("chips".to_string())
                    .description("Current chip count".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        TypeSpecBuilder::default()
            .type_name("CrapsTableView".to_string())
            .summary(
                "Visible table state during craps — point, bets, roll history, bankroll"
                    .to_string(),
            )
            .categories(vec![point, active_bets, other_bets, roll_history, bankroll])
            .build()
            .expect("valid TypeSpec")
    }
}

elicitation::inventory::submit!(TypeSpecInventoryKey::new(
    "CrapsTableView",
    <CrapsTableView as ElicitSpec>::type_spec,
    std::any::TypeId::of::<CrapsTableView>
));
