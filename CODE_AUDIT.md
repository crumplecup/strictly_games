# Code Standards Audit

**Date:** 2026-02-14  
**Status:** 🔴 Non-Compliant  
**Standards Reference:** CLAUDE.md

## Executive Summary

The codebase has **significant compliance gaps** across all major standards categories. Key findings:

- **6 of 146 functions** (4%) have `#[instrument]` - **96% missing**
- **0 error types** use `derive_more` pattern - **100% non-compliant**
- **No builder pattern usage** - struct literals everywhere
- **lib.rs has definitions** - violates module organization
- **2 inline test modules** in source files

**Estimated Effort:** Medium to High (systematic refactor needed)

---

## Quick Reference: Critical Violations

| Category | Standard | Compliance | Priority |
|----------|----------|------------|----------|
| **Tracing** | All functions `#[instrument]` | 🔴 4% | P0 - Critical |
| **Errors** | Use `derive_more` pattern | 🔴 0% | P0 - Critical |
| **Testing** | Tests in `tests/` directory | 🟡 Mostly OK | P1 - High |
| **Builders** | Always use builders | 🔴 0% | P1 - High |
| **lib.rs** | Only mod + pub use | 🔴 Has content | P1 - High |
| **Imports** | `use crate::{Type}` | 🟡 Partial | P2 - Medium |
| **Linting** | Never `#[allow]` | ✅ Clean | ✅ OK |
| **Docs** | All public items | 🟡 Partial | P2 - Medium |

---

## Detailed Violations by Category

### 1. Tracing & Instrumentation (🔴 CRITICAL)

**Standard:** ALL functions (public and private) must have `#[instrument]`

**Current State:**
- Total functions: ~146
- Instrumented: 6 (4%)
- Missing: ~140 (96%)

**Impact:** Debugging and error tracking severely hampered. AI-assisted debugging impossible without trace context.

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

### 2. Error Handling (🔴 CRITICAL)

**Standard:** Use `derive_more::Display` + `derive_more::Error` on ALL error types

**Current State:**
- Error types using derive_more: 0
- Manual implementations: Multiple
- Pattern compliance: 0%

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

### 3. Testing (🟡 MOSTLY OK)

**Standard:** No `#[cfg(test)] mod tests` in source files

**Current State:**
- Found 2 inline test modules
- Most tests properly in `tests/` directory ✅

**Violations:**

1. **Check which files have inline tests:**
   ```bash
   grep -r "#\[cfg(test)\]" src/
   ```

**Action Required:**
- Move inline test modules to `tests/` directory
- Remove `#[cfg(test)]` from source files

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

### 5. Module Organization (🔴 HIGH PRIORITY)

**Standard:** lib.rs should ONLY contain `mod` and `pub use` statements

**Current State:**
```rust
// src/lib.rs
#![warn(missing_docs)]  // ❌ Attributes OK, but...

pub mod agent_config;
pub mod agent_handler;
pub mod games;
pub mod llm_client;
pub mod server;
pub mod session;
```

**Issues:**
1. Missing `pub use` exports for crate-level types
2. No clear public API surface
3. Users must import via module paths: `use strictly_games::llm_client::LlmClient`

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

### Phase 1: Foundation (P0 - Critical)

**Goal:** Establish error handling and tracing infrastructure

- [ ] **Task 1.1:** Add derive_more patterns to all error types
  - [ ] ConfigError in agent_config.rs
  - [ ] LlmError in llm_client.rs
  - [ ] Add location tracking (file, line)
  - [ ] Add #[track_caller] to constructors
  - [ ] Update all error creation sites

- [ ] **Task 1.2:** Add #[instrument] to all functions
  - [ ] agent_config.rs (8 functions)
  - [ ] agent_handler.rs (15 functions)
  - [ ] llm_client.rs (20 functions)
  - [ ] server.rs (25 functions)
  - [ ] session.rs (30 functions)
  - [ ] tui/mod.rs (15 functions)
  - [ ] tui/http_client.rs (12 functions)
  - [ ] games/tictactoe/*.rs (21 functions)

**Validation:** `just check-all` passes with zero warnings

---

### Phase 2: Structure (P1 - High)

**Goal:** Fix module organization and builder patterns

- [ ] **Task 2.1:** Fix lib.rs
  - [ ] Make mod declarations private
  - [ ] Add pub use exports for all public types
  - [ ] Document crate-level API

- [ ] **Task 2.2:** Add builders to all types
  - [ ] Add derive_new to simple types (LlmConfig, AgentConfig, etc.)
  - [ ] Add derive_builder to complex types
  - [ ] Replace all struct literal construction
  - [ ] Update tests and examples

- [ ] **Task 2.3:** Fix imports
  - [ ] Replace all `use crate::module::Type` with `use crate::{Type}`
  - [ ] Verify no compilation errors

- [ ] **Task 2.4:** Move inline tests
  - [ ] Identify files with #[cfg(test)]
  - [ ] Create test files in tests/ directory
  - [ ] Remove inline test modules

**Validation:** `cargo clippy` and `cargo test` pass

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

- [ ] **Run complete audit:**
  ```bash
  cargo clippy --all-targets  # Zero warnings
  cargo test                  # All pass
  cargo doc --no-deps         # Zero warnings
  grep -r "#\[cfg(test)\]" src/  # Zero results
  grep -r "#\[allow" src/     # Zero results
  ```

- [ ] **Update this document:**
  - [ ] Mark all tasks complete
  - [ ] Update compliance percentages
  - [ ] Change status to 🟢 Compliant

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

**Last Updated:** 2026-02-14  
**Next Review:** After Phase 1 completion
