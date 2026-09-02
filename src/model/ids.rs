use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identity for a folder or task.
///
/// A newtype rather than a bare `Uuid` so a node id can never be confused with
/// any other id we might add later.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(Uuid);

impl NodeId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Short form used for `egui` widget ids and log lines.
    #[must_use]
    pub fn short(&self) -> String {
        self.0.simple().to_string()[..8].to_owned()
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({})", self.short())
    }
}
