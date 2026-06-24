# Glossary

**Access mode**: The safety policy controlling which tools are available. Current modes are `read-only`, `workspace-edit`, and `full-access`.

**Active provider**: The provider Cassady resolves for the current session after applying config and CLI overrides.

**Bundled docs**: Markdown files embedded into the binary at build time and installed to `~/.cass/docs`. Cassady can read them; writes under this directory are blocked.

**Cassady / Cass**: The project name is Cassady. The short command is `cass`; `cassady` is also installed.

**Chat**: A persisted conversation with a model for one workspace. Chats are saved under `~/.cass/conversations` and can be resumed.

**Config root**: The `~/.cass` directory containing config, conversations, global instructions, and installed docs.

**Exact edit**: An `edit` tool replacement where each `old_text` must match exactly once in the original file before anything is written.

**Global instructions**: Optional text in `~/.cass/global.md` included in new chat system prompts.

**Model metadata**: The `models.json` entry describing a model id, owning provider, display name, context limits, tool/streaming support, and reasoning behavior.

**OpenAI-compatible provider**: A provider exposing an API compatible with the OpenAI-style chat/completions behavior Cassady uses.

**Provider**: A connection definition in `providers.json`, including id, base URL, API key reference, and optional default model.

**Reasoning effort**: Runtime setting (`off`, `low`, `medium`, `high`) used for models with reasoning support. Press `Tab` while idle to cycle it.

**Tool call**: A model-requested operation such as `ls`, `read`, `grep`, `write`, `edit`, or `shell`.

**Workspace**: The launch cwd, either the current directory or the path passed with `--cwd`. In workspace-edit mode, writes must stay inside this root.
