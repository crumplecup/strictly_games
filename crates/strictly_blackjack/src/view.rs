//! Blackjack game state view for agent exploration.
//!
//! [`BlackjackPlayerView`] snapshots the visible game state during a
//! player's turn. Its [`ElicitSpec`] impl registers categories that map
//! 1:1 to the explore variants in [`BlackjackAction`](crate::BlackjackAction).

use elicitation::{
    ElicitSpec, SpecCategoryBuilder, SpecEntryBuilder, TypeSpec, TypeSpecBuilder,
    TypeSpecInventoryKey,
};
use tracing::instrument;

use crate::{GamePlayerTurn, Hand, HandValue, MultiRound};

/// Snapshot of visible blackjack state during a player's turn.
///
/// Built from live [`GamePlayerTurn`] state. Each field corresponds to
/// a TypeSpec category that agents can query via explore actions.
#[derive(Debug, Clone)]
pub struct BlackjackPlayerView {
    hand_descriptions: Vec<String>,
    current_hand_index: usize,
    dealer_up_card: String,
    other_hands: Vec<String>,
    shoe_remaining: usize,
    shoe_total: usize,
    bankroll: u64,
}

impl BlackjackPlayerView {
    /// Builds a view snapshot from live game state.
    #[instrument(skip(state))]
    pub fn from_game_state(state: &GamePlayerTurn, seat_index: usize, bankroll: u64) -> Self {
        let hands = state.player_hands();
        let hand_descriptions: Vec<String> = hands.iter().map(format_hand).collect();

        let dealer = state.dealer_hand();
        let dealer_up_card = if dealer.cards().is_empty() {
            "Unknown".to_string()
        } else {
            format!("{}", dealer.cards()[0])
        };

        let _ = seat_index;
        let other_hands = Vec::new();

        Self {
            hand_descriptions,
            current_hand_index: state.current_hand_index(),
            dealer_up_card,
            other_hands,
            shoe_remaining: state.shoe.remaining(),
            shoe_total: state.shoe.total(),
            bankroll,
        }
    }

    /// Builds a view snapshot from multi-player round state.
    ///
    /// Uses the shared [`MultiRound`] rather than the single-player
    /// [`GamePlayerTurn`], populating other players' visible cards from
    /// the round's seat list.
    #[instrument(skip(round))]
    pub fn from_multi_round(round: &MultiRound, seat_idx: usize, bankroll: u64) -> Self {
        let seat = &round.seats[seat_idx];
        let hand_descriptions = vec![format_hand(&seat.hand)];

        let dealer_up_card = if round.dealer_hand.cards().is_empty() {
            "Unknown".to_string()
        } else {
            format!("{}", round.dealer_hand.cards()[0])
        };

        let other_hands: Vec<String> = round
            .seats
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != seat_idx)
            .map(|(_, s)| format!("{}: {}", s.name, format_hand(&s.hand)))
            .collect();

        Self {
            hand_descriptions,
            current_hand_index: 0,
            dealer_up_card,
            other_hands,
            shoe_remaining: round.shoe.remaining(),
            shoe_total: round.shoe.total(),
            bankroll,
        }
    }

    /// Formats the response for a given explore category.
    #[instrument(skip(self))]
    pub fn describe_category(&self, category: &str) -> Option<String> {
        match category {
            "your_hand" => {
                let desc = if self.hand_descriptions.is_empty() {
                    "No cards dealt".to_string()
                } else if self.hand_descriptions.len() == 1 {
                    self.hand_descriptions[0].clone()
                } else {
                    self.hand_descriptions
                        .iter()
                        .enumerate()
                        .map(|(i, h)| {
                            let marker = if i == self.current_hand_index {
                                " ← acting"
                            } else {
                                ""
                            };
                            format!("Hand {}: {}{}", i + 1, h, marker)
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                Some(desc)
            }
            "dealer_showing" => Some(format!("Dealer shows: {}", self.dealer_up_card)),
            "other_players" => {
                if self.other_hands.is_empty() {
                    Some("No other players visible".to_string())
                } else {
                    Some(self.other_hands.join("\n"))
                }
            }
            "shoe_status" => Some(format!(
                "{} of {} cards remaining",
                self.shoe_remaining, self.shoe_total
            )),
            "bankroll" => Some(format!("Bankroll: ${}", self.bankroll)),
            _ => None,
        }
    }
}

/// Formats a hand for display: cards and value summary.
#[instrument(skip(hand))]
fn format_hand(hand: &Hand) -> String {
    let cards: Vec<String> = hand.cards().iter().map(|c| format!("{c}")).collect();
    let cards_str = cards.join(" ");
    let value = hand.value();

    let value_str = format_hand_value(&value);

    if hand.is_blackjack() {
        format!("{cards_str} — Blackjack!")
    } else if hand.is_bust() {
        format!("{cards_str} — {value_str} (BUST)")
    } else {
        format!("{cards_str} — {value_str}")
    }
}

/// Formats a hand value as "hard N" or "soft N/hard N".
#[instrument]
fn format_hand_value(value: &HandValue) -> String {
    match value.soft() {
        Some(soft) if value.is_soft() => format!("soft {}/hard {}", soft, value.hard()),
        _ => format!("hard {}", value.hard()),
    }
}

impl ElicitSpec for BlackjackPlayerView {
    fn type_spec() -> TypeSpec {
        let your_hand = SpecCategoryBuilder::default()
            .name("your_hand".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("cards".to_string())
                    .description("Cards in your hand with suit symbols".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("value".to_string())
                    .description("Hand total (hard/soft if applicable)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("status".to_string())
                    .description("Blackjack, bust, or can-split indicators".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        let dealer_showing = SpecCategoryBuilder::default()
            .name("dealer_showing".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("up_card".to_string())
                    .description("The dealer's single visible card".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        let other_players = SpecCategoryBuilder::default()
            .name("other_players".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("visible_cards".to_string())
                    .description("Other players' face-up cards and totals".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        let shoe_status = SpecCategoryBuilder::default()
            .name("shoe_status".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("remaining".to_string())
                    .description("Cards remaining in the shoe vs total".to_string())
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
            .type_name("BlackjackPlayerView".to_string())
            .summary(
                "Visible game state during a blackjack player turn — hand, dealer, shoe, bankroll"
                    .to_string(),
            )
            .categories(vec![
                your_hand,
                dealer_showing,
                other_players,
                shoe_status,
                bankroll,
            ])
            .build()
            .expect("valid TypeSpec")
    }
}

elicitation::inventory::submit!(TypeSpecInventoryKey::new(
    "BlackjackPlayerView",
    <BlackjackPlayerView as ElicitSpec>::type_spec,
    std::any::TypeId::of::<BlackjackPlayerView>
));
