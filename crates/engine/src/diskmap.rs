//! Disk usage map (`map`): the folder shown in the active panel as tiles whose area follows the
//! size of what is inside (a squarified treemap). Folder sizes are counted in the background,
//! once, and only after the command; tiles appear as the counts arrive. Clicking a folder tile
//! goes into it (`map into`), `map up` goes back, `map show` puts the panel on the folder.
//!
//! The layout is a plain function ([`squarify`]) so it is tested without any window.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::dirsize::walk;
use crate::session::Session;
use crate::EngineError;

/// Tiles drawn at most; the smallest ones are left out (their count is reported).
const MAX_TILES: usize = 300;

/// What `map` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapAction {
    /// Map the folder of the active panel.
    Open,
    Close,
    /// Map a folder of the current map.
    Into(String),
    /// Map the parent folder.
    Up,
    /// Show the mapped folder in the active panel.
    Show,
}

/// One entry of the mapped folder; `size` is `None` while a folder is still being counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: Option<u64>,
}

/// A tile to draw, in the units of the area it was laid out in.
#[derive(Debug, Clone, PartialEq)]
pub struct MapTile {
    pub item: MapItem,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The state of an open map. The counting thread fills `items`.
#[derive(Debug)]
pub struct DiskMap {
    pub root: PathBuf,
    items: Arc<Mutex<Vec<MapItem>>>,
    running: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    version: Arc<AtomicU64>,
}

impl DiskMap {
    fn start(root: PathBuf) -> DiskMap {
        let map = DiskMap {
            root: root.clone(),
            items: Arc::default(),
            running: Arc::new(AtomicBool::new(true)),
            cancel: Arc::default(),
            version: Arc::default(),
        };
        let (items, running, cancel, version) = (
            map.items.clone(),
            map.running.clone(),
            map.cancel.clone(),
            map.version.clone(),
        );
        std::thread::spawn(move || {
            let mut listed = Vec::new();
            for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let is_dir = kind.is_dir();
                if !is_dir && !kind.is_file() {
                    continue;
                }
                listed.push(MapItem {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path(),
                    is_dir,
                    size: (!is_dir).then(|| entry.metadata().map_or(0, |m| m.len())),
                });
            }
            let folders: Vec<usize> = (0..listed.len()).filter(|&i| listed[i].is_dir).collect();
            if let Ok(mut shared) = items.lock() {
                *shared = listed.clone();
            }
            version.fetch_add(1, Ordering::Relaxed);
            for i in folders {
                let Some(total) = walk(&listed[i].path, &cancel) else {
                    break;
                };
                if let Ok(mut shared) = items.lock() {
                    shared[i].size = Some(total);
                }
                version.fetch_add(1, Ordering::Relaxed);
            }
            running.store(false, Ordering::Relaxed);
        });
        map
    }

    /// Changes whenever an item or a count arrives.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// True while folders are still being counted.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    fn snapshot(&self) -> Vec<MapItem> {
        self.items.lock().map(|i| i.clone()).unwrap_or_default()
    }
}

impl Drop for DiskMap {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Lays out `sizes` (largest first, all above zero) as rectangles filling `width` x `height`,
/// each with an area proportional to its size and as close to a square as the method gets
/// (Bruls, Huizing, van Wijk). The result is in the order of the input.
pub fn squarify(sizes: &[u64], width: f32, height: f32) -> Vec<(f32, f32, f32, f32)> {
    let total: f64 = sizes.iter().map(|&s| s as f64).sum();
    if sizes.is_empty() || total <= 0.0 || width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    let scale = f64::from(width) * f64::from(height) / total;
    let areas: Vec<f64> = sizes.iter().map(|&s| s as f64 * scale).collect();
    let mut out = vec![(0.0f32, 0.0f32, 0.0f32, 0.0f32); areas.len()];
    let (mut x, mut y) = (0.0f64, 0.0f64);
    let (mut w, mut h) = (f64::from(width), f64::from(height));
    let worst = |row: &[f64], sum: f64, side: f64| -> f64 {
        row.iter().fold(0.0f64, |acc, &a| {
            acc.max((side * side * a / (sum * sum)).max(sum * sum / (side * side * a)))
        })
    };
    let mut start = 0;
    while start < areas.len() {
        let side = w.min(h);
        let (mut end, mut sum, mut best) = (start, 0.0f64, f64::INFINITY);
        while end < areas.len() {
            let with = sum + areas[end];
            let ratio = worst(&areas[start..=end], with, side);
            if end > start && ratio > best {
                break;
            }
            best = ratio;
            sum = with;
            end += 1;
        }
        let thickness = sum / side;
        if w >= h {
            let mut at = y;
            for k in start..end {
                let tile = areas[k] / thickness;
                out[k] = (x as f32, at as f32, thickness as f32, tile as f32);
                at += tile;
            }
            x += thickness;
            w -= thickness;
        } else {
            let mut at = x;
            for k in start..end {
                let tile = areas[k] / thickness;
                out[k] = (at as f32, y as f32, tile as f32, thickness as f32);
                at += tile;
            }
            y += thickness;
            h -= thickness;
        }
        start = end;
    }
    out
}

impl Session {
    /// The tiles of the open map laid out in `width` x `height`, the biggest first, and how
    /// many smaller entries (or still uncounted folders) were left out.
    pub fn map_tiles(&self, width: f32, height: f32) -> (Vec<MapTile>, usize) {
        let Some(map) = &self.diskmap else {
            return (Vec::new(), 0);
        };
        let mut items = map.snapshot();
        let uncounted = items.iter().filter(|i| i.size.is_none()).count();
        items.retain(|i| i.size.is_some_and(|s| s > 0));
        items.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        let left_out = items.len().saturating_sub(MAX_TILES) + uncounted;
        items.truncate(MAX_TILES);
        let sizes: Vec<u64> = items.iter().map(|i| i.size.unwrap_or(0)).collect();
        let tiles = squarify(&sizes, width, height)
            .into_iter()
            .zip(items)
            .map(|((x, y, w, h), item)| MapTile { item, x, y, w, h })
            .collect();
        (tiles, left_out)
    }

    /// The total of the counted entries and how many folders are still being counted.
    pub fn map_summary(&self) -> Option<(u64, usize)> {
        let map = self.diskmap.as_ref()?;
        let items = map.snapshot();
        let total = items.iter().filter_map(|i| i.size).sum();
        Some((total, items.iter().filter(|i| i.size.is_none()).count()))
    }

    fn map_start(&mut self, root: PathBuf) {
        self.diskmap = Some(DiskMap::start(root.clone()));
        self.status = format!("Mapa zajętości: liczę {}…", root.display());
    }

    pub(crate) fn map_action(&mut self, action: MapAction) -> Result<(), EngineError> {
        match action {
            MapAction::Open => {
                let panel = self.active_panel();
                if panel.archive.is_some() || panel.remote.is_some() {
                    return Err(EngineError(
                        "mapa zajętości działa tylko na katalogu na dysku".into(),
                    ));
                }
                let root = panel.location();
                self.map_start(root);
            }
            MapAction::Close => {
                self.diskmap = None;
            }
            MapAction::Into(name) => {
                let map = self
                    .diskmap
                    .as_ref()
                    .ok_or_else(|| EngineError("mapa nie jest otwarta (komenda map)".into()))?;
                let target = map
                    .snapshot()
                    .into_iter()
                    .find(|i| i.is_dir && i.name == name.trim())
                    .ok_or_else(|| {
                        EngineError(format!("na mapie nie ma katalogu „{}”", name.trim()))
                    })?;
                self.map_start(target.path);
            }
            MapAction::Up => {
                let map = self
                    .diskmap
                    .as_ref()
                    .ok_or_else(|| EngineError("mapa nie jest otwarta (komenda map)".into()))?;
                let parent = map
                    .root
                    .parent()
                    .map(PathBuf::from)
                    .ok_or_else(|| EngineError("to jest katalog główny".into()))?;
                self.map_start(parent);
            }
            MapAction::Show => {
                let root = self
                    .diskmap
                    .as_ref()
                    .map(|m| m.root.clone())
                    .ok_or_else(|| EngineError("mapa nie jest otwarta (komenda map)".into()))?;
                self.run_line(&format!("cd {}", root.display()))?;
            }
        }
        Ok(())
    }
}

/// `map [close|into <katalog>|up|show]`
pub(crate) fn parse_map(rest: &str) -> Result<MapAction, EngineError> {
    let rest = rest.trim();
    let (verb, tail) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(v, t)| (v, t.trim()));
    Ok(match verb.to_ascii_lowercase().as_str() {
        "" | "open" => MapAction::Open,
        "close" => MapAction::Close,
        "into" => MapAction::Into(tail.to_string()),
        "up" => MapAction::Up,
        "show" => MapAction::Show,
        other => {
            return Err(EngineError(format!(
                "map: nieznane „{other}” (close, into, up, show)"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(r: (f32, f32, f32, f32)) -> f32 {
        r.2 * r.3
    }

    #[test]
    fn tiles_fill_the_area_in_proportion_without_overlapping() {
        let sizes = [600u64, 300, 200, 100, 100, 50, 50, 1];
        let rects = squarify(&sizes, 400.0, 300.0);
        assert_eq!(rects.len(), sizes.len());
        let total: f32 = rects.iter().map(|&r| area(r)).sum();
        assert!((total - 400.0 * 300.0).abs() < 1.0, "{total}");
        let sum: u64 = sizes.iter().sum();
        for (r, s) in rects.iter().zip(sizes) {
            let want = 400.0 * 300.0 * s as f32 / sum as f32;
            assert!((area(*r) - want).abs() < 1.0, "{r:?} {want}");
            assert!(r.0 >= -0.01 && r.1 >= -0.01 && r.0 + r.2 <= 400.01 && r.1 + r.3 <= 300.01);
        }
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let overlap_w = (a.0 + a.2).min(b.0 + b.2) - a.0.max(b.0);
                let overlap_h = (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1);
                assert!(overlap_w <= 0.01 || overlap_h <= 0.01, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn degenerate_input_gives_no_tiles_and_a_single_tile_fills_everything() {
        assert!(squarify(&[], 10.0, 10.0).is_empty());
        assert!(squarify(&[5], 0.0, 10.0).is_empty());
        assert!(squarify(&[0, 0], 10.0, 10.0).is_empty());
        let one = squarify(&[42], 120.0, 80.0);
        assert_eq!(one.len(), 1);
        assert!((one[0].2 - 120.0).abs() < 0.01 && (one[0].3 - 80.0).abs() < 0.01);
        // Many equal tiles stay roughly square.
        let rects = squarify(&[10; 16], 400.0, 400.0);
        for r in rects {
            assert!((r.2 / r.3 - 1.0).abs() < 0.5, "{r:?}");
        }
    }

    fn session(tmp: &std::path::Path) -> Session {
        std::fs::create_dir_all(tmp.join("l/Zażółć/głębiej")).unwrap();
        std::fs::create_dir_all(tmp.join("l/pusty")).unwrap();
        std::fs::create_dir_all(tmp.join("r")).unwrap();
        std::fs::write(tmp.join("l/duży.bin"), vec![0u8; 6000]).unwrap();
        std::fs::write(tmp.join("l/Zażółć/a.bin"), vec![0u8; 3000]).unwrap();
        std::fs::write(tmp.join("l/Zażółć/głębiej/b.bin"), vec![0u8; 2000]).unwrap();
        Session::new(tmp.join("l"), tmp.join("r"))
    }

    fn settle(s: &Session) {
        for _ in 0..500 {
            if !s.diskmap.as_ref().unwrap().is_running() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the map did not finish");
    }

    #[test]
    fn the_map_counts_folders_and_files_and_goes_into_a_folder_and_back() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("map").unwrap();
        settle(&s);
        let (tiles, left_out) = s.map_tiles(300.0, 200.0);
        // The empty folder has no area; it is not a tile and not "left out" either.
        assert_eq!(left_out, 0);
        let names: Vec<&str> = tiles.iter().map(|t| t.item.name.as_str()).collect();
        assert_eq!(names, ["duży.bin", "Zażółć"]);
        assert_eq!(tiles[1].item.size, Some(5000));
        assert_eq!(s.map_summary(), Some((11_000, 0)));

        s.run_line("map into Zażółć").unwrap();
        settle(&s);
        let (tiles, _) = s.map_tiles(300.0, 200.0);
        assert_eq!(tiles[0].item.name, "a.bin");
        assert!(s.run_line("map into nie ma").is_err());

        s.run_line("map up").unwrap();
        settle(&s);
        assert_eq!(s.diskmap.as_ref().unwrap().root, tmp.path().join("l"));

        s.run_line("map into Zażółć").unwrap();
        s.run_line("map show").unwrap();
        assert!(s.active_panel().location().ends_with("Zażółć"));
        s.run_line("map close").unwrap();
        assert!(s.diskmap.is_none());
        assert!(s.run_line("map up").is_err());
    }

    #[test]
    fn a_folder_emptied_after_the_scan_gives_an_empty_map() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("map").unwrap();
        settle(&s);
        std::fs::remove_dir_all(tmp.path().join("l/Zażółć")).unwrap();
        s.run_line("map into pusty").unwrap();
        settle(&s);
        let (tiles, left_out) = s.map_tiles(100.0, 100.0);
        assert!(tiles.is_empty() && left_out == 0);
    }
}
