pub mod dialogs;
pub mod rows;
pub mod sidebar;
pub mod theme;
pub mod views;

use eframe::egui::Id;

use crate::model::{Node, NodeId, Tree};

/// Which side panel is showing.
///
/// One enum rather than two booleans: the folder tree and the comments panel
/// are mutually exclusive by design, so opening one closes the other. Modelling
/// it as a single value makes that structural instead of a rule four different
/// call sites have to remember.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Panel {
    /// Neither, so the content has the full width.
    #[default]
    None,
    /// The folder tree, on the left.
    Folders,
    /// The comments notebook, overlaying the right.
    Comments,
}

impl Panel {
    /// Turns this panel on, or off if it is already the one showing.
    ///
    /// The directional keys use this, so `Ctrl+Right` twice returns the content
    /// to the middle rather than being a one way trip.
    #[must_use]
    pub fn toggled_to(self, target: Self) -> Self {
        if self == target { Self::None } else { target }
    }
}

/// A command raised by a click, anywhere in the UI. The app owns every mutation
/// and every view change, so the views only ever say what was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    OpenFolder(NodeId),
    OpenTask(NodeId),
    /// Create a blank task in this folder and prompt for its details.
    NewTaskIn(NodeId),
    /// Create a folder under this parent, or at the root when `None`.
    NewFolderIn(Option<NodeId>),
    RenameFolder(NodeId),
    DeleteFolder(NodeId),
    /// Show or hide the comments notebook.
    ToggleComments,
    /// Reparent `node` into `into`, or to the root when `into` is `None`.
    Move {
        node: NodeId,
        into: Option<NodeId>,
    },
}

/// The one panel id the app and the sidebar both need to agree on.
pub const SIDEBAR_ID: &str = "trackcrab_sidebar";

#[must_use]
pub fn sidebar_id() -> Id {
    Id::new(SIDEBAR_ID)
}

/// egui derives the resize handle's widget id from the panel id this way, so
/// reproducing it lets us read the handle's drag response and tell a deliberate
/// resize apart from our own automatic width tracking.
#[must_use]
pub fn sidebar_resize_id() -> Id {
    sidebar_id().with("__resize")
}

/// The sidebar search box's id, fixed rather than derived from layout.
///
/// The keyboard navigation needs to tell "the caret is in the search box" apart
/// from "the caret is in a body of text": a single line search field has no use
/// for the up and down arrows, so they should still walk the tree from there,
/// which makes type-then-arrow-then-Enter one continuous flow.
#[must_use]
pub fn search_box_id() -> Id {
    Id::new("trackcrab_search")
}

/// Stable id for a folder's collapse state, so the sidebar and the navigation
/// code open and close the same rows.
///
/// Filtering uses a *separate* namespace, so auto expanding to matches never
/// clobbers how you had the tree arranged, and clearing the filter puts it back
/// exactly as it was.
#[must_use]
pub fn collapse_id(folder: NodeId, filtering: bool) -> Id {
    let salt = if filtering {
        "trackcrab_collapse_filtered"
    } else {
        "trackcrab_collapse"
    };
    Id::new(salt).with(folder)
}

/// What the sidebar is being narrowed to.
///
/// The status flags are an **allowlist**: none selected means no status
/// filtering at all, and selecting some narrows to exactly those. Inclusive
/// rather than exclusive, so the default state is empty and every click adds
/// something rather than taking it away.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// Free text, matched case insensitively against names and descriptions.
    pub text: String,
    /// One flag per status, indexed by [`Status::ordinal`]. All off means every
    /// status passes.
    pub statuses: [bool; 5],
}

impl Filter {
    /// True when at least one status has been picked out.
    #[must_use]
    pub fn any_status_selected(&self) -> bool {
        self.statuses.iter().any(|on| *on)
    }

    /// True when the filter would hide anything at all.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.text.trim().is_empty() || self.any_status_selected()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Does this task pass on its own merits?
    fn matches_task(&self, task: &crate::model::Task) -> bool {
        // Nothing picked out means no status filtering, not "hide everything".
        if self.any_status_selected() && !self.statuses[task.status.ordinal() as usize] {
            return false;
        }
        let needle = self.text.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        task.title.to_lowercase().contains(&needle)
            || task
                .description
                .as_deref()
                .is_some_and(|d| d.to_lowercase().contains(&needle))
            || task.notes.to_lowercase().contains(&needle)
    }

    /// Does this folder pass on its own name or on anything written in its
    /// comments? A folder is also kept when something beneath it passes, which
    /// [`visible`] works out.
    ///
    /// Comments count because they are where a project's broader context lives:
    /// searching for the customer's name should find the folder whose kickoff
    /// note mentions them, not only folders named after them.
    fn matches_folder(&self, folder: &crate::model::Folder) -> bool {
        let needle = self.text.trim().to_lowercase();
        if needle.is_empty() {
            return false;
        }
        folder.name.to_lowercase().contains(&needle)
            || folder.comments.iter().any(|space| space.matches(&needle))
    }

    /// Which comment space in this folder the current text first matches.
    ///
    /// Opening a search hit lands on the page that actually contains it, so a
    /// folder with a dozen spaces does not leave you cycling to find the match.
    #[must_use]
    pub fn matching_space(&self, folder: &crate::model::Folder) -> Option<usize> {
        let needle = self.text.trim().to_lowercase();
        if needle.is_empty() {
            return None;
        }
        folder
            .comments
            .iter()
            .position(|space| space.matches(&needle))
    }
}

/// The set of nodes the sidebar should draw under a filter.
///
/// A node is kept if it matches, or if any descendant matches, so the path down
/// to a match stays navigable. `None` means no filter is active and everything
/// is shown.
#[must_use]
pub fn visible(tree: &Tree, filter: &Filter) -> Option<std::collections::HashSet<NodeId>> {
    if !filter.is_active() {
        return None;
    }
    let mut keep = std::collections::HashSet::new();
    for root in tree.roots() {
        retain(tree, *root, filter, &mut keep);
    }
    Some(keep)
}

/// Depth first, returning whether this node earned its place.
fn retain(
    tree: &Tree,
    id: NodeId,
    filter: &Filter,
    keep: &mut std::collections::HashSet<NodeId>,
) -> bool {
    let Some(node) = tree.get(id) else {
        return false;
    };
    let mut kept = match &node.kind {
        crate::model::NodeKind::Task(task) => filter.matches_task(task),
        crate::model::NodeKind::Folder(folder) => filter.matches_folder(folder),
    };
    if let Some(folder) = node.as_folder() {
        for child in &folder.children {
            // Every child is walked, not short circuited, so the whole matching
            // subtree ends up in the set rather than just the first branch.
            if retain(tree, *child, filter, keep) {
                kept = true;
            }
        }
    }
    if kept {
        keep.insert(id);
    }
    kept
}

/// Every sidebar row currently on screen, top to bottom, as one flat list.
///
/// This is what the up and down keys walk: the tree read as a flat list,
/// ignoring depth entirely, exactly as the eye reads it. A folder's children are
/// listed only when its row is actually expanded, and only rows the filter keeps
/// are included, so the keyboard can never land on something invisible.
///
/// Collapse state lives in egui's memory rather than in the tree, so this needs
/// the context. It is deliberately derived from the same [`collapse_id`] and
/// [`sorted_children`] the sidebar renders from, rather than kept as a second
/// copy that could drift.
#[must_use]
pub fn flat_rows(ctx: &eframe::egui::Context, tree: &Tree, filter: &Filter) -> Vec<NodeId> {
    let shown = visible(tree, filter);
    let filtering = shown.is_some();
    let mut rows = Vec::new();
    for root in sorted_children(tree, None) {
        push_row(ctx, tree, root, shown.as_ref(), filtering, &mut rows);
    }
    rows
}

fn push_row(
    ctx: &eframe::egui::Context,
    tree: &Tree,
    id: NodeId,
    shown: Option<&std::collections::HashSet<NodeId>>,
    filtering: bool,
    rows: &mut Vec<NodeId>,
) {
    if shown.is_some_and(|set| !set.contains(&id)) {
        return;
    }
    let Some(node) = tree.get(id) else { return };
    rows.push(id);
    if !node.is_folder() {
        return;
    }
    // `load` rather than `load_with_default_open`, so merely asking which rows
    // are on screen never writes collapse state.
    let expanded =
        eframe::egui::collapsing_header::CollapsingState::load(ctx, collapse_id(id, filtering))
            .map_or(filtering, |state| state.is_open());
    if expanded {
        for child in sorted_children(tree, Some(id)) {
            push_row(ctx, tree, child, shown, filtering, rows);
        }
    }
}

/// Children with folders first, then tasks, each keeping insertion order.
///
/// File explorer convention. Sorting lives in the UI rather than the tree, so
/// the model stays a faithful record of the order things were added.
#[must_use]
pub fn sorted_children(tree: &Tree, parent: Option<NodeId>) -> Vec<NodeId> {
    let Ok(children) = tree.children(parent) else {
        return Vec::new();
    };
    let mut folders = Vec::new();
    let mut tasks = Vec::new();
    for id in children {
        match tree.get(*id) {
            Some(node) if node.is_folder() => folders.push(*id),
            Some(_) => tasks.push(*id),
            None => {}
        }
    }
    folders.extend(tasks);
    folders
}

/// Whether `dragged` may be dropped onto `onto`, where `None` means the root.
///
/// Mirrors what [`Tree::move_node`] would allow, plus two things the tree is
/// right not to care about but a UI should: a drop onto the node's existing
/// parent is a no-op, and so is dropping something onto itself. Highlighting
/// either as a valid target would be a lie.
#[must_use]
pub fn can_drop(tree: &Tree, dragged: NodeId, onto: Option<NodeId>) -> bool {
    let Some(node) = tree.get(dragged) else {
        return false;
    };
    match onto {
        // Only folders may sit at the root, and one already there cannot move
        // there again.
        None => node.is_folder() && node.parent.is_some(),
        Some(target) => {
            target != dragged
                && node.parent != Some(target)
                && tree.get(target).is_some_and(Node::is_folder)
                // A folder cannot be moved inside itself.
                && !tree.is_descendant_of(target, dragged)
        }
    }
}

/// Formats a timestamp as `HH:MM:SS DD/MM/YYYY` in the machine's local zone.
/// Stored as UTC, shown as local.
#[must_use]
pub fn local_stamp(when: chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Local;
    when.with_timezone(&Local)
        .format("%H:%M:%S %d/%m/%Y")
        .to_string()
}

/// Folder name or task title, with a stand in for a task whose title has been
/// left empty. Folder names can never be empty, the tree rejects that.
#[must_use]
pub fn row_label(node: &Node) -> &str {
    let name = node.display_name();
    if name.trim().is_empty() {
        "Untitled task"
    } else {
        name
    }
}
