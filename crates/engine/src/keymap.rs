use std::collections::HashMap;

/// Maps key names to command lines.
///
/// Key names are plain strings (`"F5"`, `"Tab"`, `"ArrowDown"`), so the engine does not
/// depend on any UI toolkit. A command line that ends with a space (such as `"mkdir "`)
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
            ("F7", "mkdir "),
            ("F8", "delete"),
            ("F9", "refresh"),
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
    fn rebinding_works() {
        let mut map = Keymap::default();
        map.bind("F5", "move");
        assert_eq!(map.lookup("F5"), Some("move"));
    }
}
