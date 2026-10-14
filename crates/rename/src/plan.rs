use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use regex::{NoExpand, Regex, RegexBuilder};

use crate::mask::{Mask, Source};

/// Local time of a file's modification, already broken down by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// A file or directory to rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub path: PathBuf,
    pub is_dir: bool,
    pub modified: Option<Stamp>,
}

/// Letter case applied after the mask and the search and replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseMode {
    #[default]
    Unchanged,
    Upper,
    Lower,
    /// Every word starts with a capital letter.
    Words,
    /// Only the first letter is a capital.
    Sentence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Mask for the name, e.g. `[N]` or `photo_[C:3]`.
    pub name_mask: String,
    /// Mask for the extension, e.g. `[E]`.
    pub ext_mask: String,
    pub find: String,
    pub replace: String,
    /// `find` is a regular expression (`$1` in `replace` refers to groups).
    pub regex: bool,
    /// Replace only the first match.
    pub once: bool,
    /// Search the extension too (the whole `name.ext`).
    pub include_ext: bool,
    pub ignore_case: bool,
    pub case: CaseMode,
    pub counter_start: i64,
    pub counter_step: i64,
    pub counter_digits: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            name_mask: "[N]".into(),
            ext_mask: "[E]".into(),
            find: String::new(),
            replace: String::new(),
            regex: false,
            once: false,
            include_ext: false,
            ignore_case: false,
            case: CaseMode::Unchanged,
            counter_start: 1,
            counter_step: 1,
            counter_digits: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// The new name equals the old one.
    Unchanged,
    Ready,
    /// Cannot be applied; the text says why (in Polish, ready to show).
    Conflict(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub from: PathBuf,
    pub old_name: String,
    pub new_name: String,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub rows: Vec<Row>,
}

impl Plan {
    pub fn changes(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.status == Status::Ready)
            .count()
    }

    pub fn conflicts(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r.status, Status::Conflict(_)))
            .count()
    }
}

/// Builds the new names. `overrides` holds names the user typed by hand for single items
/// (keyed by the current path); they replace whatever the settings produce.
///
/// Errors are about the settings themselves (a broken mask or regular expression).
pub fn plan(
    items: &[Item],
    settings: &Settings,
    overrides: &HashMap<PathBuf, String>,
) -> Result<Plan, String> {
    let name_mask = Mask::parse(&settings.name_mask)?;
    let ext_mask = Mask::parse(&settings.ext_mask)?;
    let finder = Finder::new(settings)?;
    // `[C10+2:4]` in a mask overrides the form's counter.
    let spec = name_mask
        .counter_spec()
        .or_else(|| ext_mask.counter_spec())
        .unwrap_or_default();
    let start = spec.start.unwrap_or(settings.counter_start);
    let step = spec.step.unwrap_or(settings.counter_step);
    let digits = spec.digits.unwrap_or(settings.counter_digits);

    let mut rows = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let old_name = file_name(item);
        let new_name = match overrides.get(&item.path) {
            Some(text) => text.clone(),
            None => {
                let counter = start.saturating_add(step.saturating_mul(index as i64));
                build_name(
                    item,
                    &old_name,
                    settings,
                    (&name_mask, &ext_mask, &finder),
                    (counter, digits),
                )
            }
        };
        rows.push(Row {
            from: item.path.clone(),
            old_name,
            new_name,
            status: Status::Ready,
        });
    }
    check(&mut rows);
    Ok(Plan { rows })
}

fn file_name(item: &Item) -> String {
    item.path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn split_name(name: &str, is_dir: bool) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && !is_dir => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

fn build_name(
    item: &Item,
    old_name: &str,
    settings: &Settings,
    (name_mask, ext_mask, finder): (&Mask, &Mask, &Finder),
    (counter, counter_digits): (i64, usize),
) -> String {
    let (stem, ext) = split_name(old_name, item.is_dir);
    let parent = item
        .path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let source = Source {
        name: stem,
        ext,
        parent: &parent,
        modified: item.modified.as_ref(),
        counter,
        counter_digits,
    };
    let mut name = name_mask.render(&source);
    // Directories have no extension: the name mask is all there is.
    let mut ext = if item.is_dir {
        String::new()
    } else {
        ext_mask.render(&source)
    };

    if settings.include_ext && !ext.is_empty() {
        let joined = finder.replace(&format!("{name}.{ext}"));
        let (n, e) = split_name(&joined, false);
        (name, ext) = (n.to_string(), e.to_string());
    } else {
        name = finder.replace(&name);
    }
    name = apply_case(&name, settings.case);
    ext = apply_case(&ext, settings.case);
    if ext.is_empty() {
        name
    } else {
        format!("{name}.{ext}")
    }
}

struct Finder {
    regex: Option<Regex>,
    literal_replacement: bool,
    replacement: String,
    once: bool,
}

impl Finder {
    fn new(settings: &Settings) -> Result<Finder, String> {
        let regex = if settings.find.is_empty() {
            None
        } else {
            let pattern = if settings.regex {
                settings.find.clone()
            } else {
                regex::escape(&settings.find)
            };
            Some(
                RegexBuilder::new(&pattern)
                    .case_insensitive(settings.ignore_case)
                    .build()
                    .map_err(|e| format!("niepoprawne wyrażenie regularne: {e}"))?,
            )
        };
        Ok(Finder {
            regex,
            literal_replacement: !settings.regex,
            replacement: settings.replace.clone(),
            once: settings.once,
        })
    }

    fn replace(&self, text: &str) -> String {
        let Some(regex) = &self.regex else {
            return text.to_string();
        };
        let limit = if self.once { 1 } else { 0 };
        if self.literal_replacement {
            regex
                .replacen(text, limit, NoExpand(&self.replacement))
                .into_owned()
        } else {
            regex
                .replacen(text, limit, self.replacement.as_str())
                .into_owned()
        }
    }
}

fn apply_case(text: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Unchanged => text.to_string(),
        CaseMode::Upper => text.to_uppercase(),
        CaseMode::Lower => text.to_lowercase(),
        CaseMode::Words => {
            let mut out = String::with_capacity(text.len());
            let mut start = true;
            for c in text.chars() {
                if c.is_alphanumeric() {
                    if start {
                        out.extend(c.to_uppercase());
                    } else {
                        out.extend(c.to_lowercase());
                    }
                    start = false;
                } else {
                    out.push(c);
                    start = true;
                }
            }
            out
        }
        CaseMode::Sentence => {
            let mut chars = text.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect(),
                None => String::new(),
            }
        }
    }
}

/// Marks unchanged rows and every row that cannot be applied.
fn check(rows: &mut [Row]) {
    // Names that end up in each directory, compared case-insensitively so a plan that
    // works on Linux does not break on a case-insensitive Windows or macOS volume.
    let mut taken: HashMap<(PathBuf, String), usize> = HashMap::new();
    for row in rows.iter() {
        let key = (parent_of(&row.from), row.new_name.to_lowercase());
        *taken.entry(key).or_default() += 1;
    }
    let sources: HashSet<(PathBuf, String)> = rows
        .iter()
        .map(|r| (parent_of(&r.from), r.old_name.to_lowercase()))
        .collect();

    for row in rows.iter_mut() {
        if row.new_name == row.old_name {
            row.status = Status::Unchanged;
            continue;
        }
        let parent = parent_of(&row.from);
        let lower = row.new_name.to_lowercase();
        let problem = if row.new_name.is_empty() {
            Some("nazwa jest pusta".to_string())
        } else if !bluepane_ops::is_valid_name(&row.new_name) {
            Some(format!("niedozwolona nazwa: {}", row.new_name))
        } else if taken.get(&(parent.clone(), lower.clone())).copied() > Some(1) {
            Some("ta sama nazwa powstaje dla kilku plików".to_string())
        } else if !sources.contains(&(parent.clone(), lower))
            && fs::symlink_metadata(parent.join(&row.new_name)).is_ok()
        {
            Some(format!("{} już istnieje", row.new_name))
        } else {
            None
        };
        row.status = match problem {
            Some(text) => Status::Conflict(text),
            None => Status::Ready,
        };
    }
}

fn parent_of(path: &std::path::Path) -> PathBuf {
    path.parent().map(PathBuf::from).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn item(path: &str) -> Item {
        Item {
            path: PathBuf::from(path),
            is_dir: false,
            modified: Some(Stamp {
                year: 2026,
                month: 10,
                day: 14,
                hour: 8,
                minute: 30,
                second: 0,
            }),
        }
    }

    fn names(plan: &Plan) -> Vec<&str> {
        plan.rows.iter().map(|r| r.new_name.as_str()).collect()
    }

    fn run(paths: &[&str], settings: &Settings) -> Plan {
        let items: Vec<Item> = paths.iter().map(|p| item(p)).collect();
        plan(&items, settings, &HashMap::new()).unwrap()
    }

    #[test]
    fn default_settings_change_nothing() {
        let p = run(
            &["/nonexistent/a.txt", "/nonexistent/b"],
            &Settings::default(),
        );
        assert_eq!(p.changes(), 0);
        assert!(p.rows.iter().all(|r| r.status == Status::Unchanged));
    }

    #[test]
    fn masks_and_counter_build_new_names() {
        let s = Settings {
            name_mask: "photo_[C]".into(),
            counter_digits: 3,
            counter_start: 5,
            counter_step: 5,
            ..Settings::default()
        };
        let p = run(&["/nonexistent/x.jpg", "/nonexistent/y.JPG"], &s);
        assert_eq!(names(&p), ["photo_005.jpg", "photo_010.JPG"]);
        assert_eq!(p.changes(), 2);
    }

    #[test]
    fn counter_in_the_mask_overrides_the_form() {
        let s = Settings {
            name_mask: "[N]-[C100+10:4]".into(),
            ..Settings::default()
        };
        let p = run(&["/nonexistent/a.txt", "/nonexistent/b.txt"], &s);
        assert_eq!(names(&p), ["a-0100.txt", "b-0110.txt"]);
    }

    #[test]
    fn dates_come_from_the_item() {
        let s = Settings {
            name_mask: "[YMD] [N]".into(),
            ..Settings::default()
        };
        let p = run(&["/nonexistent/a.txt"], &s);
        assert_eq!(names(&p), ["2026-10-14 a.txt"]);
    }

    #[test]
    fn plain_search_and_replace_is_literal() {
        let s = Settings {
            find: "a.b".into(),
            replace: "$1".into(),
            ..Settings::default()
        };
        let p = run(&["/nonexistent/xa.byz.txt", "/nonexistent/aXb.txt"], &s);
        assert_eq!(names(&p), ["x$1yz.txt", "aXb.txt"]);
    }

    #[test]
    fn regex_replace_supports_groups_and_first_match_only() {
        let s = Settings {
            find: r"(\d+)-(\d+)".into(),
            replace: "$2-$1".into(),
            regex: true,
            ..Settings::default()
        };
        let p = run(&["/nonexistent/12-34 56-78.txt"], &s);
        assert_eq!(names(&p), ["34-12 78-56.txt"]);
        let once = Settings { once: true, ..s };
        let p = run(&["/nonexistent/12-34 56-78.txt"], &once);
        assert_eq!(names(&p), ["34-12 56-78.txt"]);
    }

    #[test]
    fn search_ignores_the_extension_unless_asked() {
        let s = Settings {
            find: "txt".into(),
            replace: "md".into(),
            ..Settings::default()
        };
        assert_eq!(names(&run(&["/nonexistent/txt.txt"], &s)), ["md.txt"]);
        let s = Settings {
            include_ext: true,
            ..s
        };
        assert_eq!(names(&run(&["/nonexistent/txt.txt"], &s)), ["md.md"]);
    }

    #[test]
    fn case_modes() {
        let go = |case, path: &str| {
            let s = Settings {
                case,
                ..Settings::default()
            };
            names(&run(&[path], &s))[0].to_string()
        };
        assert_eq!(go(CaseMode::Upper, "/n/my file.Txt"), "MY FILE.TXT");
        assert_eq!(go(CaseMode::Lower, "/n/My FILE.Txt"), "my file.txt");
        assert_eq!(
            go(CaseMode::Words, "/n/my-FILE name.txt"),
            "My-File Name.Txt"
        );
        assert_eq!(go(CaseMode::Sentence, "/n/mY fILE.TXT"), "My file.Txt");
    }

    #[test]
    fn directories_keep_dots_in_their_names() {
        let mut dir = item("/nonexistent/v1.2");
        dir.is_dir = true;
        let s = Settings {
            name_mask: "[N]_old".into(),
            ..Settings::default()
        };
        let p = plan(&[dir], &s, &HashMap::new()).unwrap();
        assert_eq!(names(&p), ["v1.2_old"]);
    }

    #[test]
    fn broken_settings_are_reported() {
        let bad_mask = Settings {
            name_mask: "[N".into(),
            ..Settings::default()
        };
        assert!(plan(&[], &bad_mask, &HashMap::new()).is_err());
        let bad_regex = Settings {
            find: "(".into(),
            regex: true,
            ..Settings::default()
        };
        assert!(plan(&[], &bad_regex, &HashMap::new()).is_err());
    }

    #[test]
    fn collisions_and_bad_names_are_conflicts() {
        let s = Settings {
            name_mask: "same".into(),
            ..Settings::default()
        };
        let p = run(&["/nonexistent/a.txt", "/nonexistent/b.txt"], &s);
        assert_eq!(p.conflicts(), 2);

        let s = Settings {
            name_mask: "a/b".into(),
            ..Settings::default()
        };
        let p = run(&["/nonexistent/a.txt"], &s);
        assert!(matches!(p.rows[0].status, Status::Conflict(_)));

        let s = Settings {
            name_mask: String::new(),
            ext_mask: String::new(),
            ..Settings::default()
        };
        let p = run(&["/nonexistent/a.txt"], &s);
        assert!(matches!(&p.rows[0].status, Status::Conflict(t) if t.contains("pusta")));
    }

    #[test]
    fn a_new_name_equal_to_an_unchanged_sibling_is_a_conflict() {
        // `a` stays `a` while `b` wants to become `A` (a clash on case-insensitive disks).
        let mut overrides = HashMap::new();
        overrides.insert(PathBuf::from("/nonexistent/b"), "A".to_string());
        let items = [item("/nonexistent/a"), item("/nonexistent/b")];
        let p = plan(&items, &Settings::default(), &overrides).unwrap();
        assert_eq!(p.rows[0].status, Status::Unchanged);
        assert!(matches!(p.rows[1].status, Status::Conflict(_)));
    }

    #[test]
    fn existing_files_outside_the_selection_are_conflicts() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.txt"), "a").unwrap();
        fs::write(tmp.path().join("taken.txt"), "t").unwrap();
        let s = Settings {
            name_mask: "taken".into(),
            ..Settings::default()
        };
        let p = run(&[tmp.path().join("a.txt").to_str().unwrap()], &s);
        assert!(matches!(&p.rows[0].status, Status::Conflict(t) if t.contains("istnieje")));
    }

    #[test]
    fn swaps_inside_the_selection_are_allowed() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        fs::write(&a, "a").unwrap();
        fs::write(&b, "b").unwrap();
        let mut overrides = HashMap::new();
        overrides.insert(a.clone(), "b".to_string());
        overrides.insert(b.clone(), "a".to_string());
        let items = [item(a.to_str().unwrap()), item(b.to_str().unwrap())];
        let p = plan(&items, &Settings::default(), &overrides).unwrap();
        assert_eq!(p.changes(), 2);
        assert_eq!(Path::new(&p.rows[0].from), a);
    }

    #[test]
    fn manual_names_win_over_the_settings() {
        let mut overrides = HashMap::new();
        overrides.insert(
            PathBuf::from("/nonexistent/a.txt"),
            "custom.dat".to_string(),
        );
        let s = Settings {
            name_mask: "x".into(),
            ..Settings::default()
        };
        let items = [item("/nonexistent/a.txt"), item("/nonexistent/b.txt")];
        let p = plan(&items, &s, &overrides).unwrap();
        assert_eq!(names(&p), ["custom.dat", "x.txt"]);
    }
}
