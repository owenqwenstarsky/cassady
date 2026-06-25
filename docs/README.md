# Cass bundled docs

These docs are embedded into the `cass`/`cassady` binary at build time and installed to `~/.cass/docs` when Cassady starts.

Cassady tools may list, search, and read this directory. Mutating tools are blocked from writing here, even in full-access mode.

## Contents

- [Commands](commands.md): CLI forms, global flags, in-chat commands, and keys.
- [Configuration](configuration.md): `~/.cass` files, setup, precedence, schema examples, and validation.
- [Providers and models](providers.md): built-in OpenAI-compatible providers, custom endpoints, model discovery, and reasoning metadata.
- [Access modes and tool safety](access-modes.md): what tools can read, write, edit, and run in each mode.
- [Experimental Rust embedding API](embedding.md): import Cassady from Rust, start headless sessions, stream events, and handle approvals.
- [Workflows](workflows.md): common ways to inspect code, apply edits, run checks, switch models, and resume chats.
- [Troubleshooting](troubleshooting.md): symptoms, likely causes, fixes, and verification commands.
- [Platform notes](platforms.md): macOS, Linux, and Windows environment/path notes.
- [Glossary](glossary.md): short definitions for Cassady terms.
