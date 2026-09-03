//! Sidebar filtering.
//!
//! The visibility set the sidebar draws from. The rule that matters is that a
//! folder is kept when anything beneath it matches, so the path down to a match
//! stays navigable rather than the match being unreachable.

use trackcrab::model::{NodeId, Status, Tree};
use trackcrab::ui::{Filter, visible};

struct Fixture {
    tree: Tree,
    work: NodeId,
    clients: NodeId,
    acme: NodeId,
    migrate: NodeId,
    blocked: NodeId,
    personal: NodeId,
    groceries: NodeId,
}

/// Work > Clients > Acme > {"Migrate the estate", "Sign off" (blocked)},
/// plus Personal > "Buy groceries".
fn fixture() -> Fixture {
    let mut tree = Tree::new();
    let work = tree.create_folder(None, "Work").unwrap();
    let clients = tree.create_folder(Some(work), "Clients").unwrap();
    let acme = tree.create_folder(Some(clients), "Acme").unwrap();
    let migrate = tree
        .create_task(
            acme,
            "Migrate the estate",
            Some("Three AZs and a transit gateway".into()),
            Status::InProgress,
        )
        .unwrap();
    let blocked = tree
        .create_task(
            acme,
            "Sign off the runbook",
            None,
            Status::Blocked("waiting on the client".into()),
        )
        .unwrap();
    let personal = tree.create_folder(None, "Personal").unwrap();
    let groceries = tree
        .create_task(personal, "Buy groceries", None, Status::Open)
        .unwrap();
    Fixture {
        tree,
        work,
        clients,
        acme,
        migrate,
        blocked,
        personal,
        groceries,
    }
}

fn text(needle: &str) -> Filter {
    Filter {
        text: needle.to_owned(),
        ..Filter::default()
    }
}

/// A filter allowing only one status.
fn only(status: &Status) -> Filter {
    let mut statuses = [false; 5];
    statuses[status.ordinal() as usize] = true;
    Filter {
        statuses,
        ..Filter::default()
    }
}

#[test]
fn a_default_filter_hides_nothing_at_all() {
    let f = fixture();
    let filter = Filter::default();
    assert!(!filter.is_active());
    assert!(
        visible(&f.tree, &filter).is_none(),
        "no filter means no visibility set to compute"
    );
}

#[test]
fn matching_a_task_keeps_every_folder_on_the_way_to_it() {
    let f = fixture();
    let keep = visible(&f.tree, &text("migrate")).unwrap();

    assert!(keep.contains(&f.migrate));
    // The whole path, or the match would be unreachable.
    assert!(keep.contains(&f.acme));
    assert!(keep.contains(&f.clients));
    assert!(keep.contains(&f.work));
    // And nothing from the branch that does not match.
    assert!(!keep.contains(&f.personal));
    assert!(!keep.contains(&f.groceries));
    assert!(!keep.contains(&f.blocked));
}

#[test]
fn the_search_is_case_insensitive_and_matches_partially() {
    let f = fixture();
    for needle in ["MIGRATE", "migrate", "MiGrAtE", "grate the est"] {
        assert!(
            visible(&f.tree, &text(needle))
                .unwrap()
                .contains(&f.migrate),
            "{needle:?} should have matched"
        );
    }
}

#[test]
fn the_search_looks_at_descriptions_too() {
    let f = fixture();
    let keep = visible(&f.tree, &text("transit gateway")).unwrap();
    assert!(
        keep.contains(&f.migrate),
        "a description match should count"
    );
}

#[test]
fn matching_a_folder_name_keeps_the_folder_and_its_ancestors() {
    let f = fixture();
    let keep = visible(&f.tree, &text("acme")).unwrap();
    assert!(keep.contains(&f.acme));
    assert!(keep.contains(&f.clients));
    assert!(keep.contains(&f.work));
    // A folder matching by name does not drag its children along; they are
    // filtered on their own merits.
    assert!(!keep.contains(&f.migrate));
}

#[test]
fn a_search_matching_nothing_keeps_nothing() {
    let f = fixture();
    let keep = visible(&f.tree, &text("zzz nothing zzz")).unwrap();
    assert!(keep.is_empty());
}

#[test]
fn a_status_filter_keeps_only_that_status_and_its_path() {
    let f = fixture();
    let keep = visible(&f.tree, &only(&Status::Blocked(String::new()))).unwrap();

    assert!(keep.contains(&f.blocked));
    assert!(keep.contains(&f.acme), "the path must survive");
    assert!(!keep.contains(&f.migrate), "In Progress was excluded");
    assert!(!keep.contains(&f.groceries), "Open was excluded");
    assert!(!keep.contains(&f.personal));
}

#[test]
fn no_status_selected_means_every_status_passes() {
    // The status flags are an allowlist, so an empty one filters nothing. This
    // is the resting state and must not read as "hide everything".
    let f = fixture();
    let filter = Filter {
        statuses: [false; 5],
        ..Filter::default()
    };
    assert!(!filter.any_status_selected());
    assert!(!filter.is_active());
    assert!(
        visible(&f.tree, &filter).is_none(),
        "an empty allowlist should not narrow anything"
    );
}

#[test]
fn selecting_every_status_also_shows_everything() {
    // The other end of the same rule: allowing all five allows all five.
    let f = fixture();
    let filter = Filter {
        statuses: [true; 5],
        ..Filter::default()
    };
    assert!(filter.is_active(), "a full allowlist is still a filter");
    let keep = visible(&f.tree, &filter).unwrap();
    for id in [f.migrate, f.blocked, f.groceries] {
        assert!(keep.contains(&id), "every task should survive");
    }
}

#[test]
fn selecting_one_status_narrows_to_exactly_that_status() {
    let f = fixture();
    // Picking In Progress should leave the blocked and open tasks out.
    let keep = visible(&f.tree, &only(&Status::InProgress)).unwrap();
    assert!(keep.contains(&f.migrate));
    assert!(!keep.contains(&f.blocked));
    assert!(!keep.contains(&f.groceries));
}

#[test]
fn selecting_two_statuses_shows_both_and_nothing_else() {
    let f = fixture();
    let mut statuses = [false; 5];
    statuses[Status::InProgress.ordinal() as usize] = true;
    statuses[Status::Open.ordinal() as usize] = true;
    let keep = visible(
        &f.tree,
        &Filter {
            statuses,
            ..Filter::default()
        },
    )
    .unwrap();

    assert!(keep.contains(&f.migrate), "In Progress was selected");
    assert!(keep.contains(&f.groceries), "Open was selected");
    assert!(!keep.contains(&f.blocked), "Blocked was not");
}

#[test]
fn text_and_status_must_both_pass() {
    let f = fixture();
    let mut filter = only(&Status::InProgress);
    filter.text = "migrate".to_owned();
    let keep = visible(&f.tree, &filter).unwrap();
    assert!(keep.contains(&f.migrate));

    // Same text, wrong status.
    let mut filter = only(&Status::Completed);
    filter.text = "migrate".to_owned();
    assert!(!visible(&f.tree, &filter).unwrap().contains(&f.migrate));
}

#[test]
fn every_branch_is_searched_not_just_the_first_that_matches() {
    // A short circuiting implementation would keep the first matching branch and
    // silently drop later ones.
    let mut tree = Tree::new();
    let root = tree.create_folder(None, "Root").unwrap();
    let mut wanted = Vec::new();
    for n in 0..6 {
        let branch = tree
            .create_folder(Some(root), format!("Branch {n}"))
            .unwrap();
        wanted.push(
            tree.create_task(branch, format!("target {n}"), None, Status::Open)
                .unwrap(),
        );
    }
    let keep = visible(&tree, &text("target")).unwrap();
    for id in wanted {
        assert!(keep.contains(&id), "a later branch was dropped");
    }
}

#[test]
fn clearing_puts_the_filter_back_to_showing_everything() {
    let f = fixture();
    let mut filter = text("migrate");
    filter.statuses[0] = true;
    assert!(filter.is_active());
    assert!(filter.any_status_selected());

    filter.clear();
    assert!(!filter.is_active());
    assert!(
        !filter.any_status_selected(),
        "clearing should empty the allowlist, not fill it"
    );
    assert!(visible(&f.tree, &filter).is_none());
}

#[test]
fn the_search_looks_at_task_notes_too() {
    let mut f = fixture();
    f.tree
        .edit_task(f.migrate, |task| {
            task.notes = "ring the DBA before the cutover".to_owned();
        })
        .unwrap();

    let keep = visible(&f.tree, &text("ring the dba")).unwrap();
    assert!(
        keep.contains(&f.migrate),
        "a note should be findable by searching for its text"
    );
    // And the path to it survives, as with any other match.
    assert!(keep.contains(&f.acme));
    assert!(!keep.contains(&f.groceries));
}

#[test]
fn a_note_match_still_respects_the_status_filter() {
    let mut f = fixture();
    f.tree
        .edit_task(f.migrate, |task| {
            task.notes = "ring the DBA".to_owned();
        })
        .unwrap();

    let mut filter = only(&Status::Completed);
    filter.text = "ring the dba".to_owned();
    assert!(
        !visible(&f.tree, &filter).unwrap().contains(&f.migrate),
        "the note matches but the status does not"
    );
}

// -------------------------------------------------------- comments (N4)

#[test]
fn a_folder_is_kept_when_its_comments_match() {
    // Broader project context is exactly where a customer name or a contract
    // reference lives, so it has to be reachable from the search.
    let mut f = fixture();
    let index = f.tree.add_comment_space(f.personal).unwrap();
    f.tree
        .edit_comment_space(f.personal, index, |space| {
            space.body = "Northwind renewal, decision by October".to_owned();
        })
        .unwrap();

    let filter = Filter {
        text: "northwind".into(),
        ..Filter::default()
    };
    let keep = visible(&f.tree, &filter).expect("the filter is active");
    assert!(
        keep.contains(&f.personal),
        "the folder whose comments mention it should survive"
    );
    assert!(
        !keep.contains(&f.work),
        "an unrelated branch should still be filtered out"
    );
}

#[test]
fn a_comment_space_title_is_searchable_too() {
    let mut f = fixture();
    let index = f.tree.add_comment_space(f.acme).unwrap();
    f.tree
        .edit_comment_space(f.acme, index, |space| {
            space.title = "Runbook blockers".to_owned();
        })
        .unwrap();

    let filter = Filter {
        text: "blockers".into(),
        ..Filter::default()
    };
    let keep = visible(&f.tree, &filter).expect("the filter is active");
    assert!(keep.contains(&f.acme));
    // The path down to it stays navigable.
    assert!(keep.contains(&f.clients) && keep.contains(&f.work));
}

#[test]
fn the_status_allowlist_does_not_hide_a_comment_match() {
    // Statuses belong to tasks. A folder matched through its comments is not a
    // task and must not be judged by one.
    let mut f = fixture();
    let index = f.tree.add_comment_space(f.personal).unwrap();
    f.tree
        .edit_comment_space(f.personal, index, |space| {
            space.body = "Northwind".to_owned();
        })
        .unwrap();

    let mut filter = Filter {
        text: "northwind".into(),
        ..Filter::default()
    };
    filter.statuses[Status::Open.ordinal() as usize] = true;
    let keep = visible(&f.tree, &filter).expect("the filter is active");
    assert!(keep.contains(&f.personal));
}

#[test]
fn the_matching_space_is_the_one_the_text_is_in() {
    let mut f = fixture();
    for _ in 0..3 {
        f.tree.add_comment_space(f.personal).unwrap();
    }
    f.tree
        .edit_comment_space(f.personal, 2, |space| {
            space.body = "Northwind renewal".to_owned();
        })
        .unwrap();

    let filter = Filter {
        text: "northwind".into(),
        ..Filter::default()
    };
    let folder = f.tree.folder(f.personal).unwrap();
    assert_eq!(filter.matching_space(folder), Some(2));

    let miss = Filter {
        text: "nothing here".into(),
        ..Filter::default()
    };
    assert_eq!(miss.matching_space(folder), None);
    assert_eq!(
        Filter::default().matching_space(folder),
        None,
        "an empty search should not claim a match"
    );
}

#[test]
fn the_arrow_requests_wrap_in_both_directions() {
    use trackcrab::ui::views::comments::Request;
    assert_eq!(Request::Next.resolve(2, 3), Some(0));
    assert_eq!(Request::Previous.resolve(0, 3), Some(2));
    assert_eq!(Request::Next.resolve(0, 3), Some(1));
    assert_eq!(Request::Previous.resolve(2, 3), Some(1));
    // A single space has nowhere to go, and no spaces must not divide by zero.
    assert_eq!(Request::Next.resolve(0, 1), Some(0));
    assert_eq!(Request::Previous.resolve(0, 0), None);
    // The requests that are not a move say so.
    assert_eq!(Request::Close.resolve(0, 3), None);
    assert_eq!(Request::Add.resolve(0, 3), None);
    assert_eq!(Request::Delete.resolve(0, 3), None);
}

// ------------------------------------------------- markdown search (D8)

/// A tree with one task and one folder note, both written in markdown.
fn markdown_fixture() -> (Tree, NodeId, NodeId) {
    let mut tree = Tree::new();
    let folder = tree.create_folder(None, "Baking").unwrap();
    let task = tree
        .create_task(
            folder,
            "Sourdough",
            Some("Use the **strong white** flour, not the *plain*.".into()),
            Status::Open,
        )
        .unwrap();
    tree.edit_task(task, |t| {
        "See [the method](https://example.com/m) and the ==green|starter== notes.\n\
         - [ ] feed the starter\n\
         1. autolyse\n"
            .clone_into(&mut t.notes);
    })
    .unwrap();
    let first = tree.add_comment_space(folder).unwrap();
    tree.edit_comment_space(folder, first, |space| {
        "# Kickoff\nSpoke to __Anya__ about the `banneton`.".clone_into(&mut space.body);
    })
    .unwrap();
    (tree, folder, task)
}

/// Does the filter keep this node?
fn keeps(tree: &Tree, needle: &str, id: NodeId) -> bool {
    visible(tree, &text(needle)).is_some_and(|set| set.contains(&id))
}

#[test]
fn a_phrase_split_by_markup_is_still_found() {
    // The point of stripping. The description reads "the strong white flour",
    // and that is what someone searching will type; the stored bytes have two
    // pairs of asterisks in the middle of it.
    let (tree, _, task) = markdown_fixture();
    assert!(keeps(&tree, "strong white flour", task));
}

#[test]
fn searching_for_a_delimiter_finds_nothing() {
    // Kyle's actual complaint: an asterisk you cannot see should not match
    // every emphasised word in the vault.
    let (tree, folder, task) = markdown_fixture();
    for needle in ["*", "**", "__", "==", "`", "[", "](", "https://"] {
        assert!(
            visible(&tree, &text(needle)).is_some_and(|set| set.is_empty()),
            "{needle:?} matched something"
        );
        assert!(!keeps(&tree, needle, task));
        assert!(!keeps(&tree, needle, folder));
    }
}

#[test]
fn a_list_marker_is_not_searchable_text() {
    // The markers are drawn, not written, so they are not words on the page.
    let (tree, _, task) = markdown_fixture();
    assert!(!keeps(&tree, "- [ ]", task));
    assert!(!keeps(&tree, "1.", task));
    // The words beside them still are.
    assert!(keeps(&tree, "feed the starter", task));
    assert!(keeps(&tree, "autolyse", task));
}

#[test]
fn a_link_label_is_searchable_and_its_address_is_not() {
    // The label is on the page and the address is not, which is exactly what
    // the reader sees.
    let (tree, _, task) = markdown_fixture();
    assert!(keeps(&tree, "the method", task));
    assert!(!keeps(&tree, "example.com/m", task));
}

#[test]
fn a_highlight_colour_is_not_searchable_text() {
    // `==green|starter==` reads as "starter". The colour is a instruction, not
    // a word.
    let (tree, _, task) = markdown_fixture();
    assert!(keeps(&tree, "starter", task));
    assert!(!keeps(&tree, "green|", task));
}

#[test]
fn a_folder_note_is_searched_as_it_reads() {
    let (tree, folder, _) = markdown_fixture();
    assert!(keeps(&tree, "Anya", folder));
    assert!(keeps(&tree, "banneton", folder));
    assert!(!keeps(&tree, "__Anya__", folder));
}

#[test]
fn a_heading_is_still_searchable_without_its_hashes() {
    let (tree, folder, _) = markdown_fixture();
    assert!(keeps(&tree, "kickoff", folder));
    assert!(!keeps(&tree, "# kickoff", folder));
}

#[test]
fn code_is_searched_exactly_as_written() {
    // Nothing formats inside code, so nothing is stripped from it either: a
    // snippet has to be findable by the characters it actually contains.
    let mut tree = Tree::new();
    let folder = tree.create_folder(None, "Snippets").unwrap();
    let first = tree.add_comment_space(folder).unwrap();
    tree.edit_comment_space(folder, first, |space| {
        "```rust\nlet x = *ptr;\n```".clone_into(&mut space.body);
    })
    .unwrap();
    assert!(keeps(&tree, "let x = *ptr;", folder));
}

#[test]
fn a_title_is_matched_as_typed() {
    // Titles are one line fields and carry no markdown, so an asterisk in a
    // title is an asterisk.
    let mut tree = Tree::new();
    let folder = tree.create_folder(None, "Odd *names*").unwrap();
    assert!(keeps(&tree, "*names*", folder));
}

#[test]
fn the_memo_does_not_answer_from_a_stale_entry() {
    // The cache is keyed on the text itself, so an edit is a different key and
    // there is nothing to invalidate. Worth pinning: the alternative keying,
    // on a node and a timestamp, is exactly where this would go wrong.
    let (mut tree, _, task) = markdown_fixture();
    assert!(keeps(&tree, "autolyse", task));

    tree.edit_task(task, |t| "**retard** overnight".clone_into(&mut t.notes))
        .unwrap();
    assert!(!keeps(&tree, "autolyse", task));
    assert!(keeps(&tree, "retard", task));
}

#[test]
fn clearing_the_memo_changes_nothing_but_speed() {
    let (tree, folder, task) = markdown_fixture();
    let asked = |needle: &str| {
        (
            keeps(&tree, needle, task),
            keeps(&tree, needle, folder),
        )
    };
    let warm: Vec<_> = ["starter", "*", "the method", "banneton"]
        .into_iter()
        .map(asked)
        .collect();
    trackcrab::ui::search::forget();
    let cold: Vec<_> = ["starter", "*", "the method", "banneton"]
        .into_iter()
        .map(asked)
        .collect();
    assert_eq!(warm, cold);
}

#[test]
fn the_matching_space_is_found_through_markup() {
    // Opening a search hit lands on the page that holds it, and that lookup has
    // to strip markup for the same reason the search itself does.
    let mut tree = Tree::new();
    let folder = tree.create_folder(None, "Project").unwrap();
    let first = tree.add_comment_space(folder).unwrap();
    tree.edit_comment_space(folder, first, |space| {
        "nothing here".clone_into(&mut space.body);
    })
    .unwrap();
    let second = tree.add_comment_space(folder).unwrap();
    tree.edit_comment_space(folder, second, |space| {
        "the **transit** gateway".clone_into(&mut space.body);
    })
    .unwrap();
    let node = tree.get(folder).unwrap();
    let space = node.as_folder().unwrap();
    assert_eq!(text("transit gateway").matching_space(space), Some(second));
}
