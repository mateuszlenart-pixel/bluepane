//! Thumbnails of pictures for the grid view, made on background threads and remembered for
//! a while. Only what the window asks for (the visible tiles) is made, newest request first,
//! so scrolling through a big folder does not queue thousands of pictures.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use bluepane_docs::PageImage;

/// The longer side of a thumbnail, in pixels.
pub const THUMB_SIZE: u32 = 128;
/// How many thumbnails are kept (about 64 kB each).
const KEEP: usize = 600;
/// Requests waiting beyond this are dropped, oldest first (the user scrolled past them).
const QUEUE_MAX: usize = 96;
/// Threads making thumbnails at once.
const WORKERS: usize = 2;
/// Files bigger than this get no thumbnail.
const MAX_FILE: u64 = 64 * 1024 * 1024;

/// What is known about one file: when it was changed, and its thumbnail (`None` when it
/// could not be made, so it is not tried again until the file changes).
type Known = (Option<SystemTime>, Option<Arc<PageImage>>);

#[derive(Debug, Default)]
struct State {
    known: HashMap<PathBuf, Known>,
    /// Oldest first, for dropping when there are too many.
    order: VecDeque<PathBuf>,
    queue: VecDeque<(PathBuf, Option<SystemTime>)>,
    /// Being made right now (not queued again meanwhile).
    making: HashSet<PathBuf>,
    working: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Thumbnails {
    state: Arc<Mutex<State>>,
}

impl Thumbnails {
    /// The thumbnail of a picture, if it is ready; otherwise asks for it (never waits).
    pub fn get(&self, path: &Path, modified: Option<SystemTime>) -> Option<Arc<PageImage>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((when, thumb)) = state.known.get(path) {
            if *when == modified {
                return thumb.clone();
            }
        }
        if !state.making.contains(path) && !state.queue.iter().any(|(p, _)| p == path) {
            state.queue.push_back((path.to_path_buf(), modified));
            while state.queue.len() > QUEUE_MAX {
                state.queue.pop_front();
            }
        }
        if state.working < WORKERS {
            state.working += 1;
            let shared = self.state.clone();
            std::thread::spawn(move || work(&shared));
        }
        None
    }

    /// True while thumbnails are being made (the window keeps redrawing).
    pub fn is_working(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.working > 0 || !state.queue.is_empty()
    }
}

fn work(shared: &Arc<Mutex<State>>) {
    loop {
        // The newest request first: it is what is on screen now.
        let job = {
            let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
            match state.queue.pop_back() {
                Some(job) => {
                    state.making.insert(job.0.clone());
                    job
                }
                None => {
                    state.working -= 1;
                    return;
                }
            }
        };
        let (path, modified) = job;
        let thumb = make(&path).map(Arc::new);
        let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
        state.making.remove(&path);
        if state
            .known
            .insert(path.clone(), (modified, thumb))
            .is_none()
        {
            state.order.push_back(path);
        }
        while state.order.len() > KEEP {
            if let Some(old) = state.order.pop_front() {
                state.known.remove(&old);
            }
        }
    }
}

/// A picture scaled so that its longer side is [`THUMB_SIZE`] (smaller ones stay as they are).
fn make(path: &Path) -> Option<PageImage> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > MAX_FILE {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let picture = bluepane_docs::decode_picture(&bytes).ok()?;
    let longer = picture.width.max(picture.height).max(1);
    if longer <= THUMB_SIZE {
        return Some(picture);
    }
    let width =
        (u64::from(picture.width) * u64::from(THUMB_SIZE) / u64::from(longer)).max(1) as u32;
    Some(bluepane_docs::scale_picture(&picture, width))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(
        thumbs: &Thumbnails,
        path: &Path,
        modified: Option<SystemTime>,
    ) -> Option<Arc<PageImage>> {
        for _ in 0..2000 {
            if let Some(thumb) = thumbs.get(path, modified) {
                return Some(thumb);
            }
            if !thumbs.is_working() {
                return thumbs.get(path, modified);
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        None
    }

    #[test]
    fn a_picture_gets_a_thumbnail_that_fits_the_box_and_is_remembered() {
        let tmp = tempfile::tempdir().unwrap();
        let wide = tmp.path().join("szeroki.png");
        image::RgbaImage::from_pixel(400, 100, image::Rgba([1, 2, 3, 255]))
            .save(&wide)
            .unwrap();
        let small = tmp.path().join("maly.png");
        image::RgbaImage::from_pixel(20, 10, image::Rgba([1, 2, 3, 255]))
            .save(&small)
            .unwrap();
        let thumbs = Thumbnails::default();
        assert!(
            thumbs.get(&wide, None).is_none(),
            "the first call only asks"
        );
        let thumb = wait(&thumbs, &wide, None).expect("made in the background");
        assert_eq!((thumb.width, thumb.height), (128, 32));
        let tiny = wait(&thumbs, &small, None).unwrap();
        assert_eq!(
            (tiny.width, tiny.height),
            (20, 10),
            "small pictures are not enlarged"
        );
        // Asked again: straight from memory, the same picture.
        assert!(Arc::ptr_eq(&thumbs.get(&wide, None).unwrap(), &thumb));
    }

    #[test]
    fn a_broken_file_is_not_tried_again_until_it_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = tmp.path().join("udawany.jpg");
        std::fs::write(&fake, "nie obraz").unwrap();
        let thumbs = Thumbnails::default();
        assert!(wait(&thumbs, &fake, None).is_none());
        assert!(!thumbs.is_working());
        // A known failure does not start a new job.
        assert!(thumbs.get(&fake, None).is_none());
        assert!(!thumbs.is_working());
        // A changed file is tried again.
        let later = Some(SystemTime::now());
        assert!(thumbs.get(&fake, later).is_none());
        assert!(wait(&thumbs, &fake, later).is_none());
    }

    #[test]
    fn viewmode_switches_the_active_panel_only() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("l")).unwrap();
        std::fs::create_dir_all(tmp.path().join("r")).unwrap();
        let mut s = crate::Session::new(tmp.path().join("l"), tmp.path().join("r"));
        assert_eq!(s.panels[0].view, crate::PanelView::List);
        s.run_line("viewmode").unwrap();
        assert_eq!(s.panels[0].view, crate::PanelView::Grid);
        assert_eq!(s.panels[1].view, crate::PanelView::List);
        s.run_line("viewmode grid").unwrap();
        assert_eq!(
            s.panels[0].view,
            crate::PanelView::Grid,
            "explicit, not a toggle"
        );
        s.run_line("viewmode list").unwrap();
        assert_eq!(s.panels[0].view, crate::PanelView::List);
        assert!(s.run_line("viewmode tiles").is_err());
    }
}
