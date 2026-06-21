# Cass bundled docs

These docs are embedded into the `cass` binary at build time and installed to `~/.cass/docs` when Cass starts.

Cass tools may list, search, and read this directory. Mutating tools are blocked from writing here, even in full-access mode.
