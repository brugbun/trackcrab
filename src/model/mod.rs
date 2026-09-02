//! Pure domain model. Deliberately free of any `egui` dependency so it can be
//! unit tested headlessly and reasoned about on its own.

pub mod ids;
pub mod node;
pub mod status;
pub mod tree;

pub use ids::NodeId;
pub use node::{CommentSpace, Folder, Node, NodeKind, Task};
pub use status::Status;
pub use tree::{Tree, TreeError};
