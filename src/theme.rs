//! Themes: the colours of everything around the pages, and the colours
//! pages are recoloured in unless set otherwise.
//!
//! Besides pdvim's own dark and light themes there are the four flavours
//! of Catppuccin (<https://catppuccin.com/palette>): Latte, which is light,
//! and Frappé, Macchiato and Mocha, from lightest to darkest.

use iced::Color;

use crate::recolor::{Palette, Rgb};

/// A theme by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    PdvimDark,
    PdvimLight,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
}

impl Theme {
    /// Every theme with its name.
    pub const NAMES: [(Self, &'static str); 6] = [
        (Self::PdvimDark, "pdvim-dark"),
        (Self::PdvimLight, "pdvim-light"),
        (Self::CatppuccinLatte, "catppuccin-latte"),
        (Self::CatppuccinFrappe, "catppuccin-frappe"),
        (Self::CatppuccinMacchiato, "catppuccin-macchiato"),
        (Self::CatppuccinMocha, "catppuccin-mocha"),
    ];

    /// The names of every theme.
    pub fn names() -> impl Iterator<Item = &'static str> {
        Self::NAMES.iter().map(|(_, name)| *name)
    }

    /// The name of the theme.
    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(theme, _)| *theme == self)
            .map_or("", |(_, name)| name)
    }

    /// The theme with this name.
    pub fn named(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(_, known)| *known == name)
            .map(|(theme, _)| *theme)
    }

    /// The colours of the theme.
    pub const fn colours(self) -> &'static Colours {
        match self {
            Self::PdvimDark => &PDVIM_DARK,
            Self::PdvimLight => &PDVIM_LIGHT,
            Self::CatppuccinLatte => &LATTE,
            Self::CatppuccinFrappe => &FRAPPE,
            Self::CatppuccinMacchiato => &MACCHIATO,
            Self::CatppuccinMocha => &MOCHA,
        }
    }

    /// The colours of the box naming the mode.
    pub const fn modes(self) -> ModeColours {
        match self {
            Self::PdvimDark => PDVIM_DARK_MODES,
            Self::PdvimLight => PDVIM_LIGHT_MODES,
            Self::CatppuccinLatte => LATTE_MODES,
            Self::CatppuccinFrappe => FRAPPE_MODES,
            Self::CatppuccinMacchiato => MACCHIATO_MODES,
            Self::CatppuccinMocha => MOCHA_MODES,
        }
    }

    /// The iced theme for what is not given colours of its own.
    pub const fn iced(self) -> iced::Theme {
        match self {
            Self::PdvimDark => iced::Theme::Dark,
            Self::PdvimLight => iced::Theme::Light,
            Self::CatppuccinLatte => iced::Theme::CatppuccinLatte,
            Self::CatppuccinFrappe => iced::Theme::CatppuccinFrappe,
            Self::CatppuccinMacchiato => iced::Theme::CatppuccinMacchiato,
            Self::CatppuccinMocha => iced::Theme::CatppuccinMocha,
        }
    }
}

/// The colours of a theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colours {
    /// The colour behind the pages.
    pub backdrop: Color,
    /// The status bar, its text, and an error in it.
    pub bar: Color,
    pub bar_text: Color,
    pub bar_error: Color,
    /// The lists over the pages: their background, the selected entry,
    /// their frame, and quieter text such as numbers and hints.
    pub list: Color,
    pub list_selected: Color,
    pub list_border: Color,
    pub list_number: Color,
    /// The tab bar and the tabs not in view, the tab in view, and the
    /// names of tabs not in view.
    pub tabs: Color,
    pub tab_current: Color,
    pub tab_text: Color,
    /// What recoloured pages are drawn in, unless set otherwise.
    pub recolor: Palette,
}

/// The colour of the box naming each kind of mode, and of the name in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModeColours {
    pub normal: Color,
    /// Selecting text, and picking where to start.
    pub visual: Color,
    /// Typing into a field of a form.
    pub insert: Color,
    /// Typing at the bottom line, or answering it.
    pub command: Color,
    /// Labels over links and markings, to pick one.
    pub label: Color,
    /// Lists and panels over the pages.
    pub list: Color,
    pub text: Color,
}

const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::from_rgb8(red, green, blue)
}

const PDVIM_DARK: Colours = Colours {
    backdrop: rgb(0x10, 0x10, 0x10),
    bar: rgb(0x20, 0x20, 0x20),
    bar_text: rgb(0xd0, 0xd0, 0xd0),
    bar_error: rgb(0xff, 0x6b, 0x6b),
    list: rgb(0x1a, 0x1a, 0x1a),
    list_selected: rgb(0x3a, 0x3a, 0x3a),
    list_border: rgb(0x40, 0x40, 0x40),
    list_number: rgb(0x80, 0x80, 0x80),
    tabs: rgb(0x18, 0x18, 0x18),
    tab_current: rgb(0x30, 0x30, 0x30),
    tab_text: rgb(0x90, 0x90, 0x90),
    recolor: Palette {
        paper: Rgb(0x1a, 0x1a, 0x1a),
        ink: Rgb(0xdd, 0xdd, 0xdd),
    },
};

const PDVIM_DARK_MODES: ModeColours = ModeColours {
    normal: rgb(0x6c, 0x9e, 0xe8),
    visual: rgb(0xb4, 0x8e, 0xd8),
    insert: rgb(0x8f, 0xc0, 0x6a),
    command: rgb(0xe0, 0x96, 0x5c),
    label: rgb(0xe5, 0xc0, 0x7b),
    list: rgb(0x56, 0xb6, 0xc2),
    text: rgb(0x10, 0x10, 0x10),
};

const PDVIM_LIGHT: Colours = Colours {
    backdrop: rgb(0xd4, 0xd4, 0xd4),
    bar: rgb(0xe8, 0xe8, 0xe8),
    bar_text: rgb(0x20, 0x20, 0x20),
    bar_error: rgb(0xc0, 0x1c, 0x1c),
    list: rgb(0xf6, 0xf6, 0xf6),
    list_selected: rgb(0xd6, 0xd6, 0xd6),
    list_border: rgb(0xb4, 0xb4, 0xb4),
    list_number: rgb(0x6a, 0x6a, 0x6a),
    tabs: rgb(0xdc, 0xdc, 0xdc),
    tab_current: rgb(0xfa, 0xfa, 0xfa),
    tab_text: rgb(0x5a, 0x5a, 0x5a),
    recolor: Palette {
        paper: Rgb(0xf4, 0xf1, 0xe8),
        ink: Rgb(0x2a, 0x2a, 0x2a),
    },
};

const PDVIM_LIGHT_MODES: ModeColours = ModeColours {
    normal: rgb(0x2a, 0x6f, 0xdb),
    visual: rgb(0x8a, 0x4f, 0xd0),
    insert: rgb(0x3c, 0x8a, 0x2e),
    command: rgb(0xc8, 0x5a, 0x14),
    label: rgb(0xa8, 0x7a, 0x00),
    list: rgb(0x1f, 0x84, 0x84),
    text: rgb(0xff, 0xff, 0xff),
};

/// The colours of a Catppuccin flavour, from its palette: the crust,
/// mantle and base backgrounds, surfaces 0 to 2, overlay 1, subtext 0,
/// text and red; and blue, mauve, green, peach, yellow and teal for the
/// modes, named in the colour of `on_accent`. Pages are recoloured into
/// text on base.
struct Flavour {
    accents: [Rgb; 6],
    on_accent: Rgb,
    crust: Rgb,
    mantle: Rgb,
    base: Rgb,
    surface: [Rgb; 3],
    overlay1: Rgb,
    subtext0: Rgb,
    text: Rgb,
    red: Rgb,
}

impl Flavour {
    const fn modes(&self) -> ModeColours {
        const fn c(Rgb(red, green, blue): Rgb) -> Color {
            rgb(red, green, blue)
        }
        let [blue, mauve, green, peach, yellow, teal] = self.accents;
        ModeColours {
            normal: c(blue),
            visual: c(mauve),
            insert: c(green),
            command: c(peach),
            label: c(yellow),
            list: c(teal),
            text: c(self.on_accent),
        }
    }

    const fn colours(&self) -> Colours {
        const fn c(Rgb(red, green, blue): Rgb) -> Color {
            rgb(red, green, blue)
        }
        let [surface0, surface1, surface2] = self.surface;
        Colours {
            backdrop: c(self.crust),
            bar: c(self.mantle),
            bar_text: c(self.text),
            bar_error: c(self.red),
            list: c(self.base),
            list_selected: c(surface1),
            list_border: c(surface2),
            list_number: c(self.overlay1),
            tabs: c(self.crust),
            tab_current: c(surface0),
            tab_text: c(self.subtext0),
            recolor: Palette {
                paper: self.base,
                ink: self.text,
            },
        }
    }
}

const LATTE_FLAVOUR: Flavour = Flavour {
    accents: [
        Rgb(0x1e, 0x66, 0xf5),
        Rgb(0x88, 0x39, 0xef),
        Rgb(0x40, 0xa0, 0x2b),
        Rgb(0xfe, 0x64, 0x0b),
        Rgb(0xdf, 0x8e, 0x1d),
        Rgb(0x17, 0x92, 0x99),
    ],
    on_accent: Rgb(0xef, 0xf1, 0xf5),
    crust: Rgb(0xdc, 0xe0, 0xe8),
    mantle: Rgb(0xe6, 0xe9, 0xef),
    base: Rgb(0xef, 0xf1, 0xf5),
    surface: [
        Rgb(0xcc, 0xd0, 0xda),
        Rgb(0xbc, 0xc0, 0xcc),
        Rgb(0xac, 0xb0, 0xbe),
    ],
    overlay1: Rgb(0x8c, 0x8f, 0xa1),
    subtext0: Rgb(0x6c, 0x6f, 0x85),
    text: Rgb(0x4c, 0x4f, 0x69),
    red: Rgb(0xd2, 0x0f, 0x39),
};
const LATTE: Colours = LATTE_FLAVOUR.colours();
const LATTE_MODES: ModeColours = LATTE_FLAVOUR.modes();

const FRAPPE_FLAVOUR: Flavour = Flavour {
    accents: [
        Rgb(0x8c, 0xaa, 0xee),
        Rgb(0xca, 0x9e, 0xe6),
        Rgb(0xa6, 0xd1, 0x89),
        Rgb(0xef, 0x9f, 0x76),
        Rgb(0xe5, 0xc8, 0x90),
        Rgb(0x81, 0xc8, 0xbe),
    ],
    on_accent: Rgb(0x23, 0x26, 0x34),
    crust: Rgb(0x23, 0x26, 0x34),
    mantle: Rgb(0x29, 0x2c, 0x3c),
    base: Rgb(0x30, 0x34, 0x46),
    surface: [
        Rgb(0x41, 0x45, 0x59),
        Rgb(0x51, 0x57, 0x6d),
        Rgb(0x62, 0x68, 0x80),
    ],
    overlay1: Rgb(0x83, 0x8b, 0xa7),
    subtext0: Rgb(0xa5, 0xad, 0xce),
    text: Rgb(0xc6, 0xd0, 0xf5),
    red: Rgb(0xe7, 0x82, 0x84),
};
const FRAPPE: Colours = FRAPPE_FLAVOUR.colours();
const FRAPPE_MODES: ModeColours = FRAPPE_FLAVOUR.modes();

const MACCHIATO_FLAVOUR: Flavour = Flavour {
    accents: [
        Rgb(0x8a, 0xad, 0xf4),
        Rgb(0xc6, 0xa0, 0xf6),
        Rgb(0xa6, 0xda, 0x95),
        Rgb(0xf5, 0xa9, 0x7f),
        Rgb(0xee, 0xd4, 0x9f),
        Rgb(0x8b, 0xd5, 0xca),
    ],
    on_accent: Rgb(0x18, 0x19, 0x26),
    crust: Rgb(0x18, 0x19, 0x26),
    mantle: Rgb(0x1e, 0x20, 0x30),
    base: Rgb(0x24, 0x27, 0x3a),
    surface: [
        Rgb(0x36, 0x3a, 0x4f),
        Rgb(0x49, 0x4d, 0x64),
        Rgb(0x5b, 0x60, 0x78),
    ],
    overlay1: Rgb(0x80, 0x87, 0xa2),
    subtext0: Rgb(0xa5, 0xad, 0xcb),
    text: Rgb(0xca, 0xd3, 0xf5),
    red: Rgb(0xed, 0x87, 0x96),
};
const MACCHIATO: Colours = MACCHIATO_FLAVOUR.colours();
const MACCHIATO_MODES: ModeColours = MACCHIATO_FLAVOUR.modes();

const MOCHA_FLAVOUR: Flavour = Flavour {
    accents: [
        Rgb(0x89, 0xb4, 0xfa),
        Rgb(0xcb, 0xa6, 0xf7),
        Rgb(0xa6, 0xe3, 0xa1),
        Rgb(0xfa, 0xb3, 0x87),
        Rgb(0xf9, 0xe2, 0xaf),
        Rgb(0x94, 0xe2, 0xd5),
    ],
    on_accent: Rgb(0x11, 0x11, 0x1b),
    crust: Rgb(0x11, 0x11, 0x1b),
    mantle: Rgb(0x18, 0x18, 0x25),
    base: Rgb(0x1e, 0x1e, 0x2e),
    surface: [
        Rgb(0x31, 0x32, 0x44),
        Rgb(0x45, 0x47, 0x5a),
        Rgb(0x58, 0x5b, 0x70),
    ],
    overlay1: Rgb(0x7f, 0x84, 0x9c),
    subtext0: Rgb(0xa6, 0xad, 0xc8),
    text: Rgb(0xcd, 0xd6, 0xf4),
    red: Rgb(0xf3, 0x8b, 0xa8),
};
const MOCHA: Colours = MOCHA_FLAVOUR.colours();
const MOCHA_MODES: ModeColours = MOCHA_FLAVOUR.modes();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_has_a_name_and_back() {
        for (theme, name) in Theme::NAMES {
            assert_eq!(Theme::named(name), Some(theme));
            assert_eq!(theme.name(), name);
        }
        assert_eq!(Theme::named("solarized"), None);
    }

    #[test]
    fn catppuccin_recolours_into_text_on_base() {
        let mocha = Theme::CatppuccinMocha.colours();
        assert_eq!(mocha.recolor.paper, Rgb(0x1e, 0x1e, 0x2e));
        assert_eq!(mocha.recolor.ink, Rgb(0xcd, 0xd6, 0xf4));
        assert_eq!(mocha.bar, rgb(0x18, 0x18, 0x25));
    }
}
