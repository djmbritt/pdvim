//! The key bindings of every mode.
//!
//! The code says what each mode can be told to do; which keys do it is
//! configuration. pdvim starts from the bindings in `defaults.toml`, built
//! in, then the configuration file changes them, then `:map` and `:unmap`.

use std::sync::LazyLock;

use crate::command::Command;
use crate::config;
use crate::input::{self, ConfirmBinding, Key, PromptBinding};
use crate::keymap::{Bindable, Keymap};
use crate::{help, hints, markings, outline, overview, picker, visual};

/// The built-in configuration: every setting and key binding at its
/// default.
pub const DEFAULTS: &str = include_str!("../defaults.toml");

/// Declares the modes: their names in the configuration, and what keys can
/// be bound to in each.
macro_rules! modes {
    ($($(#[$meta:meta])* $variant:ident, $field:ident, $name:literal: $bound:ty;)*) => {
        /// A state that takes keys its own way.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Mode {
            $($(#[$meta])* $variant,)*
        }

        impl Mode {
            /// Every mode, in the order the help sheet shows them.
            pub const ALL: &[Self] = &[$(Self::$variant,)*];

            /// The name of the mode in the configuration and with `:map`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            /// The mode with this name.
            pub fn named(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|mode| mode.name() == name)
            }

            /// What in this mode has this name.
            pub fn bound(self, name: &str) -> Option<Bound> {
                match self {
                    $(Self::$variant => <$bound>::named(name).map(Bound::$variant),)*
                }
            }

            /// The names of what keys can be bound to in this mode.
            pub fn names(self) -> Vec<&'static str> {
                match self {
                    $(Self::$variant => <$bound>::names().collect(),)*
                }
            }
        }

        /// Something keys can be bound to, in some mode.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Bound {
            $($variant($bound),)*
        }

        impl Bound {
            /// The mode it is in.
            #[cfg(test)]
            pub const fn mode(self) -> Mode {
                match self {
                    $(Self::$variant(_) => Mode::$variant,)*
                }
            }

            /// Its name.
            #[cfg(test)]
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant(bound) => bound.name(),)*
                }
            }
        }

        /// The bindings of every mode.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Keymaps {
            $(pub $field: Keymap<$bound>,)*
        }

        impl Keymaps {
            /// No bindings in any mode.
            pub fn empty() -> Self {
                Self {
                    $($field: Keymap::default(),)*
                }
            }

            /// Binds a key sequence in the mode of what it is bound to,
            /// replacing what it was bound to there.
            pub fn bind(&mut self, keys: Vec<Key>, bound: Bound) {
                match bound {
                    $(Bound::$variant(bound) => self.$field.bind(keys, bound),)*
                }
            }

            /// Unbinds a key sequence in a mode. Returns whether it was
            /// bound.
            pub fn unbind(&mut self, mode: Mode, keys: &[Key]) -> bool {
                match mode {
                    $(Mode::$variant => self.$field.unbind(keys),)*
                }
            }

            /// Unbinds every key sequence in a mode.
            pub fn clear(&mut self, mode: Mode) {
                match mode {
                    $(Mode::$variant => self.$field.clear(),)*
                }
            }

            /// Every binding, sorted, so that two sets of keymaps can be
            /// compared whatever order their bindings were made in.
            #[cfg(test)]
            pub fn sorted(&self) -> Vec<(&'static str, String, &'static str)> {
                let mut sorted = Vec::new();
                $(
                    sorted.extend(
                        self.$field
                            .sorted()
                            .into_iter()
                            .map(|(keys, bound)| ($name, keys, bound)),
                    );
                )*
                sorted
            }
        }
    };
}

modes! {
    /// Reading: scrolling, paging, zooming, and starting everything else.
    Normal, normal, "normal": input::Binding;
    /// Selecting text with a cursor, to copy or highlight it.
    Visual, visual, "visual": visual::Binding;
    /// Labels shown on links or lines, to pick one by typing its label.
    Hints, hints, "hints": hints::Binding;
    /// The table of contents, in a sidebar.
    Outline, outline, "outline": outline::Binding;
    /// Every page small, side by side, to pick one.
    Overview, overview, "overview": overview::Binding;
    /// The list of highlights and other markings, in a sidebar.
    Markings, markings, "markings": markings::Binding;
    /// The lists of recent documents and of open tabs.
    Recent, recent, "recent": picker::Binding;
    /// The help sheet.
    Help, help, "help": help::Binding;
    /// Typing a command or a search.
    Prompt, prompt, "prompt": PromptBinding;
    /// A question, such as whether to quit.
    Confirm, confirm, "confirm": ConfirmBinding;
}

/// The built-in bindings, read once.
static BUILT_IN: LazyLock<Keymaps> = LazyLock::new(|| {
    let mut keymaps = Keymaps::empty();
    let (commands, _) = config::parse(DEFAULTS);
    for command in commands {
        if let Command::Map(keys, bound) = command {
            keymaps.bind(keys, bound);
        }
    }
    keymaps
});

impl Default for Keymaps {
    /// The built-in bindings, from `defaults.toml`.
    fn default() -> Self {
        BUILT_IN.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::parse_keys;

    #[test]
    fn the_built_in_configuration_is_read_without_a_problem() {
        let (_, problems) = config::parse(DEFAULTS);
        assert_eq!(problems, []);
    }

    /// Every action can be reached with the built-in keys, except `help`,
    /// which `:help` shows.
    #[test]
    fn every_action_has_a_key_by_default() {
        let keymaps = Keymaps::default();
        for &mode in Mode::ALL {
            for name in mode.names() {
                let Some(bound) = mode.bound(name) else {
                    panic!("{name} has no binding");
                };
                let bound_somewhere = keymaps
                    .sorted()
                    .iter()
                    .any(|&(in_mode, _, to)| in_mode == mode.name() && to == bound.name());
                assert!(
                    bound_somewhere || name == "help",
                    "{} {name} has no key",
                    mode.name()
                );
            }
        }
    }

    #[test]
    fn modes_and_what_they_bind_have_names() {
        for &mode in Mode::ALL {
            assert_eq!(Mode::named(mode.name()), Some(mode));
            for name in mode.names() {
                let bound = mode.bound(name).unwrap();
                assert_eq!(bound.mode(), mode);
                assert_eq!(bound.name(), name);
            }
        }
        assert_eq!(Mode::named("insert"), None);
        assert_eq!(Mode::Visual.bound("scroll-down"), None);
    }

    #[test]
    fn modes_are_bound_apart() {
        let mut keymaps = Keymaps::default();
        let j = parse_keys("j").unwrap();
        keymaps.bind(j.clone(), Bound::Visual(visual::Binding::Up));
        assert!(keymaps.unbind(Mode::Normal, &j));
        assert!(!keymaps.unbind(Mode::Normal, &j));
        assert_eq!(
            keymaps.visual.keys_for(visual::Binding::Up).count(),
            3,
            "k, up and j"
        );
        keymaps.clear(Mode::Visual);
        assert_eq!(keymaps.visual.keys_for(visual::Binding::Up).count(), 0);
        assert!(keymaps.recent.keys_for(picker::Binding::Up).count() > 0);
    }
}
