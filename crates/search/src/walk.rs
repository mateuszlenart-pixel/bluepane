//! Walking the roots and handing out hits.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bluepane_archive::{detect, list, visit, ArchiveError};
use bluepane_ops::Progress;
use bluepane_vfs::EntryKind;
use regex::Regex;

use crate::matcher::{mask_regex, name_matcher, text_matcher};
use crate::text::scan;
use crate::{Hit, Location, Query, Summary};

struct Search<'a> {
    query: &'a Query,
    name: Option<Regex>,
    exclude: Option<Regex>,
    text: Option<Regex>,
    progress: &'a Progress,
    summary: Summary,
    stop: bool,
}

/// Runs the search. `each` gets every hit as it is found. The result is an error only for
/// a query that cannot run (no roots, nothing to look for, a broken pattern); trouble with
/// single folders, files and archives is counted in the [`Summary`] instead.
pub fn search(
    query: &Query,
    progress: &Progress,
    each: &mut dyn FnMut(Hit),
) -> Result<Summary, String> {
    if query.roots.is_empty() {
        return Err("nie wskazano, gdzie szukać".into());
    }
    let text_given = query.text.as_ref().is_some_and(|t| !t.pattern.is_empty());
    if query.name.trim().is_empty() && !text_given {
        return Err("podaj nazwę (maskę) albo szukany tekst".into());
    }
    let mut search = Search {
        query,
        name: name_matcher(&query.name, query.name_regex, query.name_case_sensitive)?,
        exclude: mask_regex(&query.exclude_dirs, false)?,
        text: match &query.text {
            Some(text) if !text.pattern.is_empty() => Some(text_matcher(text)?),
            _ => None,
        },
        progress,
        summary: Summary::default(),
        stop: false,
    };
    let roots = distinct_roots(&query.roots);
    for root in roots {
        if search.stop {
            break;
        }
        search.root(&root, each);
    }
    search.summary.cancelled |= progress.is_cancelled();
    Ok(search.summary)
}

/// Drops roots that lie inside another root (they would be searched twice).
fn distinct_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = Vec::new();
    for root in roots {
        if kept.iter().any(|k| root.starts_with(k)) {
            continue;
        }
        kept.retain(|k| !k.starts_with(root));
        kept.push(root.clone());
    }
    kept
}

impl Search<'_> {
    fn root(&mut self, root: &Path, each: &mut dyn FnMut(Hit)) {
        let Ok(meta) = fs::symlink_metadata(root) else {
            self.summary.unreadable += 1;
            return;
        };
        if !meta.is_dir() {
            self.file(root, meta.len(), meta.modified().ok(), each);
            return;
        }
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            if self.should_stop() {
                return;
            }
            self.progress.set_current(&dir.display().to_string());
            let Ok(entries) = bluepane_vfs::list_dir(&dir) else {
                self.summary.unreadable += 1;
                continue;
            };
            self.summary.folders += 1;
            let mut subdirs = Vec::new();
            for entry in entries {
                if self.should_stop() {
                    return;
                }
                if entry.hidden && !self.query.include_hidden {
                    continue;
                }
                match entry.kind {
                    EntryKind::Dir => {
                        // Links to folders are listed but never entered (no loops).
                        let is_link = fs::symlink_metadata(&entry.path)
                            .map(|m| m.is_symlink())
                            .unwrap_or(true);
                        // A size limit is about files: folders then stay out of the results.
                        let size_limited =
                            self.query.min_size.is_some() || self.query.max_size.is_some();
                        if self.name_matches(&entry.name)
                            && self.text.is_none()
                            && !size_limited
                            && self.attributes_match(0, entry.modified)
                        {
                            self.emit(
                                Hit {
                                    location: Location::File(entry.path.clone()),
                                    is_dir: true,
                                    size: 0,
                                    modified: entry.modified,
                                    matching_lines: 0,
                                    lines: Vec::new(),
                                },
                                each,
                            );
                        }
                        let excluded = self
                            .exclude
                            .as_ref()
                            .is_some_and(|re| re.is_match(&entry.name));
                        if !is_link && !excluded {
                            subdirs.push(entry.path);
                        }
                    }
                    EntryKind::File => {
                        self.file(&entry.path, entry.size, entry.modified, each);
                    }
                    EntryKind::Symlink | EntryKind::Other => {}
                }
            }
            // Keep the listing order (the stack pops from the end).
            stack.extend(subdirs.into_iter().rev());
        }
    }

    fn should_stop(&mut self) -> bool {
        if self.progress.is_cancelled() {
            self.stop = true;
        }
        self.stop
    }

    fn name_matches(&self, name: &str) -> bool {
        self.name.as_ref().is_none_or(|re| re.is_match(name))
    }

    fn attributes_match(&self, size: u64, modified: Option<SystemTime>) -> bool {
        attributes_ok(self.query, size, modified)
    }

    fn emit(&mut self, hit: Hit, each: &mut dyn FnMut(Hit)) {
        self.summary.hits += 1;
        each(hit);
        let limit = self.query.max_results;
        if limit > 0 && self.summary.hits >= limit as u64 {
            self.summary.truncated = true;
            self.stop = true;
        }
    }

    fn file(
        &mut self,
        path: &Path,
        size: u64,
        modified: Option<SystemTime>,
        each: &mut dyn FnMut(Hit),
    ) {
        self.summary.files += 1;
        self.progress.finish_item();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_archive = self.query.archives && detect(path).is_some();
        if self.name_matches(&name) && self.attributes_match(size, modified) {
            match &self.text {
                None => self.emit(
                    Hit {
                        location: Location::File(path.to_path_buf()),
                        is_dir: false,
                        size,
                        modified,
                        matching_lines: 0,
                        lines: Vec::new(),
                    },
                    each,
                ),
                // The raw bytes of an archive are not text: its files are searched below.
                Some(_) if is_archive => {}
                Some(matcher) => {
                    let matcher = matcher.clone();
                    match self.scan_file(path, &matcher) {
                        Ok(found) if found.binary => self.summary.binary += 1,
                        Ok(found) if found.matching_lines > 0 => self.emit(
                            Hit {
                                location: Location::File(path.to_path_buf()),
                                is_dir: false,
                                size,
                                modified,
                                matching_lines: found.matching_lines,
                                lines: found.lines,
                            },
                            each,
                        ),
                        Ok(_) => {}
                        Err(_) => self.summary.unreadable += 1,
                    }
                }
            }
        }
        if is_archive && !self.should_stop() {
            self.archive(path, each);
        }
    }

    fn scan_file(&self, path: &Path, matcher: &Regex) -> std::io::Result<crate::text::Scan> {
        let query = self.query.text.as_ref().expect("text query");
        let mut file = fs::File::open(path)?;
        let progress = self.progress;
        scan(&mut file, query, matcher, &|| progress.is_cancelled())
    }

    fn archive(&mut self, archive: &Path, each: &mut dyn FnMut(Hit)) {
        self.summary.archives += 1;
        self.progress.set_current(&archive.display().to_string());
        let password = self.query.archive_password.clone();
        let text = self.text.clone();
        let max_entry = self.query.max_archive_entry;
        let mut hits: Vec<Hit> = Vec::new();
        let mut binary = 0;
        let outcome = {
            let (query, progress) = (self.query, self.progress);
            let name = self.name.clone();
            let mut found = |entry: &bluepane_archive::VisitedEntry, data: &mut dyn Read| {
                let file_name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
                if !name.as_ref().is_none_or(|re| re.is_match(file_name)) {
                    return Ok(());
                }
                if !attributes_ok(query, entry.size, entry.modified) {
                    return Ok(());
                }
                let location = Location::InArchive {
                    archive: archive.to_path_buf(),
                    inner: entry.path.clone(),
                };
                match &text {
                    None => hits.push(Hit {
                        location,
                        is_dir: false,
                        size: entry.size,
                        modified: entry.modified,
                        matching_lines: 0,
                        lines: Vec::new(),
                    }),
                    Some(matcher) => {
                        if !entry.readable || entry.size > max_entry {
                            return Ok(());
                        }
                        let text_query = query.text.as_ref().expect("text query");
                        match scan(data, text_query, matcher, &|| progress.is_cancelled()) {
                            Ok(found) if found.binary => binary += 1,
                            Ok(found) if found.matching_lines > 0 => hits.push(Hit {
                                location,
                                is_dir: false,
                                size: entry.size,
                                modified: entry.modified,
                                matching_lines: found.matching_lines,
                                lines: found.lines,
                            }),
                            // A damaged entry must not hide the rest of the archive.
                            _ => {}
                        }
                    }
                }
                Ok(())
            };
            visit(archive, password.as_deref(), self.progress, &mut found)
        };
        self.summary.binary += binary;
        match outcome {
            Ok(report) => self.summary.encrypted += report.unreadable,
            Err(ArchiveError::Cancelled) => self.stop = true,
            Err(ArchiveError::NeedsPassword | ArchiveError::WrongPassword) => {
                self.summary.encrypted += 1;
                // Names may still be readable (7z with visible names); content is not.
                if text.is_none() {
                    if let Ok(listing) = list(archive, password.as_deref(), self.progress) {
                        for entry in listing.entries.iter().filter(|e| !e.is_dir && !e.special) {
                            let file_name = entry.name();
                            if self.name_matches(file_name)
                                && self.attributes_match(entry.size, entry.modified)
                            {
                                hits.push(Hit {
                                    location: Location::InArchive {
                                        archive: archive.to_path_buf(),
                                        inner: entry.path.clone(),
                                    },
                                    is_dir: false,
                                    size: entry.size,
                                    modified: entry.modified,
                                    matching_lines: 0,
                                    lines: Vec::new(),
                                });
                            }
                        }
                    }
                }
            }
            Err(_) => self.summary.unreadable += 1,
        }
        for hit in hits {
            if self.stop {
                break;
            }
            self.emit(hit, each);
        }
    }
}

fn attributes_ok(query: &Query, size: u64, modified: Option<SystemTime>) -> bool {
    if query.min_size.is_some_and(|min| size < min) || query.max_size.is_some_and(|max| size > max)
    {
        return false;
    }
    if query.modified_after.is_some() || query.modified_before.is_some() {
        let Some(time) = modified else {
            return false;
        };
        if query.modified_after.is_some_and(|after| time < after)
            || query.modified_before.is_some_and(|before| time > before)
        {
            return false;
        }
    }
    true
}
