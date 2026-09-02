use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{NodeId, Status};

/// One page of a folder's comments.
///
/// A folder holds several of these and you cycle between them, so a project can
/// keep a kickoff note, a list of blockers and a scratch page side by side
/// without them running together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommentSpace {
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CommentSpace {
    /// Prefix on an auto numbered title, before the user renames it.
    pub const AUTO_PREFIX: &'static str = "Comments ";

    /// A new, empty space. The title is auto numbered by the caller, which knows
    /// what numbers are already in play.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            title: title.into(),
            body: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// The auto numbered title for a given number.
    #[must_use]
    pub fn auto_title(number: usize) -> String {
        format!("{}{number}", Self::AUTO_PREFIX)
    }

    /// The number in a title that is still auto numbered, if it is.
    #[must_use]
    pub fn auto_number(title: &str) -> Option<usize> {
        title
            .trim()
            .strip_prefix(Self::AUTO_PREFIX)?
            .trim()
            .parse()
            .ok()
    }

    /// Nothing the user has actually put here.
    ///
    /// An auto numbered title is machinery, not content, so a space carrying
    /// only "Comments 2" counts as blank. A title they chose does not, since
    /// naming a page is an investment in it even before anything is written.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.body.trim().is_empty()
            && (self.title.trim().is_empty() || Self::auto_number(&self.title).is_some())
    }

    /// Does this space mention `needle`? Expects `needle` already lowercased.
    #[must_use]
    pub fn matches(&self, needle: &str) -> bool {
        self.title.to_lowercase().contains(needle) || self.body.to_lowercase().contains(needle)
    }
}

/// A folder. Holds an ordered list of children, which may be folders or tasks,
/// nested to any depth.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub name: String,
    pub created_at: DateTime<Utc>,
    /// Bumped whenever *anything* beneath this folder changes, at any depth.
    pub updated_at: DateTime<Utc>,
    pub children: Vec<NodeId>,
    /// Project level context, as several titled pages. `default` so a schema
    /// version 1 file, written before comments existed, still loads.
    #[serde(default)]
    pub comments: Vec<CommentSpace>,
}

/// A task. Always has a folder parent, never sits at the root.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub title: String,
    /// `None` rather than an empty string, so "no description" is one state.
    pub description: Option<String>,
    pub status: Status,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Manually attributed time. Stored as whole minutes, entered as hours and
    /// minutes. No seconds by design.
    pub attributed_minutes: u32,
    /// Free notes about this one task, separate from the description.
    ///
    /// A plain `String` rather than an `Option` like `description`: this is a
    /// scratch area where empty is the resting state, and nothing branches on
    /// present-versus-empty. `default` so a schema version 1 file still loads.
    #[serde(default)]
    pub notes: String,
}

impl Task {
    /// Split the attributed total into whole hours and leftover minutes.
    #[must_use]
    pub const fn attributed_hm(&self) -> (u32, u32) {
        (self.attributed_minutes / 60, self.attributed_minutes % 60)
    }

    /// Set attributed time from hour and minute boxes. Minutes above 59 roll
    /// into hours, so typing `90` in the minutes box gives `1h 30m`.
    pub fn set_attributed_hm(&mut self, hours: u32, minutes: u32) {
        self.attributed_minutes = hours.saturating_mul(60).saturating_add(minutes);
    }

    /// Display form: `15h`, `1h 30m`, `45m`, or empty when nothing is logged.
    #[must_use]
    pub fn attributed_label(&self) -> String {
        match self.attributed_hm() {
            (0, 0) => String::new(),
            (0, m) => format!("{m}m"),
            (h, 0) => format!("{h}h"),
            (h, m) => format!("{h}h {m}m"),
        }
    }

    /// Normalises whitespace-only descriptions to `None`.
    pub fn set_description(&mut self, text: impl Into<String>) {
        let text: String = text.into();
        self.description = if text.trim().is_empty() {
            None
        } else {
            Some(text)
        };
    }

    #[must_use]
    pub fn description_str(&self) -> &str {
        self.description.as_deref().unwrap_or("")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Folder(Folder),
    Task(Task),
}

/// An entry in the tree arena. Carries its own id and a parent back-pointer,
/// which is what makes `updated_at` bubbling and reparenting cheap.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    /// `None` means the node sits at the root. Only folders may do that.
    pub parent: Option<NodeId>,
    pub kind: NodeKind,
}

impl Node {
    #[must_use]
    pub const fn is_folder(&self) -> bool {
        matches!(self.kind, NodeKind::Folder(_))
    }

    #[must_use]
    pub const fn is_task(&self) -> bool {
        matches!(self.kind, NodeKind::Task(_))
    }

    #[must_use]
    pub const fn as_folder(&self) -> Option<&Folder> {
        match &self.kind {
            NodeKind::Folder(f) => Some(f),
            NodeKind::Task(_) => None,
        }
    }

    pub const fn as_folder_mut(&mut self) -> Option<&mut Folder> {
        match &mut self.kind {
            NodeKind::Folder(f) => Some(f),
            NodeKind::Task(_) => None,
        }
    }

    #[must_use]
    pub const fn as_task(&self) -> Option<&Task> {
        match &self.kind {
            NodeKind::Task(t) => Some(t),
            NodeKind::Folder(_) => None,
        }
    }

    pub const fn as_task_mut(&mut self) -> Option<&mut Task> {
        match &mut self.kind {
            NodeKind::Task(t) => Some(t),
            NodeKind::Folder(_) => None,
        }
    }

    /// Folder name or task title, whichever this is.
    #[must_use]
    pub fn display_name(&self) -> &str {
        match &self.kind {
            NodeKind::Folder(f) => &f.name,
            NodeKind::Task(t) => &t.title,
        }
    }

    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        match &self.kind {
            NodeKind::Folder(f) => f.updated_at,
            NodeKind::Task(t) => t.updated_at,
        }
    }

    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        match &self.kind {
            NodeKind::Folder(f) => f.created_at,
            NodeKind::Task(t) => t.created_at,
        }
    }
}
