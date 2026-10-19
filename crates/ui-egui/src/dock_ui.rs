//! The window as a dock: the two file panels and every tool (viewer, pages, synchronization,
//! search, rename, scanner, queue, sites) are tabs that can be dragged, split, floated and
//! closed. Which tools are open is the session's business (`sync`, `queue`, ...); the dock
//! only gives each open tool a tab and takes it away when the tool is closed, and a closed
//! tab sends the tool's close command. The layout is remembered in `layout.json`.

use std::path::Path;
use std::time::{Duration, Instant};

use bluepane_engine::Tool;
use egui::{Id, RichText, Ui, WidgetText};
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{DockArea, DockState, NodeIndex, NodePath, SurfaceIndex, TabViewer};
use serde::{Deserialize, Serialize};

use crate::BluepaneUi;

/// One tab of the dock: a file panel or a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Tab {
    /// A file panel (0 = left, 1 = right).
    Panel(u8),
    /// Tools are closed at start, so they are never saved with the layout.
    #[serde(skip)]
    Tool(Tool),
}

impl Tab {
    pub(crate) fn is_tool(self) -> bool {
        matches!(self, Tab::Tool(_))
    }
}

/// True while the session has this tab's content (panels always).
fn is_open(session: &bluepane_engine::Session, tab: Tab) -> bool {
    match tab {
        Tab::Panel(_) => true,
        Tab::Tool(tool) => tool.is_open(session),
    }
}

/// The two panels side by side; tools join them on the right when they open.
pub(crate) fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Panel(0)]);
    dock.main_surface_mut()
        .split_right(NodeIndex::root(), 0.5, vec![Tab::Panel(1)]);
    dock
}

/// Keeps both file panels on screen: a panel never shares its group of tabs with another
/// tab (where it would hide behind it). Whatever was dropped next to a panel moves out: the
/// other panel to a split on its right, a tool to the tools' group. Returns true if it moved
/// something.
pub(crate) fn keep_panels_visible(dock: &mut DockState<Tab>) -> bool {
    let mut moved = false;
    // A few rounds at most: each one empties one crowded group.
    for _ in 0..8 {
        let crowded = dock.iter_leaves().find_map(|(path, leaf)| {
            let tabs = leaf.tabs();
            let first_panel = tabs.iter().copied().find(|t| !t.is_tool())?;
            (tabs.len() > 1).then(|| {
                let others: Vec<Tab> = tabs.iter().copied().filter(|t| *t != first_panel).collect();
                (path, others)
            })
        });
        let Some((path, others)) = crowded else {
            break;
        };
        for tab in others {
            if let Some(found) = dock.find_tab(&tab) {
                dock.remove_tab(found);
            }
            if tab.is_tool() {
                add_tool(dock, tab);
            } else {
                dock[path.surface].split_right(path.node, 0.5, vec![tab]);
            }
        }
        moved = true;
    }
    moved
}

/// Share of the width the panels keep when the first tool opens beside them.
const PANELS_SHARE: f32 = 0.58;

/// Gives every open tool a tab and takes the tab of every closed one away. A new tab joins
/// the other tool tabs, or opens on the right of the panels, and gets the focus.
pub(crate) fn sync_tools(dock: &mut DockState<Tab>, session: &bluepane_engine::Session) {
    for tool in Tool::ALL {
        let tab = Tab::Tool(tool);
        match (tool.is_open(session), dock.find_tab(&tab)) {
            (true, None) => add_tool(dock, tab),
            (false, Some(_)) => {
                while let Some(path) = dock.find_tab(&tab) {
                    dock.remove_tab(path);
                }
            }
            _ => {}
        }
    }
}

fn add_tool(dock: &mut DockState<Tab>, tab: Tab) {
    // Next to the tools that are already open...
    let leaf = dock
        .iter_leaves()
        .find(|(_, leaf)| leaf.tabs().iter().any(|t| t.is_tool()))
        .map(|(path, _)| path);
    if let Some(path) = leaf {
        if let Ok(leaf) = dock.leaf_mut(path) {
            leaf.append_tab(tab);
        }
        dock.set_focused_node_and_surface(path);
        return;
    }
    // ...or on the right of everything else.
    let [_, new] = dock
        .main_surface_mut()
        .split_right(NodeIndex::root(), PANELS_SHARE, vec![tab]);
    dock.set_focused_node_and_surface(NodePath {
        surface: SurfaceIndex::main(),
        node: new,
    });
}

/// The saved layout, if the file is there and sound: both panels exactly once. Tools are
/// not restored (they are closed at start), only where the panels were put.
pub(crate) fn load_layout(path: &Path) -> Option<DockState<Tab>> {
    parse_layout(&std::fs::read_to_string(path).ok()?)
}

/// A layout from its text, if it is sound (see [`load_layout`]).
pub(crate) fn parse_layout(text: &str) -> Option<DockState<Tab>> {
    let mut dock: DockState<Tab> = serde_json::from_str(text).ok()?;
    without_tools(&mut dock);
    // A file written by an older version may hold a broken tree (see `without_tools`).
    let root_sound = dock
        .iter_all_nodes()
        .next()
        .is_some_and(|(_, node)| node.is_leaf() || node.is_parent());
    if !root_sound {
        return None;
    }
    let count = |wanted: Tab| dock.iter_all_tabs().filter(|(_, t)| **t == wanted).count();
    (count(Tab::Panel(0)) == 1 && count(Tab::Panel(1)) == 1).then_some(dock)
}

/// Takes the tool tabs out one by one. (`retain_tabs` of egui_dock leaves the tree broken
/// when it empties a whole group: the split above it keeps pointing at empty nodes.)
fn without_tools(dock: &mut DockState<Tab>) {
    while let Some(path) = dock.find_tab_from(|tab| tab.is_tool()) {
        dock.remove_tab(path);
    }
}

/// The layout as text, without the tool tabs (see [`load_layout`]).
pub(crate) fn layout_text(dock: &DockState<Tab>) -> String {
    let mut copy = dock.clone();
    without_tools(&mut copy);
    // The sizes on screen are worked out again when the dock is drawn; before the first
    // frame they are "infinite", which JSON cannot hold.
    for (_, node) in copy.iter_all_nodes_mut() {
        node.set_rect(egui::Rect::ZERO);
        if let egui_dock::Node::Leaf(leaf) = node {
            leaf.viewport = egui::Rect::ZERO;
        }
    }
    serde_json::to_string(&copy).unwrap_or_default()
}

/// Draws one tab and remembers which tool tabs were closed with their ×.
struct DockViewer<'a> {
    app: &'a mut BluepaneUi,
    closed: Vec<Tab>,
}

impl TabViewer for DockViewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> Id {
        Id::new(("bluepane-tab", *tab))
    }

    fn title(&mut self, tab: &mut Tab) -> WidgetText {
        match *tab {
            Tab::Panel(i) => {
                let panel = &self.app.session.panels[usize::from(i)];
                let place = panel
                    .location()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| panel.location().display().to_string());
                let side = if i == 0 { "Lewy panel" } else { "Prawy panel" };
                if self.app.focused_tool().is_none() && usize::from(i) == self.app.session.active {
                    RichText::new(format!(
                        "{} {side}: {place}",
                        egui_phosphor::regular::KEYBOARD
                    ))
                    .color(self.app.theme.accent)
                    .into()
                } else {
                    RichText::new(format!("{side}: {place}")).into()
                }
            }
            Tab::Tool(tool) => {
                // The tool that has the keyboard says so on its tab.
                if self.app.focused_tool() == Some(tool) {
                    RichText::new(format!(
                        "{} {}",
                        egui_phosphor::regular::KEYBOARD,
                        tool.label()
                    ))
                    .color(self.app.theme.accent)
                    .into()
                } else {
                    RichText::new(tool.label()).into()
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Tab) {
        let app = &mut *self.app;
        match *tab {
            Tab::Panel(i) => {
                let index = usize::from(i);
                // Quick View takes the place of the panel that does not have the keyboard.
                if app.session.quick.is_some()
                    && !app.session.quick_in_tab()
                    && index != app.session.active
                {
                    app.quick_ui(ui, index);
                } else {
                    app.panel_ui(ui, index);
                }
            }
            Tab::Tool(tool) => app.tool_ui(ui, tool),
        }
    }

    fn is_closeable(&self, tab: &Tab) -> bool {
        tab.is_tool()
    }

    fn on_close(&mut self, tab: &mut Tab) -> OnCloseResponse {
        // The tool is closed through its own command; the next frame drops the tab.
        self.closed.push(*tab);
        OnCloseResponse::Ignore
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        // The panels scroll their own lists; the tools may be taller than their tab.
        if tab.is_tool() {
            [false, true]
        } else {
            [false, false]
        }
    }
}

impl BluepaneUi {
    /// The whole central area: syncs the tabs with the session, draws the dock, saves the
    /// layout when it changed.
    pub(crate) fn dock_ui(&mut self, ui: &mut Ui) {
        let mut dock = self.dock.take().unwrap_or_else(|| {
            self.session
                .config_dir()
                .and_then(|dir| load_layout(&dir.join("layout.json")))
                .unwrap_or_else(default_layout)
        });
        if let Some(request) = self.session.layout_request.take() {
            match request {
                bluepane_engine::LayoutAction::Reset => dock = default_layout(),
                bluepane_engine::LayoutAction::Save(name) => {
                    let text = layout_text(&dock);
                    let _ = self.session.store_layout(&name, &text);
                }
                bluepane_engine::LayoutAction::Load(name) => {
                    match self
                        .session
                        .layout_text(&name)
                        .ok()
                        .and_then(|t| parse_layout(&t))
                    {
                        Some(loaded) => {
                            dock = loaded;
                            self.session.status = format!("Wczytano układ „{name}”");
                        }
                        None => {
                            self.session.status = format!("Układ „{name}” jest uszkodzony");
                            self.session.status_is_error = true;
                        }
                    }
                }
            }
        }
        sync_tools(&mut dock, &self.session);
        let style = egui_dock::Style::from_egui(ui.style().as_ref());
        let mut viewer = DockViewer {
            app: self,
            closed: Vec::new(),
        };
        DockArea::new(&mut dock)
            .style(style)
            .show_close_buttons(true)
            .show_add_buttons(false)
            .show_inside(ui, &mut viewer);
        let closed = std::mem::take(&mut viewer.closed);
        for tab in closed {
            if let Tab::Tool(tool) = tab {
                self.dispatch(tool.close_line());
            }
        }
        keep_panels_visible(&mut dock);
        // The tab the user last clicked in decides where the keys go.
        if let Some((_, tab)) = dock.find_active_focused() {
            self.focus = *tab;
        }
        self.save_layout(&dock);
        self.dock = Some(dock);
    }

    /// The tool that has the keyboard, if the focused tab is an open tool.
    pub(crate) fn focused_tool(&self) -> Option<Tool> {
        match self.focus {
            Tab::Tool(tool) if is_open(&self.session, self.focus) => Some(tool),
            _ => None,
        }
    }

    /// Draws a tool in its tab.
    fn tool_ui(&mut self, ui: &mut Ui, tool: Tool) {
        match tool {
            Tool::Pages => self.pages_ui(ui),
            Tool::Viewer => self.viewer_ui(ui),
            Tool::Sync => self.sync_ui(ui),
            Tool::Scan => self.scan_ui(ui),
            Tool::Queue => self.queue_ui(ui),
            Tool::Sites => self.sites_ui(ui),
            Tool::Rename => self.rename_ui(ui),
            Tool::Find => self.find_ui(ui),
            Tool::Quick => self.quick_ui(ui, 0),
        }
    }

    /// The keys of the tool whose tab has the focus. Typing never reaches the panel filter.
    pub(crate) fn tool_keys(&mut self, tool: Tool, pressed: &[String]) {
        match tool {
            Tool::Pages => self.pages_keys(pressed),
            Tool::Viewer => {
                for key in pressed {
                    if let Some(line) = self.session.keymap.lookup_viewer(key).map(str::to_owned) {
                        self.dispatch(&line);
                    }
                }
            }
            Tool::Scan => self.scan_keys(pressed),
            Tool::Queue => self.queue_keys(pressed),
            Tool::Sites => self.sites_keys(pressed),
            Tool::Find => self.find_keys(pressed),
            Tool::Rename => self.rename_keys(pressed),
            Tool::Quick => {
                for key in pressed {
                    let line = match key.as_str() {
                        "ArrowDown" => "quick scroll 1",
                        "ArrowUp" => "quick scroll -1",
                        "PageDown" => "quick scroll 20",
                        "PageUp" => "quick scroll -20",
                        "ArrowRight" => "quick page 1",
                        "ArrowLeft" => "quick page -1",
                        "Escape" => "quick",
                        _ => continue,
                    };
                    self.dispatch(line);
                }
            }
            Tool::Sync => {
                let dirty = self.sync_form_is_dirty();
                for key in pressed {
                    match key.as_str() {
                        "Escape" => self.dispatch("syncclose"),
                        // Running needs the settings on screen to be the ones compared.
                        "F5" if !dirty => self.dispatch("syncrun"),
                        "F9" => self.sync_compare_now(),
                        _ => {}
                    }
                }
            }
        }
    }

    fn save_layout(&mut self, dock: &DockState<Tab>) {
        let Some(dir) = self.session.config_dir() else {
            return;
        };
        if self.layout_saved_at.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.layout_saved_at = Instant::now();
        let text = layout_text(dock);
        if text.is_empty() || text == self.layout_saved {
            return;
        }
        let path = dir.join("layout.json");
        if std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&path, &text).is_ok() {
            self.layout_saved = text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bluepane_engine::Session;

    fn tabs(dock: &DockState<Tab>) -> Vec<Tab> {
        dock.iter_all_tabs().map(|(_, t)| *t).collect()
    }

    #[test]
    fn the_default_layout_has_both_panels_and_no_tools() {
        let dock = default_layout();
        let all = tabs(&dock);
        assert!(all.contains(&Tab::Panel(0)) && all.contains(&Tab::Panel(1)));
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn open_tools_get_tabs_beside_the_panels_and_closed_ones_lose_them() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Session::new(tmp.path(), tmp.path());
        let mut dock = default_layout();
        sync_tools(&mut dock, &s);
        assert_eq!(tabs(&dock).len(), 2, "nothing is open");

        s.run_line("queue").unwrap();
        s.run_line("sites").unwrap();
        sync_tools(&mut dock, &s);
        let all = tabs(&dock);
        assert!(all.contains(&Tab::Tool(Tool::Queue)) && all.contains(&Tab::Tool(Tool::Sites)));
        // Both tools share one leaf; the panels are in others.
        let tool_leaves = dock
            .iter_leaves()
            .filter(|(_, leaf)| leaf.tabs().iter().any(|t| t.is_tool()))
            .count();
        assert_eq!(tool_leaves, 1);
        // Opening again does not duplicate a tab.
        sync_tools(&mut dock, &s);
        assert_eq!(tabs(&dock).len(), 4);

        s.run_line("queueclose").unwrap();
        sync_tools(&mut dock, &s);
        let all = tabs(&dock);
        assert!(!all.contains(&Tab::Tool(Tool::Queue)) && all.contains(&Tab::Tool(Tool::Sites)));
        s.run_line("siteclose").unwrap();
        sync_tools(&mut dock, &s);
        assert_eq!(tabs(&dock).len(), 2);
    }

    #[test]
    fn the_layout_survives_a_round_trip_without_tools_and_bad_files_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("layout.json");
        let mut dock = default_layout();
        dock.push_to_first_leaf(Tab::Tool(Tool::Queue));
        std::fs::write(&path, layout_text(&dock)).unwrap();
        let loaded = load_layout(&path).expect("a sound file loads");
        let all = tabs(&loaded);
        assert!(all.contains(&Tab::Panel(0)) && all.contains(&Tab::Panel(1)));
        assert!(
            !all.contains(&Tab::Tool(Tool::Queue)),
            "tools are not restored"
        );

        std::fs::write(&path, "not json").unwrap();
        assert!(load_layout(&path).is_none());
        // A layout missing a panel would leave the user without one: refused.
        let only_left = DockState::new(vec![Tab::Panel(0)]);
        std::fs::write(&path, layout_text(&only_left)).unwrap();
        assert!(load_layout(&path).is_none());
        assert!(load_layout(&tmp.path().join("missing.json")).is_none());
    }

    #[test]
    fn a_tab_dropped_next_to_a_panel_is_moved_out_so_both_panels_stay_visible() {
        // The right panel and a tool dragged into the left panel's group.
        let mut dock = DockState::new(vec![Tab::Panel(0), Tab::Panel(1), Tab::Tool(Tool::Queue)]);
        assert!(keep_panels_visible(&mut dock));
        for (_, leaf) in dock.iter_leaves() {
            let tabs = leaf.tabs();
            if tabs.iter().any(|t| !t.is_tool()) {
                assert_eq!(tabs.len(), 1, "a panel is alone in its group: {tabs:?}");
            }
        }
        let all = tabs(&dock);
        assert!(all.contains(&Tab::Panel(0)) && all.contains(&Tab::Panel(1)));
        assert!(
            all.contains(&Tab::Tool(Tool::Queue)),
            "the tool is kept, elsewhere"
        );
        // Nothing to do in the default layout.
        assert!(!keep_panels_visible(&mut default_layout()));
    }

    #[test]
    fn quick_view_gets_a_tab_only_when_asked_for_one() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Session::new(tmp.path(), tmp.path());
        let mut dock = default_layout();
        s.run_line("quick").unwrap();
        sync_tools(&mut dock, &s);
        assert!(
            !tabs(&dock).contains(&Tab::Tool(Tool::Quick)),
            "it takes the other panel's place"
        );
        s.run_line("quick").unwrap();
        s.run_line("quick tab").unwrap();
        s.run_line("quick").unwrap();
        sync_tools(&mut dock, &s);
        assert!(tabs(&dock).contains(&Tab::Tool(Tool::Quick)));
        s.run_line("quick").unwrap();
        sync_tools(&mut dock, &s);
        assert!(!tabs(&dock).contains(&Tab::Tool(Tool::Quick)));
    }

    #[test]
    fn a_layout_saved_with_tools_open_loads_as_a_sound_tree_that_takes_tools_again() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Session::new(tmp.path(), tmp.path());
        let mut dock = default_layout();
        s.run_line("queue").unwrap();
        s.run_line("sites").unwrap();
        sync_tools(&mut dock, &s);
        let loaded = parse_layout(&layout_text(&dock)).expect("sound");
        let (_, root) = loaded.iter_all_nodes().next().unwrap();
        assert!(root.is_parent(), "the panels' split is the root");
        // Opening a tool in the loaded layout must not trip over the tree.
        let mut loaded = loaded;
        sync_tools(&mut loaded, &s);
        let all = tabs(&loaded);
        assert!(all.contains(&Tab::Tool(Tool::Queue)) && all.contains(&Tab::Panel(1)));
    }
}
