use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use bluepane_archive::Listing;

use bluepane_vfs::{Entry, EntryKind};

use crate::{EngineError, Mask};

/// Name of the synthetic row that leads to the parent directory.
pub const PARENT_NAME: &str = "..";

/// How many visited directories each panel remembers.
const HISTORY_LIMIT: usize = 100;

/// The marked entries of a panel. Reads work like a `BTreeSet`; every change counts, so what is
/// derived from the selection (the summary line) is recomputed only when it changed.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    set: BTreeSet<PathBuf>,
    revision: u64,
}

impl Selection {
    /// Grows with every change of the selection.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn insert(&mut self, path: PathBuf) -> bool {
        self.revision += 1;
        self.set.insert(path)
    }

    pub fn remove(&mut self, path: &Path) -> bool {
        self.revision += 1;
        self.set.remove(path)
    }

    pub fn clear(&mut self) {
        self.revision += 1;
        self.set.clear();
    }

    pub fn extend(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.revision += 1;
        self.set.extend(paths);
    }

    pub fn retain(&mut self, keep: impl FnMut(&PathBuf) -> bool) {
        self.revision += 1;
        self.set.retain(keep);
    }
}

impl std::ops::Deref for Selection {
    type Target = BTreeSet<PathBuf>;

    fn deref(&self) -> &BTreeSet<PathBuf> {
        &self.set
    }
}

impl<'a> IntoIterator for &'a Selection {
    type Item = &'a PathBuf;
    type IntoIter = std::collections::btree_set::Iter<'a, PathBuf>;

    fn into_iter(self) -> Self::IntoIter {
        self.set.iter()
    }
}

impl FromIterator<PathBuf> for Selection {
    fn from_iter<I: IntoIterator<Item = PathBuf>>(iter: I) -> Self {
        Selection {
            set: iter.into_iter().collect(),
            revision: 1,
        }
    }
}

/// The rows for one directory of an archive.
fn archive_entries(view: &ArchiveView) -> Vec<Entry> {
    view.listing
        .children(&view.inner)
        .map(|e| Entry {
            name: e.name().to_string(),
            path: view.archive.join(&e.path),
            kind: if e.is_dir {
                EntryKind::Dir
            } else if e.special {
                EntryKind::Symlink
            } else {
                EntryKind::File
            },
            size: e.size,
            modified: e.modified,
            created: None,
            mode: None,
            owner: None,
            hidden: false,
        })
        .collect()
}

fn extension_of(entry: &Entry) -> &str {
    crate::name_parts(&entry.name, entry.is_dir()).1
}

/// True for the synthetic `..` row (no real entry can have this name).
pub fn is_parent_entry(entry: &Entry) -> bool {
    entry.name == PARENT_NAME
}

/// What the listing is ordered by. Directories always come first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Ext,
    Size,
    Date,
}

impl SortKey {
    pub fn parse(text: &str) -> Option<SortKey> {
        match text.to_ascii_lowercase().as_str() {
            "name" => Some(SortKey::Name),
            "ext" => Some(SortKey::Ext),
            "size" => Some(SortKey::Size),
            "date" => Some(SortKey::Date),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub descending: bool,
}

/// A panel that is browsing the inside of an archive instead of a directory.
///
/// `Panel::cwd` stays the directory that contains the archive; the entries shown are the
/// archive's contents at `inner`. Their `path`s are virtual (`<archive>/<inner>/<name>`):
/// they identify rows for selection but are not paths on disk.
#[derive(Debug, Clone)]
pub struct ArchiveView {
    pub archive: PathBuf,
    pub listing: Arc<Listing>,
    /// The directory inside the archive being shown (`""` is the top level), `/` separated.
    pub inner: String,
    /// The password to use for this archive, once the user has given one.
    pub password: Option<String>,
}

impl ArchiveView {
    /// Where the panel "is", for the path bar and tab titles: `/dir/a.zip/inner`.
    pub fn location(&self) -> PathBuf {
        if self.inner.is_empty() {
            self.archive.clone()
        } else {
            self.archive.join(&self.inner)
        }
    }

    /// The archive-internal path of a row of this view (`None` for the `..` row).
    pub fn inner_path_of(&self, entry: &Entry) -> Option<String> {
        if is_parent_entry(entry) {
            return None;
        }
        let name = entry.name.as_str();
        Some(if self.inner.is_empty() {
            name.to_string()
        } else {
            format!("{}/{name}", self.inner)
        })
    }
}

fn parent_inner(inner: &str) -> &str {
    inner.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// One of the two file panels.
///
/// A panel that shows a folder on an SFTP server.
#[derive(Debug, Clone)]
pub struct RemoteView {
    /// The saved site's name.
    pub site: String,
    /// `user@host:port`, for the path bar.
    pub label: String,
    /// The server folder being shown (`/` separated).
    pub path: String,
    pub conn: std::sync::Arc<dyn bluepane_remote::RemoteFs>,
    /// Folders visited on this server, oldest first, and where in them the panel is
    /// (Alt+Left / Alt+Right).
    pub history: Vec<String>,
    pub history_index: usize,
}

impl RemoteView {
    /// Where the panel "is", for the path bar: `sftp://user@host:22/dir`.
    pub fn location(&self) -> PathBuf {
        PathBuf::from(format!("sftp://{}{}", self.label, self.path))
    }
}

/// A panel that shows a list of files from anywhere (search results) instead of a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualList {
    pub title: String,
    pub paths: Vec<PathBuf>,
}

/// Which modifier keys were held during a mouse click on a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClickMode {
    /// Only moves the cursor (the selection is kept) and sets the range start.
    #[default]
    Plain,
    /// Ctrl+click: toggles the row, like in a Windows file list.
    Ctrl,
    /// Shift+click: selects the range from the last clicked row to this one.
    Shift,
    /// Ctrl+Shift+click: adds that range to the selection.
    CtrlShift,
}

/// How a panel shows its entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelView {
    /// Rows with name, type, size and date.
    #[default]
    List,
    /// Tiles with a thumbnail for pictures and an icon for the rest.
    Grid,
}

/// `entries` is what the user sees: the `..` row (when there is a parent directory)
/// followed by the directory contents that match `filter`.
#[derive(Debug, Clone)]
pub struct Panel {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub selected: Selection,
    /// The row a Shift+click range starts from (the last plain or Ctrl+click).
    anchor: Option<PathBuf>,
    /// Grows every time `entries` is recomputed (listing, filter, sort).
    pub revision: u64,
    /// Quick filter: name fragments and criteria (see [`crate::FilterSpec`]); only the entries
    /// that pass are shown.
    pub filter: String,
    pub sort: Sort,
    /// A list of rows, or a grid of thumbnails (`viewmode`).
    pub view: PanelView,
    /// Set while browsing the inside of an archive.
    pub archive: Option<ArchiveView>,
    /// Set while the panel shows a list of files (search results); `cwd` stays where the
    /// panel was, and leaving the list goes back there.
    pub virtual_list: Option<VirtualList>,
    /// Set while the panel shows a folder on an SFTP server; the listing is replaced by
    /// the session when a background listing finishes.
    pub remote: Option<RemoteView>,
    /// Set when the current directory could not be listed.
    pub error: Option<String>,
    /// The whole directory listing, before filtering and without the `..` row.
    listing: Vec<Entry>,
    history: Vec<PathBuf>,
    history_index: usize,
}

impl Panel {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        let mut panel = Panel {
            history: vec![cwd.clone()],
            history_index: 0,
            cwd,
            entries: Vec::new(),
            cursor: 0,
            selected: Selection::default(),
            anchor: None,
            revision: 0,
            filter: String::new(),
            sort: Sort {
                key: SortKey::Name,
                descending: false,
            },
            view: PanelView::List,
            archive: None,
            virtual_list: None,
            remote: None,
            error: None,
            listing: Vec::new(),
        };
        panel.reload();
        panel
    }

    /// An empty placeholder used while tabs are swapped; it is never shown.
    pub(crate) fn blank() -> Panel {
        Panel {
            cwd: PathBuf::new(),
            entries: Vec::new(),
            cursor: 0,
            selected: Selection::default(),
            anchor: None,
            revision: 0,
            filter: String::new(),
            sort: Sort {
                key: SortKey::Name,
                descending: false,
            },
            view: PanelView::List,
            archive: None,
            virtual_list: None,
            remote: None,
            error: None,
            listing: Vec::new(),
            history: Vec::new(),
            history_index: 0,
        }
    }

    /// Re-reads the directory, keeping the cursor entry and the still-valid selection.
    pub fn reload(&mut self) {
        if self.remote.is_some() {
            // The server is only asked by a background job (`Session::remote_refresh`).
            let present: HashSet<&Path> = self.listing.iter().map(|e| e.path.as_path()).collect();
            self.selected
                .retain(|path| present.contains(path.as_path()));
            let keep = self.current().map(|e| e.path.clone());
            self.rebuild(keep.as_deref());
            return;
        }
        let listed = match (&self.archive, &self.virtual_list) {
            (Some(view), _) => Ok(archive_entries(view)),
            // Entries that no longer exist (deleted, moved) drop out of the list.
            (None, Some(list)) => Ok(list
                .paths
                .iter()
                .filter_map(|p| bluepane_vfs::entry_of(p).ok())
                .collect()),
            (None, None) => bluepane_vfs::list_dir(&self.cwd),
        };
        match listed {
            Ok(entries) => {
                self.listing = entries;
                self.sort_listing();
                self.error = None;
            }
            Err(err) => {
                self.listing.clear();
                self.error = Some(err.to_string());
            }
        }
        let present: HashSet<&Path> = self.listing.iter().map(|e| e.path.as_path()).collect();
        self.selected
            .retain(|path| present.contains(path.as_path()));
        let keep = self.current().map(|e| e.path.clone());
        self.rebuild(keep.as_deref());
    }

    /// Orders `listing` by the current sort: directories first, then by key (reversed when
    /// descending), ties always by name ascending.
    fn sort_listing(&mut self) {
        let (key, descending) = (self.sort.key, self.sort.descending);
        // Keys are computed once per entry: lower-casing inside the comparator would
        // allocate on every comparison, which matters at 100k entries.
        let mut keyed: Vec<_> = std::mem::take(&mut self.listing)
            .into_iter()
            .map(|e| {
                let lname = e.name.to_lowercase();
                let primary = match key {
                    SortKey::Name => lname.clone(),
                    SortKey::Ext => extension_of(&e).to_lowercase(),
                    SortKey::Size | SortKey::Date => String::new(),
                };
                let number = match key {
                    // Directories have no size, so they stay ordered by name.
                    SortKey::Size if e.is_dir() => 0,
                    SortKey::Size => u128::from(e.size),
                    SortKey::Date => e
                        .modified
                        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos()),
                    _ => 0,
                };
                ((!e.is_dir(), primary, number, lname), e)
            })
            .collect();
        keyed.sort_by(|(a, ea), (b, eb)| {
            a.0.cmp(&b.0)
                .then_with(|| {
                    let order = a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2));
                    if descending {
                        order.reverse()
                    } else {
                        order
                    }
                })
                .then_with(|| a.3.cmp(&b.3))
                .then_with(|| ea.name.cmp(&eb.name))
        });
        self.listing = keyed.into_iter().map(|(_, e)| e).collect();
    }

    /// Changes the sort order. Without an explicit direction, choosing the current key
    /// again flips the direction and a new key starts ascending.
    pub fn set_sort(&mut self, key: SortKey, descending: Option<bool>) {
        let descending = descending.unwrap_or(key == self.sort.key && !self.sort.descending);
        self.sort = Sort { key, descending };
        let keep = self.current().map(|e| e.path.clone());
        self.sort_listing();
        self.rebuild(keep.as_deref());
    }

    /// Visible entries other than the `..` row.
    fn real_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| !is_parent_entry(e))
    }

    /// Selects (or unselects) the visible entries whose name matches the mask.
    /// Returns how many entries matched.
    pub fn select_matching(&mut self, mask: &Mask, select: bool) -> usize {
        let matched: Vec<PathBuf> = self
            .real_entries()
            .filter(|e| mask.matches(&e.name))
            .map(|e| e.path.clone())
            .collect();
        let count = matched.len();
        for path in matched {
            if select {
                self.selected.insert(path);
            } else {
                self.selected.remove(&path);
            }
        }
        count
    }

    /// Selects every visible entry (not `..`).
    pub fn select_all(&mut self) -> usize {
        let paths: Vec<PathBuf> = self.real_entries().map(|e| e.path.clone()).collect();
        let count = paths.len();
        self.selected.extend(paths);
        count
    }

    /// Clears the selection, including entries currently hidden by the filter.
    pub fn unselect_all(&mut self) {
        self.selected.clear();
    }

    /// Inverts the selection of the visible entries (not `..`).
    pub fn invert_selection(&mut self) {
        let paths: Vec<PathBuf> = self.real_entries().map(|e| e.path.clone()).collect();
        for path in paths {
            if !self.selected.remove(&path) {
                self.selected.insert(path);
            }
        }
    }

    /// Recomputes `entries` from the listing and the filter and puts the cursor on `keep`
    /// (or, when that is gone, near the old position).
    fn rebuild(&mut self, keep: Option<&Path>) {
        self.revision += 1;
        let spec = crate::FilterSpec::parse(&self.filter);
        let now = std::time::SystemTime::now();
        self.entries.clear();
        let parent = match &self.archive {
            // The `..` row of a list leaves it, back to the directory it was opened from.
            None if self.virtual_list.is_some() => Some(self.cwd.clone()),
            None if self.remote.is_some() => self
                .remote
                .as_ref()
                .filter(|r| r.path != "/")
                .map(|r| PathBuf::from(crate::remote_job::remote_parent(&r.path))),
            // In an archive the `..` row always exists: it leads up a level or out of it.
            Some(view) if view.inner.is_empty() => Some(self.cwd.clone()),
            Some(view) => Some(view.archive.join(parent_inner(&view.inner))),
            None => self.cwd.parent().map(Path::to_path_buf),
        };
        if let Some(parent) = parent {
            self.entries.push(Entry {
                name: PARENT_NAME.to_string(),
                path: parent,
                kind: EntryKind::Dir,
                size: 0,
                modified: None,
                created: None,
                mode: None,
                owner: None,
                hidden: false,
            });
        }
        self.entries.extend(
            self.listing
                .iter()
                .filter(|e| spec.is_empty() || spec.matches(e, now))
                .cloned(),
        );
        if let Some(index) = keep.and_then(|k| self.entries.iter().position(|e| e.path == k)) {
            self.cursor = index;
        }
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
    }

    /// How many entries the directory has, before the filter (without the `..` row).
    pub fn listing_len(&self) -> usize {
        self.listing.len()
    }

    /// Puts the cursor on the entry with this path, if it is visible.
    pub fn put_cursor_on(&mut self, path: &Path) {
        if let Some(index) = self.entries.iter().position(|e| e.path == path) {
            self.cursor = index;
        }
    }

    /// The entry under the cursor.
    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    /// What an operation acts on: the visible selected entries, or the entry under the
    /// cursor. The `..` row is never a target. Selected entries hidden by the filter are
    /// left alone, so an operation only touches what the user can see.
    pub fn target_entries(&self) -> Vec<&Entry> {
        let selected: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| self.selected.contains(&e.path))
            .collect();
        if !selected.is_empty() {
            return selected;
        }
        self.current()
            .filter(|e| !is_parent_entry(e))
            .into_iter()
            .collect()
    }

    /// The paths of [`Panel::target_entries`].
    pub fn targets(&self) -> Vec<PathBuf> {
        self.target_entries()
            .into_iter()
            .map(|e| e.path.clone())
            .collect()
    }

    /// Navigates to `dir` and records it in the history (forward history is dropped).
    pub fn change_dir(&mut self, dir: PathBuf) {
        if self.history.get(self.history_index) != Some(&dir) {
            self.history.truncate(self.history_index + 1);
            self.history.push(dir.clone());
            if self.history.len() > HISTORY_LIMIT {
                self.history.remove(0);
            }
            self.history_index = self.history.len() - 1;
        }
        self.show_dir(dir);
    }

    /// Switches the listing to `dir` without touching the history.
    fn show_dir(&mut self, dir: PathBuf) {
        self.archive = None;
        self.virtual_list = None;
        self.remote = None;
        self.cwd = dir;
        self.cursor = 0;
        self.selected.clear();
        self.filter.clear();
        self.reload();
    }

    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    /// Goes to the previously visited directory.
    pub fn back(&mut self) -> Result<(), EngineError> {
        self.step_history(-1)
    }

    /// Goes to the directory visited after the current one (after `back`).
    pub fn forward(&mut self) -> Result<(), EngineError> {
        self.step_history(1)
    }

    fn step_history(&mut self, delta: isize) -> Result<(), EngineError> {
        let target = self
            .history_index
            .checked_add_signed(delta)
            .filter(|&i| i < self.history.len())
            .ok_or_else(|| {
                EngineError(if delta < 0 {
                    "brak wcześniejszego katalogu w historii".into()
                } else {
                    "brak późniejszego katalogu w historii".into()
                })
            })?;
        let dir = self.history[target].clone();
        if !dir.is_dir() {
            return Err(EngineError(format!(
                "katalog nie istnieje: {}",
                dir.display()
            )));
        }
        self.history_index = target;
        self.show_dir(dir);
        Ok(())
    }

    /// Enters the directory under the cursor; on the `..` row goes up instead.
    pub fn enter(&mut self) {
        let Some(entry) = self.current().filter(|e| e.is_dir()) else {
            return;
        };
        if is_parent_entry(entry) {
            self.up();
        } else if let Some(view) = &self.archive {
            let inner = view.inner_path_of(entry).unwrap_or_default();
            self.set_inner(inner);
        } else {
            let dir = entry.path.clone();
            self.change_dir(dir);
        }
    }

    /// Goes to the parent directory and puts the cursor on the directory just left. Inside an
    /// archive this goes up one level, and from its top level leaves the archive.
    pub fn up(&mut self) {
        if self.virtual_list.is_some() {
            self.virtual_list = None;
            self.selected.clear();
            self.filter.clear();
            self.cursor = 0;
            self.reload();
            return;
        }
        if let Some(view) = &self.archive {
            let left = view.location();
            if view.inner.is_empty() {
                self.archive = None;
                self.selected.clear();
                self.filter.clear();
                self.cursor = 0;
                self.reload();
            } else {
                self.set_inner(parent_inner(&view.inner).to_string());
            }
            self.put_cursor_on(&left);
            return;
        }
        let Some(parent) = self.cwd.parent().map(Path::to_path_buf) else {
            return;
        };
        let left = self.cwd.clone();
        self.change_dir(parent);
        if let Some(index) = self.entries.iter().position(|e| e.path == left) {
            self.cursor = index;
        }
    }

    /// Shows `paths` (from anywhere) as one list until the panel is sent somewhere else.
    pub fn show_paths(&mut self, title: String, paths: Vec<PathBuf>) {
        self.archive = None;
        self.virtual_list = Some(VirtualList { title, paths });
        self.selected.clear();
        self.filter.clear();
        self.cursor = 0;
        self.reload();
    }

    /// Where the panel is: the directory, or `archive/inner` while browsing an archive.
    pub fn location(&self) -> PathBuf {
        if let Some(remote) = &self.remote {
            return remote.location();
        }
        self.archive
            .as_ref()
            .map_or_else(|| self.cwd.clone(), ArchiveView::location)
    }

    /// Shows the content of a server folder (a background listing finished).
    pub fn show_remote(&mut self, path: String, entries: Vec<bluepane_vfs::Entry>) {
        if let Some(remote) = &mut self.remote {
            remote.path = path;
        }
        self.listing = entries;
        self.sort_listing();
        self.selected.clear();
        self.filter.clear();
        self.cursor = 0;
        self.error = None;
        self.rebuild(None);
    }

    /// Starts browsing the inside of `archive` (which lies in `cwd`).
    pub fn open_archive(
        &mut self,
        archive: PathBuf,
        listing: Arc<Listing>,
        password: Option<String>,
    ) {
        self.archive = Some(ArchiveView {
            archive,
            listing,
            inner: String::new(),
            password,
        });
        self.cursor = 0;
        self.selected.clear();
        self.filter.clear();
        self.reload();
    }

    /// Shows new contents of the archive after it was changed. When the directory being
    /// browsed is gone, falls back to the nearest one that still exists.
    pub fn replace_listing(&mut self, listing: Arc<Listing>) {
        if let Some(view) = &mut self.archive {
            view.listing = listing;
            while !view.inner.is_empty()
                && !view.listing.find(&view.inner).is_some_and(|e| e.is_dir)
            {
                view.inner = parent_inner(&view.inner).to_string();
            }
            self.selected.clear();
            self.reload();
        }
    }

    fn set_inner(&mut self, inner: String) {
        if let Some(view) = &mut self.archive {
            view.inner = inner;
        }
        self.cursor = 0;
        self.selected.clear();
        self.filter.clear();
        self.reload();
    }

    pub fn move_cursor(&mut self, delta: i32) {
        let last = self.entries.len().saturating_sub(1) as i64;
        self.cursor = (self.cursor as i64 + i64::from(delta)).clamp(0, last) as usize;
    }

    /// Toggles selection of the entry under the cursor, then moves down (as Total Commander does).
    /// The `..` row cannot be selected; the cursor just moves down.
    pub fn toggle_select(&mut self) {
        if let Some(entry) = self.current().filter(|e| !is_parent_entry(e)) {
            let path = entry.path.clone();
            if !self.selected.remove(&path) {
                self.selected.insert(path);
            }
        }
        self.move_cursor(1);
    }

    /// A mouse click on `row`. Unlike Space, clicking never moves the cursor on.
    /// The `..` row is never selected.
    pub fn click(&mut self, row: usize, mode: ClickMode) {
        let Some(entry) = self.entries.get(row) else {
            return;
        };
        let path = entry.path.clone();
        let selectable = !is_parent_entry(entry);
        let anchor_row = self
            .anchor
            .as_ref()
            .and_then(|a| self.entries.iter().position(|e| &e.path == a))
            .unwrap_or(self.cursor);
        self.cursor = row;
        match mode {
            ClickMode::Plain => self.anchor = Some(path),
            ClickMode::Ctrl => {
                if selectable && !self.selected.remove(&path) {
                    self.selected.insert(path.clone());
                }
                self.anchor = Some(path);
            }
            ClickMode::Shift | ClickMode::CtrlShift => {
                if mode == ClickMode::Shift {
                    self.selected.clear();
                }
                let (from, to) = (anchor_row.min(row), anchor_row.max(row));
                let range: Vec<PathBuf> = self.entries[from..=to]
                    .iter()
                    .filter(|e| !is_parent_entry(e))
                    .map(|e| e.path.clone())
                    .collect();
                self.selected.extend(range);
                if self.anchor.is_none() {
                    self.anchor = self.entries.get(anchor_row).map(|e| e.path.clone());
                }
            }
        }
    }

    /// Sets the quick filter (empty text clears it). With a non-empty filter the cursor
    /// jumps to the first match.
    pub fn set_filter(&mut self, text: &str) {
        let keep = self.current().map(|e| e.path.clone());
        self.filter = text.to_string();
        if self.filter.is_empty() {
            self.rebuild(keep.as_deref());
        } else {
            self.cursor = 0;
            self.rebuild(None);
            let first_match = self.entries.iter().position(|e| !is_parent_entry(e));
            self.cursor = first_match.unwrap_or(0);
        }
    }

    /// Removes the last character of the filter. Returns false when there was no filter.
    pub fn filter_backspace(&mut self) -> bool {
        if self.filter.is_empty() {
            return false;
        }
        let mut text = self.filter.clone();
        text.pop();
        self.set_filter(&text);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bluepane_vfs::EntryKind;

    /// Panel on a temp dir holding `dir/`, `a.txt`, `b.txt`, `Alpha.md`.
    /// Rows: `..`, `dir`, `a.txt`, `Alpha.md`, `b.txt`.
    fn sample() -> (tempfile::TempDir, Panel) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("dir")).unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
        std::fs::write(tmp.path().join("Alpha.md"), "x").unwrap();
        let panel = Panel::new(tmp.path());
        (tmp, panel)
    }

    fn names(panel: &Panel) -> Vec<&str> {
        panel.entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn parent_row_comes_first() {
        let (tmp, panel) = sample();
        assert_eq!(names(&panel), ["..", "dir", "a.txt", "Alpha.md", "b.txt"]);
        assert_eq!(panel.entries[0].path, tmp.path().parent().unwrap());
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn cursor_is_clamped() {
        let (_tmp, mut panel) = sample();
        panel.move_cursor(-5);
        assert_eq!(panel.cursor, 0);
        panel.move_cursor(100);
        assert_eq!(panel.cursor, 4);
    }

    #[test]
    fn enter_and_up_round_trip() {
        let (tmp, mut panel) = sample();
        panel.move_cursor(1);
        assert_eq!(panel.current().unwrap().name, "dir");
        panel.enter();
        assert_eq!(panel.cwd, tmp.path().join("dir"));
        panel.up();
        assert_eq!(panel.cwd, tmp.path());
        assert_eq!(panel.current().unwrap().name, "dir");
    }

    #[test]
    fn enter_on_parent_row_goes_up() {
        let (tmp, mut panel) = sample();
        panel.enter();
        assert_eq!(panel.cwd, tmp.path().parent().unwrap());
    }

    #[test]
    fn parent_row_is_never_selected_or_targeted() {
        let (_tmp, mut panel) = sample();
        panel.toggle_select();
        assert!(panel.selected.is_empty());
        assert_eq!(panel.cursor, 1);
        panel.cursor = 0;
        assert!(panel.targets().is_empty());
    }

    #[test]
    fn toggle_selects_and_advances() {
        let (_tmp, mut panel) = sample();
        panel.move_cursor(2);
        panel.toggle_select();
        assert_eq!(panel.selected.len(), 1);
        assert_eq!(panel.cursor, 3);
        panel.move_cursor(-1);
        panel.toggle_select();
        assert!(panel.selected.is_empty());
    }

    #[test]
    fn targets_prefer_selection_over_cursor() {
        let (_tmp, mut panel) = sample();
        panel.move_cursor(1);
        assert_eq!(panel.targets().len(), 1);
        panel.toggle_select();
        panel.toggle_select();
        assert_eq!(panel.targets().len(), 2);
    }

    #[test]
    fn filter_matches_case_insensitively_and_keeps_parent_row() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("AL");
        assert_eq!(names(&panel), ["..", "Alpha.md"]);
        assert_eq!(panel.current().unwrap().name, "Alpha.md");
        panel.set_filter(".TXT");
        assert_eq!(names(&panel), ["..", "a.txt", "b.txt"]);
        panel.set_filter("zzz");
        assert_eq!(names(&panel), [".."]);
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn clearing_filter_keeps_cursor_on_the_same_entry() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("b.t");
        assert_eq!(panel.current().unwrap().name, "b.txt");
        panel.set_filter("");
        assert_eq!(names(&panel).len(), 5);
        assert_eq!(panel.current().unwrap().name, "b.txt");
    }

    #[test]
    fn filter_backspace_shortens_then_reports_empty() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("a.");
        assert!(panel.filter_backspace());
        assert_eq!(panel.filter, "a");
        assert!(panel.filter_backspace());
        assert!(!panel.filter_backspace());
        assert_eq!(panel.entries.len(), 5);
    }

    #[test]
    fn changing_directory_clears_filter_and_selection() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("dir");
        panel.enter_by_name("dir");
        assert!(panel.filter.is_empty());
        assert!(panel.selected.is_empty());
    }

    #[test]
    fn hidden_selection_is_not_a_target() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("a.txt");
        panel.toggle_select();
        panel.set_filter("b.txt");
        // a.txt is selected but filtered out; the cursor entry b.txt is the target.
        assert_eq!(panel.selected.len(), 1);
        let targets = panel.targets();
        assert_eq!(targets.len(), 1);
        assert!(targets[0].ends_with("b.txt"));
    }

    #[test]
    fn history_goes_back_and_forward() {
        let (tmp, mut panel) = sample();
        panel.enter_by_name("dir");
        panel.up();
        assert!(panel.can_go_back());
        assert!(!panel.can_go_forward());

        panel.back().unwrap();
        assert_eq!(panel.cwd, tmp.path().join("dir"));
        panel.back().unwrap();
        assert_eq!(panel.cwd, tmp.path());
        assert!(!panel.can_go_back());
        assert!(panel.back().is_err());

        panel.forward().unwrap();
        assert_eq!(panel.cwd, tmp.path().join("dir"));
        assert!(panel.can_go_forward());
    }

    #[test]
    fn new_navigation_drops_forward_history() {
        let (tmp, mut panel) = sample();
        panel.enter_by_name("dir");
        panel.back().unwrap();
        assert!(panel.can_go_forward());
        panel.change_dir(tmp.path().parent().unwrap().to_path_buf());
        assert!(!panel.can_go_forward());
        assert!(panel.forward().is_err());
    }

    #[test]
    fn history_skips_removed_directory() {
        let (tmp, mut panel) = sample();
        panel.enter_by_name("dir");
        panel.up();
        std::fs::remove_dir(tmp.path().join("dir")).unwrap();
        let err = panel.back().unwrap_err();
        assert!(err.0.contains("nie istnieje"));
        assert_eq!(panel.cwd, tmp.path());
    }

    fn fake(name: &str, size: u64, secs: u64, dir: bool) -> Entry {
        Entry {
            name: name.to_string(),
            path: PathBuf::from("/synthetic").join(name),
            kind: if dir { EntryKind::Dir } else { EntryKind::File },
            size,
            modified: Some(UNIX_EPOCH + std::time::Duration::from_secs(secs)),
            created: None,
            mode: None,
            owner: None,
            hidden: false,
        }
    }

    /// Panel over an in-memory listing (no file system access).
    fn synthetic(entries: Vec<Entry>) -> Panel {
        let mut panel = Panel::new("/synthetic");
        panel.listing = entries;
        panel.sort_listing();
        panel.rebuild(None);
        panel
    }

    fn mixed() -> Panel {
        synthetic(vec![
            fake("b.txt", 30, 300, false),
            fake("A.md", 10, 500, false),
            fake("c.txt", 20, 100, false),
            fake("zdir", 0, 200, true),
            fake("adir", 0, 400, true),
            fake("noext", 5, 50, false),
        ])
    }

    #[test]
    fn sorts_by_each_key_with_directories_first() {
        let mut p = mixed();
        assert_eq!(
            names(&p),
            ["..", "adir", "zdir", "A.md", "b.txt", "c.txt", "noext"]
        );
        p.set_sort(SortKey::Size, None);
        assert_eq!(
            names(&p),
            ["..", "adir", "zdir", "noext", "A.md", "c.txt", "b.txt"]
        );
        p.set_sort(SortKey::Date, None);
        assert_eq!(
            names(&p),
            ["..", "zdir", "adir", "noext", "c.txt", "b.txt", "A.md"]
        );
        p.set_sort(SortKey::Ext, None);
        assert_eq!(
            names(&p),
            ["..", "adir", "zdir", "noext", "A.md", "b.txt", "c.txt"]
        );
    }

    #[test]
    fn repeating_a_key_flips_the_direction_but_keeps_directories_on_top() {
        let mut p = mixed();
        p.set_sort(SortKey::Size, None);
        p.set_sort(SortKey::Size, None);
        assert!(p.sort.descending);
        assert_eq!(
            names(&p),
            ["..", "adir", "zdir", "b.txt", "c.txt", "A.md", "noext"]
        );
        // A different key starts ascending again; an explicit direction wins.
        p.set_sort(SortKey::Name, None);
        assert!(!p.sort.descending);
        p.set_sort(SortKey::Name, Some(true));
        assert_eq!(
            names(&p),
            ["..", "zdir", "adir", "noext", "c.txt", "b.txt", "A.md"]
        );
    }

    #[test]
    fn ties_stay_in_name_order_when_descending() {
        let mut p = synthetic(vec![
            fake("b", 7, 1, false),
            fake("a", 7, 1, false),
            fake("c", 9, 1, false),
        ]);
        p.set_sort(SortKey::Size, Some(true));
        assert_eq!(names(&p), ["..", "c", "a", "b"]);
    }

    #[test]
    fn sorting_keeps_the_cursor_on_the_same_entry() {
        let mut p = mixed();
        p.cursor = p.entries.iter().position(|e| e.name == "b.txt").unwrap();
        p.set_sort(SortKey::Size, Some(true));
        assert_eq!(p.current().unwrap().name, "b.txt");
    }

    #[test]
    fn mask_selection_and_unselection() {
        let mut p = mixed();
        let count = p.select_matching(&Mask::parse("*.txt").unwrap(), true);
        assert_eq!(count, 2);
        assert_eq!(p.selected.len(), 2);
        p.select_matching(&Mask::parse("c.*").unwrap(), false);
        assert_eq!(p.selected.len(), 1);
        assert!(p.selected.contains(&PathBuf::from("/synthetic/b.txt")));
    }

    #[test]
    fn select_all_invert_and_unselect_skip_the_parent_row() {
        let mut p = mixed();
        assert_eq!(p.select_all(), 6);
        assert_eq!(p.selected.len(), 6);
        p.invert_selection();
        assert!(p.selected.is_empty());
        p.selected.insert(PathBuf::from("/synthetic/noext"));
        p.invert_selection();
        assert_eq!(p.selected.len(), 5);
        assert!(!p.selected.contains(&PathBuf::from("/synthetic/noext")));
        assert!(!p.selected.contains(&p.entries[0].path));
        p.unselect_all();
        assert!(p.selected.is_empty());
    }

    #[test]
    fn selection_commands_only_touch_visible_entries() {
        let mut p = mixed();
        p.set_filter("txt");
        assert_eq!(p.select_all(), 2);
        p.set_filter("");
        assert_eq!(p.selected.len(), 2);
        p.set_filter("md");
        p.invert_selection();
        assert_eq!(p.selected.len(), 3);
    }

    /// Stage 3 goal: a directory with 100k files must not make the UI stall. The listing
    /// is synthetic so the test does not depend on disk speed; the bound is deliberately
    /// generous for debug builds on slow CI machines.
    #[test]
    fn hundred_thousand_entries_stay_responsive() {
        let entries: Vec<Entry> = (0..100_000u64)
            .map(|i| {
                fake(
                    &format!("File{:06}.dat", (i * 7919) % 100_000),
                    i % 977,
                    i,
                    false,
                )
            })
            .collect();
        let started = std::time::Instant::now();
        let mut p = synthetic(entries);
        p.set_sort(SortKey::Size, Some(true));
        p.set_filter("99");
        p.set_filter("");
        assert_eq!(p.select_all(), 100_000);
        p.invert_selection();
        p.select_matching(&Mask::parse("*7*.dat").unwrap(), true);
        assert!(!p.targets().is_empty());
        p.reload(); // exercises the selection clean-up for a large selection
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "100k entries took {elapsed:?}"
        );
    }

    /// Same with real files; slow to set up, so run it by hand:
    /// `cargo test -p bluepane-engine --release -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn hundred_thousand_real_files() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..100_000 {
            std::fs::write(tmp.path().join(format!("f{i:06}.txt")), "").unwrap();
        }
        let started = std::time::Instant::now();
        let mut panel = Panel::new(tmp.path());
        let listed = started.elapsed();
        panel.select_all();
        panel.reload();
        panel.set_sort(SortKey::Date, None);
        println!("list: {listed:?}, everything: {:?}", started.elapsed());
        assert_eq!(panel.entries.len(), 100_001);
    }

    /// A panel that has entered `box.zip` (containing `docs/a.txt`, `docs/sub/b.txt`,
    /// `top.txt`), plus the temp dir it lives in.
    fn in_archive() -> (tempfile::TempDir, Panel) {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join("docs/sub")).unwrap();
        std::fs::write(src.join("docs/a.txt"), "a").unwrap();
        std::fs::write(src.join("docs/sub/b.txt"), "bb").unwrap();
        std::fs::write(src.join("top.txt"), "t").unwrap();
        let archive = tmp.path().join("box.zip");
        let options = bluepane_archive::CreateOptions {
            format: bluepane_archive::Format::Zip,
            level: bluepane_archive::Level::Normal,
            password: None,
            split: None,
            encrypt_names: false,
        };
        let sources = [src.join("docs"), src.join("top.txt")];
        bluepane_archive::create(
            &sources,
            &archive,
            &options,
            &bluepane_ops::Progress::new(),
            &mut bluepane_ops::FailOnConflict,
        )
        .unwrap();
        let listing =
            bluepane_archive::list(&archive, None, &bluepane_ops::Progress::new()).unwrap();
        let mut panel = Panel::new(tmp.path());
        panel.open_archive(archive, Arc::new(listing), None);
        (tmp, panel)
    }

    #[test]
    fn an_archive_panel_shows_the_top_level_with_a_parent_row() {
        let (tmp, panel) = in_archive();
        assert_eq!(names(&panel), ["..", "docs", "top.txt"]);
        assert_eq!(
            panel.entries[0].path,
            tmp.path(),
            "leaving goes to the containing dir"
        );
        assert_eq!(panel.location(), tmp.path().join("box.zip"));
        assert_eq!(panel.entries[2].size, 1);
        assert!(panel.entries[1].is_dir());
        // The real directory is untouched.
        assert_eq!(panel.cwd, tmp.path());
    }

    #[test]
    fn entering_directories_inside_an_archive_and_going_back_up() {
        let (tmp, mut panel) = in_archive();
        panel.enter_by_name("docs");
        assert_eq!(names(&panel), ["..", "sub", "a.txt"]);
        assert_eq!(panel.location(), tmp.path().join("box.zip/docs"));
        panel.enter_by_name("sub");
        assert_eq!(names(&panel), ["..", "b.txt"]);
        panel.up();
        assert_eq!(names(&panel), ["..", "sub", "a.txt"]);
        assert_eq!(
            panel.current().unwrap().name,
            "sub",
            "the cursor returns to where we were"
        );
        panel.enter_by_name("..");
        assert_eq!(names(&panel), ["..", "docs", "top.txt"]);
        assert_eq!(panel.current().unwrap().name, "docs");
    }

    #[test]
    fn a_new_listing_keeps_the_directory_or_falls_back_to_one_that_exists() {
        let (tmp, mut panel) = in_archive();
        panel.enter_by_name("docs");
        panel.enter_by_name("sub");
        let archive = tmp.path().join("box.zip");
        let read = || {
            Arc::new(
                bluepane_archive::list(&archive, None, &bluepane_ops::Progress::new()).unwrap(),
            )
        };

        // Same contents: we stay where we are.
        panel.replace_listing(read());
        assert_eq!(panel.archive.as_ref().unwrap().inner, "docs/sub");

        // `docs` is gone: the panel falls back to the top level.
        bluepane_archive::modify(
            &archive,
            &read(),
            &bluepane_archive::Change::Remove {
                entries: vec!["docs".to_string()],
            },
            None,
            &bluepane_ops::Progress::new(),
            &mut bluepane_ops::FailOnConflict,
        )
        .unwrap();
        panel.replace_listing(read());
        assert_eq!(panel.archive.as_ref().unwrap().inner, "");
        assert_eq!(names(&panel), ["..", "top.txt"]);
    }

    #[test]
    fn going_up_from_the_top_level_leaves_the_archive() {
        let (tmp, mut panel) = in_archive();
        panel.up();
        assert!(panel.archive.is_none());
        assert_eq!(
            panel.current().unwrap().name,
            "box.zip",
            "the cursor lands on the archive"
        );
        assert!(names(&panel).contains(&"src"));
        assert_eq!(panel.location(), tmp.path());
    }

    #[test]
    fn changing_directory_or_history_leaves_the_archive() {
        let (tmp, mut panel) = in_archive();
        panel.change_dir(tmp.path().join("src"));
        assert!(panel.archive.is_none());
        let (_tmp2, mut panel2) = in_archive();
        panel2.change_dir(_tmp2.path().join("src"));
        panel2.back().unwrap();
        assert!(panel2.archive.is_none());
    }

    #[test]
    fn rows_in_an_archive_select_filter_and_sort_like_any_other() {
        let (_tmp, mut panel) = in_archive();
        panel.move_cursor(2);
        panel.toggle_select();
        assert_eq!(panel.selected.len(), 1);
        panel.set_filter("doc");
        assert_eq!(names(&panel), ["..", "docs"]);
        panel.set_filter("");
        panel.set_sort(SortKey::Size, Some(true));
        assert_eq!(
            names(&panel),
            ["..", "docs", "top.txt"],
            "directories stay first"
        );
    }

    #[test]
    fn the_inner_path_of_a_row_includes_the_directory_being_shown() {
        let (_tmp, mut panel) = in_archive();
        let view = panel.archive.clone().unwrap();
        assert_eq!(
            view.inner_path_of(&panel.entries[1]).as_deref(),
            Some("docs")
        );
        assert_eq!(view.inner_path_of(&panel.entries[0]), None);
        panel.enter_by_name("docs");
        let view = panel.archive.clone().unwrap();
        let a = panel.entries.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!(view.inner_path_of(a).as_deref(), Some("docs/a.txt"));
    }

    #[test]
    fn mouse_clicks_select_like_a_windows_list() {
        let mut p = mixed();
        let names = |p: &Panel| -> Vec<String> {
            p.entries
                .iter()
                .filter(|e| p.selected.contains(&e.path))
                .map(|e| e.name.clone())
                .collect()
        };
        // Rows: .., adir, zdir, A.md, b.txt, c.txt, noext
        p.click(3, ClickMode::Plain);
        assert_eq!(p.cursor, 3);
        assert!(p.selected.is_empty(), "a plain click only moves the cursor");

        p.click(5, ClickMode::Ctrl);
        assert_eq!(names(&p), ["c.txt"]);
        assert_eq!(p.cursor, 5, "Ctrl+click does not move on");
        p.click(5, ClickMode::Ctrl);
        assert!(p.selected.is_empty(), "Ctrl+click again unselects");

        p.click(3, ClickMode::Plain);
        p.click(5, ClickMode::Shift);
        assert_eq!(names(&p), ["A.md", "b.txt", "c.txt"]);
        p.click(4, ClickMode::Shift);
        assert_eq!(names(&p), ["A.md", "b.txt"], "Shift replaces the range");

        p.click(6, ClickMode::CtrlShift);
        assert_eq!(
            names(&p),
            ["A.md", "b.txt", "c.txt", "noext"],
            "the range from the anchor is added"
        );
    }

    #[test]
    fn the_parent_row_is_never_selected_by_clicks() {
        let mut p = mixed();
        p.click(0, ClickMode::Ctrl);
        assert!(p.selected.is_empty());
        p.click(3, ClickMode::Plain);
        p.click(0, ClickMode::Shift);
        assert_eq!(p.selected.len(), 3, "adir, zdir, A.md");
        p.click(99, ClickMode::Plain);
        assert_eq!(p.cursor, 0, "a row that is not there is ignored");
    }

    impl Panel {
        fn enter_by_name(&mut self, name: &str) {
            self.cursor = self.entries.iter().position(|e| e.name == name).unwrap();
            self.enter();
        }
    }
}
