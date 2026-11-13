//! Remembering where each document was left.
//!
//! The view of every document that has been opened is kept in a small
//! SQLite database under the user's data directory, keyed by file path, and
//! restored the next time that file is opened.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::layout::Rotation;

/// How the window was fitted to the page, if it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// The zoom is whatever was last chosen.
    #[default]
    None,
    /// The page width fills the window.
    Width,
    /// A whole page fills the window.
    Page,
}

impl Fit {
    const fn code(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Width => 1,
            Self::Page => 2,
        }
    }

    const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Width,
            2 => Self::Page,
            _ => Self::None,
        }
    }
}

/// The view of a document: everything needed to show it again as it was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// The left edge of the window in the document, in points.
    pub x: f32,
    /// The top edge of the window in the document, in points.
    pub y: f32,
    /// Screen pixels per point.
    pub zoom: f32,
    pub fit: Fit,
    pub rotation: Rotation,
    pub recolor: bool,
    /// Whether the margins are trimmed.
    pub trim: bool,
}

/// Why the history could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no data directory: neither XDG_DATA_HOME nor HOME is set")]
    NoDataDirectory,
    #[error("cannot create {0}: {1}")]
    CreateDirectory(PathBuf, std::io::Error),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
}

/// The history database.
pub struct History {
    connection: Connection,
}

impl History {
    /// Opens the database in the user's data directory, creating it first
    /// if need be.
    pub fn open() -> Result<Self, Error> {
        let directory = data_directory().ok_or(Error::NoDataDirectory)?;
        std::fs::create_dir_all(&directory)
            .map_err(|error| Error::CreateDirectory(directory.clone(), error))?;
        Self::with(Connection::open(directory.join("history.sqlite"))?)
    }

    /// Prepares a connection for use as the history.
    fn with(connection: Connection) -> Result<Self, Error> {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS view (
                path     TEXT PRIMARY KEY,
                x        REAL NOT NULL,
                y        REAL NOT NULL,
                zoom     REAL NOT NULL,
                fit      INTEGER NOT NULL,
                rotation INTEGER NOT NULL,
                recolor  INTEGER NOT NULL
            ) STRICT",
        )?;
        // Databases from before margins could be trimmed lack the column.
        let has_trim = connection
            .prepare("SELECT 1 FROM pragma_table_info('view') WHERE name = 'trim'")?
            .exists([])?;
        if !has_trim {
            connection
                .execute_batch("ALTER TABLE view ADD COLUMN trim INTEGER NOT NULL DEFAULT 0")?;
        }
        Ok(Self { connection })
    }

    /// The view a document was left in, if it has been opened before.
    pub fn load(&self, path: &Path) -> Result<Option<View>, Error> {
        let view = self
            .connection
            .query_row(
                "SELECT x, y, zoom, fit, rotation, recolor, trim FROM view WHERE path = ?1",
                params![path.to_string_lossy()],
                |row| {
                    Ok(View {
                        x: row.get(0)?,
                        y: row.get(1)?,
                        zoom: row.get(2)?,
                        fit: Fit::from_code(row.get(3)?),
                        rotation: Rotation::from_quarters(row.get(4)?),
                        recolor: row.get(5)?,
                        trim: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(view)
    }

    /// Records the view a document is being left in.
    pub fn save(&self, path: &Path, view: View) -> Result<(), Error> {
        self.connection.execute(
            "INSERT INTO view (path, x, y, zoom, fit, rotation, recolor, trim)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (path) DO UPDATE SET
                x = excluded.x, y = excluded.y, zoom = excluded.zoom,
                fit = excluded.fit, rotation = excluded.rotation,
                recolor = excluded.recolor, trim = excluded.trim",
            params![
                path.to_string_lossy(),
                view.x,
                view.y,
                view.zoom,
                view.fit.code(),
                view.rotation.quarters(),
                view.recolor,
                view.trim,
            ],
        )?;
        Ok(())
    }
}

/// The directory the database lives in: `$XDG_DATA_HOME/pdvim`, or
/// `~/.local/share/pdvim` when that is not set.
fn data_directory() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;
    Some(base.join("pdvim"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> History {
        History::with(Connection::open_in_memory().unwrap()).unwrap()
    }

    #[test]
    fn an_older_database_gains_the_trim_column() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE view (
                    path TEXT PRIMARY KEY, x REAL NOT NULL, y REAL NOT NULL,
                    zoom REAL NOT NULL, fit INTEGER NOT NULL,
                    rotation INTEGER NOT NULL, recolor INTEGER NOT NULL
                ) STRICT;
                INSERT INTO view VALUES ('/tmp/old.pdf', 0, 50, 1.5, 0, 0, 1);",
            )
            .unwrap();
        let history = History::with(connection).unwrap();
        let view = history.load(Path::new("/tmp/old.pdf")).unwrap().unwrap();
        assert!(view.recolor);
        assert!(!view.trim);
        // Opening it a second time changes nothing.
        let history = History::with(history.connection).unwrap();
        assert!(history.load(Path::new("/tmp/old.pdf")).unwrap().is_some());
    }

    #[test]
    fn an_unseen_document_has_no_view() {
        let history = memory();
        assert_eq!(history.load(Path::new("/tmp/a.pdf")).unwrap(), None);
    }

    #[test]
    fn a_saved_view_comes_back_and_can_be_replaced() {
        let history = memory();
        let path = Path::new("/tmp/a.pdf");
        let first = View {
            x: -12.5,
            y: 3000.25,
            zoom: 1.5,
            fit: Fit::Width,
            rotation: Rotation::Quarter,
            recolor: true,
            trim: true,
        };
        history.save(path, first).unwrap();
        assert_eq!(history.load(path).unwrap(), Some(first));

        let second = View {
            y: 10.0,
            fit: Fit::None,
            recolor: false,
            ..first
        };
        history.save(path, second).unwrap();
        assert_eq!(history.load(path).unwrap(), Some(second));
        assert_eq!(history.load(Path::new("/tmp/b.pdf")).unwrap(), None);
    }
}
