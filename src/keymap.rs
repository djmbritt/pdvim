//! Which keys do what, the same way in every mode.
//!
//! Each mode names the things it can be told to do in an enum declared with
//! [`bindable!`]. A [`Keymap`] binds key sequences to them, and [`Pending`]
//! holds what has been typed so far, with a count in front, until it makes
//! up a bound sequence. Which keys are bound comes from the configuration:
//! see [`crate::bindings`].

use crate::input::Key;

/// Something keys can be bound to: one of the things a mode can be told to
/// do, with a name for the configuration and a few words for the help
/// sheet.
pub trait Bindable: Copy + Eq + std::fmt::Debug + 'static {
    /// Every one, in the order the help sheet shows them, with its name and
    /// what it does.
    const ALL: &'static [(Self, &'static str, &'static str)];

    /// The name used in the configuration and with `:map`.
    fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(bound, ..)| *bound == self)
            .map_or("", |(_, name, _)| name)
    }

    /// What it does, in a few words.
    fn description(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(bound, ..)| *bound == self)
            .map_or("", |(.., description)| description)
    }

    /// The one with this name.
    fn named(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, known, _)| *known == name)
            .map(|(bound, ..)| *bound)
    }

    /// The names of every one.
    fn names() -> impl Iterator<Item = &'static str> {
        Self::ALL.iter().map(|(_, name, _)| *name)
    }
}

/// Declares an enum of things keys can be bound to, each with its name and
/// what it does:
///
/// ```ignore
/// bindable! {
///     /// What the list can be told to do.
///     pub enum Binding {
///         Down = "down", "Select the next entry";
///     }
/// }
/// ```
macro_rules! bindable {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident = $text:literal, $description:literal;
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis enum $name {
            $($(#[$variant_meta])* $variant,)*
        }

        impl $crate::keymap::Bindable for $name {
            const ALL: &'static [(Self, &'static str, &'static str)] =
                &[$((Self::$variant, $text, $description),)*];
        }
    };
}
pub(crate) use bindable;

/// Why a key sequence or binding could not be understood.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("no keys given")]
    NoKeys,
    #[error("unknown key: {0}")]
    UnknownKey(String),
    #[error("unknown action: {0}")]
    UnknownAction(String),
    #[error("unknown mode: {0}")]
    UnknownMode(String),
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
            function => match function.strip_prefix('f').map(str::parse::<u8>) {
                Some(Ok(number @ 1..=12)) => Key::Function(number),
                _ => return Err(unknown()),
            },
        },
        _ => return Err(unknown()),
    };
    Ok(key)
}

/// A key sequence as written in the configuration.
pub fn written(keys: &[Key]) -> String {
    let keys: Vec<String> = keys.iter().map(ToString::to_string).collect();
    keys.join(" ")
}

/// What the keys typed so far amount to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match<A> {
    /// A whole sequence, bound to this.
    Bound(A),
    /// A whole sequence, bound to the command line with this place among
    /// the keymap's.
    Command(usize),
    /// The start of one or more sequences.
    Prefix,
    /// Nothing.
    None,
}

/// The key bindings of one mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap<A> {
    bindings: Vec<(Vec<Key>, A)>,
    /// Key sequences bound to command lines, as typed after `:`, which
    /// may be several separated by `;`.
    commands: Vec<(Vec<Key>, String)>,
}

impl<A> Default for Keymap<A> {
    /// No bindings at all.
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            commands: Vec::new(),
        }
    }
}

impl<A: Bindable> Keymap<A> {
    /// Binds a key sequence, replacing what it was bound to.
    pub fn bind(&mut self, keys: Vec<Key>, bound: A) {
        self.unbind(&keys);
        self.bindings.push((keys, bound));
    }

    /// Binds a key sequence to a command line, replacing what it was bound
    /// to.
    pub fn bind_command(&mut self, keys: Vec<Key>, line: String) {
        self.unbind(&keys);
        self.commands.push((keys, line));
    }

    /// Unbinds a key sequence. Returns whether it was bound.
    pub fn unbind(&mut self, keys: &[Key]) -> bool {
        let before = self.bindings.len().saturating_add(self.commands.len());
        self.bindings.retain(|(bound, _)| bound != keys);
        self.commands.retain(|(bound, _)| bound != keys);
        self.bindings.len().saturating_add(self.commands.len()) != before
    }

    /// Unbinds every key sequence.
    pub fn clear(&mut self) {
        self.bindings.clear();
        self.commands.clear();
    }

    /// The key sequences bound to command lines, and the lines, in the
    /// order they were bound.
    pub fn commands(&self) -> impl Iterator<Item = (&[Key], &str)> {
        self.commands
            .iter()
            .map(|(keys, line)| (keys.as_slice(), line.as_str()))
    }

    /// Every binding, sorted, so that two keymaps can be compared whatever
    /// order their bindings were made in.
    #[cfg(test)]
    pub fn sorted(&self) -> Vec<(String, &'static str)> {
        let mut bindings: Vec<_> = self
            .bindings
            .iter()
            .map(|(keys, bound)| (written(keys), bound.name()))
            .collect();
        bindings.sort_unstable();
        bindings
    }

    /// The key sequences bound to something, in the order they were bound.
    pub fn keys_for(&self, bound: A) -> impl Iterator<Item = &[Key]> {
        self.bindings
            .iter()
            .filter(move |(_, to)| *to == bound)
            .map(|(keys, _)| keys.as_slice())
    }

    /// The first key sequence bound to something, as written, for a hint
    /// on screen; or a dash when nothing is bound to it.
    pub fn first(&self, bound: A) -> String {
        self.keys_for(bound)
            .next()
            .map_or_else(|| "-".to_owned(), written)
    }

    /// What a sequence of keys amounts to. A whole sequence wins over a
    /// longer one it starts, so binding `g` hides `gg`.
    pub fn lookup(&self, keys: &[Key]) -> Match<A> {
        let mut prefix = false;
        for (bound, to) in &self.bindings {
            if bound == keys {
                return Match::Bound(*to);
            }
            prefix |= bound.starts_with(keys);
        }
        for (index, (bound, _)) in self.commands.iter().enumerate() {
            if bound == keys {
                return Match::Command(index);
            }
            prefix |= bound.starts_with(keys);
        }
        if prefix { Match::Prefix } else { Match::None }
    }
}

/// Whether digits typed before a sequence make a count, as in `5j`, or are
/// keys like any other, as in a list where `3` picks the third entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counts {
    Taken,
    NotTaken,
}

/// What a key typed comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fed<A> {
    /// A bound sequence is complete, after this count if one was typed.
    Bound(A, Option<u32>),
    /// A sequence bound to this command line is complete, after this count
    /// if one was typed.
    Command(String, Option<u32>),
    /// The keys typed so far could still become a bound sequence.
    Waiting,
    /// The keys typed, those of a sequence that is bound to nothing. A mode
    /// that takes text, such as the prompt, takes them as text.
    Unbound(Vec<Key>),
}

/// The part of a command typed so far: a count, then the start of a bound
/// sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending<A> {
    count: Option<u32>,
    keys: Vec<Key>,
    bound: std::marker::PhantomData<A>,
}

impl<A> Default for Pending<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A> Pending<A> {
    /// Nothing typed.
    pub const fn new() -> Self {
        Self {
            count: None,
            keys: Vec::new(),
            bound: std::marker::PhantomData,
        }
    }
}

impl<A: Bindable> Pending<A> {
    /// What has been typed so far, for the status bar.
    pub fn display(&self) -> String {
        let mut shown = self
            .count
            .map(|count| count.to_string())
            .unwrap_or_default();
        shown.push_str(&written(&self.keys));
        shown
    }

    /// Whether anything has been typed.
    #[cfg(test)]
    pub const fn is_empty(&self) -> bool {
        self.count.is_none() && self.keys.is_empty()
    }

    /// Takes the next key. Keys that make up a sequence bound to nothing
    /// clear what was typed.
    pub fn feed(&mut self, key: Key, keymap: &Keymap<A>, counts: Counts) -> Fed<A> {
        // A digit is part of a count, unless it comes in the middle of a
        // sequence, as in `z0`. A count cannot start with 0, so 0 can be
        // bound on its own.
        if counts == Counts::Taken
            && self.keys.is_empty()
            && let Key::Char(digit @ '0'..='9') = key
            && let Some(value) = digit.to_digit(10)
            && (self.count.is_some() || value != 0)
        {
            let count = self.count.unwrap_or(0);
            self.count = Some(count.saturating_mul(10).saturating_add(value));
            return Fed::Waiting;
        }
        self.keys.push(key);
        match keymap.lookup(&self.keys) {
            Match::Prefix => Fed::Waiting,
            Match::Bound(bound) => {
                let count = self.count.take();
                self.keys.clear();
                Fed::Bound(bound, count)
            }
            Match::Command(index) => {
                let count = self.count.take();
                self.keys.clear();
                let line = keymap
                    .commands
                    .get(index)
                    .map(|(_, line)| line.clone())
                    .unwrap_or_default();
                Fed::Command(line, count)
            }
            Match::None => {
                let keys = std::mem::take(&mut self.keys);
                self.count = None;
                Fed::Unbound(keys)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    bindable! {
        enum Test {
            Down = "down", "Go down";
            Top = "top", "Go to the top";
            Zero = "zero", "Go to the start";
        }
    }

    fn keymap() -> Keymap<Test> {
        let mut keymap = Keymap::default();
        keymap.bind(parse_keys("j").unwrap(), Test::Down);
        keymap.bind(parse_keys("down").unwrap(), Test::Down);
        keymap.bind(parse_keys("g g").unwrap(), Test::Top);
        keymap.bind(parse_keys("0").unwrap(), Test::Zero);
        keymap
    }

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
        assert_eq!(parse_keys("F11 f1"), Ok(vec![Key::Function(11), Key::Function(1)]));
        assert_eq!(parse_keys("f13"), Err(Error::UnknownKey("f13".into())));
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
    fn every_bindable_has_a_name_and_back() {
        for &(bound, name, description) in Test::ALL {
            assert_eq!(Test::named(name), Some(bound));
            assert_eq!(bound.name(), name);
            assert_eq!(bound.description(), description);
        }
        assert_eq!(Test::named("fly"), None);
    }

    #[test]
    fn lookup_finds_whole_sequences_and_prefixes() {
        let mut keymap = keymap();
        let g = [Key::Char('g')];
        assert_eq!(keymap.lookup(&g), Match::Prefix);
        assert_eq!(
            keymap.lookup(&[Key::Char('g'), Key::Char('g')]),
            Match::Bound(Test::Top)
        );
        assert_eq!(keymap.lookup(&[Key::Char('x')]), Match::None);
        let keys: Vec<_> = keymap.keys_for(Test::Down).map(written).collect();
        assert_eq!(keys, ["j", "down"]);

        keymap.bind(vec![Key::Char('x')], Test::Top);
        assert_eq!(keymap.lookup(&[Key::Char('x')]), Match::Bound(Test::Top));
        assert!(keymap.unbind(&[Key::Char('x')]));
        assert!(!keymap.unbind(&[Key::Char('x')]));

        keymap.clear();
        assert_eq!(keymap.lookup(&[Key::Char('j')]), Match::None);
    }

    #[test]
    fn counts_come_before_sequences_when_taken() {
        let keymap = keymap();
        let mut pending = Pending::default();
        assert_eq!(
            pending.feed(Key::Char('1'), &keymap, Counts::Taken),
            Fed::Waiting
        );
        assert_eq!(
            pending.feed(Key::Char('0'), &keymap, Counts::Taken),
            Fed::Waiting
        );
        assert_eq!(
            pending.feed(Key::Char('g'), &keymap, Counts::Taken),
            Fed::Waiting
        );
        assert_eq!(pending.display(), "10g");
        assert_eq!(
            pending.feed(Key::Char('g'), &keymap, Counts::Taken),
            Fed::Bound(Test::Top, Some(10))
        );
        assert!(pending.is_empty());
        // A 0 on its own is a key.
        assert_eq!(
            pending.feed(Key::Char('0'), &keymap, Counts::Taken),
            Fed::Bound(Test::Zero, None)
        );
        // Not taken, a digit is a key like any other.
        assert_eq!(
            pending.feed(Key::Char('3'), &keymap, Counts::NotTaken),
            Fed::Unbound(vec![Key::Char('3')])
        );
    }

    #[test]
    fn a_sequence_bound_to_nothing_is_handed_back_and_forgotten() {
        let keymap = keymap();
        let mut pending = Pending::default();
        pending.feed(Key::Char('4'), &keymap, Counts::Taken);
        pending.feed(Key::Char('g'), &keymap, Counts::Taken);
        assert_eq!(
            pending.feed(Key::Char('x'), &keymap, Counts::Taken),
            Fed::Unbound(vec![Key::Char('g'), Key::Char('x')])
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn keys_bound_to_a_command_line_hand_it_back() {
        let mut keymap = keymap();
        keymap.bind_command(parse_keys("ctrl+t").unwrap(), "theme auto".into());
        keymap.bind_command(parse_keys("g x").unwrap(), "w; q".into());
        let mut pending = Pending::default();
        assert_eq!(
            pending.feed(Key::Ctrl('t'), &keymap, Counts::Taken),
            Fed::Command("theme auto".into(), None)
        );
        assert_eq!(
            pending.feed(Key::Char('g'), &keymap, Counts::Taken),
            Fed::Waiting
        );
        assert_eq!(
            pending.feed(Key::Char('x'), &keymap, Counts::Taken),
            Fed::Command("w; q".into(), None)
        );
        // Binding the keys to an action takes the line away, and back.
        keymap.bind(parse_keys("ctrl+t").unwrap(), Test::Top);
        assert_eq!(keymap.lookup(&[Key::Ctrl('t')]), Match::Bound(Test::Top));
        keymap.bind_command(parse_keys("ctrl+t").unwrap(), "help".into());
        assert_eq!(keymap.lookup(&[Key::Ctrl('t')]), Match::Command(1));
        assert!(keymap.unbind(&[Key::Ctrl('t')]));
        assert_eq!(keymap.commands().count(), 1);
    }

    #[test]
    fn huge_counts_saturate() {
        let keymap = keymap();
        let mut pending = Pending::default();
        for _ in 0..20 {
            pending.feed(Key::Char('9'), &keymap, Counts::Taken);
        }
        assert_eq!(
            pending.feed(Key::Char('j'), &keymap, Counts::Taken),
            Fed::Bound(Test::Down, Some(u32::MAX))
        );
    }
}
