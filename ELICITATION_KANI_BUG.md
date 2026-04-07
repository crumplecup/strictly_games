# Bug: `kani_newtype_wrapper_harness` generates vacuous proof for multi-field structs

**Repo:** `elicitation`  
**Files:**
- `crates/elicitation/src/verification/proof_helpers.rs` — `kani_newtype_wrapper_harness()`
- `crates/elicitation_derive/src/struct_impl.rs` — `generate_elicit_impl_simple()` / `generate_elicit_impl_styled()`

---

## What happens today

When `#[derive(Elicit)]` is applied to a **multi-field struct**, the macro calls
`kani_newtype_wrapper_harness(type_name_str)` to produce the struct's own Kani harness.
That function always emits:

```rust
#[cfg_attr(kani, ::kani::proof)]
fn verify_TYPENAME_newtype_wrapper() {
    let established: bool = true;
    assert!(established, "TYPENAME newtype wrapper structural proof");
}
```

`assert!(true)` is a tautology. Kani verifies it instantly and it proves nothing.

### Concrete example — `Board`

`strictly_tictactoe::Board` has one field: `squares: [Square; 9]`.
Its `kani_proof()` emits the vacuous wrapper harness above **plus** `Square::kani_proof()`
(which proves `Square` is constructible as an enum). The field-type recursion is correct.
The wrapper harness itself is useless.

Same issue for `Move`, `GameSetup`, `GameInProgress`, `GameFinished`, and any
`#[derive(Elicit)]` struct in any downstream crate.

---

## Root cause

`kani_newtype_wrapper_harness` accepts only a `&str` (the type name for display / function
naming). At `proof_helpers.rs` call time there is no Rust type information — no path, no
`TokenStream` referring to the actual type — so the function cannot emit code that
**constructs** the struct.

The derive macro (`struct_impl.rs`) *does* have the type `Ident` (`#name`) and the field
types (`#elicited_types`) in scope when it calls `kani_newtype_wrapper_harness`, but it
only passes the string:

```rust
// struct_impl.rs ~line 1012
fn kani_proof() -> elicitation::proc_macro2::TokenStream {
    let mut ts = elicitation::verification::proof_helpers::kani_newtype_wrapper_harness(
        #wrapper_name_str  // ← only the string "Board", not the path `Board`
    );
    #(ts.extend(<#elicited_types as elicitation::Elicitation>::kani_proof());)*
    ts
}
```

Because the type path is never threaded through, `kani_newtype_wrapper_harness` cannot
generate construction code.

---

## What the harness *should* prove

For a struct that derives `kani::Arbitrary` (which all game structs do under
`#[cfg_attr(kani, derive(kani::Arbitrary))]`), the correct harness is:

```rust
#[kani::proof]
fn verify_board_newtype_wrapper() {
    // Proves: Board is constructible from arbitrary valid field combinations.
    let b: Board = kani::any();
    // Optionally assert field-level invariants here if the macro can see them.
    let _ = b;
}
```

For a true zero-field marker struct (`struct BetDeducted;`), the existing
`assert!(true)` body is **correct** — there are no fields to exercise and the
type's only property is existence, which the type system already guarantees.
The bug is only triggered for structs with one or more elicitable fields.

---

## Proposed fix

### Option A — pass the type path into `kani_newtype_wrapper_harness`

Add a second variant / new function:

```rust
// proof_helpers.rs
pub fn kani_struct_constructible_harness(wrapper_name: &str, type_path: TokenStream) -> TokenStream {
    let fn_ident = /* verify_TYPENAME_newtype_wrapper as before */;
    quote! {
        #[cfg_attr(kani, ::kani::proof)]
        fn #fn_ident() {
            let _: #type_path = kani::any();
        }
    }
}
```

Then in `struct_impl.rs`, when the struct has at least one elicitable field:

```rust
fn kani_proof() -> elicitation::proc_macro2::TokenStream {
    // Use the richer helper that constructs via kani::any()
    let mut ts = elicitation::verification::proof_helpers::kani_struct_constructible_harness(
        #wrapper_name_str,
        elicitation::proc_macro2::TokenStream::from(
            elicitation::quote::quote! { #name }
        ),
    );
    #(ts.extend(<#elicited_types as elicitation::Elicitation>::kani_proof());)*
    ts
}
```

### Option B — generate the harness body inline in the macro

Skip the `proof_helpers` detour entirely for structs. The `struct_impl.rs` `quote!`
already has `#name` and `#elicited_types`; it can emit the `kani::any()` body directly:

```rust
// struct_impl.rs — proof_methods block
fn kani_proof() -> elicitation::proc_macro2::TokenStream {
    let fn_name = elicitation::quote::format_ident!(
        "verify_{}_newtype_wrapper",
        #wrapper_name_str.to_lowercase()
    );
    let mut ts = elicitation::quote::quote! {
        #[cfg_attr(kani, ::kani::proof)]
        fn #fn_name() {
            let _: #name = kani::any();
        }
    }.to_string();
    let mut ts: elicitation::proc_macro2::TokenStream = ts.parse().unwrap();
    #(ts.extend(<#elicited_types as elicitation::Elicitation>::kani_proof());)*
    ts
}
```

Option B is simpler but mixes code-gen concern with the macro; Option A keeps `proof_helpers`
as the canonical source of harness templates.

---

## Discriminating marker structs from data structs

The fix should only emit `kani::any()` construction when the struct has ≥ 1 elicitable
field. The existing code already has this information (`elicited_fields.is_empty()`).
Zero-field structs (markers) should keep `assert!(true)` — it is semantically correct for them.

---

## Impact

Every `#[derive(Elicit)]` struct in every downstream crate currently emits a provably-useless
`verify_*_newtype_wrapper` harness. For the `strictly_games` showcase specifically:
`Board`, `Move`, `GameSetup`, `GameInProgress`, `GameFinished`, `BankrollLedger`, `Card`.
All 7 foundation struct harnesses reduce to tautologies today.
