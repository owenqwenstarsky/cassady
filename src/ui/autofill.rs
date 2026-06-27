//! Re-exports the shared autocomplete menu types from [`crate::commands`].
//!
//! The `AutoFillItem`/`AutoFillMenu` value types and their helpers live in
//! `crate::commands` so both the CLI and the desktop share one definition.
//! This module keeps the historical `crate::ui::autofill` import path working
//! for the TUI renderer.
pub use crate::commands::{AutoFillItem, AutoFillMenu};
