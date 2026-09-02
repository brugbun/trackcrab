use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use super::{CommentSpace, Folder, Node, NodeId, NodeKind, Status, Task};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TreeError {
    #[error("no node with id {0}")]
    NotFound(NodeId),
    #[error("node {0} is a task, a folder was expected")]
    NotAFolder(NodeId),
    #[error("node {0} is a folder, a task was expected")]
    NotATask(NodeId),
    #[error("tasks must live inside a folder, they cannot sit at the root")]
    TaskAtRoot,
    #[error("\"{name}\" still contains {count} item(s), empty it before deleting")]
    FolderNotEmpty { name: String, count: usize },
    #[error("a folder cannot be moved into itself or into one of its own descendants")]
    CycleRejected,
    #[error("a blocked task needs a blocked reason")]
    MissingBlockedReason,
    #[error("a name cannot be empty")]
    EmptyName,
    #[error("\"{name}\" has no comment space at position {index}")]
    NoSuchCommentSpace { name: String, index: usize },
}

type Result<T> = std::result::Result<T, TreeError>;

/// Flat arena of nodes plus an ordered list of root folders.
///
/// A flat map with parent back-pointers rather than a nested structure. That
/// keeps mutation and rendering from fighting the borrow checker, and makes
/// ancestor walks (for `updated_at` bubbling) and reparenting straightforward.
#[derive(Debug, Default, Clone)]
pub struct Tree {
    nodes: HashMap<NodeId, Node>,
    roots: Vec<NodeId>,
}

impl Tree {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds a tree from persisted parts. Used only by the store layer.
    pub(crate) fn from_parts(nodes: HashMap<NodeId, Node>, roots: Vec<NodeId>) -> Self {
        Self { nodes, roots }
    }

    pub(crate) const fn nodes_map(&self) -> &HashMap<NodeId, Node> {
        &self.nodes
    }

    // ---------------------------------------------------------------- reading

    #[must_use]
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[must_use]
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn node(&self, id: NodeId) -> Result<&Node> {
        self.nodes.get(&id).ok_or(TreeError::NotFound(id))
    }

    pub fn folder(&self, id: NodeId) -> Result<&Folder> {
        self.node(id)?.as_folder().ok_or(TreeError::NotAFolder(id))
    }

    pub fn task(&self, id: NodeId) -> Result<&Task> {
        self.node(id)?.as_task().ok_or(TreeError::NotATask(id))
    }

    /// Children of a folder, or the root folders when `parent` is `None`.
    pub fn children(&self, parent: Option<NodeId>) -> Result<&[NodeId]> {
        match parent {
            None => Ok(&self.roots),
            Some(id) => Ok(&self.folder(id)?.children),
        }
    }

    /// Ancestors from the immediate parent upwards to a root.
    #[must_use]
    pub fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cursor = self.nodes.get(&id).and_then(|n| n.parent);
        while let Some(current) = cursor {
            out.push(current);
            cursor = self.nodes.get(&current).and_then(|n| n.parent);
        }
        out
    }

    /// Names from the outermost folder down to and including this node, for
    /// breadcrumbs.
    #[must_use]
    pub fn path_names(&self, id: NodeId) -> Vec<String> {
        let mut names: Vec<String> = self
            .ancestors(id)
            .iter()
            .filter_map(|a| self.nodes.get(a))
            .map(|n| n.display_name().to_owned())
            .collect();
        names.reverse();
        if let Some(node) = self.nodes.get(&id) {
            names.push(node.display_name().to_owned());
        }
        names
    }

    /// True when `candidate` sits anywhere beneath `ancestor`.
    #[must_use]
    pub fn is_descendant_of(&self, candidate: NodeId, ancestor: NodeId) -> bool {
        self.ancestors(candidate).contains(&ancestor)
    }

    /// Every node beneath `parent`, depth first, `parent` excluded.
    #[must_use]
    pub fn descendants(&self, parent: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self
            .get(parent)
            .and_then(Node::as_folder)
            .map(|f| f.children.iter().copied().rev().collect())
            .unwrap_or_default();
        while let Some(id) = stack.pop() {
            out.push(id);
            if let Some(folder) = self.get(id).and_then(Node::as_folder) {
                stack.extend(folder.children.iter().copied().rev());
            }
        }
        out
    }

    // ---------------------------------------------------------------- writing

    /// Creates a folder under `parent`, or at the root when `parent` is `None`.
    pub fn create_folder(
        &mut self,
        parent: Option<NodeId>,
        name: impl Into<String>,
    ) -> Result<NodeId> {
        let name = clean_name(name)?;
        if let Some(parent) = parent {
            // Reject up front so we never leave a half-inserted node behind.
            self.folder(parent)?;
        }
        let now = Utc::now();
        let id = NodeId::new();
        self.nodes.insert(
            id,
            Node {
                id,
                parent,
                kind: NodeKind::Folder(Folder {
                    name,
                    created_at: now,
                    updated_at: now,
                    children: Vec::new(),
                    comments: Vec::new(),
                }),
            },
        );
        self.attach(id, parent);
        self.touch_ancestors(id, now);
        Ok(id)
    }

    /// Creates a task inside `parent`. There is no root variant, by design: a
    /// task always has a folder parent and can never be orphaned.
    pub fn create_task(
        &mut self,
        parent: NodeId,
        title: impl Into<String>,
        description: Option<String>,
        status: Status,
    ) -> Result<NodeId> {
        self.folder(parent)?;
        validate_status(&status)?;
        let now = Utc::now();
        let id = NodeId::new();
        let mut task = Task {
            title: title.into(),
            description: None,
            status,
            created_at: now,
            updated_at: now,
            attributed_minutes: 0,
            notes: String::new(),
        };
        task.set_description(description.unwrap_or_default());
        self.nodes.insert(
            id,
            Node {
                id,
                parent: Some(parent),
                kind: NodeKind::Task(task),
            },
        );
        self.attach(id, Some(parent));
        self.touch_ancestors(id, now);
        Ok(id)
    }

    pub fn rename_folder(&mut self, id: NodeId, name: impl Into<String>) -> Result<()> {
        let name = clean_name(name)?;
        let now = Utc::now();
        let folder = self
            .nodes
            .get_mut(&id)
            .ok_or(TreeError::NotFound(id))?
            .as_folder_mut()
            .ok_or(TreeError::NotAFolder(id))?;
        folder.name = name;
        folder.updated_at = now;
        self.touch_ancestors(id, now);
        Ok(())
    }

    /// Mutate a task through a closure, then validate and stamp times.
    ///
    /// The single write path for task edits, so `updated_at` and the blocked
    /// reason rule can never be forgotten at a call site. A rejected edit is
    /// rolled back.
    pub fn edit_task<F>(&mut self, id: NodeId, edit: F) -> Result<()>
    where
        F: FnOnce(&mut Task),
    {
        let now = Utc::now();
        let node = self.nodes.get_mut(&id).ok_or(TreeError::NotFound(id))?;
        let task = node.as_task_mut().ok_or(TreeError::NotATask(id))?;
        let snapshot = task.clone();
        edit(task);
        if let Err(err) = validate_status(&task.status) {
            *task = snapshot;
            return Err(err);
        }
        task.updated_at = now;
        self.touch_ancestors(id, now);
        Ok(())
    }

    // --------------------------------------------------------------- comments

    /// Adds an empty comment space to a folder and returns its index.
    ///
    /// The title is auto numbered from the count that already exists, so it is
    /// never blank and never blocks you on naming a page before you can dump a
    /// thought into it.
    pub fn add_comment_space(&mut self, id: NodeId) -> Result<usize> {
        let now = Utc::now();
        let folder = self
            .nodes
            .get_mut(&id)
            .ok_or(TreeError::NotFound(id))?
            .as_folder_mut()
            .ok_or(TreeError::NotAFolder(id))?;

        // Numbered by the highest existing number rather than the count, so
        // deleting space 2 of 3 does not make the next one a duplicate.
        let next = folder
            .comments
            .iter()
            .filter_map(|space| CommentSpace::auto_number(&space.title))
            .max()
            .map_or(folder.comments.len() + 1, |highest| highest + 1);

        folder
            .comments
            .push(CommentSpace::new(CommentSpace::auto_title(next)));
        folder.updated_at = now;
        let index = folder.comments.len() - 1;
        self.touch_ancestors(id, now);
        Ok(index)
    }

    /// Mutates one comment space through a closure, then stamps the folder and
    /// everything above it.
    ///
    /// The single write path for comment edits, for the same reason
    /// [`Self::edit_task`] is: bubbling cannot be forgotten at a call site.
    pub fn edit_comment_space<F>(&mut self, id: NodeId, index: usize, edit: F) -> Result<()>
    where
        F: FnOnce(&mut CommentSpace),
    {
        let now = Utc::now();
        let folder = self
            .nodes
            .get_mut(&id)
            .ok_or(TreeError::NotFound(id))?
            .as_folder_mut()
            .ok_or(TreeError::NotAFolder(id))?;

        let name = folder.name.clone();
        let space = folder
            .comments
            .get_mut(index)
            .ok_or(TreeError::NoSuchCommentSpace { name, index })?;
        edit(space);
        space.updated_at = now;
        folder.updated_at = now;
        self.touch_ancestors(id, now);
        Ok(())
    }

    /// Removes a comment space. Returns the index that should be shown next,
    /// clamped so the caller never lands past the end.
    pub fn delete_comment_space(&mut self, id: NodeId, index: usize) -> Result<usize> {
        let now = Utc::now();
        let folder = self
            .nodes
            .get_mut(&id)
            .ok_or(TreeError::NotFound(id))?
            .as_folder_mut()
            .ok_or(TreeError::NotAFolder(id))?;

        if index >= folder.comments.len() {
            return Err(TreeError::NoSuchCommentSpace {
                name: folder.name.clone(),
                index,
            });
        }
        folder.comments.remove(index);
        folder.updated_at = now;
        let next = index.min(folder.comments.len().saturating_sub(1));
        self.touch_ancestors(id, now);
        Ok(next)
    }

    /// The comment spaces of a folder, or an empty slice for anything else.
    #[must_use]
    pub fn comment_spaces(&self, id: NodeId) -> &[CommentSpace] {
        self.get(id)
            .and_then(Node::as_folder)
            .map_or(&[], |folder| folder.comments.as_slice())
    }

    pub fn delete_task(&mut self, id: NodeId) -> Result<()> {
        self.task(id)?;
        let parent = self.nodes[&id].parent;
        let now = Utc::now();
        self.detach(id, parent);
        self.nodes.remove(&id);
        if let Some(parent) = parent {
            self.touch_self_and_ancestors(parent, now);
        }
        Ok(())
    }

    /// Deletes an empty folder. A folder holding anything at all is refused, so
    /// nothing is ever removed by surprise.
    pub fn delete_folder(&mut self, id: NodeId) -> Result<()> {
        let folder = self.folder(id)?;
        if !folder.children.is_empty() {
            return Err(TreeError::FolderNotEmpty {
                name: folder.name.clone(),
                count: folder.children.len(),
            });
        }
        let parent = self.nodes[&id].parent;
        let now = Utc::now();
        self.detach(id, parent);
        self.nodes.remove(&id);
        if let Some(parent) = parent {
            self.touch_self_and_ancestors(parent, now);
        }
        Ok(())
    }

    /// Reparents a node. Both the old and the new ancestor chains are stamped,
    /// since both changed.
    pub fn move_node(&mut self, id: NodeId, new_parent: Option<NodeId>) -> Result<()> {
        let node = self.node(id)?;
        let is_task = node.is_task();
        let old_parent = node.parent;

        match new_parent {
            None if is_task => return Err(TreeError::TaskAtRoot),
            None => {}
            Some(parent) => {
                self.folder(parent)?;
                if parent == id || self.is_descendant_of(parent, id) {
                    return Err(TreeError::CycleRejected);
                }
            }
        }

        if old_parent == new_parent {
            return Ok(());
        }

        let now = Utc::now();
        self.detach(id, old_parent);
        if let Some(node) = self.nodes.get_mut(&id) {
            node.parent = new_parent;
        }
        self.attach(id, new_parent);

        if let Some(old) = old_parent {
            self.touch_self_and_ancestors(old, now);
        }
        self.touch_ancestors(id, now);
        Ok(())
    }

    /// Reorders a child within its current parent.
    pub fn reorder_child(&mut self, id: NodeId, new_index: usize) -> Result<()> {
        let parent = self.node(id)?.parent;
        let now = Utc::now();
        let siblings = match parent {
            None => &mut self.roots,
            Some(parent) => {
                &mut self
                    .nodes
                    .get_mut(&parent)
                    .ok_or(TreeError::NotFound(parent))?
                    .as_folder_mut()
                    .ok_or(TreeError::NotAFolder(parent))?
                    .children
            }
        };
        if let Some(current) = siblings.iter().position(|s| *s == id) {
            let item = siblings.remove(current);
            let target = new_index.min(siblings.len());
            siblings.insert(target, item);
        }
        if let Some(parent) = parent {
            self.touch_self_and_ancestors(parent, now);
        }
        Ok(())
    }

    // --------------------------------------------------------------- internal

    fn attach(&mut self, id: NodeId, parent: Option<NodeId>) {
        match parent {
            None => self.roots.push(id),
            Some(parent) => {
                if let Some(folder) = self.nodes.get_mut(&parent).and_then(Node::as_folder_mut) {
                    folder.children.push(id);
                }
            }
        }
    }

    fn detach(&mut self, id: NodeId, parent: Option<NodeId>) {
        match parent {
            None => self.roots.retain(|r| *r != id),
            Some(parent) => {
                if let Some(folder) = self.nodes.get_mut(&parent).and_then(Node::as_folder_mut) {
                    folder.children.retain(|c| *c != id);
                }
            }
        }
    }

    /// Stamps every ancestor of `id`, so a change at any depth surfaces on the
    /// folders above it.
    fn touch_ancestors(&mut self, id: NodeId, now: DateTime<Utc>) {
        for ancestor in self.ancestors(id) {
            if let Some(folder) = self.nodes.get_mut(&ancestor).and_then(Node::as_folder_mut) {
                folder.updated_at = now;
            }
        }
    }

    fn touch_self_and_ancestors(&mut self, id: NodeId, now: DateTime<Utc>) {
        if let Some(folder) = self.nodes.get_mut(&id).and_then(Node::as_folder_mut) {
            folder.updated_at = now;
        }
        self.touch_ancestors(id, now);
    }

    /// Structural self check: every root is a parentless folder, every listed
    /// child exists and points back at its parent, and nothing is unreachable.
    /// Run after loading from disk, and asserted in tests.
    pub fn validate(&self) -> std::result::Result<(), String> {
        let mut seen: HashSet<NodeId> = HashSet::new();

        for root in &self.roots {
            let node = self
                .nodes
                .get(root)
                .ok_or_else(|| format!("root {root} is not present in the node table"))?;
            if !node.is_folder() {
                return Err(format!(
                    "root {root} is a task, only folders may sit at the root"
                ));
            }
            if node.parent.is_some() {
                return Err(format!("root {root} claims a parent"));
            }
        }

        let mut stack: Vec<NodeId> = self.roots.clone();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                return Err(format!(
                    "node {id} is reachable more than once, the tree has a cycle or a shared child"
                ));
            }
            let node = self
                .nodes
                .get(&id)
                .ok_or_else(|| format!("node {id} is referenced but missing"))?;
            if let Some(folder) = node.as_folder() {
                for child in &folder.children {
                    let child_node = self
                        .nodes
                        .get(child)
                        .ok_or_else(|| format!("folder {id} lists missing child {child}"))?;
                    if child_node.parent != Some(id) {
                        return Err(format!(
                            "child {child} is listed under {id} but points at {:?}",
                            child_node.parent
                        ));
                    }
                    stack.push(*child);
                }
            }
        }

        if seen.len() != self.nodes.len() {
            let orphans: Vec<String> = self
                .nodes
                .keys()
                .filter(|id| !seen.contains(id))
                .map(ToString::to_string)
                .collect();
            return Err(format!(
                "{} node(s) are unreachable from any root: {}",
                orphans.len(),
                orphans.join(", ")
            ));
        }

        Ok(())
    }
}

fn clean_name(name: impl Into<String>) -> Result<String> {
    let name: String = name.into();
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(TreeError::EmptyName);
    }
    Ok(trimmed.to_owned())
}

fn validate_status(status: &Status) -> Result<()> {
    match status {
        Status::Blocked(reason) if reason.trim().is_empty() => Err(TreeError::MissingBlockedReason),
        _ => Ok(()),
    }
}
