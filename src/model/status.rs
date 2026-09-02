use serde::{Deserialize, Serialize};

/// Task status.
///
/// `Blocked` carries its reason inside the variant, so "blocked without a
/// reason" is unrepresentable in the type rather than being a rule we have to
/// remember to check. The reason may still be *empty*, which the tree layer
/// rejects on write.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    #[default]
    Open,
    InProgress,
    Completed,
    Blocked(String),
    Cancelled,
}

impl Status {
    /// One of each variant, for building selectors. `Blocked` comes back with an
    /// empty reason.
    #[must_use]
    pub fn variants() -> [Self; 5] {
        [
            Self::Open,
            Self::InProgress,
            Self::Completed,
            Self::Blocked(String::new()),
            Self::Cancelled,
        ]
    }

    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::InProgress => "In Progress",
            Self::Completed => "Completed",
            Self::Blocked(_) => "Blocked",
            Self::Cancelled => "Cancelled",
        }
    }

    /// Status dot colour as plain RGB, keeping this module free of `egui`.
    #[must_use]
    pub const fn rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::Open => (109, 190, 255),      // light blue
            Self::InProgress => (240, 200, 60), // yellow
            Self::Completed => (110, 224, 170), // mint green
            Self::Blocked(_) => (58, 62, 68),   // dark grey, near black
            Self::Cancelled => (232, 76, 76),   // red
        }
    }

    /// Stable ordinal, used for sorting and for driving radio selectors without
    /// comparing the blocked reason.
    #[must_use]
    pub const fn ordinal(&self) -> u8 {
        match self {
            Self::Open => 0,
            Self::InProgress => 1,
            Self::Completed => 2,
            Self::Blocked(_) => 3,
            Self::Cancelled => 4,
        }
    }

    /// True when both are the same variant, ignoring the blocked reason.
    #[must_use]
    pub const fn same_variant(&self, other: &Self) -> bool {
        self.ordinal() == other.ordinal()
    }

    #[must_use]
    pub const fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked(_))
    }

    #[must_use]
    pub fn blocked_reason(&self) -> Option<&str> {
        match self {
            Self::Blocked(reason) => Some(reason.as_str()),
            _ => None,
        }
    }
}
