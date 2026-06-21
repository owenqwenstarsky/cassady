use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessMode {
    #[default]
    ReadOnly,
    FullAccess,
}

impl AccessMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::ReadOnly => Self::FullAccess,
            Self::FullAccess => Self::ReadOnly,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::FullAccess => "full-access",
        }
    }

    pub fn can_write(self) -> bool {
        matches!(self, Self::FullAccess)
    }
}

impl fmt::Display for AccessMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
