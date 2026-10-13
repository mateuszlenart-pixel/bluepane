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
        Self {
            bindings: pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
}

impl Keymap {
    /// The command line bound to a key, if any.
    pub fn lookup(&self, key: &str) -> Option<&str> {
        self.bindings.get(key).map(String::as_str)
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
    fn rebinding_works() {
        let mut map = Keymap::default();
        map.bind("F5", "move");
        assert_eq!(map.lookup("F5"), Some("move"));
    }
}
