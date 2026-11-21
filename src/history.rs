//! Remembering where each document was left.
//!
//! The view of every document that has been opened is kept in a small
//! SQLite database under the user's data directory, keyed by file path, and
//! restored the next time that file is opened. The time each document was
//! last left is kept too, so that recent documents can be listed.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::layout::Rotation;
use crate::reader::Place;

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
    /// How tall the document was, in points: a book laid out on the window
    /// comes out taller or shorter in a window of another size, and the
    /// view is moved to match. Nothing for views saved before this was
    /// kept.
    pub extent: f32,
}

/// Why the history could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no data directory: none of XDG_DATA_HOME, LOCALAPPDATA or HOME is set")]
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
        // Nor do those from before recent documents were listed. Their
        // documents count as left at the start of 1970, so they sort last.
        let has_left = connection
            .prepare("SELECT 1 FROM pragma_table_info('view') WHERE name = 'left_at'")?
            .exists([])?;
        if !has_left {
            connection
                .execute_batch("ALTER TABLE view ADD COLUMN left_at INTEGER NOT NULL DEFAULT 0")?;
        }
        // Nor do those from before books were laid out on the window.
        let has_extent = connection
            .prepare("SELECT 1 FROM pragma_table_info('view') WHERE name = 'extent'")?
            .exists([])?;
        if !has_extent {
            connection
                .execute_batch("ALTER TABLE view ADD COLUMN extent REAL NOT NULL DEFAULT 0")?;
        }
        // Marks: a letter for a place. A small letter belongs to one
        // document; a capital belongs to all, and is in the table once.
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS mark (
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                page INTEGER NOT NULL,
                down REAL NOT NULL,
                PRIMARY KEY (path, name)
            ) STRICT",
        )?;
        // Lines typed at the prompts, each once, numbered in the order
        // they were last typed.
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS typed (
                prompt TEXT NOT NULL,
                line   TEXT NOT NULL,
                seq    INTEGER NOT NULL,
                PRIMARY KEY (prompt, line)
            ) STRICT",
        )?;
        Ok(Self { connection })
    }

    /// Puts a mark at a place in a document. A capital mark leaves any
    /// other document it was in.
    pub fn set_mark(&self, path: &Path, name: char, place: Place) -> Result<(), Error> {
        let name = name.to_string();
        if name.chars().all(char::is_uppercase) {
            self.connection
                .execute("DELETE FROM mark WHERE name = ?1", params![name])?;
        }
        self.connection.execute(
            "INSERT INTO mark (path, name, page, down) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (path, name) DO UPDATE SET page = excluded.page, down = excluded.down",
            params![path.to_string_lossy(), name, place.page, place.down],
        )?;
        Ok(())
    }

    /// The marks in a document, small and capital, by letter.
    pub fn marks(&self, path: &Path) -> Result<Vec<(char, Place)>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT name, page, down FROM mark WHERE path = ?1 ORDER BY name")?;
        let marks = statement
            .query_map(params![path.to_string_lossy()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    Place {
                        page: row.get(1)?,
                        down: row.get(2)?,
                    },
                ))
            })?
            .filter_map(|mark| {
                mark.map(|(name, place)| Some((name.chars().next()?, place)))
                    .transpose()
            })
            .collect::<Result<_, _>>()?;
        Ok(marks)
    }

    /// Where a capital mark is: its document and its place there.
    pub fn global_mark(&self, name: char) -> Result<Option<(PathBuf, Place)>, Error> {
        let mark = self
            .connection
            .query_row(
                "SELECT path, page, down FROM mark WHERE name = ?1",
                params![name.to_string()],
                |row| {
                    Ok((
                        PathBuf::from(row.get::<_, String>(0)?),
                        Place {
                            page: row.get(1)?,
                            down: row.get(2)?,
                        },
                    ))
                },
            )
            .optional()?;
        Ok(mark)
    }

    /// Every capital mark, by letter, with its document.
    pub fn global_marks(&self) -> Result<Vec<(char, PathBuf)>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT name, path FROM mark WHERE name BETWEEN 'A' AND 'Z' ORDER BY name")?;
        let marks = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    PathBuf::from(row.get::<_, String>(1)?),
                ))
            })?
            .filter_map(|mark| {
                mark.map(|(name, path)| Some((name.chars().next()?, path)))
                    .transpose()
            })
            .collect::<Result<_, _>>()?;
        Ok(marks)
    }

    /// Takes marks away: those of a document with these letters, small or
    /// capital.
    pub fn delete_marks(&self, path: &Path, names: &[char]) -> Result<(), Error> {
        for name in names {
            let name = name.to_string();
            if name.chars().all(char::is_uppercase) {
                self.connection
                    .execute("DELETE FROM mark WHERE name = ?1", params![name])?;
            } else {
                self.connection.execute(
                    "DELETE FROM mark WHERE path = ?1 AND name = ?2",
                    params![path.to_string_lossy(), name],
                )?;
            }
        }
        Ok(())
    }

    /// The view a document was left in, if it has been opened before.
    pub fn load(&self, path: &Path) -> Result<Option<View>, Error> {
        let view = self
            .connection
            .query_row(
                "SELECT x, y, zoom, fit, rotation, recolor, trim, extent FROM view WHERE path = ?1",
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
                        extent: row.get(7)?,
                    })
                },
            )
            .optional()?;
        Ok(view)
    }

    /// Records the view a document is being left in, and that it was left
    /// just now.
    pub fn save(&self, path: &Path, view: View) -> Result<(), Error> {
        self.save_at(path, view, now())
    }

    /// Records the view a document was left in, and when, in seconds since
    /// the Unix epoch.
    fn save_at(&self, path: &Path, view: View, left_at: i64) -> Result<(), Error> {
        self.connection.execute(
            "INSERT INTO view (path, x, y, zoom, fit, rotation, recolor, trim, left_at, extent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (path) DO UPDATE SET
                x = excluded.x, y = excluded.y, zoom = excluded.zoom,
                fit = excluded.fit, rotation = excluded.rotation,
                recolor = excluded.recolor, trim = excluded.trim,
                left_at = excluded.left_at, extent = excluded.extent",
            params![
                path.to_string_lossy(),
                view.x,
                view.y,
                view.zoom,
                view.fit.code(),
                view.rotation.quarters(),
                view.recolor,
                view.trim,
                left_at,
                view.extent,
            ],
        )?;
        Ok(())
    }

    /// The lines typed at a prompt, the oldest first, as many as are kept.
    pub fn typed(&self, prompt: &str) -> Result<Vec<String>, Error> {
        let mut statement = self.connection.prepare(
            "SELECT line FROM (
                SELECT line, seq FROM typed WHERE prompt = ?1 ORDER BY seq DESC LIMIT ?2
            ) ORDER BY seq",
        )?;
        let kept = i64::try_from(crate::recall::KEPT).unwrap_or(i64::MAX);
        let lines = statement
            .query_map(rusqlite::params![prompt, kept], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<_, _>>()?;
        Ok(lines)
    }

    /// Remembers a line typed at a prompt as the newest, and forgets the
    /// oldest beyond as many as are kept.
    pub fn add_typed(&self, prompt: &str, line: &str) -> Result<(), Error> {
        let kept = i64::try_from(crate::recall::KEPT).unwrap_or(i64::MAX);
        self.connection.execute(
            "INSERT INTO typed (prompt, line, seq)
                VALUES (?1, ?2, (SELECT COALESCE(MAX(seq), 0) + 1 FROM typed))
                ON CONFLICT (prompt, line) DO UPDATE SET seq = excluded.seq",
            rusqlite::params![prompt, line],
        )?;
        self.connection.execute(
            "DELETE FROM typed WHERE prompt = ?1 AND seq NOT IN (
                SELECT seq FROM typed WHERE prompt = ?1 ORDER BY seq DESC LIMIT ?2
            )",
            rusqlite::params![prompt, kept],
        )?;
        Ok(())
    }

    /// Every document in the history, the one left most recently first.
    pub fn recent(&self) -> Result<Vec<PathBuf>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT path FROM view ORDER BY left_at DESC, rowid DESC")?;
        let paths = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|path| path.map(PathBuf::from))
            .collect::<Result<_, _>>()?;
        Ok(paths)
    }
}

/// The time now, in whole seconds since the Unix epoch. A clock set before
/// 1970 reads as the epoch itself.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// The directory the database lives in.
fn data_directory() -> Option<PathBuf> {
    crate::dirs::data()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_kept_by_document_and_capitals_across_them() {
        let history = memory();
        let one = Path::new("/tmp/one.pdf");
        let two = Path::new("/tmp/two.pdf");
        let at = |page, down| Place { page, down };
        history.set_mark(one, 'a', at(3, 0.5)).unwrap();
        history.set_mark(two, 'a', at(7, 0.0)).unwrap();
        history.set_mark(one, 'a', at(4, 0.25)).unwrap();
        history.set_mark(one, 'Q', at(1, 0.0)).unwrap();
        assert_eq!(
            history.marks(one).unwrap(),
            [('Q', at(1, 0.0)), ('a', at(4, 0.25))]
        );
        assert_eq!(history.marks(two).unwrap(), [('a', at(7, 0.0))]);

        // A capital is in one document only.
        history.set_mark(two, 'Q', at(9, 0.5)).unwrap();
        assert_eq!(history.marks(one).unwrap(), [('a', at(4, 0.25))]);
        assert_eq!(
            history.global_mark('Q').unwrap(),
            Some((two.to_path_buf(), at(9, 0.5)))
        );
        assert_eq!(history.global_marks().unwrap(), [('Q', two.to_path_buf())]);

        history.delete_marks(two, &['a', 'Q']).unwrap();
        assert_eq!(history.marks(two).unwrap(), []);
        assert_eq!(history.global_mark('Q').unwrap(), None);
    }

    #[test]
    fn lines_typed_are_kept_by_prompt_newest_last() {
        let history = memory();
        history.add_typed("command", "zoom 150").unwrap();
        history.add_typed("search", "needle").unwrap();
        history.add_typed("command", "open a.pdf").unwrap();
        history.add_typed("command", "zoom 150").unwrap();
        assert_eq!(
            history.typed("command").unwrap(),
            ["open a.pdf", "zoom 150"]
        );
        assert_eq!(history.typed("search").unwrap(), ["needle"]);
        for number in 0..crate::recall::KEPT {
            history.add_typed("search", &number.to_string()).unwrap();
        }
        let searches = history.typed("search").unwrap();
        assert_eq!(searches.len(), crate::recall::KEPT);
        assert_eq!(searches.first().map(String::as_str), Some("0"));
    }

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
        assert!(view.extent.abs() < f32::EPSILON);
        // Opening it a second time changes nothing.
        let history = History::with(history.connection).unwrap();
        assert!(history.load(Path::new("/tmp/old.pdf")).unwrap().is_some());
    }

    #[test]
    fn recent_documents_come_newest_first() {
        let history = memory();
        let view = View {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            fit: Fit::None,
            rotation: Rotation::None,
            recolor: false,
            trim: false,
            extent: 0.0,
        };
        history.save_at(Path::new("/tmp/a.pdf"), view, 100).unwrap();
        history.save_at(Path::new("/tmp/b.pdf"), view, 300).unwrap();
        history.save_at(Path::new("/tmp/c.pdf"), view, 200).unwrap();
        assert_eq!(
            history.recent().unwrap(),
            ["/tmp/b.pdf", "/tmp/c.pdf", "/tmp/a.pdf"].map(PathBuf::from)
        );
        // Leaving a document again brings it to the front.
        history.save_at(Path::new("/tmp/a.pdf"), view, 400).unwrap();
        assert_eq!(
            history.recent().unwrap().first(),
            Some(&PathBuf::from("/tmp/a.pdf"))
        );
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
            extent: 4000.0,
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
