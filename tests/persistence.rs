use std::fs;
use std::path::PathBuf;

use trackcrab::model::{Status, Tree};
use trackcrab::store::{DataStore, LoadOutcome, SCHEMA_VERSION};

/// Scratch directory that cleans itself up, so the tests never touch the real
/// data file.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "trackcrab-test-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self) -> PathBuf {
        self.0.join("data.json")
    }

    fn store(&self) -> DataStore {
        DataStore::at(self.file())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn populated() -> Tree {
    let mut tree = Tree::new();
    let work = tree.create_folder(None, "Work").unwrap();
    let clients = tree.create_folder(Some(work), "Clients").unwrap();
    let deep = tree.create_folder(Some(clients), "Acme").unwrap();
    tree.create_task(deep, "Open one", None, Status::Open)
        .unwrap();
    tree.create_task(
        deep,
        "Blocked one",
        Some("with a description".into()),
        Status::Blocked("waiting on quota".into()),
    )
    .unwrap();
    let timed = tree
        .create_task(clients, "Timed one", None, Status::InProgress)
        .unwrap();
    tree.edit_task(timed, |t| t.set_attributed_hm(15, 0))
        .unwrap();
    tree.create_folder(None, "Personal").unwrap();
    tree
}

#[test]
fn a_full_tree_survives_a_save_and_load_round_trip() {
    let scratch = Scratch::new("roundtrip");
    let store = scratch.store();
    let original = populated();
    store.save(&original).unwrap();

    let loaded = match store.load() {
        LoadOutcome::Loaded { tree, existed } => {
            assert!(existed, "the file we just wrote should be seen as existing");
            tree
        }
        other @ LoadOutcome::Recovered { .. } => panic!("expected a clean load, got {other:?}"),
    };

    assert_eq!(loaded.len(), original.len());
    assert_eq!(loaded.roots().len(), original.roots().len());
    assert!(loaded.validate().is_ok());

    // Compare node for node, including timestamps and the blocked reason.
    for id in original.roots().iter().flat_map(|r| {
        let mut all = vec![*r];
        all.extend(original.descendants(*r));
        all
    }) {
        assert_eq!(
            loaded.get(id),
            original.get(id),
            "node {id} did not survive the round trip"
        );
    }

    // Spot check the values we care about most.
    let timed = loaded
        .roots()
        .iter()
        .flat_map(|r| loaded.descendants(*r))
        .filter_map(|id| loaded.get(id).and_then(|n| n.as_task()))
        .find(|t| t.title == "Timed one")
        .expect("the timed task should be there");
    assert_eq!(timed.attributed_label(), "15h");

    let blocked = loaded
        .roots()
        .iter()
        .flat_map(|r| loaded.descendants(*r))
        .filter_map(|id| loaded.get(id).and_then(|n| n.as_task()))
        .find(|t| t.title == "Blocked one")
        .expect("the blocked task should be there");
    assert_eq!(blocked.status.blocked_reason(), Some("waiting on quota"));
    assert_eq!(blocked.description.as_deref(), Some("with a description"));
}

#[test]
fn a_missing_file_starts_empty_without_complaining() {
    let scratch = Scratch::new("missing");
    match scratch.store().load() {
        LoadOutcome::Loaded { tree, existed } => {
            assert!(!existed);
            assert!(tree.is_empty());
        }
        other @ LoadOutcome::Recovered { .. } => {
            panic!("a missing file is not an error, got {other:?}")
        }
    }
}

#[test]
fn saving_creates_missing_parent_directories() {
    let scratch = Scratch::new("mkdir");
    let nested = scratch.0.join("a").join("b").join("data.json");
    let store = DataStore::at(&nested);
    store.save(&populated()).unwrap();
    assert!(nested.exists());
}

#[test]
fn a_save_leaves_no_temp_file_behind() {
    let scratch = Scratch::new("notmp");
    scratch.store().save(&populated()).unwrap();
    let leftovers: Vec<_> = fs::read_dir(&scratch.0)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"))
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );
}

#[test]
fn re_saving_over_an_existing_file_replaces_it_cleanly() {
    let scratch = Scratch::new("resave");
    let store = scratch.store();
    store.save(&populated()).unwrap();

    let mut second = Tree::new();
    second.create_folder(None, "Only one").unwrap();
    store.save(&second).unwrap();

    let loaded = store.load().into_tree();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded.roots().len(), 1);
}

#[test]
fn malformed_json_is_quarantined_never_deleted() {
    let scratch = Scratch::new("malformed");
    let store = scratch.store();
    fs::write(scratch.file(), "{ this is not json at all").unwrap();

    match store.load() {
        LoadOutcome::Recovered {
            tree,
            quarantined,
            reason,
        } => {
            assert!(tree.is_empty());
            assert!(reason.contains("malformed"), "unhelpful reason: {reason}");
            let saved = quarantined.expect("the bad file should have been moved aside");
            assert!(saved.exists(), "the user's original file must survive");
            assert!(
                !scratch.file().exists(),
                "the bad file should no longer be at the live path"
            );
            assert_eq!(
                fs::read_to_string(saved).unwrap(),
                "{ this is not json at all",
                "quarantined content must be byte identical"
            );
        }
        other @ LoadOutcome::Loaded { .. } => panic!("expected recovery, got {other:?}"),
    }
}

#[test]
fn a_structurally_broken_file_is_rejected_rather_than_half_loaded() {
    let scratch = Scratch::new("broken");
    let store = scratch.store();
    // A root id that has no matching node.
    let json = format!(
        r#"{{ "schema_version": {SCHEMA_VERSION},
              "roots": ["6f0b3f4e-0000-4000-8000-000000000001"],
              "nodes": [] }}"#
    );
    fs::write(scratch.file(), json).unwrap();

    match store.load() {
        LoadOutcome::Recovered { tree, reason, .. } => {
            assert!(tree.is_empty());
            assert!(
                reason.contains("not present"),
                "reason should name the problem: {reason}"
            );
        }
        other @ LoadOutcome::Loaded { .. } => panic!("expected recovery, got {other:?}"),
    }
}

#[test]
fn a_task_placed_at_the_root_on_disk_is_rejected() {
    let scratch = Scratch::new("taskroot");
    let store = scratch.store();
    let id = "6f0b3f4e-0000-4000-8000-0000000000aa";
    let json = format!(
        r#"{{ "schema_version": {SCHEMA_VERSION},
              "roots": ["{id}"],
              "nodes": [
                {{ "id": "{id}", "parent": null, "kind": {{ "Task": {{
                    "title": "orphan",
                    "description": null,
                    "status": "Open",
                    "created_at": "2026-09-01T12:00:00Z",
                    "updated_at": "2026-09-01T12:00:00Z",
                    "attributed_minutes": 0 }} }} }}
              ] }}"#
    );
    fs::write(scratch.file(), json).unwrap();

    match store.load() {
        LoadOutcome::Recovered { reason, .. } => {
            assert!(
                reason.contains("only folders"),
                "reason should explain the rule: {reason}"
            );
        }
        other @ LoadOutcome::Loaded { .. } => {
            panic!("a task at the root is invalid, got {other:?}")
        }
    }
}

#[test]
fn a_newer_schema_version_is_refused_rather_than_misread() {
    let scratch = Scratch::new("newer");
    let store = scratch.store();
    let json = format!(
        r#"{{ "schema_version": {}, "roots": [], "nodes": [] }}"#,
        SCHEMA_VERSION + 7
    );
    fs::write(scratch.file(), json).unwrap();

    match store.load() {
        LoadOutcome::Recovered { reason, .. } => {
            assert!(reason.contains("schema version"), "reason: {reason}");
        }
        other @ LoadOutcome::Loaded { .. } => panic!("expected refusal, got {other:?}"),
    }
}

#[test]
fn the_written_file_is_readable_json_with_a_schema_version() {
    let scratch = Scratch::new("shape");
    scratch.store().save(&populated()).unwrap();
    let raw = fs::read_to_string(scratch.file()).unwrap();
    assert!(raw.contains("\"schema_version\""));
    assert!(raw.contains('\n'), "the file should be pretty printed");
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["schema_version"], SCHEMA_VERSION);
    assert!(parsed["nodes"].as_array().unwrap().len() > 1);
}

#[test]
fn saves_are_byte_stable_so_the_file_does_not_churn() {
    let scratch = Scratch::new("stable");
    let store = scratch.store();
    let tree = populated();
    store.save(&tree).unwrap();
    let first = fs::read_to_string(scratch.file()).unwrap();
    store.save(&tree).unwrap();
    let second = fs::read_to_string(scratch.file()).unwrap();
    assert_eq!(
        first, second,
        "saving the same tree twice should be identical"
    );
}

#[test]
fn the_env_override_wins_over_the_platform_data_dir() {
    // Serialised via a lock because env vars are process wide.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap();
    let scratch = Scratch::new("env");
    let target = scratch.0.join("custom.json");
    unsafe { std::env::set_var("TRACKCRAB_DATA", &target) };
    let store = DataStore::discover().unwrap();
    unsafe { std::env::remove_var("TRACKCRAB_DATA") };
    assert_eq!(store.path(), target.as_path());
}

// ------------------------------------------------------- notes and comments

#[test]
fn notes_and_comment_spaces_survive_a_round_trip() {
    let scratch = Scratch::new("addenda");
    let store = scratch.store();

    let mut tree = Tree::new();
    let work = tree.create_folder(None, "Work").unwrap();
    let task = tree
        .create_task(work, "Migrate the estate", None, Status::Open)
        .unwrap();
    tree.edit_task(task, |t| {
        t.notes = "Ring the DBA before the cutover.".to_owned();
    })
    .unwrap();

    let first = tree.add_comment_space(work).unwrap();
    tree.edit_comment_space(work, first, |space| {
        space.title = "Kickoff".to_owned();
        space.body = "Budget signed off, three week window.".to_owned();
    })
    .unwrap();
    let second = tree.add_comment_space(work).unwrap();
    tree.edit_comment_space(work, second, |space| {
        space.body = "Blocked on the third party.".to_owned();
    })
    .unwrap();

    store.save(&tree).unwrap();
    let loaded = store.load().into_tree();

    assert_eq!(
        loaded.task(task).unwrap().notes,
        "Ring the DBA before the cutover."
    );
    let spaces = loaded.comment_spaces(work);
    assert_eq!(spaces.len(), 2);
    assert_eq!(spaces[0].title, "Kickoff");
    assert_eq!(spaces[0].body, "Budget signed off, three week window.");
    assert_eq!(spaces[1].title, "Comments 2");
    assert_eq!(spaces[1].body, "Blocked on the third party.");
    // Timestamps come back too.
    assert_eq!(
        spaces[0].created_at,
        tree.comment_spaces(work)[0].created_at
    );
}

#[test]
fn a_schema_version_1_file_still_loads_without_notes_or_comments() {
    // Written before either feature existed. It must load cleanly rather than
    // be quarantined, since plenty of these exist on disk already.
    let scratch = Scratch::new("v1");
    let store = scratch.store();
    let folder = "6f0b3f4e-0000-4000-8000-0000000000f1";
    let task = "6f0b3f4e-0000-4000-8000-0000000000t1".replace('t', "a");
    let json = format!(
        r#"{{ "schema_version": 1,
              "roots": ["{folder}"],
              "nodes": [
                {{ "id": "{folder}", "parent": null, "kind": {{ "Folder": {{
                    "name": "Legacy",
                    "created_at": "2026-01-01T09:00:00Z",
                    "updated_at": "2026-01-01T09:00:00Z",
                    "children": ["{task}"] }} }} }},
                {{ "id": "{task}", "parent": "{folder}", "kind": {{ "Task": {{
                    "title": "An old task",
                    "description": null,
                    "status": "Open",
                    "created_at": "2026-01-01T09:00:00Z",
                    "updated_at": "2026-01-01T09:00:00Z",
                    "attributed_minutes": 30 }} }} }}
              ] }}"#
    );
    fs::write(scratch.file(), json).unwrap();

    let tree = match store.load() {
        LoadOutcome::Loaded { tree, existed } => {
            assert!(existed);
            tree
        }
        other @ LoadOutcome::Recovered { .. } => {
            panic!("a version 1 file must still load, got {other:?}")
        }
    };

    let root = tree.roots()[0];
    assert_eq!(tree.folder(root).unwrap().name, "Legacy");
    assert!(
        tree.comment_spaces(root).is_empty(),
        "an old folder should simply have no comment spaces"
    );
    let child = tree.children(Some(root)).unwrap()[0];
    let task = tree.task(child).unwrap();
    assert_eq!(task.attributed_minutes, 30);
    assert_eq!(task.notes, "", "an old task should simply have no notes");
}

#[test]
fn re_saving_a_loaded_version_1_file_writes_it_out_as_version_2() {
    let scratch = Scratch::new("upgrade");
    let store = scratch.store();
    let mut tree = Tree::new();
    tree.create_folder(None, "Anything").unwrap();
    store.save(&tree).unwrap();

    let raw = fs::read_to_string(scratch.file()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["schema_version"], 2);
}
