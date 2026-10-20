//! The tools that open beside the panels (viewer, pages, synchronization, search, rename,
//! scanner, queue, sites) as one list. A front end asks here which tools are open, what they
//! are called and how each one is closed, instead of knowing every tool's state field.

use crate::session::Session;

/// One of the tool windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    /// A document as pages (F3 on a PDF or Office file).
    Pages,
    /// A file as text or hex (F3).
    Viewer,
    Sync,
    Scan,
    Queue,
    Sites,
    Rename,
    Find,
    /// Quick View in a tab of its own (`quick tab`); otherwise it takes the other panel's place.
    Quick,
    /// The disk usage map (`map`).
    Map,
    /// The duplicate finder (`dupes`).
    Dupes,
    /// Two files side by side, or the result of a checksum check (`compare`, `checksum`).
    Compare,
}

impl Tool {
    /// Every tool, in the order their tabs are offered.
    pub const ALL: [Tool; 12] = [
        Tool::Pages,
        Tool::Viewer,
        Tool::Sync,
        Tool::Scan,
        Tool::Queue,
        Tool::Sites,
        Tool::Rename,
        Tool::Find,
        Tool::Quick,
        Tool::Map,
        Tool::Dupes,
        Tool::Compare,
    ];

    /// The name shown on its tab.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Pages => "Strony",
            Tool::Viewer => "Podgląd",
            Tool::Sync => "Synchronizacja",
            Tool::Scan => "Skaner sieci",
            Tool::Queue => "Kolejka",
            Tool::Sites => "Serwery",
            Tool::Rename => "Zmiana nazw",
            Tool::Find => "Szukanie",
            Tool::Quick => "Quick View",
            Tool::Map => "Mapa zajętości",
            Tool::Dupes => "Duplikaty",
            Tool::Compare => "Porównanie",
        }
    }

    /// The command line that closes the tool.
    pub fn close_line(self) -> &'static str {
        match self {
            Tool::Pages => "pageclose",
            Tool::Viewer => "viewer close",
            Tool::Sync => "syncclose",
            Tool::Scan => "scanclose",
            Tool::Queue => "queueclose",
            Tool::Sites => "siteclose",
            Tool::Rename => "mrclose",
            Tool::Find => "findclose",
            Tool::Quick => "quick",
            Tool::Map => "map close",
            Tool::Dupes => "dupes close",
            Tool::Compare => "compare close",
        }
    }

    /// True while the session has this tool open.
    pub fn is_open(self, session: &Session) -> bool {
        match self {
            Tool::Pages => session.pages.is_some(),
            Tool::Viewer => session.viewer.is_some(),
            Tool::Sync => session.sync.is_some(),
            Tool::Scan => session.scan.as_ref().is_some_and(|v| v.visible),
            Tool::Queue => session.queue_open,
            Tool::Sites => session.sites_open,
            Tool::Rename => session.rename.is_some(),
            Tool::Find => session.find.as_ref().is_some_and(|f| f.visible),
            Tool::Quick => session.quick.is_some() && session.quick_in_tab(),
            Tool::Map => session.diskmap.is_some(),
            Tool::Dupes => session.duplicates.is_some(),
            Tool::Compare => session.compare.is_some(),
        }
    }
}

impl Session {
    /// The tools that are open now, in [`Tool::ALL`] order.
    pub fn open_tools(&self) -> Vec<Tool> {
        Tool::ALL.into_iter().filter(|t| t.is_open(self)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;

    #[test]
    fn every_tool_opens_and_its_close_command_closes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join("a.txt"), "hello").unwrap();
        let mut s = Session::new(&left, tmp.path().join("right"));
        std::fs::create_dir_all(tmp.path().join("right")).unwrap();
        s.use_config_dir(&tmp.path().join("config"));
        assert!(s.open_tools().is_empty());

        // How each tool is opened by the user.
        let open = |s: &mut Session, tool: Tool| match tool {
            Tool::Pages => {
                // A document waiting for conversion is enough to have the page view open.
                s.pages = Some(crate::PageViewer::converting(
                    &tmp.path().join("x.docx"),
                    std::path::PathBuf::from("/nonexistent-office"),
                    tmp.path().join("scratch"),
                ));
            }
            Tool::Viewer => {
                s.run_line("select a.txt").unwrap();
                s.run_line("view").unwrap();
            }
            Tool::Sync => s.run_line("sync").unwrap(),
            Tool::Scan => s.run_line("scan").unwrap(),
            Tool::Queue => s.run_line("queue").unwrap(),
            Tool::Sites => s.run_line("sites").unwrap(),
            Tool::Rename => {
                s.run_line("select a.txt").unwrap();
                s.run_line("multirename").unwrap();
            }
            Tool::Find => s.run_line("find").unwrap(),
            Tool::Quick => {
                s.run_line("quick tab").unwrap();
                s.run_line("quick").unwrap();
            }
            Tool::Map => s.run_line("map").unwrap(),
            Tool::Dupes => s.run_line("dupes").unwrap(),
            Tool::Compare => {
                s.run_line("select a.txt").unwrap();
                s.run_line("checksum create sha256").unwrap();
            }
        };
        for tool in Tool::ALL {
            open(&mut s, tool);
            assert!(tool.is_open(&s), "{tool:?} opens");
            assert!(s.open_tools().contains(&tool));
            assert!(Command::parse(tool.close_line()).is_ok(), "{tool:?}");
            let _ = s.run_line(tool.close_line());
            assert!(
                !tool.is_open(&s),
                "{tool:?} closes with `{}`",
                tool.close_line()
            );
        }
        assert!(s.open_tools().is_empty());
    }

    #[test]
    fn labels_are_distinct() {
        let mut labels: Vec<&str> = Tool::ALL.iter().map(|t| t.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Tool::ALL.len());
    }
}
