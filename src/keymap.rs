//! Which keys do what: the bindings of named actions to key sequences, the
//! built-in ones and those changed in the configuration file or with `map`
//! and `unmap`.

use crate::input::{Action, Key, Prompt};

/// Something a key sequence can be bound to. Each has a name, used in the
/// configuration file and with `:map`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    ScrollDown,
    ScrollUp,
    ScrollLeft,
    ScrollRight,
    ScrollHalfDown,
    ScrollHalfUp,
    ScrollFullDown,
    ScrollFullUp,
    NextPage,
    PreviousPage,
    /// The first page, or the page given by a count.
    FirstPage,
    /// The last page, or the page given by a count.
    LastPage,
    PageTop,
    PageBottom,
    ZoomIn,
    ZoomOut,
    /// Zoom to the percentage given by a count, or to 100%.
    ZoomTo,
    /// Zoom to 100%, whatever the count.
    ZoomReset,
    FitWidth,
    FitPage,
    Rotate,
    ToggleRecolor,
    ToggleTrim,
    SearchNext,
    SearchPrevious,
    SearchForward,
    SearchBackward,
    CommandPrompt,
    Recent,
    /// The next tab, or the tab given by a count.
    NextTab,
    PreviousTab,
    Abort,
    Quit,
}

impl Binding {
    /// Every binding with its name.
    const NAMES: [(Self, &'static str); 33] = [
        (Self::ScrollDown, "scroll-down"),
        (Self::ScrollUp, "scroll-up"),
        (Self::ScrollLeft, "scroll-left"),
        (Self::ScrollRight, "scroll-right"),
        (Self::ScrollHalfDown, "scroll-half-down"),
        (Self::ScrollHalfUp, "scroll-half-up"),
        (Self::ScrollFullDown, "scroll-full-down"),
        (Self::ScrollFullUp, "scroll-full-up"),
        (Self::NextPage, "next-page"),
        (Self::PreviousPage, "previous-page"),
        (Self::FirstPage, "first-page"),
        (Self::LastPage, "last-page"),
        (Self::PageTop, "page-top"),
        (Self::PageBottom, "page-bottom"),
        (Self::ZoomIn, "zoom-in"),
        (Self::ZoomOut, "zoom-out"),
        (Self::ZoomTo, "zoom-to"),
        (Self::ZoomReset, "zoom-reset"),
        (Self::FitWidth, "fit-width"),
        (Self::FitPage, "fit-page"),
        (Self::Rotate, "rotate"),
        (Self::ToggleRecolor, "toggle-recolor"),
        (Self::ToggleTrim, "toggle-trim"),
        (Self::SearchNext, "search-next"),
        (Self::SearchPrevious, "search-previous"),
        (Self::SearchForward, "search-forward"),
        (Self::SearchBackward, "search-backward"),
        (Self::CommandPrompt, "command-prompt"),
        (Self::Recent, "recent"),
        (Self::NextTab, "next-tab"),
        (Self::PreviousTab, "previous-tab"),
        (Self::Abort, "abort"),
        (Self::Quit, "quit"),
    ];

    /// The names of every binding.
    pub fn names() -> impl Iterator<Item = &'static str> {
        Self::NAMES.iter().map(|(_, name)| *name)
    }

    /// The binding with this name.
    pub fn named(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(_, known)| *known == name)
            .map(|(binding, _)| *binding)
    }

    /// What the binding does, given the count typed before its keys.
    pub fn action(self, count: Option<u32>) -> Action {
        let times = i32::try_from(count.unwrap_or(1)).unwrap_or(i32::MAX);
        let back = times.saturating_neg();
        match self {
            Self::ScrollDown => Action::Scroll {
                right: 0,
                down: times,
            },
            Self::ScrollUp => Action::Scroll {
                right: 0,
                down: back,
            },
            Self::ScrollRight => Action::Scroll {
                right: times,
                down: 0,
            },
            Self::ScrollLeft => Action::Scroll {
                right: back,
                down: 0,
            },
            Self::ScrollHalfDown => Action::HalfScreen(times),
            Self::ScrollHalfUp => Action::HalfScreen(back),
            Self::ScrollFullDown => Action::FullScreen(times),
            Self::ScrollFullUp => Action::FullScreen(back),
            Self::NextPage => Action::Page(times),
            Self::PreviousPage => Action::Page(back),
            Self::FirstPage => count.map_or(Action::GotoFirst, Action::GotoPage),
            Self::LastPage => count.map_or(Action::GotoLast, Action::GotoPage),
            Self::PageTop => Action::PageTop,
            Self::PageBottom => Action::PageBottom,
            Self::ZoomIn => Action::ZoomIn,
            Self::ZoomOut => Action::ZoomOut,
            Self::ZoomTo => Action::ZoomTo(count.unwrap_or(100)),
            Self::ZoomReset => Action::ZoomTo(100),
            Self::FitWidth => Action::FitWidth,
            Self::FitPage => Action::FitPage,
            Self::Rotate => Action::Rotate,
            Self::ToggleRecolor => Action::ToggleRecolor,
            Self::ToggleTrim => Action::ToggleTrim,
            Self::SearchNext => Action::SearchNext,
            Self::SearchPrevious => Action::SearchPrevious,
            Self::SearchForward => Action::OpenPrompt(Prompt::SearchForward),
            Self::SearchBackward => Action::OpenPrompt(Prompt::SearchBackward),
            Self::CommandPrompt => Action::OpenPrompt(Prompt::Command),
            Self::Recent => Action::ShowRecent,
            Self::NextTab => count.map_or(Action::NextTab, Action::GotoTab),
            Self::PreviousTab => Action::PreviousTab(count.unwrap_or(1)),
            Self::Abort => Action::Abort,
            Self::Quit => Action::Quit,
        }
    }
}

/// Why a key sequence or binding could not be understood.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("no keys given")]
    NoKeys,
    #[error("unknown key: {0}")]
    UnknownKey(String),
    #[error("unknown action: {0}")]
    UnknownAction(String),
}

/// Parses a sequence of keys separated by spaces, as in `g g` or
/// `ctrl+w j`. Each key is a single character, which stands for itself, or
/// a named key such as `space` or `pagedown`, with modifiers in front:
/// `ctrl+d`, `shift+space`, `ctrl+shift+tab`. Names and modifiers are not
/// case sensitive; single characters are.
pub fn parse_keys(written: &str) -> Result<Vec<Key>, Error> {
    let keys = written
        .split_whitespace()
        .map(parse_key)
        .collect::<Result<Vec<_>, _>>()?;
    if keys.is_empty() {
        return Err(Error::NoKeys);
    }
    Ok(keys)
}

/// Parses one key, with its modifiers.
fn parse_key(written: &str) -> Result<Key, Error> {
    let unknown = || Error::UnknownKey(written.to_owned());
    if written.chars().count() == 1 {
        return written.chars().next().map(Key::Char).ok_or_else(unknown);
    }
    // The key comes after the last `+`, unless the key is `+` itself.
    let (modifiers, name) = written.strip_suffix("++").map_or_else(
        || written.rsplit_once('+').unwrap_or(("", written)),
        |modifiers| (modifiers, "+"),
    );
    let mut ctrl = false;
    let mut shift = false;
    for modifier in modifiers.split('+').filter(|modifier| !modifier.is_empty()) {
        match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "shift" => shift = true,
            _ => return Err(unknown()),
        }
    }
    let mut characters = name.chars();
    let single = characters.next().filter(|_| characters.next().is_none());
    let key = match (single, name.to_ascii_lowercase().as_str(), ctrl, shift) {
        (_, "tab", false, false) => Key::Tab,
        (_, "tab", false, true) => Key::ShiftTab,
        (_, "tab", true, false) => Key::CtrlTab,
        (_, "tab", true, true) => Key::CtrlShiftTab,
        (_, "space", false, false) => Key::Char(' '),
        (_, "space", false, true) => Key::ShiftSpace,
        (Some(character), _, true, false) => Key::Ctrl(character.to_ascii_lowercase()),
        (Some(character), _, false, true) if character.is_alphabetic() => {
            Key::Char(character.to_uppercase().next().unwrap_or(character))
        }
        (_, named, false, false) => match named {
            "esc" | "escape" => Key::Escape,
            "enter" | "return" => Key::Enter,
            "backspace" => Key::Backspace,
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            "home" => Key::Home,
            "end" => Key::End,
            _ => return Err(unknown()),
        },
        _ => return Err(unknown()),
    };
    Ok(key)
}

/// What the keys typed so far amount to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    /// A whole sequence, bound to this.
    Bound(Binding),
    /// The start of one or more sequences.
    Prefix,
    /// Nothing.
    None,
}

/// The key bindings in force.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<(Vec<Key>, Binding)>,
}

impl Default for Keymap {
    /// The bindings pdvim starts with.
    fn default() -> Self {
        use Binding as B;
        const DEFAULTS: [(&str, Binding); 53] = [
            ("j", B::ScrollDown),
            ("down", B::ScrollDown),
            ("k", B::ScrollUp),
            ("up", B::ScrollUp),
            ("h", B::ScrollLeft),
            ("left", B::ScrollLeft),
            ("l", B::ScrollRight),
            ("right", B::ScrollRight),
            ("ctrl+d", B::ScrollHalfDown),
            ("ctrl+u", B::ScrollHalfUp),
            ("ctrl+f", B::ScrollFullDown),
            ("space", B::ScrollFullDown),
            ("ctrl+b", B::ScrollFullUp),
            ("shift+space", B::ScrollFullUp),
            ("J", B::NextPage),
            ("pagedown", B::NextPage),
            ("K", B::PreviousPage),
            ("pageup", B::PreviousPage),
            ("g g", B::FirstPage),
            ("home", B::FirstPage),
            ("G", B::LastPage),
            ("end", B::LastPage),
            ("H", B::PageTop),
            ("L", B::PageBottom),
            ("+", B::ZoomIn),
            ("z i", B::ZoomIn),
            ("z I", B::ZoomIn),
            ("-", B::ZoomOut),
            ("z o", B::ZoomOut),
            ("z O", B::ZoomOut),
            ("=", B::ZoomTo),
            ("z z", B::ZoomTo),
            ("z Z", B::ZoomTo),
            ("z 0", B::ZoomReset),
            ("s", B::FitWidth),
            ("a", B::FitPage),
            ("r", B::Rotate),
            ("ctrl+r", B::ToggleRecolor),
            ("t", B::ToggleTrim),
            ("n", B::SearchNext),
            ("N", B::SearchPrevious),
            ("/", B::SearchForward),
            ("?", B::SearchBackward),
            (":", B::CommandPrompt),
            ("o", B::Recent),
            ("g t", B::NextTab),
            ("ctrl+tab", B::NextTab),
            ("g T", B::PreviousTab),
            ("ctrl+shift+tab", B::PreviousTab),
            ("escape", B::Abort),
            ("ctrl+c", B::Abort),
            ("ctrl+[", B::Abort),
            ("q", B::Quit),
        ];
        let mut keymap = Self {
            bindings: Vec::new(),
        };
        for (keys, binding) in DEFAULTS {
            if let Ok(keys) = parse_keys(keys) {
                keymap.bind(keys, binding);
            }
        }
        keymap
    }
}

impl Keymap {
    /// Binds a key sequence, replacing what it was bound to.
    pub fn bind(&mut self, keys: Vec<Key>, binding: Binding) {
        self.unbind(&keys);
        self.bindings.push((keys, binding));
    }

    /// Unbinds a key sequence. Returns whether it was bound.
    pub fn unbind(&mut self, keys: &[Key]) -> bool {
        let before = self.bindings.len();
        self.bindings.retain(|(bound, _)| bound != keys);
        self.bindings.len() != before
    }

    /// Unbinds every key sequence.
    pub fn clear(&mut self) {
        self.bindings.clear();
    }

    /// Every binding, sorted, so that two keymaps can be compared whatever
    /// order their bindings were made in.
    #[cfg(test)]
    pub fn sorted(&self) -> Vec<(String, Binding)> {
        let mut bindings: Vec<_> = self
            .bindings
            .iter()
            .map(|(keys, binding)| (format!("{keys:?}"), *binding))
            .collect();
        bindings.sort_by(|left, right| left.0.cmp(&right.0));
        bindings
    }

    /// What a sequence of keys amounts to. A whole sequence wins over a
    /// longer one it starts, so binding `g` hides `gg`.
    pub fn lookup(&self, keys: &[Key]) -> Match {
        let mut prefix = false;
        for (bound, binding) in &self.bindings {
            if bound == keys {
                return Match::Bound(*binding);
            }
            prefix |= bound.starts_with(keys);
        }
        if prefix { Match::Prefix } else { Match::None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_parse() {
        assert_eq!(parse_keys("g  g"), Ok(vec![Key::Char('g'), Key::Char('g')]));
        assert_eq!(parse_keys("ctrl+d"), Ok(vec![Key::Ctrl('d')]));
        assert_eq!(parse_keys("Ctrl+D"), Ok(vec![Key::Ctrl('d')]));
        assert_eq!(parse_keys("shift+q"), Ok(vec![Key::Char('Q')]));
        assert_eq!(parse_keys("Q"), Ok(vec![Key::Char('Q')]));
        assert_eq!(
            parse_keys("space shift+space"),
            Ok(vec![Key::Char(' '), Key::ShiftSpace])
        );
        assert_eq!(parse_keys("ctrl+shift+tab"), Ok(vec![Key::CtrlShiftTab]));
        assert_eq!(
            parse_keys("tab shift+tab"),
            Ok(vec![Key::Tab, Key::ShiftTab])
        );
        assert_eq!(parse_keys("ESC"), Ok(vec![Key::Escape]));
        assert_eq!(
            parse_keys("+ ctrl++"),
            Ok(vec![Key::Char('+'), Key::Ctrl('+')])
        );
        assert_eq!(parse_keys("ctrl+["), Ok(vec![Key::Ctrl('[')]));
        assert_eq!(parse_keys("é"), Ok(vec![Key::Char('é')]));
    }

    #[test]
    fn bad_keys_are_explained() {
        assert_eq!(parse_keys("  "), Err(Error::NoKeys));
        assert_eq!(parse_keys("nope"), Err(Error::UnknownKey("nope".into())));
        assert_eq!(
            parse_keys("ctrl+dd"),
            Err(Error::UnknownKey("ctrl+dd".into()))
        );
        assert_eq!(parse_keys("alt+x"), Err(Error::UnknownKey("alt+x".into())));
        assert_eq!(
            parse_keys("shift+1"),
            Err(Error::UnknownKey("shift+1".into()))
        );
    }

    #[test]
    fn every_default_binding_parses() {
        let keymap = Keymap::default();
        assert_eq!(keymap.bindings.len(), 53);
    }

    #[test]
    fn every_binding_has_a_name_and_back() {
        for (binding, name) in Binding::NAMES {
            assert_eq!(Binding::named(name), Some(binding));
        }
        assert_eq!(Binding::named("fly"), None);
    }

    #[test]
    fn lookup_finds_whole_sequences_and_prefixes() {
        let mut keymap = Keymap::default();
        let g = [Key::Char('g')];
        assert_eq!(keymap.lookup(&g), Match::Prefix);
        assert_eq!(
            keymap.lookup(&[Key::Char('g'), Key::Char('g')]),
            Match::Bound(Binding::FirstPage)
        );
        assert_eq!(keymap.lookup(&[Key::Char('x')]), Match::None);

        keymap.bind(vec![Key::Char('x')], Binding::Quit);
        assert_eq!(
            keymap.lookup(&[Key::Char('x')]),
            Match::Bound(Binding::Quit)
        );
        assert!(keymap.unbind(&[Key::Char('x')]));
        assert!(!keymap.unbind(&[Key::Char('x')]));

        keymap.clear();
        assert_eq!(keymap.lookup(&[Key::Char('j')]), Match::None);
    }
}
