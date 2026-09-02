use std::thread::sleep;
use std::time::Duration;

use trackcrab::model::{NodeId, Status, Tree, TreeError};

/// `Utc::now()` is nanosecond resolution, but a tiny pause makes the timestamp
/// assertions robust regardless of clock granularity on the host.
fn tick() {
    sleep(Duration::from_millis(2));
}

struct Fixture {
    tree: Tree,
    work: NodeId,
    clients: NodeId,
    acme: NodeId,
    task: NodeId,
}

/// Work > Clients > Acme > "Migrate the estate"
fn fixture() -> Fixture {
    let mut tree = Tree::new();
    let work = tree.create_folder(None, "Work").unwrap();
    let clients = tree.create_folder(Some(work), "Clients").unwrap();
    let acme = tree.create_folder(Some(clients), "Acme").unwrap();
    let task = tree
        .create_task(acme, "Migrate the estate", None, Status::Open)
        .unwrap();
    Fixture {
        tree,
        work,
        clients,
        acme,
        task,
    }
}

// --------------------------------------------------------------- structure

#[test]
fn folders_nest_arbitrarily_deep() {
    let mut tree = Tree::new();
    let mut parent = tree.create_folder(None, "L0").unwrap();
    for depth in 1..=200 {
        parent = tree
            .create_folder(Some(parent), format!("L{depth}"))
            .unwrap();
    }
    assert_eq!(tree.ancestors(parent).len(), 200);
    let path = tree.path_names(parent);
    assert_eq!(path.first().unwrap(), "L0");
    assert_eq!(path.last().unwrap(), "L200");
    assert_eq!(path.len(), 201);
}

#[test]
fn a_task_always_has_a_folder_parent() {
    let f = fixture();
    assert_eq!(f.tree.node(f.task).unwrap().parent, Some(f.acme));
}

#[test]
fn tasks_cannot_be_moved_to_the_root() {
    let mut f = fixture();
    assert_eq!(f.tree.move_node(f.task, None), Err(TreeError::TaskAtRoot));
    // The rejected move left the task exactly where it was.
    assert_eq!(f.tree.node(f.task).unwrap().parent, Some(f.acme));
    assert!(!f.tree.roots().contains(&f.task));
}

#[test]
fn tasks_cannot_parent_anything() {
    let mut f = fixture();
    let err = f.tree.create_task(f.task, "Nope", None, Status::Open);
    assert_eq!(err.unwrap_err(), TreeError::NotAFolder(f.task));
    let err = f.tree.create_folder(Some(f.task), "Nope");
    assert_eq!(err.unwrap_err(), TreeError::NotAFolder(f.task));
}

#[test]
fn a_folder_cannot_be_moved_into_its_own_descendant() {
    let mut f = fixture();
    assert_eq!(
        f.tree.move_node(f.clients, Some(f.acme)),
        Err(TreeError::CycleRejected)
    );
    assert_eq!(
        f.tree.move_node(f.clients, Some(f.clients)),
        Err(TreeError::CycleRejected)
    );
    // Still intact.
    assert!(f.tree.validate().is_ok());
}

#[test]
fn moving_a_folder_carries_its_whole_subtree() {
    let mut f = fixture();
    let archive = f.tree.create_folder(None, "Archive").unwrap();
    f.tree.move_node(f.clients, Some(archive)).unwrap();

    assert_eq!(f.tree.node(f.clients).unwrap().parent, Some(archive));
    // The task is still four levels down, now under Archive.
    assert_eq!(f.tree.path_names(f.task).first().unwrap(), "Archive");
    assert!(f.tree.is_descendant_of(f.task, archive));
    assert!(!f.tree.is_descendant_of(f.task, f.work));
    assert!(f.tree.validate().is_ok());
}

#[test]
fn descendants_walks_the_whole_subtree() {
    let f = fixture();
    let found = f.tree.descendants(f.work);
    assert_eq!(found.len(), 3);
    assert!(found.contains(&f.clients));
    assert!(found.contains(&f.acme));
    assert!(found.contains(&f.task));
}

// ------------------------------------------------------------- timestamps

#[test]
fn editing_a_deep_task_bubbles_updated_at_to_every_ancestor() {
    let mut f = fixture();
    let before: Vec<_> = [f.work, f.clients, f.acme]
        .iter()
        .map(|id| f.tree.folder(*id).unwrap().updated_at)
        .collect();

    tick();
    f.tree
        .edit_task(f.task, |t| t.title = "Migrate the estate, phase 2".into())
        .unwrap();

    for (id, was) in [f.work, f.clients, f.acme].iter().zip(before) {
        let now = f.tree.folder(*id).unwrap().updated_at;
        assert!(now > was, "folder {id} did not have updated_at bubbled");
    }
}

#[test]
fn deleting_a_task_bubbles_updated_at_upwards() {
    let mut f = fixture();
    let before = f.tree.folder(f.work).unwrap().updated_at;
    tick();
    f.tree.delete_task(f.task).unwrap();
    assert!(f.tree.folder(f.work).unwrap().updated_at > before);
    assert!(!f.tree.contains(f.task));
    assert!(f.tree.folder(f.acme).unwrap().children.is_empty());
}

#[test]
fn moving_a_node_stamps_both_the_old_and_the_new_chain() {
    let mut f = fixture();
    let other = f.tree.create_folder(None, "Other").unwrap();
    let inner = f.tree.create_folder(Some(other), "Inner").unwrap();

    let old_chain_before = f.tree.folder(f.acme).unwrap().updated_at;
    let new_chain_before = f.tree.folder(other).unwrap().updated_at;

    tick();
    f.tree.move_node(f.task, Some(inner)).unwrap();

    assert!(
        f.tree.folder(f.acme).unwrap().updated_at > old_chain_before,
        "the folder the task left was not stamped"
    );
    assert!(
        f.tree.folder(other).unwrap().updated_at > new_chain_before,
        "the folder the task arrived under was not stamped"
    );
}

#[test]
fn created_at_is_never_touched_by_an_edit() {
    let mut f = fixture();
    let created = f.tree.task(f.task).unwrap().created_at;
    tick();
    f.tree
        .edit_task(f.task, |t| t.title = "Renamed".into())
        .unwrap();
    assert_eq!(f.tree.task(f.task).unwrap().created_at, created);
}

// ---------------------------------------------------------------- deletion

#[test]
fn a_folder_holding_anything_refuses_to_be_deleted() {
    let mut f = fixture();
    match f.tree.delete_folder(f.acme) {
        Err(TreeError::FolderNotEmpty { name, count }) => {
            assert_eq!(name, "Acme");
            assert_eq!(count, 1);
        }
        other => panic!("expected FolderNotEmpty, got {other:?}"),
    }
    assert!(f.tree.contains(f.acme));
    assert!(f.tree.contains(f.task));
}

#[test]
fn an_emptied_folder_deletes_cleanly() {
    let mut f = fixture();
    f.tree.delete_task(f.task).unwrap();
    f.tree.delete_folder(f.acme).unwrap();
    assert!(!f.tree.contains(f.acme));
    assert!(f.tree.folder(f.clients).unwrap().children.is_empty());
    assert!(f.tree.validate().is_ok());
}

// ------------------------------------------------------------------ status

#[test]
fn blocked_without_a_reason_is_refused_on_create_and_on_edit() {
    let mut f = fixture();
    assert_eq!(
        f.tree
            .create_task(f.acme, "Stuck", None, Status::Blocked(String::new()))
            .unwrap_err(),
        TreeError::MissingBlockedReason
    );
    assert_eq!(
        f.tree
            .create_task(f.acme, "Stuck", None, Status::Blocked("   ".into()))
            .unwrap_err(),
        TreeError::MissingBlockedReason
    );
    assert_eq!(
        f.tree
            .edit_task(f.task, |t| t.status = Status::Blocked(String::new()))
            .unwrap_err(),
        TreeError::MissingBlockedReason
    );
}

#[test]
fn a_refused_edit_rolls_the_task_back_completely() {
    let mut f = fixture();
    let before = f.tree.task(f.task).unwrap().clone();
    let err = f.tree.edit_task(f.task, |t| {
        t.title = "Half applied".into();
        t.attributed_minutes = 999;
        t.status = Status::Blocked(String::new());
    });
    assert_eq!(err.unwrap_err(), TreeError::MissingBlockedReason);
    assert_eq!(
        f.tree.task(f.task).unwrap(),
        &before,
        "a rejected edit must not leave partial changes behind"
    );
}

#[test]
fn blocked_with_a_reason_is_accepted_and_readable() {
    let mut f = fixture();
    f.tree
        .edit_task(f.task, |t| {
            t.status = Status::Blocked("waiting on AWS quota increase".into());
        })
        .unwrap();
    assert_eq!(
        f.tree.task(f.task).unwrap().status.blocked_reason(),
        Some("waiting on AWS quota increase")
    );
}

#[test]
fn every_status_has_its_agreed_colour() {
    assert_eq!(Status::Open.rgb(), (109, 190, 255));
    assert_eq!(Status::InProgress.rgb(), (240, 200, 60));
    assert_eq!(Status::Completed.rgb(), (110, 224, 170));
    assert_eq!(Status::Blocked(String::new()).rgb(), (58, 62, 68));
    assert_eq!(Status::Cancelled.rgb(), (232, 76, 76));
    assert_eq!(Status::variants().len(), 5);
}

// ------------------------------------------------------------------- misc

#[test]
fn attributed_time_formats_exactly_as_specified() {
    let mut f = fixture();
    let cases = [
        (0u32, 0u32, ""),
        (15, 0, "15h"),
        (2, 0, "2h"),
        (1, 30, "1h 30m"),
        (0, 45, "45m"),
    ];
    for (h, m, expected) in cases {
        f.tree
            .edit_task(f.task, |t| t.set_attributed_hm(h, m))
            .unwrap();
        assert_eq!(
            f.tree.task(f.task).unwrap().attributed_label(),
            expected,
            "{h}h {m}m formatted wrongly"
        );
    }
}

#[test]
fn minutes_over_sixty_roll_into_hours() {
    let mut f = fixture();
    f.tree
        .edit_task(f.task, |t| t.set_attributed_hm(1, 90))
        .unwrap();
    let task = f.tree.task(f.task).unwrap();
    assert_eq!(task.attributed_minutes, 150);
    assert_eq!(task.attributed_hm(), (2, 30));
    assert_eq!(task.attributed_label(), "2h 30m");
}

#[test]
fn a_blank_description_is_stored_as_none() {
    let mut f = fixture();
    f.tree
        .edit_task(f.task, |t| t.set_description("   \n  "))
        .unwrap();
    assert_eq!(f.tree.task(f.task).unwrap().description, None);
    assert_eq!(f.tree.task(f.task).unwrap().description_str(), "");

    f.tree
        .edit_task(f.task, |t| t.set_description("real content"))
        .unwrap();
    assert_eq!(
        f.tree.task(f.task).unwrap().description.as_deref(),
        Some("real content")
    );
}

#[test]
fn names_are_trimmed_and_blank_names_refused() {
    let mut tree = Tree::new();
    let id = tree.create_folder(None, "  Padded  ").unwrap();
    assert_eq!(tree.folder(id).unwrap().name, "Padded");
    assert_eq!(
        tree.create_folder(None, "   ").unwrap_err(),
        TreeError::EmptyName
    );
    assert_eq!(
        tree.rename_folder(id, "").unwrap_err(),
        TreeError::EmptyName
    );
}

#[test]
fn a_failed_create_leaves_no_stray_node_behind() {
    let mut tree = Tree::new();
    let before = tree.len();
    let _ = tree.create_folder(None, "");
    let _ = tree.create_folder(Some(NodeId::new()), "orphan parent");
    assert_eq!(tree.len(), before);
    assert!(tree.validate().is_ok());
}

#[test]
fn reordering_moves_a_child_within_its_parent() {
    let mut tree = Tree::new();
    let root = tree.create_folder(None, "Root").unwrap();
    let a = tree.create_task(root, "A", None, Status::Open).unwrap();
    let b = tree.create_task(root, "B", None, Status::Open).unwrap();
    let c = tree.create_task(root, "C", None, Status::Open).unwrap();
    assert_eq!(tree.children(Some(root)).unwrap().to_vec(), vec![a, b, c]);

    tree.reorder_child(c, 0).unwrap();
    assert_eq!(tree.children(Some(root)).unwrap().to_vec(), vec![c, a, b]);

    // An index past the end clamps rather than panicking.
    tree.reorder_child(c, 99).unwrap();
    assert_eq!(tree.children(Some(root)).unwrap().to_vec(), vec![a, b, c]);
}

#[test]
fn multiple_root_folders_are_allowed() {
    let mut tree = Tree::new();
    tree.create_folder(None, "One").unwrap();
    tree.create_folder(None, "Two").unwrap();
    tree.create_folder(None, "Three").unwrap();
    assert_eq!(tree.roots().len(), 3);
    assert!(tree.validate().is_ok());
}

// ------------------------------------------------------- notes and comments

#[test]
fn a_task_starts_with_no_notes_and_keeps_what_it_is_given() {
    let mut f = fixture();
    assert_eq!(f.tree.task(f.task).unwrap().notes, "");

    f.tree
        .edit_task(f.task, |t| t.notes = "Ring the DBA first.".to_owned())
        .unwrap();
    assert_eq!(f.tree.task(f.task).unwrap().notes, "Ring the DBA first.");
}

#[test]
fn editing_notes_bubbles_updated_at_like_any_other_edit() {
    let mut f = fixture();
    let before = f.tree.folder(f.work).unwrap().updated_at;
    tick();
    f.tree
        .edit_task(f.task, |t| t.notes = "something".to_owned())
        .unwrap();
    assert!(
        f.tree.folder(f.work).unwrap().updated_at > before,
        "a note is a change like any other"
    );
}

#[test]
fn a_folder_starts_with_no_comment_spaces() {
    let f = fixture();
    assert!(f.tree.comment_spaces(f.work).is_empty());
}

#[test]
fn adding_comment_spaces_numbers_them_in_order() {
    let mut f = fixture();
    assert_eq!(f.tree.add_comment_space(f.work).unwrap(), 0);
    assert_eq!(f.tree.add_comment_space(f.work).unwrap(), 1);
    assert_eq!(f.tree.add_comment_space(f.work).unwrap(), 2);

    let titles: Vec<&str> = f
        .tree
        .comment_spaces(f.work)
        .iter()
        .map(|s| s.title.as_str())
        .collect();
    assert_eq!(titles, ["Comments 1", "Comments 2", "Comments 3"]);
}

#[test]
fn a_new_space_never_reuses_a_number_still_in_play() {
    // Numbering from the highest existing number rather than the count, so
    // deleting from the middle cannot produce two spaces with the same title.
    let mut f = fixture();
    for _ in 0..3 {
        f.tree.add_comment_space(f.work).unwrap();
    }
    f.tree.delete_comment_space(f.work, 1).unwrap();

    let index = f.tree.add_comment_space(f.work).unwrap();
    assert_eq!(f.tree.comment_spaces(f.work)[index].title, "Comments 4");
    let titles: Vec<&str> = f
        .tree
        .comment_spaces(f.work)
        .iter()
        .map(|s| s.title.as_str())
        .collect();
    assert_eq!(titles, ["Comments 1", "Comments 3", "Comments 4"]);
}

#[test]
fn a_renamed_space_does_not_confuse_the_numbering() {
    let mut f = fixture();
    let first = f.tree.add_comment_space(f.work).unwrap();
    f.tree
        .edit_comment_space(f.work, first, |s| s.title = "Kickoff".to_owned())
        .unwrap();
    let second = f.tree.add_comment_space(f.work).unwrap();
    // Nothing numbered remains, so it falls back to the count.
    assert_eq!(f.tree.comment_spaces(f.work)[second].title, "Comments 2");
}

#[test]
fn editing_a_comment_space_bubbles_updated_at() {
    let mut f = fixture();
    let index = f.tree.add_comment_space(f.work).unwrap();
    let before = f.tree.folder(f.clients).unwrap().updated_at;
    let root_before = f.tree.folder(f.work).unwrap().updated_at;
    tick();

    f.tree
        .edit_comment_space(f.work, index, |s| s.body = "Budget signed off.".to_owned())
        .unwrap();

    assert!(f.tree.folder(f.work).unwrap().updated_at > root_before);
    // Clients is *below* Work, so it must not have moved.
    assert_eq!(f.tree.folder(f.clients).unwrap().updated_at, before);
}

#[test]
fn editing_a_comment_on_a_nested_folder_stamps_every_ancestor() {
    let mut f = fixture();
    let index = f.tree.add_comment_space(f.acme).unwrap();
    let before: Vec<_> = [f.work, f.clients, f.acme]
        .iter()
        .map(|id| f.tree.folder(*id).unwrap().updated_at)
        .collect();
    tick();

    f.tree
        .edit_comment_space(f.acme, index, |s| s.body = "deep".to_owned())
        .unwrap();

    for (id, was) in [f.work, f.clients, f.acme].iter().zip(before) {
        assert!(
            f.tree.folder(*id).unwrap().updated_at > was,
            "folder {id} was not stamped"
        );
    }
}

#[test]
fn deleting_a_space_reports_where_to_land_next() {
    let mut f = fixture();
    for _ in 0..3 {
        f.tree.add_comment_space(f.work).unwrap();
    }
    // Deleting the middle leaves you on the one that slid into its place.
    assert_eq!(f.tree.delete_comment_space(f.work, 1).unwrap(), 1);
    // Deleting the last clamps back rather than pointing past the end.
    assert_eq!(f.tree.delete_comment_space(f.work, 1).unwrap(), 0);
    // Deleting the only one leaves nothing, and index 0 is the honest answer.
    assert_eq!(f.tree.delete_comment_space(f.work, 0).unwrap(), 0);
    assert!(f.tree.comment_spaces(f.work).is_empty());
}

#[test]
fn a_bad_comment_index_is_refused_rather_than_panicking() {
    let mut f = fixture();
    f.tree.add_comment_space(f.work).unwrap();

    assert!(matches!(
        f.tree.edit_comment_space(f.work, 7, |_| {}),
        Err(TreeError::NoSuchCommentSpace { .. })
    ));
    assert!(matches!(
        f.tree.delete_comment_space(f.work, 7),
        Err(TreeError::NoSuchCommentSpace { .. })
    ));
}

#[test]
fn a_task_cannot_hold_comment_spaces() {
    let mut f = fixture();
    assert_eq!(
        f.tree.add_comment_space(f.task).unwrap_err(),
        TreeError::NotAFolder(f.task)
    );
    assert!(
        f.tree.comment_spaces(f.task).is_empty(),
        "asking a task for comment spaces should be empty, not an error"
    );
}

#[test]
fn a_blank_space_reports_itself_as_blank() {
    let mut f = fixture();
    let index = f.tree.add_comment_space(f.work).unwrap();
    // An auto numbered title alone does not count as content.
    assert!(f.tree.comment_spaces(f.work)[index].is_blank());

    f.tree
        .edit_comment_space(f.work, index, |s| s.body = "  ".to_owned())
        .unwrap();
    assert!(
        f.tree.comment_spaces(f.work)[index].is_blank(),
        "whitespace is not content"
    );

    f.tree
        .edit_comment_space(f.work, index, |s| s.body = "real".to_owned())
        .unwrap();
    assert!(!f.tree.comment_spaces(f.work)[index].is_blank());
}

#[test]
fn a_space_the_user_named_is_not_blank_even_with_no_body() {
    // Naming a page is an investment in it, so deleting it should still ask.
    let mut f = fixture();
    let index = f.tree.add_comment_space(f.work).unwrap();
    f.tree
        .edit_comment_space(f.work, index, |s| s.title = "Kickoff".to_owned())
        .unwrap();
    assert!(!f.tree.comment_spaces(f.work)[index].is_blank());
}
