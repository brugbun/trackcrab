use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::model::{Node, NodeId, Tree};

/// Bumped whenever the on-disk shape changes.
///
/// | Version | Change |
/// |---|---|
/// | 1 | Folders and tasks as originally shipped |
/// | 2 | `Task::notes` and `Folder::comments` added |
///
/// Version 2 is a pure addition: both new fields are `#[serde(default)]`, so a
/// version 1 file loads unchanged and simply has no notes or comments. The
/// number is still bumped, because a build that predates them would otherwise
/// read a version 2 file, silently ignore both fields, and then drop them on the
/// next save. Refusing loudly and quarantining the file is the better failure.
pub const SCHEMA_VERSION: u32 = 2;

/// Override the data file location, handy for pointing a WSL build and a Windows
/// build at the same file.
const PATH_ENV: &str = "TRACKCRAB_DATA";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not work out where to store data on this system")]
    NoDataDir,
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not encode data: {0}")]
    Encode(#[from] serde_json::Error),
}

/// On-disk shape. Nodes are a flat list rather than a map keyed by id, which
/// keeps the file readable and lets each node carry its own id.
#[derive(Debug, Serialize, Deserialize)]
struct Persisted {
    schema_version: u32,
    roots: Vec<NodeId>,
    nodes: Vec<Node>,
}

/// What happened when we tried to load.
#[derive(Debug)]
pub enum LoadOutcome {
    /// Loaded cleanly, or there was no file yet and we started empty.
    Loaded { tree: Tree, existed: bool },
    /// The file was unreadable or structurally broken. The original has been
    /// moved aside, never deleted, and we started from empty.
    Recovered {
        tree: Tree,
        quarantined: Option<PathBuf>,
        reason: String,
    },
}

impl LoadOutcome {
    #[must_use]
    pub fn into_tree(self) -> Tree {
        match self {
            Self::Loaded { tree, .. } | Self::Recovered { tree, .. } => tree,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DataStore {
    path: PathBuf,
}

impl DataStore {
    /// Resolves the data file: `$TRACKCRAB_DATA` if set, otherwise the platform
    /// data dir (`~/.local/share/trackcrab` on Linux, `%APPDATA%\\trackcrab` on
    /// Windows).
    pub fn discover() -> Result<Self, StoreError> {
        if let Some(custom) = std::env::var_os(PATH_ENV) {
            let path = PathBuf::from(custom);
            return Ok(Self { path });
        }
        let dirs =
            directories::ProjectDirs::from("", "", "trackcrab").ok_or(StoreError::NoDataDir)?;
        Ok(Self {
            path: dirs.data_dir().join("data.json"),
        })
    }

    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where this app's data used to live, before it was renamed from `work++`.
    ///
    /// Read once, if the current file does not exist yet, so the rename does not
    /// silently orphan anyone's tasks. Safe to delete this and its caller in a
    /// later version.
    fn legacy_path() -> Option<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "work_plus_plus")?;
        Some(dirs.data_dir().join("data.json"))
    }

    /// Never fails. A missing file gives an empty tree, a broken file is moved
    /// aside and reported so the UI can say so.
    #[must_use]
    pub fn load(&self) -> LoadOutcome {
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                // Nothing here yet. Adopt the pre-rename file if there is one.
                if let Some(legacy) = Self::legacy_path()
                    && legacy != self.path
                    && let Ok(raw) = fs::read_to_string(&legacy)
                    && let Ok(tree) = Self::decode(&raw)
                {
                    log::info!(
                        "adopted data from the pre-rename location {}",
                        legacy.display()
                    );
                    return LoadOutcome::Loaded {
                        tree,
                        existed: true,
                    };
                }
                return LoadOutcome::Loaded {
                    tree: Tree::new(),
                    existed: false,
                };
            }
            Err(err) => {
                return LoadOutcome::Recovered {
                    tree: Tree::new(),
                    quarantined: None,
                    reason: format!("could not read {}: {err}", self.path.display()),
                };
            }
        };

        match Self::decode(&raw) {
            Ok(tree) => LoadOutcome::Loaded {
                tree,
                existed: true,
            },
            Err(reason) => LoadOutcome::Recovered {
                tree: Tree::new(),
                quarantined: self.quarantine().ok(),
                reason,
            },
        }
    }

    fn decode(raw: &str) -> Result<Tree, String> {
        let parsed: Persisted =
            serde_json::from_str(raw).map_err(|err| format!("malformed JSON: {err}"))?;

        if parsed.schema_version > SCHEMA_VERSION {
            return Err(format!(
                "file is schema version {} but this build only understands up to {SCHEMA_VERSION}",
                parsed.schema_version
            ));
        }

        let mut nodes: HashMap<NodeId, Node> = HashMap::with_capacity(parsed.nodes.len());
        for node in parsed.nodes {
            if nodes.insert(node.id, node).is_some() {
                return Err("the same node id appears twice".to_owned());
            }
        }

        let tree = Tree::from_parts(nodes, parsed.roots);
        tree.validate()?;
        Ok(tree)
    }

    /// Moves a bad file aside with a timestamped name. The user's data is never
    /// destroyed, only renamed.
    fn quarantine(&self) -> Result<PathBuf, std::io::Error> {
        let stamp = Utc::now().format("%Y%m%d-%H%M%S");
        let target = self.path.with_extension(format!("corrupt.{stamp}.json"));
        fs::rename(&self.path, &target)?;
        Ok(target)
    }

    /// Writes to a sibling temp file, flushes it, then renames over the real
    /// one. A crash mid-write leaves the previous good file intact.
    pub fn save(&self, tree: &Tree) -> Result<(), StoreError> {
        let mut nodes: Vec<Node> = tree.nodes_map().values().cloned().collect();
        // Stable order so the file does not churn between saves.
        nodes.sort_by_key(|n| n.id);

        let payload = Persisted {
            schema_version: SCHEMA_VERSION,
            roots: tree.roots().to_vec(),
            nodes,
        };
        let json = serde_json::to_string_pretty(&payload)?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| StoreError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let tmp = self.path.with_extension("json.tmp");
        {
            let mut file = fs::File::create(&tmp).map_err(|source| StoreError::Io {
                path: tmp.clone(),
                source,
            })?;
            file.write_all(json.as_bytes())
                .map_err(|source| StoreError::Io {
                    path: tmp.clone(),
                    source,
                })?;
            file.sync_all().map_err(|source| StoreError::Io {
                path: tmp.clone(),
                source,
            })?;
        }
        fs::rename(&tmp, &self.path).map_err(|source| StoreError::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}
