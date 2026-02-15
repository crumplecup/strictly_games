# Code Standards Audit

**Date:** 2026-02-14  
**Status:** 🟡 Partially Compliant (Phase 1 & 2 partial complete)  
**Standards Reference:** CLAUDE.md

## Executive Summary

The codebase is **progressing toward full compliance**. Significant improvements made:

- **~146 of 146 functions** (100%) have `#[instrument]` - ✅ **COMPLETE**
- **2 of 2 error types** use `derive_more` pattern - ✅ **COMPLETE**
- **No builder pattern usage** - struct literals everywhere - 🔴 **TODO**
- **lib.rs module organization** - ✅ **COMPLETE**
- **0 inline test modules** in source files - ✅ **COMPLETE**

**Phase 1 Complete:** Foundation established (instrumentation + errors)  
**Phase 2 Progress:** 2 of 4 tasks complete (lib.rs + test migration)  
**Remaining Effort:** Medium (builders + imports)

---

## Quick Reference: Critical Violations

| Category | Standard | Compliance | Status | Priority |
|----------|----------|------------|--------|----------|
| **Tracing** | All functions `#[instrument]` | ✅ 100% | ✅ DONE | P0 - Critical |
| **Errors** | Use `derive_more` pattern | ✅ 100% | ✅ DONE | P0 - Critical |
| **Testing** | Tests in `tests/` directory | ✅ 100% | ✅ DONE | P1 - High |
| **lib.rs** | Only mod + pub use | ✅ 100% | ✅ DONE | P1 - High |
| **Builders** | Always use builders | 🔴 0% | TODO | P1 - High |
| **Imports** | `use crate::{Type}` | 🟡 Partial | TODO | P2 - Medium |
| **Linting** | Never `#[allow]` | ✅ Clean | ✅ OK | P2 - Medium |
| **Docs** | All public items | 🟡 Partial | OK | P2 - Medium |

---

## Detailed Violations by Category

### 1. Tracing & Instrumentation (✅ COMPLETE)

**Standard:** ALL functions (public and private) must have `#[instrument]`

**Current State:**
- Total functions: ~146
- Instrumented: ~146 (100%)
- Missing: 0

**Status:** ✅ **COMPLETE** - All functions now instrumented with proper field skipping and context.

**Files Needing Work:**
```
src/agent_config.rs     - Some functions instrumented, most missing
src/agent_handler.rs    - Missing instrumentation
src/llm_client.rs       - Partial instrumentation
src/server.rs           - Missing instrumentation
src/session.rs          - Missing instrumentation
src/tui/mod.rs          - Missing instrumentation
src/tui/http_client.rs  - Missing instrumentation
src/games/tictactoe/*.rs - Missing instrumentation
```

**Example Violations:**

```rust
// ❌ BAD: No instrumentation
pub fn make_move(&mut self, pos: usize) -> Result<(), String> {
    // ...
}

// ✅ GOOD: Properly instrumented
#[instrument(skip(self), fields(pos))]
pub fn make_move(&mut self, pos: usize) -> Result<(), String> {
    debug!(position = pos, "Making move");
    // ...
}
```

---

### 2. Error Handling (✅ COMPLETE)

**Standard:** Use `derive_more::Display` + `derive_more::Error` on ALL error types

**Current State:**
- Error types using derive_more: 2 (ConfigError, LlmError)
- Both have location tracking (file, line)
- Both use #[track_caller]
- Fields are public per standard

**Status:** ✅ **COMPLETE** - All error types follow the standard pattern.

**Violations Found:**

1. **src/agent_config.rs**
   ```rust
   // ❌ BAD: Manual Display implementation
   #[derive(Debug, Clone, Display, Error)]
   pub struct ConfigError {
       message: String,
   }
   
   impl ConfigError {
       pub fn new(message: impl Into<String>) -> Self {
           Self {
               message: message.into(),
           }
       }
   }
   ```

   Should be:
   ```rust
   // ✅ GOOD: Use derive_more with #[display(...)]
   #[derive(Debug, Clone, derive_more::Display, derive_more::Error)]
   #[display("Config error: {} at {}:{}", message, file, line)]
   pub struct ConfigError {
       pub message: String,
       pub line: u32,
       pub file: &'static str,
   }
   
   impl ConfigError {
       #[track_caller]
       pub fn new(message: impl Into<String>) -> Self {
           let loc = std::panic::Location::caller();
           Self {
               message: message.into(),
               line: loc.line(),
               file: loc.file(),
           }
       }
   }
   ```

2. **src/llm_client.rs**
   ```rust
   // ❌ BAD: Similar issue
   pub struct LlmError {
       message: String,
   }
   ```

3. **Missing ErrorKind Pattern**
   - No errors follow the ErrorKind + Wrapper pattern
   - No location tracking (`file`, `line` fields)
   - No `#[track_caller]` on constructors

**Required Changes:**
- Add `derive_more::Display` with `#[display(...)]` to all errors
- Add `derive_more::Error` to all errors
- Add location tracking (file, line) to all errors
- Add `#[track_caller]` to all error constructors
- Implement ErrorKind pattern where appropriate

---

### 3. Testing (✅ COMPLETE)

**Standard:** No `#[cfg(test)] mod tests` in source files

**Current State:**
- Inline test modules: 0
- All tests in `tests/` directory ✅
- New test files created:
  - tests/tictactoe_contracts_test.rs (5 tests)
  - tests/tictactoe_position_test.rs (4 tests)

**Status:** ✅ **COMPLETE** - All inline tests moved to proper location.

---

### 4. Type Construction - Builders (🔴 CRITICAL)

**Standard:** Always use builders, never struct literals

**Current State:**
- Builder usage: 0%
- Struct literals: Everywhere

**Major Violations:**

1. **src/llm_client.rs:38-45**
   ```rust
   // ❌ BAD: Struct literal
   pub fn new(provider: LlmProvider, api_key: String, model: String, max_tokens: u32) -> Self {
       Self {
           provider,
           api_key,
           model,
           max_tokens,
       }
   }
   ```

   Should use `derive_new` or builder:
   ```rust
   // ✅ Option 1: derive_new for simple types
   #[derive(Debug, Clone, derive_new::new)]
   pub struct LlmConfig {
       provider: LlmProvider,
       api_key: String,
       model: String,
       max_tokens: u32,
   }
   ```

   Or:
   ```rust
   // ✅ Option 2: derive_builder for complex types
   #[derive(Debug, Clone, derive_builder::Builder)]
   pub struct LlmConfig {
       provider: LlmProvider,
       api_key: String,
       model: String,
       max_tokens: u32,
   }
   ```

**Required Actions:**
- Add `derive_new` to simple types (4-5 fields, all required)
- Add `derive_builder::Builder` to complex types
- Replace all struct literal construction with builder calls
- Update all call sites

---

### 5. Module Organization (✅ COMPLETE)

**Standard:** lib.rs should ONLY contain `mod` and `pub use` statements

**Current State:**
```rust
// src/lib.rs
#![warn(missing_docs)]
#![forbid(unsafe_code)]

// Private module declarations
mod agent_config;
mod agent_handler;
// ... etc

// Crate-level exports
pub use agent_config::{AgentConfig, ConfigError};
pub use llm_client::{LlmClient, LlmConfig, LlmError, LlmProvider};
// ... etc
```

**Status:** ✅ **COMPLETE** - All modules private, comprehensive pub use exports, documented API.

**Required Pattern:**
```rust
// src/lib.rs
#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod agent_config;
mod agent_handler;
mod games;
mod llm_client;
mod server;
mod session;

// Crate-level exports
pub use agent_config::{AgentConfig, ConfigError};
pub use agent_handler::GameAgent;
pub use llm_client::{LlmClient, LlmConfig, LlmProvider, LlmError};
pub use server::{GameServer, RegisterPlayerRequest, MakeMoveRequest};
pub use session::{GameSession, Player, PlayerType, SessionManager};
```

**Benefits:**
- Single import path per type
- Clear public API
- Users import as: `use strictly_games::{LlmClient, GameServer}`

---

### 6. Import Patterns (🟡 PARTIAL)

**Standard:** Always `use crate::{Type}`, never `use crate::module::Type`

**Current State:**
- Found 20 crate-level imports (some compliance)
- Many module path imports still exist

**Example Violation:**
```rust
// ❌ BAD
use crate::llm_client::{LlmConfig, LlmProvider};

// ✅ GOOD (after fixing lib.rs exports)
use crate::{LlmConfig, LlmProvider};
```

**Depends On:** Fixing lib.rs exports first (Item #5)

---

### 7. Derives & Field Access (🟡 MEDIUM)

**Standard:** Private fields + derive_getters/derive_setters

**Current State:**
- Some types use `derive_getters` ✅
- Many types have public fields ❌
- Inconsistent encapsulation

**Example Violations:**

```rust
// ❌ BAD: Public fields
pub struct RegisterPlayerRequest {
    pub session_id: String,
    pub player_name: String,
}
```

Should be:
```rust
// ✅ GOOD: Private fields + getters
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct RegisterPlayerRequest {
    session_id: String,
    player_name: String,
}
```

**Required Actions:**
- Make all struct fields private
- Add `derive_getters::Getters` to all structs
- Add `derive_setters::Setters` to mutable config objects
- Update call sites to use getters

---

### 8. Documentation (🟡 PARTIAL)

**Standard:** All public items must have doc comments

**Current State:**
- `#![warn(missing_docs)]` present ✅
- Many items documented ✅
- Some items missing docs ❌

**Action Required:**
- Run `cargo doc --no-deps 2>&1 | grep warning`
- Add missing documentation
- Ensure all public functions have:
  - What (concise first line)
  - Parameters/returns (when not obvious)
  - Errors (for Result-returning functions)

---

### 9. Workspace Organization (✅ N/A)

**Standard:** No re-exports between workspace crates

**Current State:** Single crate project - not applicable

**Future Consideration:** When splitting into workspace, enforce this rule

---

### 10. Linting (✅ CLEAN)

**Standard:** Never use `#[allow]` directives

**Current State:** No `#[allow]` found ✅

**Status:** Compliant ✅

---

## Systematic Remediation Plan

### Phase 1: Foundation (P0 - Critical) ✅ COMPLETE

**Goal:** Establish error handling and tracing infrastructure

- [x] **Task 1.1:** Add derive_more patterns to all error types
  - [x] ConfigError in agent_config.rs
  - [x] LlmError in llm_client.rs
  - [x] Add location tracking (file, line)
  - [x] Add #[track_caller] to constructors
  - [x] Update all error creation sites

- [x] **Task 1.2:** Add #[instrument] to all functions
  - [x] agent_config.rs (8 functions)
  - [x] agent_handler.rs (5 functions)
  - [x] llm_client.rs (9 functions)
  - [x] server.rs (7 functions)
  - [x] session.rs (12 functions)
  - [x] tui/mod.rs (3 functions)
  - [x] tui/http_client.rs (5 functions)
  - [x] games/tictactoe/*.rs (43 functions)

**Validation:** ✅ `cargo check` passes - Commit: 4862cba

---

### Phase 2: Structure (P1 - High) - IN PROGRESS (50% complete)

**Goal:** Fix module organization and builder patterns

- [x] **Task 2.1:** Fix lib.rs ✅ COMPLETE
  - [x] Make mod declarations private
  - [x] Add pub use exports for all public types
  - [x] Document crate-level API
  - [x] Add #![forbid(unsafe_code)]
  - Commit: 7cea4fc

- [ ] **Task 2.2:** Add builders to all types
  - [ ] Add derive_new to simple types (LlmConfig, AgentConfig, etc.)
  - [ ] Add derive_builder to complex types
  - [ ] Replace all struct literal construction
  - [ ] Update call sites

- [ ] **Task 2.3:** Fix imports
  - [ ] Replace all `use crate::module::Type` with `use crate::{Type}`
  - [ ] Verify no compilation errors

- [x] **Task 2.4:** Move inline tests ✅ COMPLETE
  - [x] Moved contracts.rs tests (5 tests)
  - [x] Moved position.rs tests (4 tests)
  - [x] Removed all #[cfg(test)] from source
  - Commit: 85a2e63

**Validation:** ✅ `cargo check` and `cargo test` pass

---

### Phase 3: Polish (P2 - Medium)

**Goal:** Complete encapsulation and documentation

- [ ] **Task 3.1:** Fix field access
  - [ ] Make all fields private
  - [ ] Add derive_getters to all structs
  - [ ] Add derive_setters where appropriate
  - [ ] Update call sites

- [ ] **Task 3.2:** Complete documentation
  - [ ] Run `cargo doc` and fix all warnings
  - [ ] Add examples where helpful
  - [ ] Ensure error documentation

- [ ] **Task 3.3:** Add unsafe_code forbid
  - [ ] Add `#![forbid(unsafe_code)]` to lib.rs

**Validation:** `cargo doc --no-deps` produces zero warnings

---

### Phase 4: Verification (Final)

- [x] **Phase 1 verification:**
  ```bash
  cargo check                 # ✅ Passes
  cargo test                  # ✅ All pass
  grep -r "#\[allow" src/     # ✅ Zero results
  ```

- [x] **Phase 2 partial verification:**
  ```bash
  cargo check                 # ✅ Passes
  cargo test                  # ✅ All pass
  grep -r "#\[cfg(test)\]" src/  # ✅ Zero results
  ```

- [ ] **Final verification (after Phase 2 & 3 complete):**
  - [ ] cargo clippy --all-targets  # Zero warnings
  - [ ] cargo doc --no-deps         # Zero warnings
  - [ ] Update compliance to 🟢 Compliant

---

## Notes

- **Order Matters:** Phase 1 must complete before Phase 2 (builders need working errors)
- **Incremental Commits:** Commit after each major task completion
- **Test Coverage:** Run tests after each phase
- **AI Assistance:** With proper tracing, AI can help debug issues

## Appendix A: Tools & Commands

```bash
# Check instrumentation coverage
grep -r "#\[instrument\]" src/ --include="*.rs" | wc -l

# Find functions without instrumentation
grep -r "fn " src/ --include="*.rs" | grep -v "#\[instrument\]"

# Check for inline tests
grep -r "#\[cfg(test)\]" src/ --include="*.rs"

# Check for allow directives
grep -r "#\[allow" src/ --include="*.rs"

# Verify derive_more usage
grep -r "derive_more::" src/ --include="*.rs"

# Check builder patterns
grep -r "Builder\|derive_new" src/ --include="*.rs"
```

## Appendix B: Quick Wins

Start here for immediate impact:

1. **Add #[instrument] to main.rs functions** (5 minutes)
2. **Fix ConfigError to use derive_more** (10 minutes)
3. **Fix LlmError to use derive_more** (10 minutes)
4. **Add derive_new to LlmConfig** (5 minutes)
5. **Fix lib.rs exports** (15 minutes)

These 5 tasks (45 minutes) establish the patterns for the rest of the codebase.

---

## Progress Summary

**Completed:**
- ✅ Phase 1: Foundation (instrumentation + error handling)
- ✅ Task 2.1: lib.rs module organization
- ✅ Task 2.4: Inline test migration

**In Progress:**
- 🟡 Phase 2: Structure (2 of 4 tasks complete)

**Remaining:**
- Task 2.2: Builder pattern implementation
- Task 2.3: Import path fixes
- Phase 3: Polish (field access + documentation)

**Commits:**
- 4862cba: Phase 1 complete (instrumentation + errors)
- 7cea4fc: Task 2.1 (lib.rs)
- 85a2e63: Task 2.4 (test migration)

---

**Last Updated:** 2026-02-14 (20:02 UTC)  
**Next Review:** After Phase 2 completion
