//! [`ElicitComplete`] registrations for tic-tac-toe types.
//!
//! Writing `impl ElicitComplete for T {}` is the compiler-enforced stamp
//! that a type has satisfied every framework obligation:
//! `Elicitation + ElicitIntrospect + ElicitSpec + Serialize + Deserialize + JsonSchema`.
//!
//! For struct types, `#[derive(Elicit)]` generates `Elicitation`, `ElicitIntrospect`,
//! and `ElicitSpec` automatically. For enum types, `#[derive(Elicit)]` generates
//! `Elicitation` and `ElicitIntrospect` but not `ElicitSpec` — that requires a
//! manual impl with a `TypeSpec` describing the enum's variants and contracts.
//!
//! The `impl ElicitComplete for T {}` line is the final registration step — if
//! any required trait were missing, the compiler would reject it here with a
//! precise error pointing at exactly what is still missing.

use elicitation::{
    ElicitComplete, ElicitSpec, SpecCategoryBuilder, SpecEntryBuilder, TypeSpec, TypeSpecBuilder,
    TypeSpecInventoryKey,
};

use crate::{Board, Player, Position, Square};

// ── ElicitSpec for enum types ─────────────────────────────────────────────────
//
// `#[derive(Elicit)]` generates ElicitSpec for structs but not for enums.
// We provide manual impls here, describing each enum's variants and their
// game-domain meaning for agent-browsable specs.

impl ElicitSpec for Player {
    fn type_spec() -> TypeSpec {
        let variants = SpecCategoryBuilder::default()
            .name("variants".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("X".to_string())
                    .description("Player X — moves first".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("O".to_string())
                    .description("Player O — moves second".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");
        TypeSpecBuilder::default()
            .type_name("Player".to_string())
            .summary("A tic-tac-toe player (X or O)".to_string())
            .categories(vec![variants])
            .build()
            .expect("valid TypeSpec")
    }
}

elicitation::inventory::submit!(TypeSpecInventoryKey::new(
    "Player",
    <Player as ElicitSpec>::type_spec,
    std::any::TypeId::of::<Player>
));

impl ElicitSpec for Square {
    fn type_spec() -> TypeSpec {
        let variants = SpecCategoryBuilder::default()
            .name("variants".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("Empty".to_string())
                    .description("An unoccupied square — valid move target".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("Occupied(Player)".to_string())
                    .description(
                        "A square claimed by the given player — not a valid move target"
                            .to_string(),
                    )
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");
        TypeSpecBuilder::default()
            .type_name("Square".to_string())
            .summary("The state of a single cell on the tic-tac-toe board".to_string())
            .categories(vec![variants])
            .build()
            .expect("valid TypeSpec")
    }
}

elicitation::inventory::submit!(TypeSpecInventoryKey::new(
    "Square",
    <Square as ElicitSpec>::type_spec,
    std::any::TypeId::of::<Square>
));

impl ElicitSpec for Position {
    fn type_spec() -> TypeSpec {
        let variants = SpecCategoryBuilder::default()
            .name("variants".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("TopLeft".to_string())
                    .description("Row 0, column 0 (index 0)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("TopCenter".to_string())
                    .description("Row 0, column 1 (index 1)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("TopRight".to_string())
                    .description("Row 0, column 2 (index 2)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("MiddleLeft".to_string())
                    .description("Row 1, column 0 (index 3)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("Center".to_string())
                    .description("Row 1, column 1 (index 4)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("MiddleRight".to_string())
                    .description("Row 1, column 2 (index 5)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("BottomLeft".to_string())
                    .description("Row 2, column 0 (index 6)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("BottomCenter".to_string())
                    .description("Row 2, column 1 (index 7)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("BottomRight".to_string())
                    .description("Row 2, column 2 (index 8)".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");
        TypeSpecBuilder::default()
            .type_name("Position".to_string())
            .summary("One of nine cells on the tic-tac-toe board, in row-major order".to_string())
            .categories(vec![variants])
            .build()
            .expect("valid TypeSpec")
    }
}

elicitation::inventory::submit!(TypeSpecInventoryKey::new(
    "Position",
    <Position as ElicitSpec>::type_spec,
    std::any::TypeId::of::<Position>
));

// ── ElicitComplete registrations ─────────────────────────────────────────────
//
// The compiler accepts these impls only when every supertrait is satisfied.
// Board gets ElicitSpec from #[derive(Elicit)]; the enums use the manual impls above.

impl ElicitComplete for Player {}
impl ElicitComplete for Square {}
impl ElicitComplete for Board {}
impl ElicitComplete for Position {}
