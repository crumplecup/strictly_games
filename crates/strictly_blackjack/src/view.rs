//! Blackjack game state view for agent exploration.
//!
//! [`BlackjackPlayerView`] snapshots the visible game state during a
//! player's turn. Its [`ElicitSpec`] impl registers categories that map
//! 1:1 to the explore variants in [`BlackjackAction`](crate::BlackjackAction).

use elicitation::{ElicitSpec, SpecCategory, SpecEntry, TypeSpec, TypeSpecInventoryKey};
#[cfg(not(kani))]
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
    #[cfg_attr(not(kani), instrument(skip(state)))]
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
    #[cfg_attr(not(kani), instrument(skip(round)))]
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
    #[cfg_attr(not(kani), instrument(skip(self)))]
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
#[cfg_attr(not(kani), instrument(skip(hand)))]
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
#[cfg_attr(not(kani), instrument)]
fn format_hand_value(value: &HandValue) -> String {
    match value.soft() {
        Some(soft) if value.is_soft() => format!("soft {}/hard {}", soft, value.hard()),
        _ => format!("hard {}", value.hard()),
    }
}

impl ElicitSpec for BlackjackPlayerView {
    fn type_spec() -> TypeSpec {
        let your_hand = SpecCategory::new(
            "your_hand",
            vec![
                SpecEntry::new("cards", "Cards in your hand with suit symbols"),
                SpecEntry::new("value", "Hand total (hard/soft if applicable)"),
                SpecEntry::new("status", "Blackjack, bust, or can-split indicators"),
            ],
        );

        let dealer_showing = SpecCategory::new(
            "dealer_showing",
            vec![SpecEntry::new(
                "up_card",
                "The dealer's single visible card",
            )],
        );

        let other_players = SpecCategory::new(
            "other_players",
            vec![SpecEntry::new(
                "visible_cards",
                "Other players' face-up cards and totals",
            )],
        );

        let shoe_status = SpecCategory::new(
            "shoe_status",
            vec![SpecEntry::new(
                "remaining",
                "Cards remaining in the shoe vs total",
            )],
        );

        let bankroll = SpecCategory::new(
            "bankroll",
            vec![SpecEntry::new("chips", "Current chip count")],
        );

        TypeSpec::new(
            "BlackjackPlayerView",
            "Visible game state during a blackjack player turn — hand, dealer, shoe, bankroll",
            vec![
                your_hand,
                dealer_showing,
                other_players,
                shoe_status,
                bankroll,
            ],
        )
    }
}

elicitation::inventory::submit!(TypeSpecInventoryKey::new(
    "BlackjackPlayerView",
    <BlackjackPlayerView as ElicitSpec>::type_spec,
    std::any::TypeId::of::<BlackjackPlayerView>
));
