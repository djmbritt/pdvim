//! Where pdvim keeps its files: the configuration, and the reading history.
//!
//! The XDG variables win wherever they are set. Otherwise Windows has its
//! own places, `%APPDATA%` and `%LOCALAPPDATA%`, and everywhere else, macOS
//! included, the XDG defaults under the home directory are used, as most
//! keyboard-driven tools do.

use std::path::PathBuf;

/// The directory of the configuration: `$XDG_CONFIG_HOME/pdvim`,
/// `%APPDATA%\pdvim` on Windows, or `~/.config/pdvim`.
pub fn config() -> Option<PathBuf> {
    pdvim("XDG_CONFIG_HOME", "APPDATA", ".config")
}

/// The directory of the reading history: `$XDG_DATA_HOME/pdvim`,
/// `%LOCALAPPDATA%\pdvim` on Windows, or `~/.local/share/pdvim`.
pub fn data() -> Option<PathBuf> {
    pdvim("XDG_DATA_HOME", "LOCALAPPDATA", ".local/share")
}

/// The `pdvim` directory in the first of these places that is known.
fn pdvim(xdg: &str, windows: &str, under_home: &str) -> Option<PathBuf> {
    let absolute = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    absolute(xdg)
        .or_else(|| absolute(windows).filter(|_| cfg!(windows)))
        .or_else(|| absolute("HOME").map(|home| home.join(under_home)))
        .map(|base| base.join("pdvim"))
}
