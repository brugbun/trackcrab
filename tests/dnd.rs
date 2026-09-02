//! Drag and drop legality.
//!
//! The predicate the UI uses to decide whether a drop is allowed, and to colour
//! the target before you release. It has to agree with what the tree would
//! actually permit, or the highlight lies.

use trackcrab::model::{NodeId, Status, Tree};
use trackcrab::ui::can_drop;

struct Fixture {
    tree: Tree,
    work: NodeId,
    clients: NodeId,
    acme: NodeId,
    task: NodeId,
    personal: NodeId,
}

/// Work > Clients > Acme > task, plus a second root folder.
fn fixture() -> Fixture {
    let mut tree = Tree::new();
    let work = tree.create_folder(None, "Work").unwrap();
    let clients = tree.create_folder(Some(work), "Clients").unwrap();
    let acme = tree.create_folder(Some(clients), "Acme").unwrap();
    let task = tree
        .create_task(acme, "Cut over the database", None, Status::Open)
        .unwrap();
    let personal = tree.create_folder(None, "Personal").unwrap();
    Fixture {
        tree,
        work,
        clients,
        acme,
        task,
        personal,
    }
}

#[test]
fn a_task_can_be_dropped_into_any_other_folder() {
    let f = fixture();
    assert!(can_drop(&f.tree, f.task, Some(f.work)));
    assert!(can_drop(&f.tree, f.task, Some(f.clients)));
    assert!(can_drop(&f.tree, f.task, Some(f.personal)));
}

#[test]
fn a_task_can_never_be_dropped_at_the_root() {
    let f = fixture();
    assert!(
        !can_drop(&f.tree, f.task, None),
        "a task must always have a folder"
    );
}

#[test]
fn a_folder_cannot_be_dropped_into_itself_or_its_own_descendants() {
    let f = fixture();
    assert!(!can_drop(&f.tree, f.work, Some(f.work)));
    assert!(!can_drop(&f.tree, f.work, Some(f.clients)));
    assert!(!can_drop(&f.tree, f.work, Some(f.acme)));
    // The other direction is fine.
    assert!(can_drop(&f.tree, f.acme, Some(f.personal)));
}

#[test]
fn a_folder_cannot_be_dropped_onto_a_task() {
    let f = fixture();
    assert!(
        !can_drop(&f.tree, f.personal, Some(f.task)),
        "nothing lives inside a task"
    );
}

#[test]
fn dropping_onto_the_current_parent_is_not_offered() {
    let f = fixture();
    // The tree would allow this as a no-op, but highlighting it as a valid
    // target would suggest something is going to happen.
    assert!(!can_drop(&f.tree, f.task, Some(f.acme)));
    assert!(!can_drop(&f.tree, f.clients, Some(f.work)));
}

#[test]
fn a_root_folder_cannot_be_dropped_at_the_root_again() {
    let f = fixture();
    assert!(!can_drop(&f.tree, f.work, None));
    assert!(!can_drop(&f.tree, f.personal, None));
}

#[test]
fn a_nested_folder_can_be_dropped_back_to_the_root() {
    let f = fixture();
    assert!(can_drop(&f.tree, f.acme, None));
    assert!(can_drop(&f.tree, f.clients, None));
}

#[test]
fn an_unknown_node_is_never_droppable() {
    let f = fixture();
    let ghost = NodeId::new();
    assert!(!can_drop(&f.tree, ghost, Some(f.work)));
    assert!(!can_drop(&f.tree, ghost, None));
}

#[test]
fn every_offered_drop_is_one_the_tree_actually_accepts() {
    // The predicate and the tree must agree. Any pair the UI would highlight
    // green has to succeed, and the tree must stay structurally sound after.
    let f = fixture();
    let all = {
        let mut all = f.tree.roots().to_vec();
        for root in f.tree.roots() {
            all.extend(f.tree.descendants(*root));
        }
        all
    };

    for dragged in &all {
        let mut targets: Vec<Option<NodeId>> = all.iter().map(|id| Some(*id)).collect();
        targets.push(None);
        for target in targets {
            if !can_drop(&f.tree, *dragged, target) {
                continue;
            }
            let mut tree = f.tree.clone();
            tree.move_node(*dragged, target).unwrap_or_else(|err| {
                panic!("can_drop offered {dragged:?} -> {target:?} but the tree refused: {err}")
            });
            tree.validate().unwrap_or_else(|err| {
                panic!("moving {dragged:?} -> {target:?} corrupted the tree: {err}")
            });
        }
    }
}

#[test]
fn a_moved_folder_brings_its_whole_subtree() {
    let mut f = fixture();
    assert!(can_drop(&f.tree, f.clients, Some(f.personal)));
    f.tree.move_node(f.clients, Some(f.personal)).unwrap();

    assert!(f.tree.is_descendant_of(f.task, f.personal));
    assert!(!f.tree.is_descendant_of(f.task, f.work));
    assert_eq!(f.tree.path_names(f.task).first().unwrap(), "Personal");
    assert!(f.tree.validate().is_ok());
}
