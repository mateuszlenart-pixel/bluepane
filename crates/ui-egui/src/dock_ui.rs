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
    let text = std::fs::read_to_string(path).ok()?;
    let mut dock: DockState<Tab> = serde_json::from_str(&text).ok()?;
    dock.retain_tabs(|tab| !tab.is_tool());
    let count = |wanted: Tab| dock.iter_all_tabs().filter(|(_, t)| **t == wanted).count();
    (count(Tab::Panel(0)) == 1 && count(Tab::Panel(1)) == 1).then_some(dock)
}

/// The layout as text, without the tool tabs (see [`load_layout`]).
pub(crate) fn layout_text(dock: &DockState<Tab>) -> String {
    let mut copy = dock.clone();
    copy.retain_tabs(|tab| !tab.is_tool());
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
                if app.session.quick.is_some() && index != app.session.active {
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
}
