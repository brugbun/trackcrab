//! UI tests driven through egui's own accessibility tree.
//!
//! These click the real interface rather than calling helpers, so they catch what
//! a unit test cannot: a folder that will not expand, a click landing on the
//! wrong widget, a view that fails to follow the sidebar.

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::Node;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use trackcrab::app::{App, View};
use trackcrab::model::Status;
use trackcrab::store::DataStore;
use trackcrab::ui::theme::metric;

/// Frames to burn so a slide or expand animation settles.
const SETTLE: usize = 30;

/// A harness over the real app with a known tree, writing to a throwaway path.
///
/// Work > Clients > Acme > two tasks, plus an empty Personal folder at the root.
fn harness() -> Harness<'static, App> {
    harness_sized(1600.0, 900.0)
}

/// As [`harness`], at a chosen window size.
fn harness_sized(width: f32, height: f32) -> Harness<'static, App> {
    harness_built(width, height, None)
}

/// As [`harness`], simulating at a fine timestep so animations can be watched
/// frame by frame.
///
/// The default is 4fps, deliberately coarse so tests do not burn time waiting
/// out tweens. That is right everywhere except a test whose subject *is* the
/// tween: at 4fps an 83ms animation is over inside one frame.
fn harness_animated() -> Harness<'static, App> {
    harness_built(1600.0, 900.0, Some(1.0 / 240.0))
}

fn harness_built(width: f32, height: f32, step_dt: Option<f32>) -> Harness<'static, App> {
    let dir = std::env::temp_dir().join(format!(
        "trackcrab-ui-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = App::new(DataStore::at(dir.join("data.json")));

    {
        let tree = app.tree_mut();
        let work = tree.create_folder(None, "Work").unwrap();
        let clients = tree.create_folder(Some(work), "Clients").unwrap();
        let acme = tree.create_folder(Some(clients), "Acme").unwrap();
        let t = tree
            .create_task(acme, "Cut over the database", None, Status::InProgress)
            .unwrap();
        tree.edit_task(t, |t| t.set_attributed_hm(2, 0)).unwrap();
        tree.create_task(acme, "Land the VPC design", None, Status::Completed)
            .unwrap();
        tree.create_folder(None, "Personal").unwrap();
    }

    let mut builder = Harness::builder().with_size(eframe::egui::vec2(width, height));
    if let Some(dt) = step_dt {
        builder = builder.with_step_dt(dt).with_max_steps(4000);
    }
    let mut harness = builder.build_ui_state(|ui, app: &mut App| app.draw(ui), app);
    harness.run();
    if step_dt.is_some() {
        // kittest zeroes animation_time so tests never wait on a tween. Put it
        // back for the tests whose subject *is* the tween, and drive time
        // forward by hand so the frames are reproducible.
        harness
            .ctx
            .all_styles_mut(|style| style.animation_time = 0.25);
    }
    harness
}

/// The timestamp the listing should be rendering for a given task title.
fn stamp_for(harness: &Harness<'_, App>, title: &str) -> String {
    let tree = harness.state().tree();
    let id = tree
        .roots()
        .iter()
        .flat_map(|r| tree.descendants(*r))
        .find(|id| tree.task(*id).is_ok_and(|t| t.title == title))
        .expect("task should exist");
    trackcrab::ui::local_stamp(tree.get(id).unwrap().updated_at())
}

fn settle(harness: &mut Harness<'_, App>) {
    for _ in 0..SETTLE {
        harness.run();
    }
}

/// The burger is the only unlabelled control, so it is found by its glyph.
fn toggle_sidebar(harness: &mut Harness<'_, App>) {
    harness.get_by_label(trackcrab::app::BURGER).click();
    settle(harness);
}

/// The sidebar subtree.
///
/// Folder and task names appear in both the sidebar and the main panel listing,
/// so an unscoped query is ambiguous by design. This walks up from the sidebar's
/// own "FOLDERS" heading to the nearest ancestor that also contains the tree
/// rows, which is self validating rather than relying on paint order.
fn sidebar<'t>(harness: &'t Harness<'_, App>, probe: &str) -> Node<'t> {
    let mut node = harness.get_by_label("FOLDERS");
    for _ in 0..8 {
        let Some(parent) = node.parent() else { break };
        node = parent;
        if node.query_all_by_label(probe).next().is_some() {
            return node;
        }
    }
    panic!("could not find a sidebar subtree containing {probe:?}");
}

/// Clicks at an arbitrary point, which the node helpers cannot do.
fn click_at(harness: &mut Harness<'_, App>, pos: Pos2) {
    harness.input_mut().events.push(Event::PointerMoved(pos));
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        });
    }
    settle(harness);
}

/// Clicks a row in the sidebar specifically, never the main panel.
///
/// Deliberately clicks near the label's *left* edge rather than its centre. A
/// deeply indented long name extends past the panel edge, so its centre can sit
/// outside the sidebar entirely, which is exactly where a naive click goes
/// astray. The left edge is always within the panel for a visible row.
fn click_in_sidebar(harness: &mut Harness<'_, App>, name: &str) {
    let rect = sidebar(harness, name).get_by_label(name).rect();
    click_at(harness, pos2(rect.left() + 2.0, rect.center().y));
}

/// Clicks the collapse arrow beside a sidebar folder row.
///
/// Walking left from the label: one dot column to the row's content edge, one
/// item gap, then the arrow occupying exactly one indent width. Every term comes
/// from the theme, so this stays correct if the metrics change.
fn click_collapse_arrow(harness: &mut Harness<'_, App>, folder: &str) {
    let rect = sidebar(harness, folder).get_by_label(folder).rect();
    let x = rect.left() - metric::DOT_COLUMN - metric::ITEM_SPACING_X - metric::INDENT / 2.0;
    click_at(harness, pos2(x, rect.center().y));
}

/// Walks the sidebar down to Acme, expanding as it goes.
fn open_acme(harness: &mut Harness<'_, App>) {
    for name in ["Work", "Clients", "Acme"] {
        click_in_sidebar(harness, name);
    }
}

// ------------------------------------------------------------------ defaults

#[test]
fn the_welcome_page_is_the_default_view() {
    let harness = harness();
    assert_eq!(*harness.state().view(), View::Welcome);
    assert!(
        harness
            .query_by_label_contains("Folders and tasks")
            .is_some()
    );
    // The sidebar starts closed, so no folder names are on screen at all.
    assert!(harness.query_all_by_label("Work").next().is_none());
}

#[test]
fn the_burger_opens_and_closes_the_sidebar() {
    let mut harness = harness();
    assert!(harness.query_all_by_label("Work").next().is_none());

    toggle_sidebar(&mut harness);
    assert!(
        harness.query_all_by_label("Work").next().is_some(),
        "root folders should be listed once the sidebar is open"
    );

    toggle_sidebar(&mut harness);
    assert!(
        harness.query_all_by_label("Work").next().is_none(),
        "the sidebar should have slid away again"
    );
}

// ---------------------------------------------------------------- tree state

#[test]
fn only_root_folders_show_until_a_folder_is_expanded() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);

    assert!(harness.query_all_by_label("Work").next().is_some());
    assert!(harness.query_all_by_label("Personal").next().is_some());
    // Nested content stays hidden until asked for.
    assert!(harness.query_all_by_label("Clients").next().is_none());
    assert!(harness.query_all_by_label("Acme").next().is_none());
}

#[test]
fn clicking_a_folder_expands_it_in_the_tree_and_opens_it_in_the_view() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Work");

    match harness.state().view() {
        View::Folder(id) => {
            assert_eq!(harness.state().tree().folder(*id).unwrap().name, "Work");
        }
        other => panic!("expected the Work folder to be open, got {other:?}"),
    }
    assert!(
        sidebar(&harness, "Work")
            .query_all_by_label("Clients")
            .next()
            .is_some(),
        "clicking a folder should expand it in the sidebar"
    );
}

#[test]
fn opening_a_deep_folder_keeps_every_ancestor_expanded() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);

    let tree = sidebar(&harness, "Work");
    for name in ["Work", "Clients", "Acme"] {
        assert!(
            tree.query_all_by_label(name).next().is_some(),
            "{name} should still be visible in the tree"
        );
    }
    assert!(
        tree.query_all_by_label("Cut over the database")
            .next()
            .is_some(),
        "the open folder's tasks should be listed in the tree"
    );
}

#[test]
fn a_collapsed_folder_hides_its_whole_subtree_again() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    assert!(harness.query_all_by_label("Acme").next().is_some());

    click_collapse_arrow(&mut harness, "Work");

    assert!(
        sidebar(&harness, "Work")
            .query_all_by_label("Clients")
            .next()
            .is_none(),
        "collapsing Work should hide everything beneath it"
    );
}

// ---------------------------------------------------------------- main panel

#[test]
fn the_folder_listing_shows_a_timestamp_and_attributed_time_per_row() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);

    // Attributed time in the agreed format. Only the listing renders this, so it
    // needs no scoping.
    assert!(
        harness.query_all_by_label("2h").next().is_some(),
        "attributed time should be shown on the row"
    );

    // The exact stamp each task row should be showing, which pins the
    // HH:MM:SS DD/MM/YYYY format as well as its presence.
    let expected: Vec<String> = {
        let tree = harness.state().tree();
        let acme = tree
            .roots()
            .iter()
            .flat_map(|r| tree.descendants(*r))
            .find(|id| tree.folder(*id).is_ok_and(|f| f.name == "Acme"))
            .expect("Acme should exist");
        tree.children(Some(acme))
            .unwrap()
            .iter()
            .filter_map(|id| tree.get(*id))
            .map(|node| trackcrab::ui::local_stamp(node.updated_at()))
            .collect()
    };
    assert_eq!(expected.len(), 2);
    for stamp in expected {
        assert_eq!(
            stamp.len(),
            19,
            "timestamp {stamp} is not HH:MM:SS DD/MM/YYYY"
        );
        assert!(
            harness.query_all_by_label(&stamp).next().is_some(),
            "the listing should show the stamp {stamp}"
        );
    }
}

#[test]
fn clicking_a_task_in_the_listing_opens_that_task() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);

    // Deliberately the listing copy, not the sidebar one.
    let listing = harness
        .get_all_by_label("Land the VPC design")
        .last()
        .expect("the task should appear in the listing");
    listing.click();
    settle(&mut harness);

    match harness.state().view() {
        View::Task(id) => {
            assert_eq!(
                harness.state().tree().task(*id).unwrap().title,
                "Land the VPC design"
            );
        }
        other => panic!("expected a task view, got {other:?}"),
    }
}

#[test]
fn clicking_a_task_in_the_sidebar_opens_that_task() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);

    click_in_sidebar(&mut harness, "Cut over the database");

    match harness.state().view() {
        View::Task(id) => {
            assert_eq!(
                harness.state().tree().task(*id).unwrap().title,
                "Cut over the database"
            );
        }
        other => panic!("expected a task view, got {other:?}"),
    }
}

#[test]
fn an_empty_folder_says_so_rather_than_showing_a_blank_panel() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");

    assert!(
        harness
            .query_all_by_label_contains("empty")
            .next()
            .is_some(),
        "an empty folder should explain itself"
    );
}

#[test]
fn the_breadcrumb_shows_every_ancestor_of_the_open_folder() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    // Close the sidebar so the only remaining copies are the breadcrumb's.
    toggle_sidebar(&mut harness);

    for ancestor in ["Work", "Clients"] {
        assert!(
            harness.query_all_by_label(ancestor).next().is_some(),
            "{ancestor} should appear in the breadcrumb"
        );
    }
}

#[test]
fn a_breadcrumb_crumb_navigates_back_up_to_that_folder() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    toggle_sidebar(&mut harness);

    // Only the breadcrumb carries "Clients" now, so this is unambiguous.
    harness.get_by_label("Clients").click();
    settle(&mut harness);

    match harness.state().view() {
        View::Folder(id) => {
            assert_eq!(harness.state().tree().folder(*id).unwrap().name, "Clients");
        }
        other => panic!("clicking a crumb should open that folder, got {other:?}"),
    }
}

#[test]
fn the_timestamp_is_dropped_before_the_attributed_time() {
    // The attributed time is the figure worth keeping and much the narrower of
    // the two, so under width pressure the timestamp is what goes. The title is
    // never sacrificed. Asserted as an ordering across widths rather than
    // against hand computed pixel thresholds.
    let mut widest = None;
    let mut narrowest = None;

    for width in [1600.0_f32, 520.0, 400.0, 320.0, 260.0, 200.0] {
        let mut harness = harness_sized(width, 700.0);
        toggle_sidebar(&mut harness);
        open_acme(&mut harness);
        // Close the sidebar so the listing owns the full width and the only
        // copies of these strings are the listing's own.
        toggle_sidebar(&mut harness);

        let stamp = stamp_for(&harness, "Cut over the database");
        let stamp_shown = harness.query_all_by_label(&stamp).next().is_some();
        let time_shown = harness.query_all_by_label("2h").next().is_some();
        let title_shown = harness
            .query_all_by_label_contains("Cut over the database")
            .next()
            .is_some();

        assert!(
            title_shown,
            "the title must never be dropped, but it was at width {width}"
        );
        assert!(
            time_shown,
            "the attributed time must always be shown, but it was gone at width {width}"
        );

        if widest.is_none() {
            widest = Some(stamp_shown);
        }
        narrowest = Some(stamp_shown);
    }

    assert!(widest.unwrap(), "a wide panel should show the timestamp");
    assert!(
        !narrowest.unwrap(),
        "a very narrow panel should have dropped the timestamp"
    );
}

#[test]
fn the_attributed_time_sits_to_the_left_of_the_timestamp() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    toggle_sidebar(&mut harness);

    let stamp = stamp_for(&harness, "Cut over the database");
    let time_rect = harness.get_by_label("2h").rect();
    // Several rows carry the same stamp, so take the one on the same row.
    let stamp_rect = harness
        .get_all_by_label(&stamp)
        .map(|node| node.rect())
        .find(|rect| (rect.center().y - time_rect.center().y).abs() < 2.0)
        .expect("the row should have a timestamp beside its time");

    assert!(
        time_rect.right() <= stamp_rect.left(),
        "the attributed time ({:.0}..{:.0}) should sit left of the timestamp ({:.0}..{:.0})",
        time_rect.left(),
        time_rect.right(),
        stamp_rect.left(),
        stamp_rect.right()
    );
}

#[test]
fn a_task_with_no_logged_time_shows_zero_rather_than_nothing() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    toggle_sidebar(&mut harness);

    // "Land the VPC design" is seeded with no attributed time.
    assert!(
        harness.query_all_by_label("0h 0m").next().is_some(),
        "an unlogged task should read 0h 0m, not blank"
    );
}

#[test]
fn the_meta_columns_stay_clear_of_the_right_edge() {
    // This is the clipping that was reported: the time column ran off the edge.
    let width = 900.0;
    let mut harness = harness_sized(width, 700.0);
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    toggle_sidebar(&mut harness);

    for node in harness.get_all_by_label("0h 0m") {
        let rect = node.rect();
        assert!(
            rect.right() < width - 16.0,
            "the time column reaches {:.0}px in a {width:.0}px window",
            rect.right()
        );
    }
}

#[test]
fn a_long_title_truncates_instead_of_wrapping_the_row() {
    let mut harness = harness_sized(700.0, 600.0);
    let long = "A deliberately very long task title that will not fit inside the listing";
    {
        let tree = harness.state_mut().tree_mut();
        let acme = tree
            .roots()
            .iter()
            .flat_map(|r| tree.descendants(*r))
            .find(|id| tree.folder(*id).is_ok_and(|f| f.name == "Acme"))
            .unwrap();
        tree.create_task(acme, long, None, Status::Open).unwrap();
    }
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    toggle_sidebar(&mut harness);

    let row = harness
        .get_all_by_label_contains("A deliberately very long")
        .next()
        .expect("the long task should be listed");
    let rect = row.rect();
    assert!(
        rect.height() < metric::ROW_HEIGHT * 1.5,
        "the title wrapped to {:.0}px instead of truncating to one line",
        rect.height()
    );
    assert!(
        rect.width() <= 700.0,
        "the title overflowed the window at {:.0}px wide",
        rect.width()
    );
}

#[test]
fn the_view_falls_back_to_welcome_when_the_open_node_is_deleted() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    click_in_sidebar(&mut harness, "Cut over the database");
    let View::Task(id) = *harness.state().view() else {
        panic!("expected a task view")
    };

    harness.state_mut().tree_mut().delete_task(id).unwrap();
    settle(&mut harness);

    assert_eq!(
        *harness.state().view(),
        View::Welcome,
        "a deleted task must not leave a dangling view"
    );
}

// ------------------------------------------------------------- M4 task detail

/// Opens the Acme folder then the named task, leaving the task view showing.
fn open_task(harness: &mut Harness<'_, App>, title: &str) {
    toggle_sidebar(harness);
    open_acme(harness);
    click_in_sidebar(harness, title);
    // Close the sidebar so queries hit the detail view unambiguously.
    toggle_sidebar(harness);
}

/// The blocked reason box. In the detail view two `TextInput`s exist once
/// Blocked is chosen, the title first and the reason second; the hint text is
/// not exposed to accessibility so it cannot be found by name. Callers reach
/// this with the sidebar closed, so its search box is not in the tree.
fn reason_field<'t>(harness: &'t Harness<'_, App>) -> Node<'t> {
    harness
        .get_all_by_role(Role::TextInput)
        .nth(1)
        .expect("the blocked reason field should be present")
}

/// The task the detail view is currently showing.
fn open_task_id(harness: &Harness<'_, App>) -> trackcrab::model::NodeId {
    match harness.state().view() {
        View::Task(id) => *id,
        other => panic!("expected a task view, got {other:?}"),
    }
}

#[test]
fn the_detail_view_shows_both_timestamps_and_the_description() {
    let mut harness = harness();
    {
        let tree = harness.state_mut().tree_mut();
        let id = tree
            .roots()
            .iter()
            .flat_map(|r| tree.descendants(*r))
            .find(|id| {
                tree.task(*id)
                    .is_ok_and(|t| t.title == "Land the VPC design")
            })
            .unwrap();
        tree.edit_task(id, |t| t.set_description("Three AZs, private data tier."))
            .unwrap();
    }
    open_task(&mut harness, "Land the VPC design");

    let id = open_task_id(&harness);
    let (created, updated) = {
        let task = harness.state().tree().task(id).unwrap();
        (
            trackcrab::ui::local_stamp(task.created_at),
            trackcrab::ui::local_stamp(task.updated_at),
        )
    };
    assert!(
        harness
            .query_all_by_label_contains(&created)
            .next()
            .is_some(),
        "created_at should be shown"
    );
    assert!(
        harness
            .query_all_by_label_contains(&updated)
            .next()
            .is_some(),
        "updated_at should be shown"
    );
    // A text area reports its content as a value rather than a label.
    assert_eq!(
        description_box(&harness).value().as_deref(),
        Some("Three AZs, private data tier."),
        "the description should be shown"
    );
}

#[test]
fn all_five_statuses_are_offered() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    for label in ["Open", "In Progress", "Completed", "Blocked", "Cancelled"] {
        assert!(
            harness.query_all_by_label(label).next().is_some(),
            "{label} should be selectable"
        );
    }
}

#[test]
fn the_blocked_reason_field_appears_only_for_blocked() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    assert!(
        harness
            .query_all_by_label("BLOCKED REASON")
            .next()
            .is_none(),
        "the reason field should be hidden until Blocked is chosen"
    );

    harness.get_by_label("Blocked").click();
    settle(&mut harness);
    assert!(
        harness
            .query_all_by_label("BLOCKED REASON")
            .next()
            .is_some(),
        "choosing Blocked should reveal the reason field"
    );

    harness.get_by_label("Completed").click();
    settle(&mut harness);
    assert!(
        harness
            .query_all_by_label("BLOCKED REASON")
            .next()
            .is_none(),
        "moving off Blocked should hide the reason field again"
    );
}

#[test]
fn choosing_blocked_without_a_reason_does_not_change_the_saved_status() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);
    let before = harness.state().tree().task(id).unwrap().status.clone();

    harness.get_by_label("Blocked").click();
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().task(id).unwrap().status,
        before,
        "the status must not change until a reason is given"
    );
    assert!(
        harness
            .query_all_by_label_contains("Not saved yet")
            .next()
            .is_some(),
        "the UI should say why the change has not taken"
    );
}

#[test]
fn an_edit_made_while_blocked_is_pending_is_still_saved() {
    // `edit_task` rolls an invalid edit back wholesale, so an implementation
    // that posted the reasonless Blocked status along with everything else
    // would silently discard whatever the user had just typed.
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);
    let before = harness.state().tree().task(id).unwrap().status.clone();

    harness.get_by_label("Blocked").click();
    settle(&mut harness);

    // Type a description while the status change is still pending.
    description_box(&harness).focus();
    harness.run();
    description_box(&harness).type_text("typed while blocked was pending");
    settle(&mut harness);

    let task = harness.state().tree().task(id).unwrap();
    assert_eq!(
        task.description.as_deref(),
        Some("typed while blocked was pending"),
        "an unrelated edit must survive a pending status change"
    );
    assert_eq!(
        task.status, before,
        "the status should still be waiting on a reason"
    );
}

#[test]
fn the_time_boxes_show_hours_and_minutes_split_out() {
    let mut harness = harness();
    open_task(&mut harness, "Cut over the database");
    // Seeded as 2h.
    assert!(
        harness.query_all_by_value("2 h").next().is_some(),
        "the hour box should show 2"
    );
    assert!(
        harness.query_all_by_value("0 m").next().is_some(),
        "the minute box should show 0"
    );

    let id = open_task_id(&harness);
    harness
        .state_mut()
        .tree_mut()
        .edit_task(id, |t| t.set_attributed_hm(0, 90))
        .unwrap();
    // Force the editor to reload by leaving and coming back.
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Cut over the database");
    settle(&mut harness);

    assert!(
        harness.query_all_by_value("1 h").next().is_some(),
        "90 minutes should be shown as 1h"
    );
    assert!(
        harness.query_all_by_value("30 m").next().is_some(),
        "90 minutes should leave 30m"
    );
}

#[test]
fn typing_a_reason_commits_the_blocked_status() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);

    harness.get_by_label("Blocked").click();
    settle(&mut harness);

    reason_field(&harness).focus();
    harness.run();
    reason_field(&harness).type_text("client change window");
    settle(&mut harness);

    let task = harness.state().tree().task(id).unwrap();
    assert_eq!(
        task.status.blocked_reason(),
        Some("client change window"),
        "the reason should be saved with the status"
    );
}

#[test]
fn deleting_a_task_asks_first_then_returns_to_its_folder() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);
    let parent = harness.state().tree().node(id).unwrap().parent.unwrap();

    harness.get_by_label("Delete task").click();
    settle(&mut harness);
    assert!(
        harness
            .query_all_by_label_contains("Delete this task")
            .next()
            .is_some(),
        "delete should ask before acting"
    );

    // Backing out leaves the task alone.
    harness.get_by_label("Cancel").click();
    settle(&mut harness);
    assert!(harness.state().tree().contains(id));

    harness.get_by_label("Delete task").click();
    settle(&mut harness);
    harness.get_by_label("Delete").click();
    settle(&mut harness);

    assert!(
        !harness.state().tree().contains(id),
        "the task should be gone"
    );
    assert_eq!(
        *harness.state().view(),
        View::Folder(parent),
        "after a delete the view should land on the folder the task was in"
    );
}

#[test]
fn an_edit_made_in_the_ui_survives_a_restart() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);

    description_box(&harness).focus();
    harness.run();
    description_box(&harness).type_text("three AZs, private data tier");
    settle(&mut harness);
    harness.get_by_label("Completed").click();
    settle(&mut harness);

    // The real app saves on a debounce from `logic`, which the harness does not
    // drive. Flushing explicitly exercises the same write path.
    harness.state_mut().save_now();
    let path = harness.state().data_path().to_path_buf();

    // A brand new instance, reading only what reached the disk.
    let restarted = App::new(DataStore::at(&path));
    let task = restarted
        .tree()
        .task(id)
        .expect("the task should have been persisted");
    assert_eq!(
        task.description.as_deref(),
        Some("three AZs, private data tier")
    );
    assert_eq!(task.status, Status::Completed);

    // And the folder above it carries the bubbled timestamp.
    let parent = restarted.tree().node(id).unwrap().parent.unwrap();
    assert!(
        restarted.tree().folder(parent).unwrap().updated_at >= task.updated_at,
        "the parent folder should have been stamped at least as recently"
    );
}

// ---------------------------------------------------------- M5 creation flow

/// A harness over an entirely empty store, which is what a first run looks like.
fn empty_harness() -> Harness<'static, App> {
    let dir = std::env::temp_dir().join(format!(
        "trackcrab-ui-empty-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let app = App::new(DataStore::at(dir.join("data.json")));
    let mut harness = Harness::builder()
        .with_size(eframe::egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app: &mut App| app.draw(ui), app);
    harness.run();
    harness
}

/// Finds a task by title, since creating one no longer opens it.
fn task_by_title(harness: &Harness<'_, App>, title: &str) -> Option<trackcrab::model::NodeId> {
    let tree = harness.state().tree();
    tree.roots()
        .iter()
        .flat_map(|r| tree.descendants(*r))
        .find(|id| tree.task(*id).is_ok_and(|t| t.title == title))
}

/// The text field belonging to the open dialog.
///
/// The sidebar's search box is a `TextInput` too, and the modal is drawn above
/// the panels, so the dialog's own field is the last one in the tree. Asserted
/// rather than assumed: the helper panics if there is no dialog field to find.
fn dialog_field<'t>(harness: &'t Harness<'_, App>) -> Node<'t> {
    harness
        .get_all_by_role(Role::TextInput)
        .last()
        .expect("a dialog should be open with a text field")
}

/// Types into the text field of a name dialog.
fn type_name(harness: &mut Harness<'_, App>, text: &str) {
    dialog_field(harness).focus();
    harness.run();
    dialog_field(harness).type_text(text);
    settle(harness);
}

#[test]
fn a_first_folder_can_be_made_from_an_empty_store() {
    // The cold start path. With no folders there is nowhere to hang a button
    // except the sidebar header, so this is the one that must work.
    let mut harness = empty_harness();
    assert!(harness.state().tree().is_empty());

    toggle_sidebar(&mut harness);
    harness.get_by_label("+").click();
    settle(&mut harness);

    type_name(&mut harness, "Work");
    harness.get_by_label("Create").click();
    settle(&mut harness);

    assert_eq!(harness.state().tree().roots().len(), 1);
    let root = harness.state().tree().roots()[0];
    assert_eq!(harness.state().tree().folder(root).unwrap().name, "Work");
    assert_eq!(
        *harness.state().view(),
        View::Welcome,
        "creating a folder should not drag you into it"
    );
    assert!(
        harness.query_all_by_label("Work").next().is_some(),
        "the new folder should still appear in the tree"
    );
}

#[test]
fn a_folder_with_a_blank_name_cannot_be_created() {
    let mut harness = empty_harness();
    toggle_sidebar(&mut harness);
    harness.get_by_label("+").click();
    settle(&mut harness);

    type_name(&mut harness, "   ");
    assert!(
        harness
            .query_all_by_label_contains("A folder needs a name")
            .next()
            .is_some(),
        "a blank name should be refused with an explanation"
    );
    harness.get_by_label("Cancel").click();
    settle(&mut harness);
    assert!(harness.state().tree().is_empty());
}

#[test]
fn the_plus_task_button_creates_the_task_before_prompting() {
    // Per the spec the blank task is inserted straight away, so it shows in the
    // sidebar and the listing while the prompt is still up.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    let acme = match harness.state().view() {
        View::Folder(id) => *id,
        other => panic!("expected the Acme folder, got {other:?}"),
    };
    let before = harness.state().tree().children(Some(acme)).unwrap().len();

    harness.get_by_label("+ Task").click();
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().children(Some(acme)).unwrap().len(),
        before + 1,
        "the blank task should exist while the prompt is open"
    );
    assert!(
        harness
            .query_all_by_label_contains("New task")
            .next()
            .is_some(),
        "the prompt should be up"
    );
}

#[test]
fn cancelling_the_new_task_prompt_removes_the_blank_task() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    let acme = match harness.state().view() {
        View::Folder(id) => *id,
        other => panic!("expected a folder, got {other:?}"),
    };
    let before = harness.state().tree().children(Some(acme)).unwrap().len();

    harness.get_by_label("+ Task").click();
    settle(&mut harness);
    harness.get_by_label("Cancel").click();
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().children(Some(acme)).unwrap().len(),
        before,
        "backing out must not leave an untitled stub behind"
    );
}

#[test]
fn dismissing_the_new_task_prompt_with_escape_also_removes_it() {
    // Escape and the backdrop have to behave exactly like Cancel, otherwise the
    // blank task is orphaned.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    let acme = match harness.state().view() {
        View::Folder(id) => *id,
        other => panic!("expected a folder, got {other:?}"),
    };
    let before = harness.state().tree().children(Some(acme)).unwrap().len();

    harness.get_by_label("+ Task").click();
    settle(&mut harness);
    harness.key_press(eframe::egui::Key::Escape);
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().children(Some(acme)).unwrap().len(),
        before,
        "Escape must clean up the blank task too"
    );
}

#[test]
fn a_created_task_keeps_its_title_description_and_status() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);

    harness.get_by_label("+ Task").click();
    settle(&mut harness);

    // The title is the dialog's own single line field; the description is its
    // only multiline one.
    dialog_field(&harness).focus();
    harness.run();
    dialog_field(&harness).type_text("Write the runbook");
    settle(&mut harness);
    harness.get_by_role(Role::MultilineTextInput).focus();
    harness.run();
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("Step by step, for the on call rota.");
    settle(&mut harness);
    harness.get_by_label("In Progress").click();
    settle(&mut harness);
    harness.get_by_label("Create").click();
    settle(&mut harness);

    // Creation no longer navigates, so the task is found rather than opened.
    let id = task_by_title(&harness, "Write the runbook").expect("the task should exist");
    assert!(
        matches!(harness.state().view(), View::Folder(_)),
        "creating a task should leave you in the folder you were filling out"
    );
    let task = harness.state().tree().task(id).unwrap();
    assert_eq!(task.title, "Write the runbook");
    assert_eq!(
        task.description.as_deref(),
        Some("Step by step, for the on call rota.")
    );
    assert_eq!(task.status, Status::InProgress);
}

#[test]
fn a_new_task_defaults_to_open_with_no_description() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);

    harness.get_by_label("+ Task").click();
    settle(&mut harness);
    dialog_field(&harness).focus();
    harness.run();
    dialog_field(&harness).type_text("Bare minimum");
    settle(&mut harness);
    harness.get_by_label("Create").click();
    settle(&mut harness);

    let id = task_by_title(&harness, "Bare minimum").expect("the task should exist");
    let task = harness.state().tree().task(id).unwrap();
    assert_eq!(task.status, Status::Open, "Open is the default");
    assert_eq!(task.description, None, "an empty description stays None");
}

#[test]
fn a_new_blocked_task_cannot_be_created_without_a_reason() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    harness.get_by_label("+ Task").click();
    settle(&mut harness);
    harness.get_by_label("Blocked").click();
    settle(&mut harness);

    assert!(
        harness
            .query_all_by_label_contains("A blocked task needs a reason")
            .next()
            .is_some(),
        "the dialog should say why it will not create yet"
    );
    // Clicking the disabled Create must do nothing.
    harness.get_by_label("Create").click();
    settle(&mut harness);
    assert!(
        harness
            .query_all_by_label_contains("New task")
            .next()
            .is_some(),
        "the dialog should still be open"
    );
}

#[test]
fn a_folder_can_be_renamed() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");
    let View::Folder(id) = *harness.state().view() else {
        panic!("expected a folder view")
    };

    // Drive the command directly; the context menu that raises it is egui's.
    harness.state_mut().request_rename_folder(id);
    settle(&mut harness);
    // Clear the prefilled name, then type a new one.
    harness.state_mut().set_dialog_name("Side projects");
    settle(&mut harness);
    harness.get_by_label("Rename").click();
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().folder(id).unwrap().name,
        "Side projects"
    );
}

#[test]
fn an_empty_folder_can_be_deleted_but_a_full_one_is_refused() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);

    // Acme has tasks in it, so deletion should be refused outright.
    open_acme(&mut harness);
    let View::Folder(acme) = *harness.state().view() else {
        panic!("expected a folder view")
    };
    harness.state_mut().request_delete_folder(acme);
    settle(&mut harness);
    assert!(
        harness
            .query_all_by_label_contains("Empty it first")
            .next()
            .is_some(),
        "a folder with contents should refuse, and say so"
    );
    harness.get_by_label("Close").click();
    settle(&mut harness);
    assert!(harness.state().tree().contains(acme));

    // Personal is empty, so it should go.
    click_in_sidebar(&mut harness, "Personal");
    let View::Folder(personal) = *harness.state().view() else {
        panic!("expected a folder view")
    };
    harness.state_mut().request_delete_folder(personal);
    settle(&mut harness);
    harness.get_by_label("Delete").click();
    settle(&mut harness);

    assert!(!harness.state().tree().contains(personal));
    assert_eq!(
        *harness.state().view(),
        View::Welcome,
        "deleting the folder on screen should fall back to the welcome page"
    );
}

#[test]
fn a_task_with_no_title_still_shows_as_something() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    harness.get_by_label("+ Task").click();
    settle(&mut harness);
    // Create it with the title left empty.
    harness.get_by_label("Create").click();
    settle(&mut harness);

    assert!(
        harness
            .query_all_by_label_contains("Untitled task")
            .next()
            .is_some(),
        "an untitled task must not render as a blank row"
    );
}

// -------------------------------------------- M8 shortcuts, filter and zoom

/// The sidebar's search box, which is the first `TextInput` while the sidebar
/// is open.
fn search_box<'t>(harness: &'t Harness<'_, App>) -> Node<'t> {
    harness
        .get_all_by_role(Role::TextInput)
        .next()
        .expect("the sidebar should be open with a search box")
}

/// The description field of the task **detail view**.
///
/// That view has two multiline fields, description then notes, in that order.
/// The new task dialog has only one, so tests there can still query by role.
fn description_box<'t>(harness: &'t Harness<'_, App>) -> Node<'t> {
    harness
        .get_all_by_role(Role::MultilineTextInput)
        .next()
        .expect("the description field should be present")
}

/// The notes field of the task detail view, which follows the description.
fn notes_box<'t>(harness: &'t Harness<'_, App>) -> Node<'t> {
    harness
        .get_all_by_role(Role::MultilineTextInput)
        .nth(1)
        .expect("the notes field should be present")
}

/// Presses a chord, as a keyboard shortcut rather than a typed character.
fn chord(harness: &mut Harness<'_, App>, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    settle(harness);
}

#[test]
fn ctrl_b_no_longer_touches_the_sidebar() {
    // Freed for bold. Ctrl+Right already opened the folder tree, so the binding
    // was doing a job something else already did, and the ambiguity with the
    // universal bold chord was not worth keeping.
    let mut harness = harness();
    assert!(harness.query_all_by_label("Work").next().is_none());

    chord(&mut harness, Modifiers::CTRL, Key::B);
    assert!(
        harness.query_all_by_label("Work").next().is_none(),
        "Ctrl+B should no longer open the sidebar"
    );
    assert_eq!(harness.state().panel(), Panel::None);

    // The replacement still works.
    chord(&mut harness, Modifiers::CTRL, Key::ArrowRight);
    assert_eq!(harness.state().panel(), Panel::Folders);
}

#[test]
fn ctrl_n_makes_a_task_in_the_folder_you_are_in() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    let View::Folder(acme) = *harness.state().view() else {
        panic!("expected a folder view")
    };
    let before = harness.state().tree().children(Some(acme)).unwrap().len();

    chord(&mut harness, Modifiers::CTRL, Key::N);

    assert!(
        harness
            .query_all_by_label_contains("New task")
            .next()
            .is_some(),
        "Ctrl+N should raise the new task prompt"
    );
    assert_eq!(
        harness.state().tree().children(Some(acme)).unwrap().len(),
        before + 1,
        "the blank task should have been inserted"
    );
}

#[test]
fn ctrl_n_does_nothing_on_the_welcome_page() {
    // There is no folder to put a task in, and inventing one would be worse.
    let mut harness = harness();
    let before = harness.state().tree().len();
    chord(&mut harness, Modifiers::CTRL, Key::N);
    assert_eq!(harness.state().tree().len(), before);
    assert!(
        harness
            .query_all_by_label_contains("New task")
            .next()
            .is_none()
    );
}

#[test]
fn ctrl_shift_n_makes_a_folder_and_is_not_swallowed_by_ctrl_n() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    let before = harness.state().tree().len();

    chord(&mut harness, Modifiers::CTRL.plus(Modifiers::SHIFT), Key::N);

    assert!(
        harness
            .query_all_by_label_contains("New folder")
            .next()
            .is_some(),
        "Ctrl+Shift+N should raise the folder prompt, not the task one"
    );
    assert_eq!(
        harness.state().tree().len(),
        before,
        "a folder is only created on confirm, unlike a task"
    );
}

#[test]
fn no_shortcut_fires_while_a_dialog_is_open() {
    // A modal owns the keyboard. Ctrl+N behind a prompt would stack a second.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    let View::Folder(acme) = *harness.state().view() else {
        panic!("expected a folder view")
    };

    chord(&mut harness, Modifiers::CTRL, Key::N);
    let during = harness.state().tree().children(Some(acme)).unwrap().len();

    chord(&mut harness, Modifiers::CTRL, Key::N);
    assert_eq!(
        harness.state().tree().children(Some(acme)).unwrap().len(),
        during,
        "a second Ctrl+N behind the prompt must be ignored"
    );
}

#[test]
fn escape_clears_an_active_filter() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);

    search_box(&harness).focus();
    harness.run();
    search_box(&harness).type_text("migrate");
    settle(&mut harness);

    chord(&mut harness, Modifiers::NONE, Key::Escape);
    assert_eq!(
        search_box(&harness).value().as_deref().unwrap_or_default(),
        "",
        "Escape should empty the search box"
    );
}

#[test]
fn searching_narrows_the_tree_and_reaches_the_match() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);

    search_box(&harness).focus();
    harness.run();
    search_box(&harness).type_text("Cut over the database");
    settle(&mut harness);

    // The match is visible without any manual expanding, and the folders on the
    // way to it are too.
    for expected in ["Work", "Clients", "Acme", "Cut over the database"] {
        assert!(
            harness
                .query_all_by_label_contains(expected)
                .next()
                .is_some(),
            "{expected} should be visible under the filter"
        );
    }
    // The other branch is gone.
    assert!(
        harness.query_all_by_label("Personal").next().is_none(),
        "a non matching branch should be hidden"
    );
}

#[test]
fn a_search_that_matches_nothing_says_so() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    search_box(&harness).focus();
    harness.run();
    search_box(&harness).type_text("zzz nothing zzz");
    settle(&mut harness);

    assert!(
        harness
            .query_all_by_label_contains("Nothing matches")
            .next()
            .is_some()
    );
}

#[test]
fn clearing_a_filter_restores_how_the_tree_was_arranged() {
    // Filtering auto expands, but in its own id namespace, so it must not leave
    // the tree opened up afterwards.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    // Deliberately leave everything collapsed.
    assert!(harness.query_all_by_label("Clients").next().is_none());

    search_box(&harness).focus();
    harness.run();
    search_box(&harness).type_text("Acme");
    settle(&mut harness);
    assert!(
        harness.query_all_by_label_contains("Acme").next().is_some(),
        "the filter should have expanded down to the match"
    );

    chord(&mut harness, Modifiers::NONE, Key::Escape);
    assert!(
        harness.query_all_by_label("Clients").next().is_none(),
        "clearing the filter should leave the tree collapsed as it was"
    );
}

#[test]
fn ctrl_plus_and_minus_change_the_zoom_and_ctrl_zero_resets_it() {
    let mut harness = harness();
    let start = harness.ctx.zoom_factor();

    chord(&mut harness, Modifiers::CTRL, Key::Plus);
    let bigger = harness.ctx.zoom_factor();
    assert!(bigger > start, "Ctrl+Plus should zoom in");

    chord(&mut harness, Modifiers::CTRL, Key::Minus);
    chord(&mut harness, Modifiers::CTRL, Key::Minus);
    assert!(
        harness.ctx.zoom_factor() < bigger,
        "Ctrl+Minus should zoom out"
    );

    chord(&mut harness, Modifiers::CTRL, Key::Num0);
    assert!(
        (harness.ctx.zoom_factor() - 1.0).abs() < f32::EPSILON,
        "Ctrl+0 should return to 1.0"
    );
}

#[test]
fn the_zoom_is_clamped_to_something_usable() {
    let mut harness = harness();
    for _ in 0..40 {
        chord(&mut harness, Modifiers::CTRL, Key::Plus);
    }
    assert!(harness.ctx.zoom_factor() <= trackcrab::store::settings::ZOOM_MAX);

    for _ in 0..80 {
        chord(&mut harness, Modifiers::CTRL, Key::Minus);
    }
    assert!(harness.ctx.zoom_factor() >= trackcrab::store::settings::ZOOM_MIN);
}

#[test]
fn the_zoom_is_remembered_across_a_restart() {
    let mut harness = harness();
    chord(&mut harness, Modifiers::CTRL, Key::Plus);
    chord(&mut harness, Modifiers::CTRL, Key::Plus);
    let chosen = harness.ctx.zoom_factor();
    assert!(chosen > 1.0);

    let path = harness.state().data_path().to_path_buf();
    let restarted = trackcrab::store::Settings::load(&path);
    assert!(
        (restarted.zoom - chosen).abs() < f32::EPSILON,
        "the chosen zoom {chosen} should have been saved, got {}",
        restarted.zoom
    );
}

#[test]
fn delete_asks_before_removing_the_open_folder() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");
    let View::Folder(personal) = *harness.state().view() else {
        panic!("expected a folder view")
    };

    chord(&mut harness, Modifiers::NONE, Key::Delete);
    assert!(
        harness
            .query_all_by_label_contains("Delete folder")
            .next()
            .is_some(),
        "Delete should ask rather than act"
    );
    assert!(harness.state().tree().contains(personal));
}

// ------------------------------------------------ Enter confirms every dialog

#[test]
fn enter_creates_a_task_once_it_has_a_title() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    harness.get_by_label("+ Task").click();
    settle(&mut harness);

    // Enter with no title yet must do nothing, since Create is disabled.
    chord(&mut harness, Modifiers::NONE, Key::Enter);
    assert!(
        harness
            .query_all_by_label_contains("New task")
            .next()
            .is_some(),
        "Enter should not create a task with no title"
    );

    dialog_field(&harness).focus();
    harness.run();
    dialog_field(&harness).type_text("Done with Enter");
    settle(&mut harness);
    chord(&mut harness, Modifiers::NONE, Key::Enter);

    assert!(
        harness
            .query_all_by_label_contains("New task")
            .next()
            .is_none(),
        "Enter should have closed the prompt"
    );
    let id = task_by_title(&harness, "Done with Enter").expect("the task should exist");
    assert_eq!(
        harness.state().tree().task(id).unwrap().status,
        Status::Open
    );
}

#[test]
fn enter_does_not_submit_while_the_caret_is_in_the_description() {
    // Enter means a new line there, so submitting would make a multiline
    // description impossible to write.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    harness.get_by_label("+ Task").click();
    settle(&mut harness);

    dialog_field(&harness).focus();
    harness.run();
    dialog_field(&harness).type_text("Has a description");
    settle(&mut harness);

    harness.get_by_role(Role::MultilineTextInput).focus();
    harness.run();
    chord(&mut harness, Modifiers::NONE, Key::Enter);

    assert!(
        harness
            .query_all_by_label_contains("New task")
            .next()
            .is_some(),
        "Enter in the description must not submit the dialog"
    );
}

#[test]
fn enter_will_not_create_a_blocked_task_without_a_reason() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    harness.get_by_label("+ Task").click();
    settle(&mut harness);

    dialog_field(&harness).focus();
    harness.run();
    dialog_field(&harness).type_text("Stuck");
    settle(&mut harness);
    harness.get_by_label("Blocked").click();
    settle(&mut harness);

    chord(&mut harness, Modifiers::NONE, Key::Enter);
    assert!(
        harness
            .query_all_by_label_contains("New task")
            .next()
            .is_some(),
        "Enter must respect the same rule the Create button does"
    );
}

#[test]
fn enter_creates_a_folder_from_the_name_dialog() {
    let mut harness = empty_harness();
    toggle_sidebar(&mut harness);
    harness.get_by_label("+").click();
    settle(&mut harness);

    // Blank name, so Enter should be refused.
    chord(&mut harness, Modifiers::NONE, Key::Enter);
    assert!(harness.state().tree().is_empty());

    type_name(&mut harness, "Made with Enter");
    chord(&mut harness, Modifiers::NONE, Key::Enter);

    assert_eq!(harness.state().tree().roots().len(), 1);
    let root = harness.state().tree().roots()[0];
    assert_eq!(
        harness.state().tree().folder(root).unwrap().name,
        "Made with Enter"
    );
}

#[test]
fn enter_confirms_deleting_a_task() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);

    harness.get_by_label("Delete task").click();
    settle(&mut harness);
    chord(&mut harness, Modifiers::NONE, Key::Enter);

    assert!(
        !harness.state().tree().contains(id),
        "Enter should have confirmed the delete"
    );
}

#[test]
fn enter_confirms_deleting_an_empty_folder_but_not_a_full_one() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);

    // Acme has tasks, so Enter must not delete it.
    open_acme(&mut harness);
    let View::Folder(acme) = *harness.state().view() else {
        panic!("expected a folder view")
    };
    harness.state_mut().request_delete_folder(acme);
    settle(&mut harness);
    chord(&mut harness, Modifiers::NONE, Key::Enter);
    assert!(
        harness.state().tree().contains(acme),
        "Enter must not delete a folder that still has contents"
    );
    harness.get_by_label("Close").click();
    settle(&mut harness);

    // Personal is empty, so Enter should take it.
    click_in_sidebar(&mut harness, "Personal");
    let View::Folder(personal) = *harness.state().view() else {
        panic!("expected a folder view")
    };
    harness.state_mut().request_delete_folder(personal);
    settle(&mut harness);
    chord(&mut harness, Modifiers::NONE, Key::Enter);
    assert!(!harness.state().tree().contains(personal));
}

// ------------------------------------------------ blocked reason in the listing

/// Comfortably wider than any listing this suite opens, so truncation is
/// guaranteed rather than dependent on the exact font metrics.
const LONG_REASON: &str = "waiting on the client's change window which is not until \
     the end of next quarter at the earliest, and even then only if the third party \
     finishes its own migration first, which nobody is willing to put a date on";

/// Sets a blocked reason on the seeded task and opens its folder listing.
fn listing_with_reason(harness: &mut Harness<'_, App>, reason: &str) {
    toggle_sidebar(harness);
    open_acme(harness);
    let id = task_by_title(harness, "Land the VPC design").expect("seeded task");
    harness
        .state_mut()
        .tree_mut()
        .edit_task(id, |task| {
            task.status = Status::Blocked(reason.to_owned());
        })
        .unwrap();
    // Close the sidebar so the listing is the only copy of these strings.
    toggle_sidebar(harness);
    settle(harness);
}

#[test]
fn a_blocked_task_shows_its_reason_in_the_listing() {
    let mut harness = harness_sized(1600.0, 700.0);
    listing_with_reason(&mut harness, "waiting on quota");

    assert!(
        harness
            .query_all_by_label_contains("waiting on quota")
            .next()
            .is_some(),
        "the blocked reason should follow the title"
    );
}

#[test]
fn only_blocked_tasks_show_a_reason() {
    let mut harness = harness_sized(1600.0, 700.0);
    listing_with_reason(&mut harness, "waiting on quota");

    // "Cut over the database" is In Progress, so it has nothing to show.
    let rows = harness
        .get_all_by_label_contains("waiting on quota")
        .count();
    assert_eq!(rows, 1, "exactly one row should carry a reason");
}

#[test]
fn the_reason_sits_between_the_title_and_the_time() {
    let mut harness = harness_sized(1600.0, 700.0);
    listing_with_reason(&mut harness, "waiting on quota");

    let reason = harness.get_by_label_contains("waiting on quota").rect();
    let title = harness
        .get_all_by_label_contains("Land the VPC design")
        .map(|node| node.rect())
        .find(|rect| (rect.center().y - reason.center().y).abs() < 4.0)
        .expect("the title should be on the same row");
    let time = harness
        .get_all_by_label("0h 0m")
        .map(|node| node.rect())
        .find(|rect| (rect.center().y - reason.center().y).abs() < 4.0)
        .expect("the time should be on the same row");

    assert!(
        title.right() <= reason.left(),
        "the reason should follow the title"
    );
    assert!(
        reason.right() <= time.left(),
        "the reason should stay left of the time"
    );
}

#[test]
fn the_reason_keeps_clear_of_the_time_column() {
    // The requested behaviour: cut the reason off before it reaches the time.
    let long = LONG_REASON;
    let mut harness = harness_sized(900.0, 700.0);
    listing_with_reason(&mut harness, long);

    let reason = harness
        .get_all_by_label_contains("waiting on the client")
        .next()
        .expect("a truncated reason should still be shown")
        .rect();
    let time = harness
        .get_all_by_label("0h 0m")
        .map(|node| node.rect())
        .find(|rect| (rect.center().y - reason.center().y).abs() < 4.0)
        .expect("the time should be on the same row");

    let clearance = time.left() - reason.right();
    assert!(
        clearance >= 30.0,
        "only {clearance:.0}px between the reason and the time, expected the 40px clearance"
    );
}

#[test]
fn a_cut_off_reason_ends_in_three_dots() {
    let long = LONG_REASON;
    let mut harness = harness_sized(900.0, 700.0);
    listing_with_reason(&mut harness, long);

    let shown = harness
        .get_all_by_label_contains("waiting on the client")
        .next()
        .expect("a truncated reason should still be shown")
        .value()
        .expect("the reason should have text");

    assert!(
        shown.ends_with("..."),
        "a cut off reason should end in three dots, got {shown:?}"
    );
    assert!(
        shown.len() < long.len(),
        "the reason should actually have been shortened"
    );
}

#[test]
fn a_short_reason_is_shown_whole_without_dots() {
    let mut harness = harness_sized(1600.0, 700.0);
    listing_with_reason(&mut harness, "on hold");

    let shown = harness
        .get_by_label_contains("on hold")
        .value()
        .expect("the reason should have text");
    assert_eq!(shown, "on hold", "a reason that fits should not be cut");
}

#[test]
fn a_reason_is_dropped_entirely_when_there_is_no_room_for_it() {
    // Better nothing than a stub that is all ellipsis.
    let mut harness = harness_sized(360.0, 700.0);
    listing_with_reason(&mut harness, "waiting on quota");

    // The title survives; the reason does not have to.
    assert!(
        harness
            .query_all_by_label_contains("Land the VPC design")
            .next()
            .is_some(),
        "the title must always survive"
    );
}

// ------------------------------------------------------------- task notes

#[test]
fn the_task_view_has_a_notes_section_under_the_status() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");

    let notes = harness.get_by_label("NOTES").rect();
    let status = harness.get_by_label("STATUS").rect();
    let time = harness.get_by_label("TIME LOGGED").rect();

    assert!(
        status.center().y < notes.center().y,
        "notes should sit below the status"
    );
    assert!(
        notes.center().y < time.center().y,
        "notes should sit above the logged time"
    );
}

#[test]
fn a_note_is_saved_and_survives_a_restart() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);

    // Description first, then notes: two multiline fields exist now, and the
    // notes one is the second.
    let field = notes_box(&harness);
    field.focus();
    harness.run();
    notes_box(&harness).type_text("ring the DBA before the cutover");
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().task(id).unwrap().notes,
        "ring the DBA before the cutover"
    );

    harness.state_mut().save_now();
    let path = harness.state().data_path().to_path_buf();
    let restarted = App::new(DataStore::at(&path));
    assert_eq!(
        restarted.tree().task(id).unwrap().notes,
        "ring the DBA before the cutover",
        "the note should have reached the disk"
    );
}

#[test]
fn notes_and_description_are_kept_separate() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);

    description_box(&harness).focus();
    harness.run();
    description_box(&harness).type_text("what the task is");
    settle(&mut harness);

    notes_box(&harness).focus();
    harness.run();
    notes_box(&harness).type_text("what I know about it");
    settle(&mut harness);

    let task = harness.state().tree().task(id).unwrap();
    assert_eq!(task.description.as_deref(), Some("what the task is"));
    assert_eq!(task.notes, "what I know about it");
}

#[test]
fn a_note_is_findable_from_the_sidebar_search() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);
    harness
        .state_mut()
        .tree_mut()
        .edit_task(id, |task| {
            task.notes = "quota increase raised with support".to_owned();
        })
        .unwrap();

    toggle_sidebar(&mut harness);
    search_box(&harness).focus();
    harness.run();
    search_box(&harness).type_text("quota increase");
    settle(&mut harness);

    assert!(
        harness
            .query_all_by_label_contains("Land the VPC design")
            .next()
            .is_some(),
        "the task should be reachable by searching its note"
    );
    assert!(
        harness.query_all_by_label("Personal").next().is_none(),
        "the non matching branch should be hidden"
    );
}

// ------------------------------------------- side panels and the directional keys

use trackcrab::ui::Panel;

#[test]
fn the_two_side_panels_can_never_both_be_open() {
    // Guaranteed by the enum rather than by four call sites remembering, but
    // asserted through the real keys so the wiring is covered too.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");
    assert_eq!(harness.state().panel(), Panel::Folders);

    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);
    assert_eq!(
        harness.state().panel(),
        Panel::Comments,
        "opening comments should have closed the folders"
    );

    chord(&mut harness, Modifiers::CTRL, Key::ArrowRight);
    assert_eq!(
        harness.state().panel(),
        Panel::Folders,
        "opening folders should have closed the comments"
    );
}

#[test]
fn the_directional_keys_return_the_content_to_the_middle() {
    let mut harness = harness();
    chord(&mut harness, Modifiers::CTRL, Key::ArrowRight);
    assert_eq!(harness.state().panel(), Panel::Folders);
    chord(&mut harness, Modifiers::CTRL, Key::ArrowRight);
    assert_eq!(
        harness.state().panel(),
        Panel::None,
        "the same direction twice should close it again"
    );
}

#[test]
fn comments_are_refused_when_there_is_no_folder_in_play() {
    // The welcome page has nothing to comment on, so opening an empty notebook
    // would be worse than doing nothing.
    let mut harness = harness();
    assert_eq!(*harness.state().view(), View::Welcome);
    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);
    assert_eq!(harness.state().panel(), Panel::None);
}

#[test]
fn comments_follow_the_folder_holding_an_open_task() {
    // Project context stays reachable while working inside one of its tasks.
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);
    assert_eq!(harness.state().panel(), Panel::Comments);
}

#[test]
fn the_directional_keys_are_the_only_way_to_move_the_panels() {
    let mut harness = harness();
    chord(&mut harness, Modifiers::CTRL, Key::ArrowRight);
    assert_eq!(harness.state().panel(), Panel::Folders);
    chord(&mut harness, Modifiers::CTRL, Key::ArrowRight);
    assert_eq!(harness.state().panel(), Panel::None);
}

#[test]
fn the_first_comment_space_appears_when_the_notebook_is_first_opened() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");
    let View::Folder(personal) = *harness.state().view() else {
        panic!("expected a folder view")
    };
    assert!(
        harness.state().tree().comment_spaces(personal).is_empty(),
        "browsing a folder must not create anything"
    );

    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);
    assert_eq!(
        harness.state().tree().comment_spaces(personal).len(),
        1,
        "opening the notebook should give you somewhere to type"
    );
    assert_eq!(
        harness.state().tree().comment_spaces(personal)[0].title,
        "Comments 1"
    );
}

#[test]
fn browsing_many_folders_creates_nothing() {
    // The reason the first space is made on opening the notebook rather than on
    // opening the folder.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    for name in ["Work", "Clients", "Acme", "Personal"] {
        click_in_sidebar(&mut harness, name);
    }
    let tree = harness.state().tree();
    let total: usize = tree
        .roots()
        .iter()
        .flat_map(|r| {
            let mut all = vec![*r];
            all.extend(tree.descendants(*r));
            all
        })
        .map(|id| tree.comment_spaces(id).len())
        .sum();
    assert_eq!(total, 0, "no comment space should exist yet");
}

#[test]
fn the_notebook_overlays_the_listing_rather_than_squeezing_it() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    // Close the folder tree first, so the only change being measured is the
    // notebook appearing. Closing the sidebar legitimately reflows the listing.
    toggle_sidebar(&mut harness);
    assert_eq!(harness.state().panel(), Panel::None);

    // The *time column* is the discriminator, not the title. A squeezed listing
    // would move it left to fit beside the notebook; an overlaid one leaves it
    // exactly where it was, simply covered up.
    let before = harness
        .get_all_by_label("2h")
        .next()
        .expect("the time column")
        .rect();

    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);
    assert_eq!(harness.state().panel(), Panel::Comments);

    let after = harness
        .get_all_by_label("2h")
        .next()
        .expect("the time column should still be laid out, just hidden")
        .rect();
    assert!(
        (before.right() - after.right()).abs() < 1.0,
        "the time column moved from {:.0} to {:.0}, so the notebook is squeezing \
         the listing rather than overlaying it",
        before.right(),
        after.right()
    );
}

#[test]
fn typing_in_the_notebook_is_saved_and_survives_a_restart() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");
    let View::Folder(personal) = *harness.state().view() else {
        panic!("expected a folder view")
    };
    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);

    // The notebook body is the only multiline field on screen now.
    harness.get_by_role(Role::MultilineTextInput).focus();
    harness.run();
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("Budget signed off, three week window.");
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().comment_spaces(personal)[0].body,
        "Budget signed off, three week window."
    );

    harness.state_mut().save_now();
    let path = harness.state().data_path().to_path_buf();
    let restarted = App::new(DataStore::at(&path));
    assert_eq!(
        restarted.tree().comment_spaces(personal)[0].body,
        "Budget signed off, three week window.",
        "the comment should have reached the disk"
    );
}

#[test]
fn the_panel_choice_is_remembered_across_a_restart() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");
    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);
    assert_eq!(harness.state().panel(), Panel::Comments);

    let path = harness.state().data_path().to_path_buf();
    let restarted = trackcrab::store::Settings::load(&path);
    assert_eq!(restarted.panel, Panel::Comments);
}

// ------------------------------------------------- the comments notebook (N4)

/// The notebook subtree, scoped the same way [`sidebar`] is.
///
/// The notebook's `+` carries exactly the same label as the sidebar's, so even
/// though the two panels can never be open together, the query says which one
/// it means.
fn notebook<'t>(harness: &'t Harness<'_, App>, probe: &str) -> Node<'t> {
    let mut node = harness.get_by_label("COMMENTS");
    for _ in 0..8 {
        let Some(parent) = node.parent() else { break };
        node = parent;
        if node.query_all_by_label(probe).next().is_some() {
            return node;
        }
    }
    panic!("could not find a notebook subtree containing {probe:?}");
}

/// Clicks one of the notebook's own controls.
fn click_in_notebook(harness: &mut Harness<'_, App>, label: &str) {
    notebook(harness, label).get_by_label(label).click();
    settle(harness);
}

/// Opens a folder and its notebook, returning the folder.
fn open_notebook(harness: &mut Harness<'_, App>, folder: &str) -> trackcrab::model::NodeId {
    toggle_sidebar(harness);
    click_in_sidebar(harness, folder);
    let View::Folder(id) = *harness.state().view() else {
        panic!("expected a folder view")
    };
    chord(harness, Modifiers::CTRL, Key::ArrowLeft);
    id
}

/// Which space the notebook is pointing at.
fn cursor(harness: &Harness<'_, App>) -> usize {
    harness
        .state()
        .comment_cursor()
        .expect("the notebook should have a cursor")
        .1
}

/// The notebook's title field. With the notebook open the folder tree is shut,
/// so this is the only single line field on screen.
fn space_title<'t>(harness: &'t Harness<'_, App>) -> Node<'t> {
    harness
        .get_all_by_role(Role::TextInput)
        .next()
        .expect("the notebook title field")
}

#[test]
fn the_overlay_starts_entirely_off_the_right_edge() {
    // The geometry the tween interpolates over, checked as a pure function so
    // the animation test below only has to prove that it moves.
    let content = eframe::egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1600.0, 900.0));
    let out = trackcrab::ui::views::comments::overlay_rect(content, 0.0);
    let settled = trackcrab::ui::views::comments::overlay_rect(content, 1.0);

    assert!(
        out.left() >= content.right(),
        "at slide 0 the panel should be wholly past the right edge, not at {:.0}",
        out.left()
    );
    assert!(
        (settled.right() - content.right()).abs() < 0.01,
        "at slide 1 the panel should sit flush with the right edge"
    );
    assert!(
        (out.width() - settled.width()).abs() < 0.01,
        "the panel should slide, not grow"
    );
    let half = trackcrab::ui::views::comments::overlay_rect(content, 0.5);
    assert!(
        half.left() > settled.left() && half.left() < out.left(),
        "halfway should be halfway"
    );
}

#[test]
fn the_notebook_slides_in_from_the_right_edge() {
    // The tween Kyle asked for, matched to the folder tree's. Measured rather
    // than assumed: the panel has to be *somewhere else* partway through, or it
    // is simply appearing.
    let mut harness = harness_animated();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");

    harness.key_press_modifiers(Modifiers::CTRL, Key::ArrowLeft);
    // step() rather than run(): run() loops frames until nothing asks for a
    // repaint, which swallows the whole animation in one call.
    let mut seen = Vec::new();
    for _ in 0..60 {
        harness.step();
        if let Some(node) = harness.query_by_label("COMMENTS") {
            seen.push(node.rect().left());
        }
    }
    let settled = *seen.last().expect("the notebook should be on screen");
    assert!(
        seen.iter().any(|left| *left > settled + 20.0),
        "the notebook never sat right of its resting place ({settled:.0}), so it \
         appeared rather than sliding: {seen:?}"
    );
    assert!(
        seen.windows(2).all(|w| w[1] <= w[0] + 0.01),
        "the slide should only ever move inwards: {seen:?}"
    );
}

#[test]
fn the_notebook_slides_back_out_before_it_goes() {
    // Closing has to tween too, rather than the panel blinking away.
    let mut harness = harness_animated();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");
    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);
    let settled = harness.get_by_label("COMMENTS").rect().left();

    harness.key_press_modifiers(Modifiers::CTRL, Key::ArrowLeft);
    let mut seen = Vec::new();
    for _ in 0..60 {
        harness.step();
        if let Some(node) = harness.query_by_label("COMMENTS") {
            seen.push(node.rect().left());
        }
    }
    assert!(
        seen.iter().any(|left| *left > settled + 20.0),
        "the notebook vanished instead of sliding out: {seen:?}"
    );
    assert!(
        harness.query_by_label("COMMENTS").is_none(),
        "once the tween is done the notebook should be gone entirely"
    );
}

#[test]
fn the_notebook_content_stays_inside_the_panel() {
    // Regression: sizing the Area's own Ui to the panel made the frame grow by
    // its margins and hang off the right edge, clipping the last word of every
    // wrapped line. Nothing visible moved, so only a measurement catches it.
    let mut harness = harness();
    open_notebook(&mut harness, "Personal");

    let panel = trackcrab::ui::views::comments::overlay_rect(
        eframe::egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1600.0, 900.0)),
        1.0,
    );
    let body = harness.get_by_role(Role::MultilineTextInput).rect();
    assert!(
        body.right() <= panel.right() - 6.0,
        "the writing area reaches {:.0}, past the panel's inner edge at {:.0}",
        body.right(),
        panel.right()
    );
    assert!(
        body.left() >= panel.left(),
        "the writing area starts left of the panel"
    );
    let close = notebook(&harness, "\u{00d7}")
        .get_by_label("\u{00d7}")
        .rect();
    assert!(
        close.right() <= panel.right(),
        "the close button hangs off the right edge"
    );
}

#[test]
fn the_plus_adds_a_space_and_lands_on_it() {
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");
    assert_eq!(cursor(&harness), 0);

    click_in_notebook(&mut harness, "+");
    assert_eq!(harness.state().tree().comment_spaces(personal).len(), 2);
    assert_eq!(
        cursor(&harness),
        1,
        "the new space should be the one showing"
    );
    assert!(
        harness.query_by_label("2 / 2").is_some(),
        "the position readout should have followed"
    );
}

#[test]
fn the_arrows_cycle_and_wrap() {
    let mut harness = harness();
    open_notebook(&mut harness, "Personal");
    click_in_notebook(&mut harness, "+");
    assert_eq!(cursor(&harness), 1);

    // Forward off the end comes back to the first.
    click_in_notebook(&mut harness, "Next space");
    assert_eq!(cursor(&harness), 0);
    // And backwards off the front to the last.
    click_in_notebook(&mut harness, "Previous space");
    assert_eq!(cursor(&harness), 1);
}

#[test]
fn the_arrows_are_dead_with_only_one_space() {
    // Nowhere to go, so an enabled arrow would be a lie.
    let mut harness = harness();
    open_notebook(&mut harness, "Personal");
    for arrow in ["Previous space", "Next space"] {
        let node = notebook(&harness, arrow).get_by_label(arrow);
        assert!(
            node.accesskit_node().is_disabled(),
            "the {arrow:?} arrow should be disabled with a single space"
        );
    }
}

#[test]
fn each_space_keeps_its_own_text() {
    // The whole point of several spaces: a kickoff note and a list of blockers
    // must not run together.
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");

    harness.get_by_role(Role::MultilineTextInput).focus();
    harness.run();
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("Kickoff on the 8th.");
    settle(&mut harness);

    click_in_notebook(&mut harness, "+");
    harness.get_by_role(Role::MultilineTextInput).focus();
    harness.run();
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("Waiting on the security review.");
    settle(&mut harness);

    let spaces = harness.state().tree().comment_spaces(personal);
    assert_eq!(spaces[0].body, "Kickoff on the 8th.");
    assert_eq!(spaces[1].body, "Waiting on the security review.");
}

#[test]
fn a_space_can_be_retitled() {
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");

    space_title(&harness).focus();
    harness.run();
    // The auto title is selected wholesale, so typing replaces it.
    harness.get_all_by_role(Role::TextInput).next().unwrap();
    for _ in 0.."Comments 1".len() {
        harness.key_press(Key::Backspace);
    }
    settle(&mut harness);
    space_title(&harness).type_text("Blockers");
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().comment_spaces(personal)[0].title,
        "Blockers"
    );
}

#[test]
fn deleting_a_space_is_confirmed_first() {
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");
    click_in_notebook(&mut harness, "+");
    assert_eq!(harness.state().tree().comment_spaces(personal).len(), 2);

    click_in_notebook(&mut harness, "Delete space");
    assert_eq!(
        harness.state().tree().comment_spaces(personal).len(),
        2,
        "nothing should go until it has been confirmed"
    );
    assert!(harness.query_by_label("Delete comment space").is_some());

    // Enter confirms, as it does everywhere else.
    chord(&mut harness, Modifiers::NONE, Key::Enter);
    assert_eq!(harness.state().tree().comment_spaces(personal).len(), 1);
    assert_eq!(
        cursor(&harness),
        0,
        "the cursor should have been pulled back into range"
    );
}

#[test]
fn backing_out_of_a_delete_keeps_the_space() {
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");
    click_in_notebook(&mut harness, "Delete space");
    chord(&mut harness, Modifiers::NONE, Key::Escape);
    assert_eq!(harness.state().tree().comment_spaces(personal).len(), 1);
}

#[test]
fn searching_finds_a_folder_through_its_comments() {
    // Project context is where a customer name or a contract number actually
    // lives, so the search has to reach into it.
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");
    harness
        .state_mut()
        .tree_mut()
        .edit_comment_space(personal, 0, |space| {
            space.body = "Renewal hinges on Northwind signing off".to_owned();
        })
        .unwrap();

    // Back to the folder tree to search.
    chord(&mut harness, Modifiers::CTRL, Key::F);
    search_box(&harness).type_text("northwind");
    settle(&mut harness);

    assert!(
        sidebar(&harness, "Personal")
            .query_all_by_label("Personal")
            .next()
            .is_some(),
        "the folder whose comments mention Northwind should survive the filter"
    );
    assert!(
        sidebar(&harness, "Personal")
            .query_all_by_label("Work")
            .next()
            .is_none(),
        "a folder with no match anywhere should be filtered out"
    );
}

#[test]
fn a_search_hit_opens_the_space_holding_it() {
    // With a dozen spaces, landing on page one and cycling to find the match
    // would make the search useless.
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");
    {
        let tree = harness.state_mut().tree_mut();
        tree.add_comment_space(personal).unwrap();
        tree.edit_comment_space(personal, 1, |space| {
            space.body = "Northwind renewal".to_owned();
        })
        .unwrap();
    }

    chord(&mut harness, Modifiers::CTRL, Key::F);
    search_box(&harness).type_text("northwind");
    settle(&mut harness);
    click_in_sidebar(&mut harness, "Personal");

    assert_eq!(
        cursor(&harness),
        1,
        "the notebook should have opened on the page carrying the match"
    );
}

// ------------------------------------------------- keyboard navigation (N5)

/// Presses a plain key and lets the frame settle.
fn press(harness: &mut Harness<'_, App>, key: Key) {
    chord(harness, Modifiers::NONE, key);
}

/// Where the folder tree's keyboard cursor is, by label.
fn cursor_label(harness: &Harness<'_, App>) -> String {
    let id = harness
        .state()
        .tree_cursor()
        .expect("the tree cursor should be somewhere");
    let tree = harness.state().tree();
    trackcrab::ui::row_label(tree.get(id).expect("a live node")).to_owned()
}

#[test]
fn down_enters_the_tree_at_the_top() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    assert!(harness.state().tree_cursor().is_none());

    press(&mut harness, Key::ArrowDown);
    assert_eq!(cursor_label(&harness), "Work");
}

#[test]
fn up_enters_the_tree_at_the_bottom() {
    // Coming in from the other end: pressing up with no cursor should not
    // silently start at the top and then go nowhere.
    let mut harness = harness();
    toggle_sidebar(&mut harness);

    press(&mut harness, Key::ArrowUp);
    assert_eq!(cursor_label(&harness), "Personal");
}

#[test]
fn the_arrows_walk_the_tree_as_a_flat_list() {
    // Depth is ignored entirely: the cursor moves the way the eye reads the
    // panel, not the way the tree is shaped.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);

    // Acme is open, so the visible rows are Work, Clients, Acme, its two tasks,
    // then Personal back at the top level. Start from the very top rather than
    // from where opening Acme left the cursor.
    let top = harness.state().tree().roots()[0];
    harness.state_mut().set_tree_cursor(Some(top));
    let mut walked = vec![cursor_label(&harness)];
    for _ in 0..5 {
        press(&mut harness, Key::ArrowDown);
        walked.push(cursor_label(&harness));
    }
    assert_eq!(
        walked,
        [
            "Work",
            "Clients",
            "Acme",
            "Cut over the database",
            "Land the VPC design",
            "Personal",
        ],
        "stepping off Acme's last task should reach Personal, three levels up"
    );

    // And back up, through exactly the same rows in reverse.
    let mut back = Vec::new();
    for _ in 0..5 {
        press(&mut harness, Key::ArrowUp);
        back.push(cursor_label(&harness));
    }
    let mut expected: Vec<String> = walked[..5].to_vec();
    expected.reverse();
    assert_eq!(back, expected);
}

#[test]
fn a_collapsed_folder_hides_its_children_from_the_keyboard() {
    // The cursor must never land on a row that is not on screen.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    // Collapse Clients, which takes Acme and both tasks with it.
    click_collapse_arrow(&mut harness, "Clients");

    let top = harness.state().tree().roots()[0];
    harness.state_mut().set_tree_cursor(Some(top));
    assert_eq!(cursor_label(&harness), "Work");
    press(&mut harness, Key::ArrowDown);
    assert_eq!(cursor_label(&harness), "Clients");
    press(&mut harness, Key::ArrowDown);
    assert_eq!(
        cursor_label(&harness),
        "Personal",
        "a collapsed subtree should be stepped straight over"
    );
}

#[test]
fn the_ends_of_the_tree_clamp() {
    // A tree has a real top and bottom. Holding a key down should stop there
    // rather than teleport to the other end.
    let mut harness = harness();
    toggle_sidebar(&mut harness);

    for _ in 0..10 {
        press(&mut harness, Key::ArrowDown);
    }
    assert_eq!(cursor_label(&harness), "Personal");
    for _ in 0..10 {
        press(&mut harness, Key::ArrowUp);
    }
    assert_eq!(cursor_label(&harness), "Work");
}

#[test]
fn enter_opens_the_folder_under_the_cursor() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::Enter);

    let View::Folder(id) = *harness.state().view() else {
        panic!("Enter on a folder should have opened a folder view")
    };
    assert_eq!(harness.state().tree().folder(id).unwrap().name, "Work");
}

#[test]
fn enter_opens_the_task_under_the_cursor() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    // Opening Acme left the cursor on it, so one step down reaches its first
    // task. That the cursor picks up from what is open is the point here.
    press(&mut harness, Key::ArrowDown);
    assert_eq!(cursor_label(&harness), "Cut over the database");

    press(&mut harness, Key::Enter);
    let View::Task(id) = *harness.state().view() else {
        panic!("Enter on a task should have opened the detail view")
    };
    assert_eq!(
        harness.state().tree().task(id).unwrap().title,
        "Cut over the database"
    );
}

#[test]
fn clicking_a_row_moves_the_cursor_there() {
    // Otherwise the next arrow press would jump back to wherever the keyboard
    // had been left, which feels like the panel losing your place.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");
    assert_eq!(cursor_label(&harness), "Personal");

    press(&mut harness, Key::ArrowUp);
    assert_eq!(
        cursor_label(&harness),
        "Work",
        "up from Personal should reach the row above it, not restart"
    );
}

#[test]
fn search_then_arrow_then_enter_is_one_flow() {
    // A single line search box has nothing to do with up and down, so the tree
    // keeps them. Typing a search and arrowing straight into the result is the
    // whole point of having both.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    search_box(&harness).focus();
    harness.run();
    search_box(&harness).type_text("VPC");
    settle(&mut harness);

    for _ in 0..4 {
        press(&mut harness, Key::ArrowDown);
    }
    assert_eq!(cursor_label(&harness), "Land the VPC design");
    press(&mut harness, Key::Enter);
    let View::Task(id) = *harness.state().view() else {
        panic!("Enter should have opened the match")
    };
    assert_eq!(
        harness.state().tree().task(id).unwrap().title,
        "Land the VPC design"
    );
}

#[test]
fn the_arrows_stay_out_of_the_way_of_a_body_of_text() {
    // A description is not a search box: up and down move the caret through
    // lines, so the tree must not take them.
    let mut harness = harness();
    // open_task leaves the sidebar closed, and the tree keys only act while it
    // has the screen, so put it back.
    open_task(&mut harness, "Land the VPC design");
    toggle_sidebar(&mut harness);
    assert_eq!(harness.state().panel(), Panel::Folders);
    let before = cursor_label(&harness);

    description_box(&harness).focus();
    harness.run();
    press(&mut harness, Key::ArrowDown);
    assert_eq!(
        cursor_label(&harness),
        before,
        "the cursor moved while the caret was in the description"
    );
}

#[test]
fn delete_stays_out_of_the_way_of_any_field() {
    // Pressing Delete while editing a description used to raise the task's
    // delete confirmation, which is about as unwelcome as a keystroke gets.
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");

    description_box(&harness).focus();
    harness.run();
    press(&mut harness, Key::Delete);
    assert!(
        harness.query_by_label("Delete task?").is_none()
            && harness
                .query_by_label_contains("cannot be undone")
                .is_none(),
        "Delete in a text field should edit the text, not offer to delete the task"
    );
}

#[test]
fn the_filter_hides_rows_from_the_keyboard_too() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    search_box(&harness).focus();
    harness.run();
    search_box(&harness).type_text("VPC");
    settle(&mut harness);

    harness.state_mut().set_tree_cursor(None);
    let mut walked = Vec::new();
    for _ in 0..5 {
        press(&mut harness, Key::ArrowDown);
        walked.push(cursor_label(&harness));
    }
    assert_eq!(
        walked,
        [
            "Work",
            "Clients",
            "Acme",
            "Land the VPC design",
            "Land the VPC design"
        ],
        "only the path to the match should be reachable, and it should clamp there"
    );
}

#[test]
fn the_tree_keys_do_nothing_while_the_notebook_has_the_screen() {
    // The two panels are mutually exclusive, and so are their keys.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    press(&mut harness, Key::ArrowDown);
    let before = cursor_label(&harness);

    click_in_sidebar(&mut harness, "Personal");
    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);
    assert_eq!(harness.state().panel(), Panel::Comments);
    press(&mut harness, Key::ArrowDown);
    assert_eq!(cursor_label(&harness), "Personal", "was {before}");
}

#[test]
fn the_plain_arrows_cycle_comment_spaces() {
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");
    click_in_notebook(&mut harness, "+");
    click_in_notebook(&mut harness, "+");
    assert_eq!(harness.state().tree().comment_spaces(personal).len(), 3);
    assert_eq!(cursor(&harness), 2);

    press(&mut harness, Key::ArrowRight);
    assert_eq!(cursor(&harness), 0, "forward off the end should wrap");
    press(&mut harness, Key::ArrowLeft);
    assert_eq!(cursor(&harness), 2, "and backwards off the front");
    press(&mut harness, Key::ArrowLeft);
    assert_eq!(cursor(&harness), 1);
}

#[test]
fn the_comment_arrows_stay_out_of_the_way_while_typing() {
    let mut harness = harness();
    open_notebook(&mut harness, "Personal");
    click_in_notebook(&mut harness, "+");
    assert_eq!(cursor(&harness), 1);

    harness.get_by_role(Role::MultilineTextInput).focus();
    harness.run();
    press(&mut harness, Key::ArrowLeft);
    assert_eq!(
        cursor(&harness),
        1,
        "a left arrow in the writing area should move the caret, not the page"
    );
}

#[test]
fn the_cursor_and_the_open_row_are_drawn_differently() {
    // The cursor says "Enter would open this"; the fill says "this is what you
    // are looking at". Same row or different rows, they must not be one look.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    click_in_sidebar(&mut harness, "Personal");
    press(&mut harness, Key::ArrowUp);

    assert_eq!(cursor_label(&harness), "Work");
    let View::Folder(open) = *harness.state().view() else {
        panic!("Personal should still be the open folder")
    };
    assert_eq!(
        harness.state().tree().folder(open).unwrap().name,
        "Personal",
        "moving the cursor must not change what is open"
    );
}

#[test]
fn the_flat_row_list_matches_what_is_on_screen() {
    // The keyboard walks this list, so it is the one thing that must never
    // disagree with the render. Checked against the rows egui actually drew.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);

    let drawn: Vec<String> = {
        let scope = sidebar(&harness, "Work");
        // Row labels in draw order, which is top to bottom.
        [
            "Work",
            "Clients",
            "Acme",
            "Cut over the database",
            "Land the VPC design",
            "Personal",
        ]
        .into_iter()
        .filter(|name| scope.query_all_by_label(name).next().is_some())
        .map(std::borrow::ToOwned::to_owned)
        .collect()
    };
    let flat: Vec<String> = trackcrab::ui::flat_rows(
        &harness.ctx,
        harness.state().tree(),
        &trackcrab::ui::Filter::default(),
    )
    .into_iter()
    .map(|id| {
        trackcrab::ui::row_label(harness.state().tree().get(id).expect("a live node")).to_owned()
    })
    .collect();

    assert_eq!(
        flat, drawn,
        "the flat list the keyboard walks does not match the rows on screen"
    );
}

#[test]
fn following_the_cursor_does_not_drag_the_tree_sideways() {
    // Regression: scrolling the cursor into view targeted both axes, so
    // arrowing onto a deeply indented row scrolled the panel right and chopped
    // the left off every label, including the roots.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    let before = sidebar(&harness, "Work").get_by_label("Work").rect().left();

    // Down onto Acme's children, which are indented well past the panel width.
    for _ in 0..3 {
        press(&mut harness, Key::ArrowDown);
    }
    settle(&mut harness);

    let after = sidebar(&harness, "Work").get_by_label("Work").rect().left();
    assert!(
        (before - after).abs() < 1.0,
        "the tree scrolled sideways from {before:.0} to {after:.0}, so root labels are cut off"
    );
}

// --------------------------------------------------- markdown rendering (D2)

/// A document using every feature the parser knows, for exercising the real
/// widget rather than the pure layout function.
const RICH: &str = "# Heading\n\
     **bold** *italic* __under__ ~~gone~~ `code`\n\
     - [x] done\n\
       - nested\n\
     1. numbered\n\
     ==yellow|marked== ==#f2c14e|hex==\n\
     [label](https://example.com) https://bare.example.com\n\
     ---\n\
     ```rust\n\
     let x = *p;\n\
     ```\n\
     escaped \\*star\\*";

#[test]
fn a_rich_document_renders_without_desyncing_the_caret() {
    // `text::layout` carries a debug assertion that the job's text matches the
    // source byte for byte, because a TextEdit maps caret positions through the
    // galley. Rendering the document through the real widget in a debug build
    // is therefore a live check of that invariant, which no pure test can be:
    // it is the wiring, not the function, being exercised.
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");
    harness
        .state_mut()
        .tree_mut()
        .edit_comment_space(personal, 0, |space| space.body = RICH.to_owned())
        .unwrap();
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().comment_spaces(personal)[0].body,
        RICH,
        "rendering must not rewrite the source"
    );
}

#[test]
fn typing_markdown_stores_exactly_what_was_typed() {
    // The layouter must never edit the buffer. If it did, typing `**` would
    // fight the renderer and characters would go missing.
    let mut harness = harness();
    let personal = open_notebook(&mut harness, "Personal");

    harness.get_by_role(Role::MultilineTextInput).focus();
    harness.run();
    let typed = "## Notes with **bold** and `code` and ==blue|mark==";
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text(typed);
    settle(&mut harness);

    assert_eq!(
        harness.state().tree().comment_spaces(personal)[0].body,
        typed
    );
}

#[test]
fn markdown_in_a_task_note_round_trips_through_the_disk() {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = match harness.state().view() {
        View::Task(id) => *id,
        other => panic!("expected a task, got {other:?}"),
    };
    harness
        .state_mut()
        .tree_mut()
        .edit_task(id, |task| task.notes = RICH.to_owned())
        .unwrap();
    settle(&mut harness);
    harness.state_mut().save_now();

    let path = harness.state().data_path().to_path_buf();
    let restarted = App::new(DataStore::at(&path));
    assert_eq!(
        restarted.tree().task(id).unwrap().notes,
        RICH,
        "markdown is stored as plain text, so it must survive verbatim"
    );
}

#[test]
fn a_description_renders_markdown_too() {
    // Kyle asked for descriptions alongside notes and comments. Leaving one of
    // the three plain would read as an oversight.
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = match harness.state().view() {
        View::Task(id) => *id,
        other => panic!("expected a task, got {other:?}"),
    };
    harness
        .state_mut()
        .tree_mut()
        .edit_task(id, |task| task.set_description(RICH.to_owned()))
        .unwrap();
    settle(&mut harness);

    assert_eq!(
        harness
            .state()
            .tree()
            .task(id)
            .unwrap()
            .description
            .as_deref(),
        Some(RICH)
    );
}

// ------------------------------------------------ markdown editing keys (D5)

/// Focuses the notebook body and returns the folder holding it.
fn notebook_body(harness: &mut Harness<'_, App>, seed: &str) -> trackcrab::model::NodeId {
    let folder = open_notebook(harness, "Personal");
    harness
        .state_mut()
        .tree_mut()
        .edit_comment_space(folder, 0, |space| seed.clone_into(&mut space.body))
        .unwrap();
    settle(harness);
    harness.get_by_role(Role::MultilineTextInput).focus();
    harness.run();
    folder
}

fn body_of(harness: &Harness<'_, App>, folder: trackcrab::model::NodeId) -> String {
    harness.state().tree().comment_spaces(folder)[0]
        .body
        .clone()
}

#[test]
fn tab_indents_a_list_item_through_the_real_widget() {
    // The pure tests cover the transformation. This covers the wiring: the key
    // has to be consumed before the widget sees it, or Tab moves focus instead.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- one\n- two");
    // Caret to the end of the second item.
    press(&mut harness, Key::End);
    press(&mut harness, Key::Tab);

    assert_eq!(body_of(&harness, folder), "- one\n  - two");
}

#[test]
fn tab_does_not_move_focus_out_of_a_markdown_field() {
    // egui uses Tab for focus by default. Consuming it first is what makes it
    // indent, and it means Escape is now the way out of a field.
    let mut harness = harness();
    notebook_body(&mut harness, "- one\n- two");
    assert!(harness.get_by_role(Role::MultilineTextInput).is_focused());
    press(&mut harness, Key::Tab);
    assert!(
        harness.get_by_role(Role::MultilineTextInput).is_focused(),
        "Tab moved focus out of the field instead of indenting"
    );
}

#[test]
fn shift_tab_outdents_rather_than_indenting() {
    // Checked before plain Tab, or the plainer chord swallows it.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- one\n  - two");
    press(&mut harness, Key::End);
    chord(&mut harness, Modifiers::SHIFT, Key::Tab);

    assert_eq!(body_of(&harness, folder), "- one\n- two");
}

#[test]
fn enter_continues_a_list_through_the_real_widget() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- one");
    press(&mut harness, Key::End);
    press(&mut harness, Key::Enter);

    assert_eq!(body_of(&harness, folder), "- one\n- ");
}

#[test]
fn enter_twice_leaves_the_list_rather_than_stacking_empty_items() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- one");
    press(&mut harness, Key::End);
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::Enter);

    assert_eq!(
        body_of(&harness, folder),
        "- one\n",
        "the second Enter should have removed the empty marker"
    );
}

#[test]
fn enter_in_prose_still_inserts_a_plain_newline() {
    // The Option the edit functions return exists for this: declining hands the
    // key back to egui, so everything that is not a list behaves as before.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "plain");
    press(&mut harness, Key::End);
    press(&mut harness, Key::Enter);

    assert_eq!(body_of(&harness, folder), "plain\n");
}

#[test]
fn backspace_after_a_marker_strips_it_through_the_real_widget() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- one");
    press(&mut harness, Key::Home);
    // Home lands before the marker, so step past it.
    for _ in 0..2 {
        press(&mut harness, Key::ArrowRight);
    }
    press(&mut harness, Key::Backspace);

    assert_eq!(body_of(&harness, folder), "one");
}

#[test]
fn backspace_mid_word_still_deletes_a_character() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- one");
    press(&mut harness, Key::End);
    press(&mut harness, Key::Backspace);

    assert_eq!(body_of(&harness, folder), "- on");
}

#[test]
fn an_edit_made_behind_the_widget_still_reaches_the_disk() {
    // The buffer is changed without the widget knowing, so `changed()` has to
    // be raised by hand or nothing would ever be saved.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- one");
    press(&mut harness, Key::End);
    press(&mut harness, Key::Enter);
    settle(&mut harness);
    harness.state_mut().save_now();

    let path = harness.state().data_path().to_path_buf();
    let restarted = App::new(DataStore::at(&path));
    assert_eq!(
        restarted.tree().comment_spaces(folder)[0].body,
        "- one\n- ",
        "the edit was not marked as a change, so it was never saved"
    );
}

#[test]
fn escape_is_the_way_out_of_a_markdown_field() {
    // Tab no longer leaves, so this is the replacement, and it is egui's own
    // behaviour rather than anything added here.
    let mut harness = harness();
    notebook_body(&mut harness, "- one");
    assert!(harness.get_by_role(Role::MultilineTextInput).is_focused());
    press(&mut harness, Key::Escape);
    assert!(
        !harness.get_by_role(Role::MultilineTextInput).is_focused(),
        "Escape should have surrendered focus"
    );
}

#[test]
fn a_task_note_gets_the_same_keys() {
    // All four fields go through one wrapper, so this is really asserting that
    // the wrapper is what they all use.
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = match harness.state().view() {
        View::Task(id) => *id,
        other => panic!("expected a task, got {other:?}"),
    };
    settle(&mut harness);
    // Typed rather than written into the tree: the detail view keeps its own
    // buffer and only reloads it when the view moves, so poking the tree behind
    // it would be overwritten by the next commit.
    notes_box(&harness).focus();
    harness.run();
    notes_box(&harness).type_text("- one");
    settle(&mut harness);
    press(&mut harness, Key::End);
    press(&mut harness, Key::Enter);

    assert_eq!(harness.state().tree().task(id).unwrap().notes, "- one\n- ");
}

#[test]
fn a_whole_list_can_be_typed_without_writing_a_single_marker() {
    // The end to end shape of D5, and the one test that would catch any of the
    // four keys regressing. Only the *first* marker of each list is typed; every
    // other one is produced by Enter, Tab or Shift+Tab.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");

    let write = |harness: &mut Harness<'_, App>, text: &str| {
        harness
            .get_by_role(Role::MultilineTextInput)
            .type_text(text);
        settle(harness);
    };

    write(&mut harness, "# Shopping");
    press(&mut harness, Key::Enter);
    write(&mut harness, "- milk");
    press(&mut harness, Key::Enter);
    write(&mut harness, "bread");
    press(&mut harness, Key::Enter);
    write(&mut harness, "flour");
    press(&mut harness, Key::Tab);
    press(&mut harness, Key::Enter);
    write(&mut harness, "plain");
    press(&mut harness, Key::Enter);
    write(&mut harness, "wholemeal");
    chord(&mut harness, Modifiers::SHIFT, Key::Tab);
    // Twice: the first continues the list, the second leaves it.
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::Enter);
    write(&mut harness, "1. first");
    press(&mut harness, Key::Enter);
    write(&mut harness, "second");
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::Enter);
    write(&mut harness, "- [ ] pay rent");
    press(&mut harness, Key::Enter);
    write(&mut harness, "call mum");

    assert_eq!(
        body_of(&harness, folder),
        "# Shopping\n\
         - milk\n\
         - bread\n\
         \u{20}\u{20}- flour\n\
         \u{20}\u{20}- plain\n\
         - wholemeal\n\
         1. first\n\
         2. second\n\
         - [ ] pay rent\n\
         - [ ] call mum"
    );
}

// -------------------------------------------- toolbar and shortcuts (D6)

/// Selects everything in the focused field.
///
/// `Modifiers::COMMAND` rather than `CTRL`: egui's own select-all checks the
/// `command` flag, which the platform layer sets from Ctrl but a synthesised
/// event does not. The app's own chords use `CTRL`, matching every other
/// shortcut in it, so the two appear side by side in these tests.
fn select_all(harness: &mut Harness<'_, App>) {
    chord(harness, Modifiers::COMMAND, Key::A);
}

/// A toolbar button in the notebook, by its accessibility label.
fn toolbar_button<'t>(harness: &'t Harness<'_, App>, label: &'t str) -> Node<'t> {
    notebook(harness, label).get_by_label(label)
}

#[test]
fn ctrl_b_bolds_the_selection() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("loud");
    settle(&mut harness);
    // Select it all.
    select_all(&mut harness);
    chord(&mut harness, Modifiers::CTRL, Key::B);

    assert_eq!(body_of(&harness, folder), "**loud**");
}

#[test]
fn ctrl_b_again_takes_the_bold_off() {
    // The selection is kept after wrapping precisely so the chord toggles.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("loud");
    settle(&mut harness);
    select_all(&mut harness);
    chord(&mut harness, Modifiers::CTRL, Key::B);
    chord(&mut harness, Modifiers::CTRL, Key::B);

    assert_eq!(body_of(&harness, folder), "loud");
}

#[test]
fn ctrl_i_and_ctrl_u_work_the_same_way() {
    for (key, expected) in [(Key::I, "*x*"), (Key::U, "__x__")] {
        let mut harness = harness();
        let folder = notebook_body(&mut harness, "");
        harness.get_by_role(Role::MultilineTextInput).type_text("x");
        settle(&mut harness);
        select_all(&mut harness);
        chord(&mut harness, Modifiers::CTRL, key);
        assert_eq!(body_of(&harness, folder), expected, "{key:?}");
    }
}

#[test]
fn the_bold_button_does_what_the_chord_does() {
    // Both route through one function, so this is really asserting that the
    // toolbar is wired to it rather than to a second implementation.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("loud");
    settle(&mut harness);
    select_all(&mut harness);

    toolbar_button(&harness, "Bold").click();
    settle(&mut harness);
    assert_eq!(body_of(&harness, folder), "**loud**");
}

#[test]
fn the_list_buttons_apply_and_remove_their_markers() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("item");
    settle(&mut harness);

    // The list icons are painted, so they are reached by their tooltips.
    toolbar_button(&harness, "Bulleted list").click();
    settle(&mut harness);
    assert_eq!(body_of(&harness, folder), "- item");

    toolbar_button(&harness, "Bulleted list").click();
    settle(&mut harness);
    assert_eq!(
        body_of(&harness, folder),
        "item",
        "the button should toggle"
    );

    toolbar_button(&harness, "Checklist").click();
    settle(&mut harness);
    assert_eq!(body_of(&harness, folder), "- [ ] item");
}

#[test]
fn the_divider_and_code_block_buttons_work() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("above");
    settle(&mut harness);

    toolbar_button(&harness, "Divider").click();
    settle(&mut harness);
    assert_eq!(body_of(&harness, folder), "above\n---\n");

    toolbar_button(&harness, "Code block").click();
    settle(&mut harness);
    assert_eq!(body_of(&harness, folder), "above\n---\n```\n\n```");
}

#[test]
fn the_link_button_leaves_the_caret_where_the_address_goes() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("the docs");
    settle(&mut harness);
    select_all(&mut harness);

    toolbar_button(&harness, "Link").click();
    settle(&mut harness);
    assert_eq!(body_of(&harness, folder), "[the docs]()");

    // The caret is inside the parentheses, so typing an address lands there.
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("https://example.com");
    settle(&mut harness);
    assert_eq!(body_of(&harness, folder), "[the docs](https://example.com)");
}

#[test]
fn a_toolbar_click_hands_focus_back_to_the_field() {
    // Otherwise every button press would need a click back into the text before
    // you could carry on typing.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    harness.get_by_role(Role::MultilineTextInput).type_text("x");
    settle(&mut harness);

    toolbar_button(&harness, "Bulleted list").click();
    settle(&mut harness);
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("yz");
    settle(&mut harness);

    assert_eq!(
        body_of(&harness, folder),
        "- xyz",
        "typing after a button press should continue in the field"
    );
}

#[test]
fn the_new_task_dialog_has_no_formatting_bar() {
    // It is a quick-entry form and already tall. The shortcuts still work
    // there, so the syntax is not out of reach.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    harness.get_by_label("+ Task").click();
    settle(&mut harness);

    assert!(
        harness.query_by_label("Bulleted list").is_none(),
        "the dialog should not carry a toolbar"
    );
}

#[test]
fn ctrl_n_no_longer_fires_while_writing_a_note() {
    // It was checked before the typing guard, so a Ctrl+N halfway through a
    // note created a task.
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    let acme = match harness.state().view() {
        View::Folder(id) => *id,
        other => panic!("expected a folder, got {other:?}"),
    };
    let before = harness.state().tree().children(Some(acme)).unwrap().len();

    chord(&mut harness, Modifiers::CTRL, Key::ArrowLeft);
    harness.get_by_role(Role::MultilineTextInput).focus();
    harness.run();
    chord(&mut harness, Modifiers::CTRL, Key::N);

    assert_eq!(
        harness.state().tree().children(Some(acme)).unwrap().len(),
        before,
        "Ctrl+N should not have created a task while the caret was in a note"
    );
}

#[test]
fn ctrl_n_still_works_when_nothing_is_focused() {
    let mut harness = harness();
    toggle_sidebar(&mut harness);
    open_acme(&mut harness);
    let acme = match harness.state().view() {
        View::Folder(id) => *id,
        other => panic!("expected a folder, got {other:?}"),
    };
    let before = harness.state().tree().children(Some(acme)).unwrap().len();

    chord(&mut harness, Modifiers::CTRL, Key::N);
    assert_eq!(
        harness.state().tree().children(Some(acme)).unwrap().len(),
        before + 1
    );
}

// ------------------------------------------------- clicks and paste (D7)

/// Any URL egui was asked to open on the most recent frame.
///
/// Read per frame rather than after settling: the platform output is replaced
/// every frame, so a command issued on the click frame is long gone by the time
/// a `run` loop has finished.
fn opened(harness: &Harness<'_, App>) -> Option<String> {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            eframe::egui::OutputCommand::OpenUrl(open) => Some(open.url.clone()),
            _ => None,
        })
}

/// Clicks with a modifier genuinely held down, returning any URL that opened.
///
/// The modifier goes on as its own event rather than only on the button, because
/// that is how egui tracks it: `InputState::modifiers` is moved by
/// `ModifiersChanged` and by nothing else, and the hover branch reads the live
/// state, exactly as it does under a real hand.
fn click_at_with(
    harness: &mut Harness<'_, App>,
    pos: Pos2,
    modifiers: Modifiers,
) -> Option<String> {
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(modifiers));
    harness.input_mut().events.push(Event::PointerMoved(pos));
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
    let mut found = None;
    for _ in 0..SETTLE {
        harness.step();
        found = found.or_else(|| opened(harness));
    }
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(Modifiers::default()));
    harness.run();
    found
}

/// The middle of the gutter on a field's first line, which is where a checkbox
/// is drawn.
///
/// Built from the theme's own metrics rather than measured, so a change to the
/// gutter moves the click with it. The notebook body draws no frame, so its
/// text starts at the widget's own corner.
fn first_gutter(harness: &Harness<'_, App>) -> Pos2 {
    let rect = harness.get_by_role(Role::MultilineTextInput).rect();
    pos2(
        rect.left() + metric::GUTTER / 2.0,
        rect.top() + metric::GUTTER / 2.0,
    )
}

/// Where a needle sits on screen inside a field, by counting characters along
/// the first line.
fn in_first_line(harness: &Harness<'_, App>, chars_in: f32) -> Pos2 {
    let rect = harness.get_by_role(Role::MultilineTextInput).rect();
    pos2(
        rect.left() + chars_in,
        rect.top() + metric::GUTTER / 2.0,
    )
}

#[test]
fn clicking_a_checkbox_ticks_it() {
    // The box is painted, not laid out, so the field knows nothing about it.
    // This is the test that the drawn thing and the hit test agree.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- [ ] pay rent");
    let pos = first_gutter(&harness);
    click_at(&mut harness, pos);

    assert_eq!(body_of(&harness, folder), "- [x] pay rent");
}

#[test]
fn clicking_a_ticked_checkbox_unticks_it() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- [x] pay rent");
    let pos = first_gutter(&harness);
    click_at(&mut harness, pos);

    assert_eq!(body_of(&harness, folder), "- [ ] pay rent");
}

#[test]
fn clicking_the_text_beside_a_checkbox_leaves_it_alone() {
    // Clicking into the words is how you edit them. Only the box toggles.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- [ ] pay rent");
    let pos = in_first_line(&harness, metric::GUTTER + 40.0);
    click_at(&mut harness, pos);

    assert_eq!(body_of(&harness, folder), "- [ ] pay rent");
}

#[test]
fn clicking_a_bullet_toggles_nothing() {
    // The gutter holds a dot on a bulleted line, and a dot is not a control.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- pay rent");
    let pos = first_gutter(&harness);
    click_at(&mut harness, pos);

    assert_eq!(body_of(&harness, folder), "- pay rent");
}

#[test]
fn ctrl_enter_ticks_the_box_on_the_caret_line() {
    // The keyboard equivalent. A box that only a mouse can tick is a box some
    // people cannot tick at all.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- [ ] one\n- [ ] two");
    press(&mut harness, Key::ArrowDown);
    chord(&mut harness, Modifiers::CTRL, Key::Enter);

    assert_eq!(body_of(&harness, folder), "- [ ] one\n- [x] two");
}

#[test]
fn ctrl_clicking_a_link_opens_it_in_the_browser() {
    let mut harness = harness();
    notebook_body(&mut harness, "https://example.com/docs");
    // Well inside the address, which starts at the very left of the line.
    let pos = in_first_line(&harness, 40.0);

    assert_eq!(
        click_at_with(&mut harness, pos, Modifiers::CTRL),
        Some("https://example.com/docs".to_owned())
    );
}

#[test]
fn ctrl_clicking_an_explicit_link_opens_its_target_not_its_label() {
    let mut harness = harness();
    notebook_body(&mut harness, "[the docs](https://example.com/x)");
    let pos = in_first_line(&harness, 20.0);

    assert_eq!(
        click_at_with(&mut harness, pos, Modifiers::CTRL),
        Some("https://example.com/x".to_owned())
    );
}

#[test]
fn a_plain_click_on_a_link_only_moves_the_caret() {
    // The field is editable, so a bare click has to keep meaning what it means
    // everywhere else. That is also why the hand cursor is modifier gated.
    let mut harness = harness();
    notebook_body(&mut harness, "https://example.com/docs");
    let pos = in_first_line(&harness, 40.0);

    assert_eq!(click_at_with(&mut harness, pos, Modifiers::NONE), None);
}

#[test]
fn ctrl_clicking_ordinary_text_opens_nothing() {
    let mut harness = harness();
    notebook_body(&mut harness, "just some words with no address in them");
    let pos = in_first_line(&harness, 40.0);

    assert_eq!(click_at_with(&mut harness, pos, Modifiers::CTRL), None);
}

#[test]
fn ctrl_clicking_past_the_end_of_a_link_opens_nothing() {
    // The reason the hit test walks glyphs instead of asking the galley for the
    // nearest caret: nearest would answer with the last character of the line
    // however far into the margin the pointer was.
    let mut harness = harness();
    notebook_body(&mut harness, "https://example.com");
    let rect = harness.get_by_role(Role::MultilineTextInput).rect();
    let pos = pos2(rect.right() - 4.0, rect.top() + metric::GUTTER / 2.0);

    assert_eq!(click_at_with(&mut harness, pos, Modifiers::CTRL), None);
}

/// Pastes text as the clipboard would deliver it.
fn paste(harness: &mut Harness<'_, App>, what: &str) {
    harness
        .input_mut()
        .events
        .push(Event::Paste(what.to_owned()));
    settle(harness);
}

#[test]
fn pasting_a_url_over_a_selection_makes_a_link_of_it() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("the docs");
    settle(&mut harness);
    select_all(&mut harness);
    paste(&mut harness, "https://example.com/d");

    assert_eq!(body_of(&harness, folder), "[the docs](https://example.com/d)");
}

#[test]
fn pasting_a_url_with_nothing_selected_pastes_it_plainly() {
    // Nothing to hang it on, and the bare address autolinks anyway, so the
    // ordinary paste is the right answer.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    paste(&mut harness, "https://example.com/d");

    assert_eq!(body_of(&harness, folder), "https://example.com/d");
}

#[test]
fn pasting_ordinary_text_over_a_selection_still_replaces_it() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text("the docs");
    settle(&mut harness);
    select_all(&mut harness);
    paste(&mut harness, "the manual");

    assert_eq!(body_of(&harness, folder), "the manual");
}

#[test]
fn clicking_a_checkbox_does_not_move_the_caret_into_the_line() {
    // The gutter is not text. Left alone, the caret the field places on a click
    // would reveal the line's own source, so ticking a box would swap the box
    // for `- [x] ` and a caret: the wrong feedback for the one action whose
    // whole point is the tick appearing.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- [ ] one\n- [ ] two");
    // Caret parked on the first line, by typing rather than clicking.
    press(&mut harness, Key::Home);
    let caret = caret_of(&harness);

    // Second row: one row down from the first.
    let rect = harness.get_by_role(Role::MultilineTextInput).rect();
    let row = row_height(&harness);
    click_at(
        &mut harness,
        pos2(
            rect.left() + metric::GUTTER / 2.0,
            rect.top() + row * 1.5,
        ),
    );

    assert_eq!(body_of(&harness, folder), "- [ ] one\n- [x] two");
    assert_eq!(caret_of(&harness), caret, "the caret followed the click");
}

/// The field's caret, in characters.
fn caret_of(harness: &Harness<'_, App>) -> Option<(usize, usize)> {
    let id = eframe::egui::Id::new("trackcrab_comment_body");
    let state = eframe::egui::widgets::text_edit::TextEditState::load(&harness.ctx, id)?;
    let range = state.cursor.char_range()?;
    Some((range.primary.index.0, range.secondary.index.0))
}

/// One row of body text, measured off the live style rather than assumed.
fn row_height(harness: &Harness<'_, App>) -> f32 {
    let size = harness
        .ctx
        .global_style()
        .text_styles
        .get(&eframe::egui::TextStyle::Body)
        .map_or(16.5, |font| font.size);
    harness
        .ctx
        .fonts_mut(|fonts| fonts.row_height(&trackcrab::ui::theme::body_font(size)))
}

#[test]
fn ticking_a_box_in_a_note_you_are_only_reading_does_not_start_editing_it() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "- [ ] pay rent");
    // Out of the field. Escape is the way out now that Tab indents.
    press(&mut harness, Key::Escape);
    settle(&mut harness);
    assert!(harness.ctx.memory(eframe::egui::Memory::focused).is_none());

    let pos = first_gutter(&harness);
    click_at(&mut harness, pos);

    assert_eq!(body_of(&harness, folder), "- [x] pay rent");
    assert!(
        harness.ctx.memory(eframe::egui::Memory::focused).is_none(),
        "clicking a box put the field into editing"
    );
}

// ------------------------------------------------- searching markdown (D8)

/// Puts a note on "Land the VPC design" and searches for `needle` through the
/// real search box, returning whether the task's row survived the filter.
fn search_finds_the_note(note: &str, needle: &str) -> bool {
    let mut harness = harness();
    open_task(&mut harness, "Land the VPC design");
    let id = open_task_id(&harness);
    harness
        .state_mut()
        .tree_mut()
        .edit_task(id, |task| note.clone_into(&mut task.notes))
        .unwrap();

    toggle_sidebar(&mut harness);
    search_box(&harness).focus();
    harness.run();
    search_box(&harness).type_text(needle);
    settle(&mut harness);

    harness
        .query_all_by_label_contains("Land the VPC design")
        .next()
        .is_some()
}

#[test]
fn the_search_box_finds_a_phrase_split_by_markup() {
    // Through the real box, because the sidebar is what calls the filter and
    // this is the behaviour the whole of D8 is for: the note reads "the strong
    // white flour" and the stored bytes have asterisks in the middle of it.
    assert!(search_finds_the_note(
        "Use the **strong white** flour",
        "strong white flour"
    ));
}

#[test]
fn the_search_box_does_not_match_markup_you_cannot_see() {
    assert!(!search_finds_the_note("Use the **strong white** flour", "**"));
}

#[test]
fn the_search_box_still_finds_ordinary_words() {
    // The regression that would matter most: stripping must not break the
    // search that already worked.
    assert!(search_finds_the_note(
        "quota increase raised with support",
        "quota increase"
    ));
}

// ------------------------------------------------- the notebook scrolls

/// A note taller than any panel, as plain lines.
fn long_note() -> String {
    use std::fmt::Write as _;
    (1..=120).fold(String::new(), |mut out, i| {
        let _ = writeln!(out, "line {i}");
        out
    })
}

/// Scrolls the wheel over a point, as a hand would.
fn wheel_at(harness: &mut Harness<'_, App>, pos: Pos2, by: f32) {
    harness.input_mut().events.push(Event::PointerMoved(pos));
    harness.input_mut().events.push(Event::MouseWheel {
        unit: eframe::egui::MouseWheelUnit::Point,
        delta: eframe::egui::vec2(0.0, by),
        modifiers: Modifiers::NONE,
        phase: eframe::egui::TouchPhase::Move,
    });
    settle(harness);
}

/// The notebook body's rect. Its top is the observable: scrolling moves the
/// whole laid-out field up, so a top that has gone negative is content that has
/// been scrolled past.
fn body_rect(harness: &Harness<'_, App>) -> eframe::egui::Rect {
    harness.get_by_role(Role::MultilineTextInput).rect()
}

#[test]
fn a_note_longer_than_the_panel_scrolls() {
    let mut harness = harness();
    notebook_body(&mut harness, &long_note());
    let before = body_rect(&harness);

    wheel_at(&mut harness, pos2(before.center().x, before.top() + 60.0), -400.0);

    let after = body_rect(&harness);
    assert!(
        after.top() < before.top() - 300.0,
        "the body did not scroll: {} then {}",
        before.top(),
        after.top()
    );
}

#[test]
fn a_note_that_fits_does_not_scroll() {
    let mut harness = harness();
    notebook_body(&mut harness, "one line");
    let before = body_rect(&harness);

    wheel_at(&mut harness, pos2(before.center().x, before.top() + 60.0), -400.0);

    let after = body_rect(&harness).top();
    assert!(
        (after - before.top()).abs() < 0.5,
        "a note that fits should stay put, moved from {} to {after}",
        before.top()
    );
}

#[test]
fn a_short_note_still_fills_the_whole_panel() {
    // The field is the click target: a note of one line has to stay clickable
    // across the whole panel rather than shrinking to one row with dead space
    // under it. That is what the row count computed from the box is for, and it
    // is the thing a naive scroll area would have taken away.
    let mut harness = harness();
    notebook_body(&mut harness, "one line");
    let one_line = body_rect(&harness).height();

    assert!(
        one_line > 400.0,
        "a one line note only filled {one_line}px of the panel"
    );
}

#[test]
fn the_toolbar_stays_put_while_the_body_scrolls() {
    // The reason the field is laid out and painted inside the scroll area while
    // the bar is left outside it. Scrolling the text must not scroll the
    // formatting bar off the top of the panel.
    let mut harness = harness();
    notebook_body(&mut harness, &long_note());
    let bar = toolbar_button(&harness, "Bold").rect();
    let before = body_rect(&harness);

    wheel_at(&mut harness, pos2(before.center().x, before.top() + 60.0), -400.0);

    assert_eq!(toolbar_button(&harness, "Bold").rect(), bar);
    assert!(body_rect(&harness).top() < before.top());
}

// ------------------------------------------- the highlight menu and refocus

/// Opens the highlight menu.
fn open_highlights(harness: &mut Harness<'_, App>) {
    notebook(harness, "\u{2022}")
        .get_by_label("\u{2022}")
        .click();
    settle(harness);
}

/// The hex entry box inside the highlight menu.
///
/// Two single line inputs are on screen with the notebook open: its title, and
/// this one. Told apart by sitting beside Apply rather than by role alone,
/// since a hint is not a name.
fn hex_field<'t>(harness: &'t Harness<'_, App>) -> Node<'t> {
    let apply = harness.get_by_label("Apply").rect().center();
    let mut inputs: Vec<Node<'t>> = harness.query_all_by_role(Role::TextInput).collect();
    inputs.sort_by(|a, b| {
        (a.rect().center() - apply)
            .length()
            .total_cmp(&(b.rect().center() - apply).length())
    });
    inputs.into_iter().next().expect("a hex field on screen")
}

#[test]
fn the_highlight_menu_stays_open_while_the_hex_field_is_used() {
    // egui closes a menu on *any* click by default, inside or out, which shut
    // this one the moment the hex box was clicked: the field could never be
    // typed into and Apply could never be reached.
    let mut harness = harness();
    notebook_body(&mut harness, "");
    open_highlights(&mut harness);
    assert_eq!(harness.query_all_by_label("Apply").count(), 1);

    hex_field(&harness).click();
    settle(&mut harness);

    assert_eq!(
        harness.query_all_by_label("Apply").count(),
        1,
        "clicking the hex field closed the menu"
    );
}

/// Types into the hex box, focusing it first.
///
/// `type_text` only pushes text at whatever already has focus, and clicking the
/// menu open left that on the button. A hand clicks the box, which focuses it;
/// this is that click.
fn type_hex(harness: &mut Harness<'_, App>, hex: &str) {
    hex_field(harness).focus();
    harness.run();
    hex_field(harness).type_text(hex);
    settle(harness);
}

#[test]
fn a_hex_colour_can_be_typed_and_applied() {
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    open_highlights(&mut harness);
    type_hex(&mut harness, "ff8800");
    harness.get_by_label("Apply").click();
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "==#ff8800|==");
}

#[test]
fn nonsense_in_the_hex_field_applies_nothing() {
    // Apply is disabled until the value parses, so clicking it does nothing.
    // Asserted through the buffer rather than the button's state, which the
    // accessibility tree does not expose.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    open_highlights(&mut harness);
    type_hex(&mut harness, "nonsense");
    harness.get_by_label("Apply").click();
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "");
}

#[test]
fn picking_a_colour_leaves_the_caret_between_the_delimiters() {
    // The bug this pins is what the whole refocus dance is for. Picking a
    // colour puts an empty highlight in and the caret inside it; if focus does
    // not come back, the next thing typed goes in wherever the click left the
    // caret and the markup comes out as `==yellow|==some` rather than
    // `==yellow|some==`, which is not a highlight at all.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    open_highlights(&mut harness);
    harness.get_by_label("yellow").click();
    settle(&mut harness);
    assert_eq!(body_of(&harness, folder), "==yellow|==");

    // Typed straight in, focusing nothing: exactly what a hand does next.
    harness
        .input_mut()
        .events
        .push(Event::Text("some".to_owned()));
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "==yellow|some==");
}

#[test]
fn a_toolbar_action_asks_for_the_frames_its_refocus_needs() {
    // The half of the refocus that a settled test cannot see. egui only draws
    // when something asks it to, so a retry that does not request a repaint
    // never runs at all: in the harness thirty settling frames hide that, and
    // in the app the field simply never gets focus back.
    let mut harness = harness();
    notebook_body(&mut harness, "");
    let budget = eframe::egui::Id::new("trackcrab_comment_body").with("refocus");

    toolbar_button(&harness, "Bold").click();
    harness.step();

    assert!(
        harness.ctx.has_requested_repaint(),
        "the action did not ask for another frame"
    );
    assert!(
        harness.ctx.data(|data| data.get_temp::<u8>(budget)).is_some(),
        "no refocus was scheduled"
    );

    settle(&mut harness);
    assert_eq!(
        harness.ctx.memory(eframe::egui::Memory::focused),
        Some(eframe::egui::Id::new("trackcrab_comment_body")),
        "focus never came back to the field"
    );
    assert!(
        harness.ctx.data(|data| data.get_temp::<u8>(budget)).is_none(),
        "the retry kept going after focus was held"
    );
}


#[test]
fn enter_in_the_hex_field_applies_it() {
    // A value you have typed and then have to go and click a button beside is a
    // field that feels broken.
    let mut harness = harness();
    let folder = notebook_body(&mut harness, "");
    open_highlights(&mut harness);
    type_hex(&mut harness, "3355ff");
    press(&mut harness, Key::Enter);

    assert_eq!(body_of(&harness, folder), "==#3355ff|==");
}

#[test]
fn a_named_colour_still_closes_the_menu() {
    // The menu no longer closes on any click, so the swatches have to close it
    // themselves. If they stopped, it would sit open over the text.
    let mut harness = harness();
    notebook_body(&mut harness, "");
    open_highlights(&mut harness);
    harness.get_by_label("blue").click();
    settle(&mut harness);

    assert_eq!(
        harness.query_all_by_label("Apply").count(),
        0,
        "the menu stayed open after a colour was picked"
    );
}

// ----------------------------------- toolbar actions and the selection

/// Types a phrase into the notebook and selects all of it.
fn selected(harness: &mut Harness<'_, App>, what: &str) -> trackcrab::model::NodeId {
    let folder = notebook_body(harness, "");
    harness
        .get_by_role(Role::MultilineTextInput)
        .type_text(what);
    settle(harness);
    select_all(harness);
    folder
}

#[test]
fn the_highlight_menu_wraps_the_selection() {
    // A toolbar action cannot read the widget's own cursor. Opening a *menu*
    // takes focus off the field and keeps it off, and egui collapses the stored
    // selection to a bare caret while it is away: the range went from (9, 0) to
    // (9, 9) before the colour was even picked, so the empty markup landed
    // beside the words instead of around them.
    let mut harness = harness();
    let folder = selected(&mut harness, "some text");
    open_highlights(&mut harness);
    harness.get_by_label("yellow").click();
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "==yellow|some text==");
}

#[test]
fn a_hex_colour_wraps_the_selection() {
    let mut harness = harness();
    let folder = selected(&mut harness, "some text");
    open_highlights(&mut harness);
    type_hex(&mut harness, "ff8800");
    harness.get_by_label("Apply").click();
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "==#ff8800|some text==");
}

#[test]
fn the_bold_button_wraps_the_selection() {
    // This one always worked, because a plain button hands the action back on
    // the same frame it was clicked. Pinned anyway: it is the same code path,
    // and it is the one that would silently break if the remembered caret ever
    // stopped keeping up.
    let mut harness = harness();
    let folder = selected(&mut harness, "some text");
    toolbar_button(&harness, "Bold").click();
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "**some text**");
}

#[test]
fn a_button_pressed_twice_takes_its_own_formatting_off() {
    // Only possible if the caret remembered after the first press is the
    // wrapped selection, not where things were before it.
    let mut harness = harness();
    let folder = selected(&mut harness, "some text");
    toolbar_button(&harness, "Bold").click();
    settle(&mut harness);
    toolbar_button(&harness, "Bold").click();
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "some text");
}

#[test]
fn two_different_buttons_compose_on_one_selection() {
    let mut harness = harness();
    let folder = selected(&mut harness, "some text");
    toolbar_button(&harness, "Bold").click();
    settle(&mut harness);
    toolbar_button(&harness, "Italic").click();
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "***some text***");
}

#[test]
fn a_bare_caret_is_not_treated_as_an_old_selection() {
    // The guard against the fix over-reaching. Remembering the last selection
    // must not mean resurrecting it: after the caret has been moved to a bare
    // position, a toolbar action belongs there and nowhere else.
    let mut harness = harness();
    let folder = selected(&mut harness, "some text");
    // Collapse the selection to the end, still inside the field.
    press(&mut harness, Key::End);
    open_highlights(&mut harness);
    harness.get_by_label("yellow").click();
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "some text==yellow|==");
}

#[test]
fn a_selection_survives_a_menu_being_opened_and_dismissed() {
    // Opened, then closed with Escape rather than by picking anything. The
    // selection has to still be there afterwards, or the next chord acts on the
    // wrong text.
    let mut harness = harness();
    let folder = selected(&mut harness, "some text");
    open_highlights(&mut harness);
    press(&mut harness, Key::Escape);
    settle(&mut harness);
    toolbar_button(&harness, "Bold").click();
    settle(&mut harness);

    assert_eq!(body_of(&harness, folder), "**some text**");
}
