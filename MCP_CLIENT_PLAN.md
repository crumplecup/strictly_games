# Custom MCP Client Implementation Plan

## Problem Statement

The elicitation-based game architecture requires MCP `sampling/createMessage` support, but current CLI clients (GitHub Copilot CLI, Claude Desktop, OpenAI Codex CLI) don't support it. Only VS Code Copilot supports sampling, but requires IDE integration.

**Solution:** Build a custom MCP client using `rmcp` that implements sampling and connects directly to LLM APIs.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│  Process 1: Game Server                                  │
│  cargo run --bin server_http                             │
│                                                           │
│  ┌────────────────────────────────────────────────────┐ │
│  │  HTTP API (Port 3000)                               │ │
│  │  - Human players via TUI                            │ │
│  └────────────────────────────────────────────────────┘ │
│                                                           │
│  ┌────────────────────────────────────────────────────┐ │
│  │  MCP Server (stdio)                                 │ │
│  │  - register_player                                  │ │
│  │  - play_game (elicitation-based)                    │ │
│  │  - make_move (traditional)                          │ │
│  └────────────────────────────────────────────────────┘ │
└───────────────┬─────────────────────────────────────────┘
                │ stdio (JSON-RPC)
                ↓
┌─────────────────────────────────────────────────────────┐
│  Process 2: Custom MCP Client                            │
│  cargo run --bin mcp_agent                               │
│                                                           │
│  ┌────────────────────────────────────────────────────┐ │
│  │  rmcp::ClientHandler                                │ │
│  │  - handle_list_tools()                              │ │
│  │  - handle_call_tool()                               │ │
│  │  - handle_sampling_create_message() ← KEY!         │ │
│  └──────────────┬─────────────────────────────────────┘ │
│                 │                                         │
│                 ↓                                         │
│  ┌────────────────────────────────────────────────────┐ │
│  │  LLM API Client                                     │ │
│  │  - OpenAI API                                       │ │
│  │  - Anthropic API                                    │ │
│  │  - Configurable via API keys                        │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

---

## Components to Build

### 1. MCP Client Binary (`src/bin/mcp_agent.rs`)

**Purpose:** Standalone agent that connects to game server via MCP

**Responsibilities:**
- Implement `rmcp::handler::client::ClientHandler` trait
- Handle `sampling/createMessage` requests from server
- Connect to LLM API (OpenAI/Anthropic)
- Manage conversation context
- Execute tool calls from LLM responses

**Key traits to implement:**
```rust
#[async_trait]
impl ClientHandler for GameAgent {
    #[instrument(skip(self, params))]
    async fn handle_sampling_create_message(
        &mut self,
        params: CreateMessageRequestParams,
    ) -> Result<CreateMessageResult, ServiceError>;
    
    // Also needed but simpler (all instrumented):
    #[instrument(skip(self))]
    async fn handle_list_resources(...);
    
    #[instrument(skip(self, params))]
    async fn handle_read_resource(...);
    
    #[instrument(skip(self))]
    async fn handle_list_prompts(...);
    
    #[instrument(skip(self, params))]
    async fn handle_get_prompt(...);
}
```

### 2. LLM API Client Abstraction (`src/llm_client.rs`)

**Purpose:** Unified interface for different LLM providers

**Trait:**
```rust
use derive_getters::Getters;

#[async_trait]
pub trait LlmClient: Send + Sync {
    #[instrument(skip(self, messages, tools))]
    async fn create_message(
        &self,
        messages: Vec<Message>,
        tools: Vec<Tool>,
        system_prompt: Option<String>,
        max_tokens: i32,
    ) -> Result<LlmResponse, LlmError>;
}

#[derive(Debug, Clone, Getters)]
pub struct LlmResponse {
    content: Vec<ContentBlock>,
    tool_calls: Vec<ToolCall>,
    stop_reason: StopReason,
    model: String,
}
```

**Implementations:**
- `OpenAiClient` - Uses `openai-api-rs` or `async-openai` crate
- `AnthropicClient` - Uses `anthropic-sdk` or direct reqwest calls

### 3. Configuration (`src/agent_config.rs`)

**Structure:**
```rust
use derive_getters::Getters;

#[derive(Debug, Clone, Deserialize, Getters)]
pub struct AgentConfig {
    /// LLM provider: "openai" or "anthropic"
    provider: LlmProvider,
    
    /// API key (from env var)
    api_key: String,
    
    /// Model to use (e.g., "gpt-4", "claude-3-5-sonnet")
    model: String,
    
    /// Max tokens per response
    max_tokens: i32,
    
    /// Temperature (0.0-1.0)
    temperature: f32,
    
    /// MCP server connection (stdio command)
    server_command: Vec<String>,
}

impl AgentConfig {
    #[instrument]
    pub fn new(
        provider: LlmProvider,
        api_key: String,
        model: String,
        max_tokens: i32,
        temperature: f32,
        server_command: Vec<String>,
    ) -> Self {
        Self {
            provider,
            api_key,
            model,
            max_tokens,
            temperature,
            server_command,
        }
    }
}
```

**Load from:**
- `agent_config.toml` file
- Environment variables
- Command-line args

---

## Implementation Steps

### Phase 1: Basic MCP Client (No Sampling)

**Goal:** Get a working MCP client that can connect and call tools

1. **Create agent binary structure**
   - [ ] Add `src/bin/mcp_agent.rs`
   - [ ] Add clap CLI with `--server-command` arg
   - [ ] Load configuration from TOML/env

2. **Implement minimal ClientHandler**
   - [ ] Stub all required methods
   - [ ] Connect to server via stdio transport
   - [ ] Handle initialize handshake
   - [ ] List available tools

3. **Test connection**
   - [ ] Start server: `cargo run --bin server_http`
   - [ ] Start agent: `cargo run --bin mcp_agent -- --server-command "cargo run --bin server_http"`
   - [ ] Verify tools are discovered

### Phase 2: LLM API Integration

**Goal:** Connect to OpenAI/Anthropic and make basic calls

4. **Add LLM client dependencies**
   ```toml
   [dependencies]
   # OpenAI
   async-openai = "0.23"
   
   # Anthropic (manual reqwest)
   reqwest = { version = "0.12", features = ["json"] }
   ```

5. **Implement LlmClient trait**
   - [ ] Create `src/llm_client.rs`
   - [ ] Define trait with `create_message` method
   - [ ] Implement `OpenAiClient`
   - [ ] Implement `AnthropicClient`

6. **Test LLM calls independently**
   - [ ] Unit test OpenAI chat completion
   - [ ] Unit test Anthropic message creation
   - [ ] Verify tool/function calling works

### Phase 3: Sampling Implementation

**Goal:** Handle server-initiated sampling requests

7. **Implement handle_sampling_create_message**
   ```rust
   #[instrument(skip(self, params), fields(num_messages = params.messages.len()))]
   async fn handle_sampling_create_message(
       &mut self,
       params: CreateMessageRequestParams,
   ) -> Result<CreateMessageResult, ServiceError> {
       debug!("Processing sampling request");
       
       // 1. Extract messages from params
       let messages = convert_mcp_messages_to_llm(params.messages);
       debug!(message_count = messages.len(), "Converted MCP messages");
       
       // 2. Call LLM API
       let response = self.llm_client.create_message(
           messages,
           vec![], // tools from params if needed
           params.system_prompt,
           params.max_tokens,
       ).await?;
       debug!(model = %response.model(), "Received LLM response");
       
       // 3. Convert response to MCP format
       let result = convert_llm_response_to_mcp(response);
       debug!("Converted response to MCP format");
       Ok(result)
   }
   ```

8. **Message format conversion**
   - [ ] MCP `SamplingMessage` → LLM provider format
   - [ ] LLM response → MCP `CreateMessageResult`
   - [ ] Handle text, tool calls, stop reasons

9. **Test sampling end-to-end**
   - [ ] Create test MCP server that requests sampling
   - [ ] Verify agent receives request
   - [ ] Verify LLM is called
   - [ ] Verify response returns to server

### Phase 4: Game Integration

**Goal:** Agent can play tic-tac-toe via elicitation

10. **Test with play_game tool**
    - [ ] Start server with tic-tac-toe session
    - [ ] Agent registers as player
    - [ ] Agent calls `play_game` tool
    - [ ] Server elicits moves via sampling
    - [ ] Agent responds with structured Move
    - [ ] Game completes with winner/draw

11. **Handle conversation context**
    - [ ] Maintain message history across sampling calls
    - [ ] Support multi-turn elicitation (Survey pattern)
    - [ ] Reset context between games

12. **Error handling**
    - [ ] LLM API errors (rate limits, auth)
    - [ ] Invalid tool responses
    - [ ] Network failures
    - [ ] Graceful shutdown

### Phase 5: Multi-Agent Support

**Goal:** Multiple agents can play simultaneously

13. **Support multiple agent instances**
    - [ ] Each agent connects independently
    - [ ] Separate sessions per agent
    - [ ] Agent vs agent games

14. **Human vs agent games**
    - [ ] Human uses TUI (HTTP)
    - [ ] Agent uses MCP client
    - [ ] Turn coordination via server

---

## Coding Standards

**All code must follow project conventions:**

### Instrumentation
- **ALL functions** (public and private) must have `#[instrument]`
- Skip large parameters: `#[instrument(skip(data, connection))]`
- Include context fields: `#[instrument(fields(count, session_id))]`
- Emit debug/info/error events at key points
- Log errors before returning

### Type Construction
- **Always use builders**, never struct literals
- Use `derive_builder::Builder` or manual builder pattern
- For simple types, use `derive_new::new`

### Field Access
- **Private fields** with `derive_getters::Getters`
- Use `derive_setters::Setters` with `#[setters(prefix = "with_")]` for mutable config
- Never `pub` fields on structs (except for simple DTOs with explicit justification)

### Error Handling
- Use `derive_more::Display` + `derive_more::Error`
- ErrorKind enum with `#[display(...)]` on variants
- Wrapper struct with location tracking
- All constructors use `#[track_caller]`

### Example:**
```rust
use derive_getters::Getters;
use derive_setters::Setters;

#[derive(Debug, Clone, Getters, Setters)]
#[setters(prefix = "with_")]
pub struct GameAgent {
    llm_client: Box<dyn LlmClient>,
    #[setters(skip)]
    session_id: String,
    config: AgentConfig,
}

impl GameAgent {
    #[instrument(skip(llm_client))]
    pub fn new(llm_client: Box<dyn LlmClient>, session_id: String, config: AgentConfig) -> Self {
        debug!("Creating GameAgent");
        Self {
            llm_client,
            session_id,
            config,
        }
    }
    
    #[instrument(skip(self), fields(session_id = %self.session_id))]
    async fn connect_to_server(&mut self) -> Result<(), AgentError> {
        debug!("Connecting to MCP server");
        // implementation
        Ok(())
    }
}
```

---

## Code Structure

```
src/
├── bin/
│   ├── mcp_agent.rs          # NEW: Custom MCP client binary
│   ├── server_http.rs         # Existing game server
│   └── stdio_http_bridge.rs  # Existing (may deprecate)
│
├── llm_client/                # NEW: LLM API abstraction
│   ├── mod.rs                 # LlmClient trait
│   ├── openai.rs              # OpenAI implementation
│   ├── anthropic.rs           # Anthropic implementation
│   └── error.rs               # LLM error types
│
├── agent/                     # NEW: Agent logic
│   ├── mod.rs                 # GameAgent struct
│   ├── handler.rs             # ClientHandler implementation
│   ├── config.rs              # AgentConfig
│   └── conversion.rs          # MCP ↔ LLM format conversion
│
├── games/                     # Existing
│   └── tictactoe/
│       ├── types.rs           # Already has Move derive(Elicit)
│       └── ...
│
└── server.rs                  # Existing server with play_game tool
```

---

## Configuration Example

**File: `agent_config.toml`**

```toml
[agent]
name = "Agent_1"
provider = "anthropic"  # or "openai"
model = "claude-3-5-sonnet-20241022"
max_tokens = 1000
temperature = 0.7

[server]
# stdio command to start game server
command = ["cargo", "run", "--bin", "server_http"]

[game]
# Which game to play
game_type = "tictactoe"
# Session ID to join/create
session_id = "game_001"
```

**Environment variables:**

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
```

---

## Testing Strategy

### Unit Tests

- [ ] LLM client mock/fake
- [ ] Message format conversion
- [ ] Configuration loading
- [ ] Error handling

### Integration Tests

1. **Server ↔ Client Handshake**
   - Start server, connect client
   - Verify tools discovered

2. **Traditional Tool Calls**
   - Client calls `register_player`
   - Client calls `make_move`

3. **Sampling Loop**
   - Client calls `play_game`
   - Server sends `sampling/createMessage`
   - Client calls LLM (mocked)
   - Server receives structured response

4. **Full Game**
   - Agent vs agent (two clients)
   - Agent vs human (client + TUI)

### Manual Testing

```bash
# Terminal 1: Start server
cargo run --bin server_http

# Terminal 2: Start agent 1
ANTHROPIC_API_KEY=sk-... cargo run --bin mcp_agent -- \
  --config agent1_config.toml

# Terminal 3: Start agent 2 (for agent vs agent)
ANTHROPIC_API_KEY=sk-... cargo run --bin mcp_agent -- \
  --config agent2_config.toml

# Terminal 4: Start TUI (for agent vs human)
cd strictly_games_tui
cargo run -- --mode http
```

---

## Dependencies to Add

```toml
[dependencies]
# LLM clients
async-openai = "0.23"          # OpenAI API
reqwest = { version = "0.12", features = ["json"] }  # Anthropic API

# Existing (verify versions)
rmcp = { version = "0.15", features = ["client", "transport-io"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
clap = { version = "4", features = ["derive"] }
toml = "0.8"

# Project standards
derive_more = { version = "2", features = ["display", "error", "from"] }
derive-getters = "0.5"
derive-setters = "0.2"
derive_builder = "0.20"
derive-new = "0.7"
```

---

## Success Criteria

### Milestone 1: Basic Client
- ✅ Client connects to server
- ✅ Client lists tools
- ✅ Client calls `register_player`

### Milestone 2: LLM Integration
- ✅ OpenAI client makes chat completions
- ✅ Anthropic client makes messages
- ✅ Function calling works

### Milestone 3: Sampling Works
- ✅ Server sends `sampling/createMessage`
- ✅ Client calls LLM
- ✅ Response flows back to server

### Milestone 4: Elicitation Game Loop
- ✅ Agent plays full tic-tac-toe game
- ✅ Server elicits moves via sampling
- ✅ Agent constructs valid Move objects
- ✅ Game completes successfully

### Milestone 5: Multi-Agent
- ✅ Two agents play each other
- ✅ Agent plays human via TUI
- ✅ Multiple concurrent games

---

## Timeline Estimate

**Phase 1:** Basic MCP Client - 2-4 hours
**Phase 2:** LLM API Integration - 3-5 hours  
**Phase 3:** Sampling Implementation - 4-6 hours
**Phase 4:** Game Integration - 3-5 hours
**Phase 5:** Multi-Agent Support - 2-3 hours

**Total:** 14-23 hours of focused development

---

## Risks & Mitigations

### Risk 1: rmcp API Changes
**Mitigation:** Pin rmcp version, follow official examples

### Risk 2: LLM API Rate Limits
**Mitigation:** Add retry logic, exponential backoff, rate limiting

### Risk 3: Message Format Incompatibilities
**Mitigation:** Comprehensive conversion tests, detailed logging

### Risk 4: Conversation Context Management
**Mitigation:** Clear state machine, context reset between games

### Risk 5: Cost (LLM API Calls)
**Mitigation:** 
- Use minimal prompts
- Low max_tokens during testing
- Support local LLMs (Ollama) as alternative

---

## Future Enhancements

### Short-term
- [ ] Support more LLM providers (Gemini, local Ollama)
- [ ] Web UI to watch agent games
- [ ] Replay/logging of game transcripts
- [ ] Agent personality/style configuration

### Long-term
- [ ] Multi-game support (chess, checkers, etc.)
- [ ] Tournament mode (round-robin, brackets)
- [ ] Agent training/fine-tuning on game data
- [ ] Distributed agent network (multiple machines)

---

## References

- **rmcp docs:** https://docs.rs/rmcp
- **MCP spec:** https://modelcontextprotocol.io
- **Elicitation crate:** /home/erik/repos/elicitation
- **OpenAI API:** https://platform.openai.com/docs
- **Anthropic API:** https://docs.anthropic.com

---

## Questions to Resolve

1. Which LLM provider to prioritize? (Anthropic Claude recommended for quality)
2. Should we support streaming responses? (Not needed initially)
3. How to handle agent identity/registration? (Auto-register on connect)
4. Store game history/transcripts? (Optional, file-based initially)
5. Support HTTP transport in addition to stdio? (stdio sufficient for now)

---

## Next Steps

1. Review this plan
2. Create feature branch: `git checkout -b feature/custom-mcp-client`
3. Start with Phase 1: Basic MCP Client
4. Implement incrementally with tests
5. Document learnings in this file
