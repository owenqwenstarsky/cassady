# Glossary

**Access mode**: The safety policy controlling which tools are available. Current modes are `read-only`, `workspace-edit`, and `full-access`.

**Active provider**: The provider Cassady resolves for the current session after applying config and CLI overrides.

**Bundled docs**: Markdown files embedded into the binary at build time and installed to `~/.cass/docs`. Cassady can read them; writes under this directory are blocked.

**Cassady / Cass**: The project name is Cassady. The short command is `cass`; `cassady` is also installed.

**Chat**: A persisted conversation with a model for one workspace. Chats are saved under `~/.cass/conversations` and can be resumed.

**Config root**: The `~/.cass` directory containing config, conversations, global instructions, and installed docs.

**Compacted tool output**: A model-facing replacement for a large tool result that keeps a head/tail excerpt plus provenance and recovery guidance so the assistant can re-read or re-search narrowly before relying on omitted details.

**Exact edit**: An `edit` tool replacement where each `old_text` must match exactly once in the original file before anything is written.

**Fast mode**: A saved preference enabled with `/fast`. It is active only when the current provider/model advertises fast-mode support; otherwise Cassady keeps the preference but reports it as unavailable.

**Global instructions**: Optional text in `~/.cass/global.md` included in new chat system prompts. Cassady follows these instructions when they fit the active request, but they cannot override runtime safety constraints such as access modes, tool denials, approvals, or workspace boundaries.

**Model metadata**: The `models.json` entry describing a model id, owning provider, display name, context limits, tool/streaming support, reasoning behavior, and fast-mode support.

**OpenAI-compatible provider**: A provider exposing an API compatible with the OpenAI-style chat/completions behavior Cassady uses.

**Provider**: A connection definition in `providers.json`, including id, base URL, API key reference, and optional default model.

**Reasoning effort**: Runtime setting (`off`, `low`, `medium`, `high`) used for models with reasoning support. Press `Tab` while idle to cycle it.

**Tool call**: A model-requested operation such as `ls`, `read`, `grep`, `write`, `edit`, or `shell`.

**Truncated tool output**: A model-facing shortened tool result produced when output exceeds `model_tool_result_limit`. Cassady tells the model that output was incomplete and suggests narrower follow-up inspection.

**Workspace**: The launch cwd, either the current directory or the path passed with `--cwd`. In workspace-edit mode, writes must stay inside this root.
