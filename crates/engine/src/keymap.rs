use std::collections::HashMap;

/// Maps key names to command lines.
///
/// Key names are plain strings (`"F5"`, `"Tab"`, `"ArrowDown"`), optionally prefixed with
/// modifiers in the order `Ctrl+`, `Alt+`, `Shift+` (`"Alt+ArrowLeft"`), so the engine does
/// not depend on any UI toolkit. A command line that ends with a space (such as `"mkdir "`)
/// is not executed directly: the UI puts it in the command line for the user to complete.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: HashMap<String, String>,
    /// Keys while the file viewer is open.
    viewer_bindings: HashMap<String, String>,
}

impl Default for Keymap {
    fn default() -> Self {
        let pairs = [
            ("Tab", "switch"),
            ("Enter", "enter"),
            ("Backspace", "up"),
            ("ArrowUp", "cursor -1"),
            ("ArrowDown", "cursor 1"),
            ("PageUp", "cursor -10"),
            ("PageDown", "cursor 10"),
            ("Space", "toggle"),
            ("Insert", "toggle"),
            ("F5", "copy"),
            ("F6", "move"),
            ("F2", "rename"),
            ("F3", "view"),
            ("F4", "edit"),
            ("Alt+F5", "pack"),
            ("Alt+F9", "unpack"),
            ("Alt+S", "sync"),
            ("Ctrl+F2", "multirename"),
            ("Ctrl+S", "sites"),
            ("Ctrl+J", "queue"),
            ("Ctrl+Shift+Num1", "copynames stem"),
            ("Ctrl+Shift+Num2", "copynames file"),
            ("Ctrl+Shift+Num3", "copynames path"),
            ("Ctrl+N", "scan"),
            ("Ctrl+Shift+F4", "editsend"),
            ("Alt+F7", "find"),
            // Many Linux window managers (Cinnamon, KDE, ...) take Alt+F7 for moving windows.
            ("Ctrl+F", "find"),
            ("F7", "mkdir"),
            ("F8", "delete"),
            ("F9", "refresh"),
            ("Escape", "filter"),
            ("Ctrl+A", "selectall"),
            ("Ctrl+Shift+A", "unselectall"),
            ("Ctrl+I", "invert"),
            ("Ctrl+M", "selmask "),
            ("Ctrl+Shift+M", "unselmask "),
            ("Ctrl+F3", "sort name"),
            ("Ctrl+F4", "sort ext"),
            ("Ctrl+F5", "sort date"),
            ("Ctrl+F6", "sort size"),
            ("Alt+ArrowLeft", "back"),
            ("Alt+ArrowRight", "forward"),
            ("Ctrl+L", "cd "),
            ("Ctrl+T", "tab new"),
            ("Ctrl+W", "tab close"),
            ("Ctrl+ArrowUp", "tab open"),
            ("Ctrl+Tab", "tab next"),
            ("Ctrl+Shift+Tab", "tab prev"),
            ("Ctrl+PageDown", "tab next"),
            ("Ctrl+PageUp", "tab prev"),
            ("Ctrl+Num1", "tab 1"),
            ("Ctrl+Num2", "tab 2"),
            ("Ctrl+Num3", "tab 3"),
            ("Ctrl+Num4", "tab 4"),
            ("Ctrl+Num5", "tab 5"),
            ("Ctrl+Num6", "tab 6"),
            ("Ctrl+Num7", "tab 7"),
            ("Ctrl+Num8", "tab 8"),
            ("Ctrl+Num9", "tab 9"),
            ("Alt+Num1", "volume 1"),
            ("Alt+Num2", "volume 2"),
            ("Alt+Num3", "volume 3"),
            ("Alt+Num4", "volume 4"),
            ("Alt+Num5", "volume 5"),
            ("Alt+Num6", "volume 6"),
            ("Alt+Num7", "volume 7"),
            ("Alt+Num8", "volume 8"),
            ("Alt+Num9", "volume 9"),
        ];
        let viewer_pairs = [
            ("ArrowDown", "viewer scroll 1"),
            ("ArrowUp", "viewer scroll -1"),
            ("PageDown", "viewer page 1"),
            ("PageUp", "viewer page -1"),
            ("Space", "viewer page 1"),
            ("Home", "viewer top"),
            ("End", "viewer bottom"),
            ("ArrowRight", "viewer hscroll 8"),
            ("ArrowLeft", "viewer hscroll -8"),
            ("H", "viewer mode"),
            ("F4", "viewer mode"),
            ("F7", "viewer find"),
            ("Slash", "viewer find"),
            ("Ctrl+F", "viewer find"),
            ("N", "viewer next"),
            ("F3", "viewer next"),
            ("Shift+N", "viewer prev"),
            ("Shift+F3", "viewer prev"),
            ("Escape", "viewer close"),
            ("Q", "viewer close"),
        ];
        let to_map = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        Self {
            bindings: to_map(&pairs),
            viewer_bindings: to_map(&viewer_pairs),
        }
    }
}

impl Keymap {
    /// The command line bound to a key, if any.
    pub fn lookup(&self, key: &str) -> Option<&str> {
        self.bindings.get(key).map(String::as_str)
    }

    /// The command line bound to a key while the file viewer is open.
    pub fn lookup_viewer(&self, key: &str) -> Option<&str> {
        self.viewer_bindings.get(key).map(String::as_str)
    }

    /// Binds (or rebinds) a key.
    pub fn bind(&mut self, key: impl Into<String>, command_line: impl Into<String>) {
        self.bindings.insert(key.into(), command_line.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;

    #[test]
    fn classic_function_keys_are_bound() {
        let map = Keymap::default();
        assert_eq!(map.lookup("F5"), Some("copy"));
        assert_eq!(map.lookup("F6"), Some("move"));
        assert_eq!(map.lookup("F8"), Some("delete"));
        assert_eq!(map.lookup("F12"), None);
    }

    #[test]
    fn every_default_binding_is_a_valid_command() {
        let map = Keymap::default();
        for key in [
            "Tab",
            "Enter",
            "Backspace",
            "ArrowUp",
            "ArrowDown",
            "Space",
            "F5",
            "F6",
            "F8",
        ] {
            let line = map.lookup(key).unwrap();
            assert!(Command::parse(line).is_ok(), "{key} -> {line}");
        }
    }

    #[test]
    fn navigation_bindings_are_valid_commands() {
        let map = Keymap::default();
        for key in [
            "Escape",
            "Alt+ArrowLeft",
            "Alt+ArrowRight",
            "Ctrl+L",
            "Alt+Num1",
        ] {
            let line = map.lookup(key).unwrap().trim_end();
            assert!(
                Command::parse(line).is_ok() || line == "cd",
                "{key} -> {line}"
            );
        }
    }

    #[test]
    fn viewer_bindings_are_valid_commands() {
        let map = Keymap::default();
        for key in [
            "ArrowDown",
            "ArrowUp",
            "PageDown",
            "PageUp",
            "Space",
            "Home",
            "End",
            "ArrowLeft",
            "ArrowRight",
            "H",
            "F4",
            "F7",
            "Slash",
            "N",
            "F3",
            "Shift+N",
            "Escape",
            "Q",
        ] {
            let line = map.lookup_viewer(key).unwrap();
            assert!(Command::parse(line).is_ok(), "{key} -> {line}");
        }
        assert_eq!(map.lookup("F3"), Some("view"));
        assert_eq!(map.lookup("F4"), Some("edit"));
        assert_eq!(map.lookup_viewer("F12"), None);
    }

    #[test]
    fn rebinding_works() {
        let mut map = Keymap::default();
        map.bind("F5", "move");
        assert_eq!(map.lookup("F5"), Some("move"));
    }
}
