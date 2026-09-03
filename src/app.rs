use eframe::egui::{self, Id};

use crate::model::{Node, NodeId, Status, Tree};
use crate::store::{DataStore, LoadOutcome, Settings};
use crate::ui::dialogs::{self, Dialog};
use crate::ui::{self, Action, Panel, sidebar, theme, views};

/// The burger glyph.
///
/// U+2630 TRIGRAM FOR HEAVEN, not the more obvious U+2261 IDENTICAL TO. Only
/// `Hack-Regular` carries U+2261 and egui maps that to the monospace style, so a
/// proportional button renders it as a missing glyph box. U+2630 lives in
/// `emoji-icon-font`, which is in the proportional fallback chain.
pub const BURGER: &str = "\u{2630}";

/// Animation id for the comments notebook's slide.
const COMMENTS_ANIM: &str = "trackcrab_comments_anim";

/// Every keyboard shortcut, in one place.
mod keys {
    use eframe::egui::{Key, KeyboardShortcut as Kb, Modifiers};

    pub const NEW_TASK: Kb = Kb::new(Modifiers::CTRL, Key::N);
    /// Checked before [`NEW_TASK`], which would otherwise swallow it.
    pub const NEW_FOLDER: Kb = Kb::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::N);
    pub const SAVE: Kb = Kb::new(Modifiers::CTRL, Key::S);
    pub const FOCUS_SEARCH: Kb = Kb::new(Modifiers::CTRL, Key::F);
    /// Push the content right: the folder tree takes the left.
    pub const PANEL_RIGHT: Kb = Kb::new(Modifiers::CTRL, Key::ArrowRight);
    /// Push the content left: the notebook takes the right.
    pub const PANEL_LEFT: Kb = Kb::new(Modifiers::CTRL, Key::ArrowLeft);
    pub const ZOOM_IN: Kb = Kb::new(Modifiers::CTRL, Key::Plus);
    /// The unshifted key on most layouts, so both reach zoom in.
    pub const ZOOM_IN_ALT: Kb = Kb::new(Modifiers::CTRL, Key::Equals);
    pub const ZOOM_OUT: Kb = Kb::new(Modifiers::CTRL, Key::Minus);
    pub const ZOOM_RESET: Kb = Kb::new(Modifiers::CTRL, Key::Num0);

    /// Walk the folder tree as a flat list, ignoring depth.
    pub const NAV_UP: Kb = Kb::new(Modifiers::NONE, Key::ArrowUp);
    pub const NAV_DOWN: Kb = Kb::new(Modifiers::NONE, Key::ArrowDown);
    /// Open whatever the tree cursor is sitting on.
    pub const NAV_OPEN: Kb = Kb::new(Modifiers::NONE, Key::Enter);
    /// Cycle comment spaces while the notebook has the screen.
    pub const SPACE_PREV: Kb = Kb::new(Modifiers::NONE, Key::ArrowLeft);
    pub const SPACE_NEXT: Kb = Kb::new(Modifiers::NONE, Key::ArrowRight);
}

/// How long to wait after the last edit before writing to disk.
const SAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(600);

/// What the main panel is currently showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Welcome,
    Folder(NodeId),
    Task(NodeId),
}

/// How the sidebar's width is being decided.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SidebarWidth {
    /// Tracks a fraction of the window width, rescaling as the window changes.
    Auto,
    /// The user dragged the handle, so their width sticks and stops rescaling.
    Manual,
}

pub struct App {
    tree: Tree,
    store: DataStore,
    dirty_since: Option<std::time::Instant>,
    /// Surfaced in the UI when loading had to recover from a bad file.
    startup_notice: Option<String>,
    last_error: Option<String>,

    view: View,
    /// Buffers for the task currently open, rebuilt when the view moves.
    editor: Option<views::task::Editor>,
    /// The modal currently up, if any.
    dialog: Option<Dialog>,
    /// What the sidebar is narrowed to.
    filter: ui::Filter,
    /// Set for one frame by Ctrl+F, so the search box takes the caret.
    focus_search: bool,
    /// Interface scale, remembered between runs in a file beside the data.
    settings: Settings,

    /// Which side panel is showing. One value, so the two can never both be
    /// open.
    panel: Panel,
    /// Which comment space is showing, and for the folder it belongs to.
    comment_cursor: Option<(NodeId, usize)>,
    /// Where the folder tree's keyboard cursor is sitting. Distinct from the
    /// open item: this is what Enter would open.
    tree_cursor: Option<NodeId>,
    /// Set for one frame after the cursor moves, so the sidebar scrolls to it
    /// then and never again.
    reveal_cursor: bool,
    sidebar_width: SidebarWidth,
    /// Style and stored zoom are applied once, on the first frame.
    initialised: bool,
}

impl App {
    #[must_use]
    pub fn new(store: DataStore) -> Self {
        let (tree, startup_notice) = match store.load() {
            LoadOutcome::Loaded { tree, existed } => {
                if existed {
                    log::info!(
                        "loaded {} node(s) from {}",
                        tree.len(),
                        store.path().display()
                    );
                } else {
                    log::info!("no data file yet, starting empty");
                }
                (tree, None)
            }
            LoadOutcome::Recovered {
                tree,
                quarantined,
                reason,
            } => {
                let notice = match quarantined {
                    Some(path) => format!(
                        "Could not read your data file ({reason}). The original was moved to {} and a fresh, empty tree was started.",
                        path.display()
                    ),
                    None => format!("Could not read your data file ({reason}). Started empty."),
                };
                log::error!("{notice}");
                (tree, Some(notice))
            }
        };

        let settings = Settings::load(store.path());
        let mut app = Self {
            tree,
            store,
            dirty_since: None,
            startup_notice,
            last_error: None,
            view: View::Welcome,
            editor: None,
            dialog: None,
            filter: ui::Filter::default(),
            focus_search: false,
            panel: settings.panel,
            settings,
            comment_cursor: None,
            tree_cursor: None,
            reveal_cursor: false,
            sidebar_width: SidebarWidth::Auto,
            initialised: false,
        };

        // Opt in only. Never invent data in someone's real file.
        if app.tree.is_empty() && std::env::var_os("TRACKCRAB_SEED").is_some() {
            app.seed_demo_tree();
        }
        app
    }

    /// Call after any mutation. Actual writing is debounced.
    pub fn mark_dirty(&mut self) {
        self.dirty_since = Some(std::time::Instant::now());
    }

    pub fn save_now(&mut self) {
        if let Err(err) = self.store.save(&self.tree) {
            log::error!("save failed: {err}");
            self.last_error = Some(format!("Save failed: {err}"));
        } else {
            self.dirty_since = None;
        }
    }

    #[must_use]
    pub const fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Mutable access to the tree. The views never use this, it is how tests and
    /// (from M5) the creation flow seed and edit content.
    pub const fn tree_mut(&mut self) -> &mut Tree {
        &mut self.tree
    }

    #[must_use]
    pub const fn view(&self) -> &View {
        &self.view
    }

    /// Raises the rename command, as the sidebar's context menu does.
    pub fn request_rename_folder(&mut self, id: NodeId) {
        let name = self
            .tree
            .folder(id)
            .map(|f| f.name.clone())
            .unwrap_or_default();
        self.dialog = Some(Dialog::RenameFolder { id, name });
    }

    /// Raises the delete command, as the sidebar's context menu does.
    pub fn request_delete_folder(&mut self, id: NodeId) {
        self.dialog = Some(Dialog::DeleteFolder { id });
    }

    /// Replaces the sidebar search text, as typing into the box does. A test
    /// seam: typing for real leaves the caret in the box, which is exactly the
    /// state some tests need to *not* be in.
    pub fn set_filter_text(&mut self, text: &str) {
        text.clone_into(&mut self.filter.text);
    }

    /// Replaces the text in whichever name dialog is open.
    pub fn set_dialog_name(&mut self, text: &str) {
        match &mut self.dialog {
            Some(Dialog::NewFolder { name, .. } | Dialog::RenameFolder { name, .. }) => {
                text.clone_into(name);
            }
            _ => log::warn!("no name dialog is open"),
        }
    }

    /// Which side panel is showing.
    #[must_use]
    pub const fn panel(&self) -> Panel {
        self.panel
    }

    /// Where the folder tree's keyboard cursor is sitting.
    #[must_use]
    pub const fn tree_cursor(&self) -> Option<NodeId> {
        self.tree_cursor
    }

    /// Places the tree cursor, as clicking a row does. A test seam, so a walk
    /// can start from a known state rather than from whatever was last opened.
    pub const fn set_tree_cursor(&mut self, at: Option<NodeId>) {
        self.tree_cursor = at;
    }

    /// Which comment space the notebook is pointing at, and for which folder.
    #[must_use]
    pub const fn comment_cursor(&self) -> Option<(NodeId, usize)> {
        self.comment_cursor
    }

    /// Where this instance reads and writes its data.
    #[must_use]
    pub fn data_path(&self) -> &std::path::Path {
        self.store.path()
    }

    // ------------------------------------------------------------- navigation

    /// Opens a folder in the main panel and expands the sidebar down to it, so
    /// the tree and the view never disagree about where you are.
    fn open_folder(&mut self, ctx: &egui::Context, id: NodeId) {
        self.view = View::Folder(id);
        // Clicking a row moves the cursor there, so up and down carry on from
        // wherever you last looked rather than from where they left off.
        self.tree_cursor = Some(id);
        // Searching found this folder through its comments, so point the
        // notebook at the page the text is actually on rather than page one.
        if let Ok(folder) = self.tree.folder(id)
            && let Some(index) = self.filter.matching_space(folder)
        {
            self.comment_cursor = Some((id, index));
        }
        self.reveal(ctx, id, true);
    }

    fn open_task(&mut self, ctx: &egui::Context, id: NodeId) {
        self.view = View::Task(id);
        self.tree_cursor = Some(id);
        self.editor = views::task::Editor::load(&self.tree, id);
        self.reveal(ctx, id, false);
    }

    /// Expands every ancestor so the node is visible in the tree.
    /// `include_self` also expands the node itself, which is what clicking a
    /// folder should do.
    fn reveal(&self, ctx: &egui::Context, id: NodeId, include_self: bool) {
        let mut to_open = self.tree.ancestors(id);
        if include_self {
            to_open.push(id);
        }
        for folder in to_open {
            let mut state =
                egui::containers::collapsing_header::CollapsingState::load_with_default_open(
                    ctx,
                    ui::collapse_id(folder, false),
                    false,
                );
            state.set_open(true);
            state.store(ctx);
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: Option<Action>) {
        let Some(action) = action else { return };
        match action {
            Action::OpenFolder(id) => self.open_folder(ctx, id),
            Action::OpenTask(id) => self.open_task(ctx, id),
            Action::NewTaskIn(parent) => self.start_new_task(ctx, parent),
            Action::NewFolderIn(parent) => {
                self.dialog = Some(Dialog::NewFolder {
                    parent,
                    name: String::new(),
                });
            }
            Action::RenameFolder(id) => {
                let name = self
                    .tree
                    .folder(id)
                    .map(|f| f.name.clone())
                    .unwrap_or_default();
                self.dialog = Some(Dialog::RenameFolder { id, name });
            }
            Action::DeleteFolder(id) => self.dialog = Some(Dialog::DeleteFolder { id }),
            Action::ToggleComments => self.set_panel(self.panel.toggled_to(Panel::Comments)),
            Action::Move { node, into } => match self.tree.move_node(node, into) {
                Ok(()) => {
                    self.mark_dirty();
                    // Show where it landed.
                    if let Some(parent) = into {
                        self.reveal(ctx, parent, true);
                    }
                }
                Err(err) => {
                    log::warn!("move refused: {err}");
                    self.last_error = Some(err.to_string());
                }
            },
        }
    }

    /// Creates the blank task first so it appears in the sidebar and the listing
    /// immediately, then prompts for its details. Cancelling the prompt removes
    /// it again.
    fn start_new_task(&mut self, ctx: &egui::Context, parent: NodeId) {
        match self.tree.create_task(parent, "", None, Status::Open) {
            Ok(id) => {
                self.mark_dirty();
                self.reveal(ctx, id, false);
                self.dialog = Some(Dialog::new_task(id));
            }
            Err(err) => {
                log::error!("could not create a task: {err}");
                self.last_error = Some(format!("Could not create a task there: {err}"));
            }
        }
    }

    /// Drops back to the welcome page if whatever we were showing is gone.
    fn reconcile_view(&mut self) {
        let alive = match self.view {
            View::Welcome => true,
            View::Folder(id) | View::Task(id) => self.tree.contains(id),
        };
        if !alive {
            self.view = View::Welcome;
        }
        // A cursor left pointing at something deleted would open nothing on
        // Enter and draw an outline around a row that is not there.
        if self.tree_cursor.is_some_and(|id| !self.tree.contains(id)) {
            self.tree_cursor = None;
        }

        // The editor must never outlive, or lag behind, the open task.
        match self.view {
            View::Task(id) => {
                if self.editor.as_ref().is_none_or(|e| e.id() != id) {
                    self.editor = views::task::Editor::load(&self.tree, id);
                }
            }
            View::Welcome | View::Folder(_) => self.editor = None,
        }
    }

    // ---------------------------------------------------------- sidebar width

    /// Sidebar width for this frame.
    ///
    /// In `Auto` the width is written straight into the panel's stored state
    /// each frame, which is how it keeps tracking the window as it resizes.
    /// egui would otherwise remember the first width forever.
    fn resolve_sidebar_width(&mut self, ui: &egui::Ui) {
        let panel_id = ui::sidebar_id();

        // Read the resize handle from last frame. A finished drag is the signal
        // that the user has taken over.
        if let Some(handle) = ui.read_response(ui::sidebar_resize_id())
            && (handle.dragged() || handle.drag_stopped())
        {
            self.sidebar_width = SidebarWidth::Manual;
        }

        if self.sidebar_width == SidebarWidth::Auto {
            let target = (ui.ctx().viewport_rect().width() * theme::metric::SIDEBAR_FRACTION)
                .clamp(theme::metric::SIDEBAR_MIN, theme::metric::SIDEBAR_MAX);
            let stored = egui::containers::panel::PanelState::load(ui.ctx(), panel_id);
            let mut outer = stored.map_or_else(
                || egui::Rect::from_min_size(ui.ctx().viewport_rect().min, egui::vec2(target, 0.0)),
                |s| s.outer_rect,
            );
            outer.max.x = outer.min.x + target;
            ui.ctx().data_mut(|data| {
                data.insert_persisted(
                    panel_id,
                    egui::containers::panel::PanelState { outer_rect: outer },
                );
            });
        }
    }

    // ------------------------------------------------------------------- seed

    /// Demo tree, only ever built into an empty store and only when
    /// `TRACKCRAB_SEED` is set. Exists so M2's navigation can be exercised before
    /// there is any way to create things in the UI.
    fn seed_demo_tree(&mut self) {
        let mut build = || -> Result<(), crate::model::TreeError> {
            let work = self.tree.create_folder(None, "Work")?;
            let clients = self.tree.create_folder(Some(work), "Clients")?;

            let acme = self.tree.create_folder(Some(clients), "Acme Migration")?;
            let t = self.tree.create_task(
                acme,
                "Land the VPC design",
                Some("Three AZs, private subnets for the data tier.".into()),
                Status::Completed,
            )?;
            self.tree.edit_task(t, |t| t.set_attributed_hm(15, 0))?;

            let t =
                self.tree
                    .create_task(acme, "Cut over the database", None, Status::InProgress)?;
            self.tree.edit_task(t, |t| t.set_attributed_hm(2, 0))?;

            self.tree.create_task(
                acme,
                "Sign off the runbook",
                None,
                Status::Blocked("waiting on the client's change window".into()),
            )?;

            let deep = self.tree.create_folder(Some(acme), "Phase 2")?;
            let deeper = self.tree.create_folder(Some(deep), "Networking")?;
            let t = self
                .tree
                .create_task(deeper, "Transit gateway peering", None, Status::Open)?;
            self.tree.edit_task(t, |t| t.set_attributed_hm(0, 45))?;

            let internal = self.tree.create_folder(Some(work), "Internal")?;
            self.tree.create_task(
                internal,
                "Rewrite the onboarding doc",
                None,
                Status::Cancelled,
            )?;
            let t = self
                .tree
                .create_task(internal, "Q3 partner review", None, Status::Open)?;
            self.tree.edit_task(t, |t| t.set_attributed_hm(1, 30))?;

            self.tree.create_folder(None, "Personal")?;
            Ok(())
        };

        if let Err(err) = build() {
            log::error!("seeding failed: {err}");
        } else {
            log::info!("seeded a demo tree ({} nodes)", self.tree.len());
            self.mark_dirty();
        }
    }

    /// The burger and the app name.
    fn topbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top(Id::new("trackcrab_topbar"))
            .exact_size(38.0)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let burger = ui.add(
                        egui::Button::new(egui::RichText::new(BURGER).size(17.0))
                            .frame(false)
                            .min_size(egui::vec2(28.0, 28.0)),
                    );
                    if burger.clicked() {
                        self.set_panel(self.panel.toggled_to(Panel::Folders));
                    }
                    burger.on_hover_text(if self.panel == Panel::Folders {
                        "Hide folders"
                    } else {
                        "Show folders"
                    });
                    ui.label(
                        egui::RichText::new("TrackCrab")
                            .strong()
                            .color(theme::color::TEXT_WEAK),
                    );
                });
            });
    }

    /// The folder new things should go into: whichever is open, or the folder
    /// holding the open task. `None` when neither, which only the folder
    /// shortcut can act on.
    fn current_folder(&self) -> Option<NodeId> {
        match self.view {
            View::Welcome => None,
            View::Folder(id) => Some(id),
            View::Task(id) => self.tree.node(id).ok().and_then(|node| node.parent),
        }
    }

    /// Keyboard shortcuts. Consumed, so a shortcut cannot also reach a widget.
    ///
    /// Nothing here fires while a dialog is up: a modal owns the keyboard, and
    /// Ctrl+N behind a prompt would stack a second one.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};

        if self.dialog.is_some() {
            return;
        }

        // Worked out up front, because several of the shortcuts below need it.
        //
        // The search box is the one field that keeps the *vertical* arrows for
        // the tree: a single line field has nothing to do with up and down, so
        // leaving them makes type, arrow, Enter one flow. Escape is left alone
        // too, further down.
        let focused = ctx.memory(egui::Memory::focused);
        let typing = focused.is_some();
        let writing = focused.is_some_and(|id| id != ui::search_box_id());

        // Shift+Ctrl+N is checked before Ctrl+N, otherwise the plainer shortcut
        // would swallow it. Both stand down while a field has the caret: a
        // Ctrl+N halfway through writing a note used to create a task.
        if !typing {
            if ctx.input_mut(|i| i.consume_shortcut(&keys::NEW_FOLDER)) {
                self.dialog = Some(Dialog::NewFolder {
                    parent: self.current_folder(),
                    name: String::new(),
                });
            } else if ctx.input_mut(|i| i.consume_shortcut(&keys::NEW_TASK))
                && let Some(folder) = self.current_folder()
            {
                self.start_new_task(ctx, folder);
            }
        }

        // The directional pair: think of it as pushing the content right (the
        // folder tree takes the left) or left (the notebook takes the right).
        // Pressing the same one again brings the content back to the middle.
        if ctx.input_mut(|i| i.consume_shortcut(&keys::PANEL_RIGHT)) {
            self.set_panel(self.panel.toggled_to(Panel::Folders));
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keys::PANEL_LEFT)) {
            self.set_panel(self.panel.toggled_to(Panel::Comments));
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keys::SAVE)) {
            self.save_now();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keys::FOCUS_SEARCH)) {
            self.set_panel(Panel::Folders);
            self.focus_search = true;
        }

        // A plain, unmodified key belongs to whatever has the caret: arrows move
        // it, Enter makes a new line, Delete deletes a character. A shortcut
        // consumed here never reaches the widget, so the plain keys below stand
        // down while a field is focused.
        //
        // The folder tree, walked as a flat list. Only while it has the screen,
        // so the arrows stay free for the notebook when that is what is open.
        if !writing && self.panel == Panel::Folders {
            if ctx.input_mut(|i| i.consume_shortcut(&keys::NAV_DOWN)) {
                self.move_tree_cursor(ctx, true);
            }
            if ctx.input_mut(|i| i.consume_shortcut(&keys::NAV_UP)) {
                self.move_tree_cursor(ctx, false);
            }
            if ctx.input_mut(|i| i.consume_shortcut(&keys::NAV_OPEN)) {
                self.open_tree_cursor(ctx);
            }
        }

        // The notebook is a carousel, so its arrows wrap where the tree's clamp.
        if !typing && self.panel == Panel::Comments {
            if ctx.input_mut(|i| i.consume_shortcut(&keys::SPACE_NEXT)) {
                self.step_comment_space(true);
            }
            if ctx.input_mut(|i| i.consume_shortcut(&keys::SPACE_PREV)) {
                self.step_comment_space(false);
            }
        }

        // Delete acts on whatever is open: a folder asks the usual way, and a
        // task raises the detail view's own confirmation rather than a second
        // one that would look different.
        if !typing && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
            match self.view {
                View::Folder(id) => self.dialog = Some(Dialog::DeleteFolder { id }),
                View::Task(_) => {
                    if let Some(editor) = &mut self.editor {
                        editor.request_delete();
                    }
                }
                View::Welcome => {}
            }
        }

        // Escape clears a filter, which is the only thing left for it to do once
        // no dialog is open.
        if self.filter.is_active() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.filter.clear();
        }

        // Zoom. egui applies the factor to every text style and spacing value,
        // so the whole interface scales together rather than us hand tuning it.
        if ctx.input_mut(|i| {
            i.consume_shortcut(&keys::ZOOM_IN) || i.consume_shortcut(&keys::ZOOM_IN_ALT)
        }) {
            self.set_zoom(ctx, ctx.zoom_factor() + 0.1);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keys::ZOOM_OUT)) {
            self.set_zoom(ctx, ctx.zoom_factor() - 0.1);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keys::ZOOM_RESET)) {
            self.set_zoom(ctx, 1.0);
        }
    }

    // ------------------------------------------------- keyboard navigation

    /// Moves the folder tree's cursor one visible row up or down.
    ///
    /// The tree is walked as a **flat** list, ignoring depth: down from the last
    /// child of a folder steps to the next top level folder, exactly as the eye
    /// reads it. Rows inside a collapsed folder, and rows a filter has hidden,
    /// are not in the list at all, so the cursor can never land somewhere
    /// invisible.
    ///
    /// The ends clamp rather than wrap. A tree has a real top and bottom, and
    /// holding a key down should stop there rather than teleport.
    fn move_tree_cursor(&mut self, ctx: &egui::Context, down: bool) {
        let rows = ui::flat_rows(ctx, &self.tree, &self.filter);
        if rows.is_empty() {
            self.tree_cursor = None;
            return;
        }
        // With no cursor yet, carry on from whatever is open, so the first press
        // continues where you are instead of jumping to the top.
        let from = self.tree_cursor.or(match self.view {
            View::Welcome => None,
            View::Folder(id) | View::Task(id) => Some(id),
        });
        let at = from.and_then(|id| rows.iter().position(|row| *row == id));
        let next = match at {
            Some(i) if down => (i + 1).min(rows.len() - 1),
            Some(i) => i.saturating_sub(1),
            // Nothing to continue from: come in at whichever end you are
            // heading away from.
            None if down => 0,
            None => rows.len() - 1,
        };
        self.tree_cursor = Some(rows[next]);
        self.reveal_cursor = true;
    }

    /// Opens whatever the tree cursor is sitting on, folder or task.
    fn open_tree_cursor(&mut self, ctx: &egui::Context) {
        let Some(id) = self.tree_cursor else { return };
        match self.tree.get(id) {
            Some(node) if node.is_folder() => self.open_folder(ctx, id),
            Some(_) => self.open_task(ctx, id),
            None => self.tree_cursor = None,
        }
    }

    /// Cycles the notebook one space, for the plain arrow keys.
    ///
    /// Wraps, matching the on screen chevrons rather than inventing a second
    /// behaviour for the same movement.
    fn step_comment_space(&mut self, forward: bool) {
        let Some(folder) = self.current_folder() else {
            return;
        };
        let count = self.tree.comment_spaces(folder).len();
        if count == 0 {
            return;
        }
        let index = match self.comment_cursor {
            Some((owner, index)) if owner == folder => index.min(count - 1),
            _ => 0,
        };
        let request = if forward {
            views::comments::Request::Next
        } else {
            views::comments::Request::Previous
        };
        if let Some(next) = request.resolve(index, count) {
            self.comment_cursor = Some((folder, next));
        }
    }

    /// Switches side panel and remembers the choice.
    ///
    /// Comments belong to a folder, so asking for them with no folder in play
    /// is quietly ignored rather than opening an empty notebook.
    fn set_panel(&mut self, next: Panel) {
        if next == Panel::Comments && self.current_folder().is_none() {
            return;
        }
        if self.panel == next {
            return;
        }
        self.panel = next;
        self.settings.panel = next;
        self.settings.save(self.store.path());
    }

    /// Applies and remembers a zoom factor.
    fn set_zoom(&mut self, ctx: &egui::Context, zoom: f32) {
        let zoom = ((zoom * 10.0).round() / 10.0).clamp(
            crate::store::settings::ZOOM_MIN,
            crate::store::settings::ZOOM_MAX,
        );
        ctx.set_zoom_factor(zoom);
        self.settings.zoom = zoom;
        self.settings.save(self.store.path());
    }

    /// The comments notebook, over the right of the content.
    ///
    /// `slide` is the shared tween factor: 1 is fully in frame, 0 fully out. The
    /// overlay keeps being drawn while it is above zero so the close animation
    /// plays out rather than the panel vanishing.
    ///
    /// Returns whether the tree changed. The first space is created here, the
    /// moment the notebook is first opened for a folder, so merely browsing a
    /// folder never writes anything.
    fn comments_overlay(&mut self, ui: &egui::Ui, content: egui::Rect, slide: f32) -> bool {
        let Some(folder) = self.current_folder() else {
            return false;
        };
        let mut dirty = false;

        // Only on the way in. Sliding shut must not write to a folder the user
        // is in the act of leaving alone.
        let opening = self.panel == Panel::Comments;
        if opening && self.tree.comment_spaces(folder).is_empty() {
            match self.tree.add_comment_space(folder) {
                Ok(_) => dirty = true,
                Err(err) => {
                    log::error!("could not start a comment space: {err}");
                    return false;
                }
            }
        }

        // Keep the cursor with the folder it belongs to, and inside range.
        let count = self.tree.comment_spaces(folder).len();
        let index = match self.comment_cursor {
            Some((owner, index)) if owner == folder => index.min(count.saturating_sub(1)),
            _ => 0,
        };
        self.comment_cursor = Some((folder, index));

        let report = views::comments::show(ui, &mut self.tree, folder, index, content, slide);
        if let Some(request) = report.request {
            use views::comments::Request;
            match request {
                Request::Close => self.set_panel(Panel::None),
                Request::Previous | Request::Next => {
                    if let Some(next) = request.resolve(index, count) {
                        self.comment_cursor = Some((folder, next));
                    }
                }
                Request::Add => match self.tree.add_comment_space(folder) {
                    // Land on what was just added, which is the "+ then start
                    // typing" flow.
                    Ok(next) => {
                        self.comment_cursor = Some((folder, next));
                        dirty = true;
                    }
                    Err(err) => {
                        log::error!("could not add a comment space: {err}");
                        self.last_error = Some(format!("Could not add a comment space: {err}"));
                    }
                },
                Request::Delete => {
                    self.dialog = Some(Dialog::DeleteCommentSpace { folder, index });
                }
            }
        }
        dirty || report.changed
    }

    /// A label following the cursor while something is being dragged, so it is
    /// obvious what you picked up. The response level drag and drop API gives no
    /// ghost of its own.
    fn drag_ghost(&self, ui: &egui::Ui) {
        let ctx = ui.ctx();
        let Some(held) = egui::DragAndDrop::payload::<NodeId>(ctx) else {
            return;
        };
        let Some(node) = self.tree.get(*held) else {
            return;
        };
        let Some(pointer) = ctx.pointer_latest_pos() else {
            return;
        };

        let label = ui::row_label(node).to_owned();
        let kind = if node.is_folder() { "folder" } else { "task" };
        egui::Area::new(Id::new("trackcrab_drag_ghost"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer + egui::vec2(14.0, 10.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("{label}  ({kind})"))
                            .small()
                            .color(theme::color::TEXT),
                    );
                });
            });
    }

    /// Any startup or save problem, shown once and dismissable.
    fn notices(&mut self, ui: &mut egui::Ui) {
        for slot in [&mut self.startup_notice, &mut self.last_error] {
            if let Some(text) = slot.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(theme::color::DANGER, text);
                    if ui.small_button("Dismiss").clicked() {
                        *slot = None;
                    }
                });
                ui.add_space(8.0);
            }
        }
    }

    /// Removes a task and moves the view to the folder it was in, which is where
    /// the user was working, rather than dumping them on the welcome page.
    fn delete_task(&mut self, id: NodeId) {
        let parent = self.tree.node(id).ok().and_then(|node| node.parent);
        match self.tree.delete_task(id) {
            Ok(()) => {
                self.editor = None;
                self.view = parent.map_or(View::Welcome, View::Folder);
                self.mark_dirty();
            }
            Err(err) => {
                log::error!("delete failed: {err}");
                self.last_error = Some(format!("Could not delete that task: {err}"));
            }
        }
    }

    /// Paints the whole app into `ui`.
    ///
    /// Split out from the `eframe::App` impl because an `eframe::Frame` cannot be
    /// constructed outside eframe, and the UI tests need to drive the real
    /// interface directly rather than a stand in.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        if !self.initialised {
            theme::install(ui.ctx());
            ui.ctx().set_zoom_factor(self.settings.zoom);
            self.initialised = true;
        }
        self.shortcuts(ui.ctx());
        self.reconcile_view();

        self.topbar(ui);

        self.resolve_sidebar_width(ui);

        let mut action = None;
        let mut dirty = false;
        let mut delete = None;
        let mut open = self.panel == Panel::Folders;
        egui::Panel::left(ui::sidebar_id())
            .resizable(true)
            .size_range(egui::Rangef::new(
                theme::metric::SIDEBAR_MIN,
                theme::metric::SIDEBAR_MAX,
            ))
            .show_collapsible(ui, &mut open, |ui| {
                action = sidebar::show(
                    ui,
                    &self.tree,
                    &self.view,
                    &mut self.filter,
                    std::mem::take(&mut self.focus_search),
                    sidebar::Nav {
                        cursor: self.tree_cursor,
                        reveal: std::mem::take(&mut self.reveal_cursor),
                    },
                );
            });
        // show_collapsible flips this itself when the panel is dragged shut, so
        // the burger and the drag gesture stay in agreement.
        if !open && self.panel == Panel::Folders {
            self.set_panel(Panel::None);
        }

        let central = egui::CentralPanel::default().show(ui, |ui| {
            self.notices(ui);

            match self.view {
                View::Welcome => views::welcome::show(ui, &self.tree, self.store.path()),
                View::Folder(id) => {
                    if let Some(next) = views::folder::show(ui, &self.tree, id) {
                        action = Some(next);
                    }
                }
                View::Task(id) => {
                    if let Some(editor) = &mut self.editor {
                        let report = views::task::show(ui, &mut self.tree, editor);
                        if report.changed {
                            dirty = true;
                        }
                        if report.delete_confirmed {
                            delete = Some(id);
                        }
                    }
                }
            }
        });

        if let Some(id) = delete {
            self.delete_task(id);
        }

        // The same call the folder sidebar's own slide uses, so both panels move
        // at one speed by construction rather than a copied duration. Drawn
        // while the factor is still above zero so closing tweens out.
        let slide = ui
            .ctx()
            .animate_bool_responsive(Id::new(COMMENTS_ANIM), self.panel == Panel::Comments);
        if slide > 0.0 {
            dirty |= self.comments_overlay(ui, central.response.rect, slide);
        }

        let report = dialogs::show(ui, &mut self.tree, &mut self.dialog);
        if report.changed {
            dirty = true;
        }
        if let Some(err) = report.error {
            self.last_error = Some(err);
        }
        if let Some(index) = report.comment_index
            && let Some((folder, _)) = self.comment_cursor
        {
            self.comment_cursor = Some((folder, index));
        }
        if let Some(id) = report.removed
            && matches!(self.view, View::Task(open) | View::Folder(open) if open == id)
        {
            // Whatever we were showing has gone. Fall back to its parent.
            self.view = View::Welcome;
        }
        if let Some(id) = report.reveal {
            // Expand the tree down to the new item so you can see it appear,
            // but stay where you are.
            self.reveal(ui.ctx(), id, self.tree.get(id).is_some_and(Node::is_folder));
        }

        if dirty {
            self.mark_dirty();
        }
        self.drag_ghost(ui);
        self.apply(ui.ctx(), action);
    }
}

impl eframe::App for App {
    /// Per frame work that paints nothing. eframe 0.36 splits this out from
    /// `ui`, which is exactly where a debounced save belongs.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(since) = self.dirty_since {
            let elapsed = since.elapsed();
            if elapsed >= SAVE_DEBOUNCE {
                self.save_now();
            } else {
                // Wake up when the debounce expires, so a pending save is not
                // left waiting on the next unrelated repaint.
                ctx.request_repaint_after(SAVE_DEBOUNCE.saturating_sub(elapsed));
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.dirty_since.is_some() {
            self.save_now();
        }
    }
}
