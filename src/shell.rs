//! `:!`: running a shell command, as vim does, with `%` standing for the
//! file of the document in view.
//!
//! The command runs in `sh -c`, or `cmd /C` on Windows, on a thread of its
//! own, so that pdvim goes on while it runs: `:!xdg-open %` hands the
//! document to another program, and `:!cp % ~/papers/` copies it. What the
//! command prints is not shown whole, only its last line, in the status
//! bar.
//!
//! The command is told where the reader is in environment variables, see
//! [`Context`], so that a script bound to a key can act on the page or the
//! selection: `:!sdcv "$PDVIM_SELECTION"`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use futures::channel::oneshot;

/// What stops a command from being run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("no file for % to stand for")]
    NoFile,
    #[error("{0} is not a name % can stand for")]
    NotUnicode(String),
    #[error("{0}")]
    NotRun(String),
}

/// How a command ended: whether it succeeded, and the last line it
/// printed, if any, on standard output or else on standard error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub succeeded: bool,
    /// The exit status, when the command was not killed by a signal.
    pub code: Option<i32>,
    pub last_line: Option<String>,
}

/// The most of the selection put in `$PDVIM_SELECTION`, in bytes. Linux
/// refuses to start a program with a variable longer than 128 KiB.
const MOST_SELECTION: usize = 64 * 1024;

/// Where the reader is, as a command run from it is told in its
/// environment. What is not known is not set, even when pdvim itself was
/// started with it set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// `$PDVIM_FILE`: the document's file, as `%` is.
    pub file: Option<PathBuf>,
    /// `$PDVIM_PAGE`: the page in view, counted from 1.
    pub page: Option<u32>,
    /// `$PDVIM_PAGES`: how many pages the document has.
    pub pages: Option<u32>,
    /// `$PDVIM_PAGE_LABEL`: the number printed on the page in view, such
    /// as `xii`, when the document gives one.
    pub label: Option<String>,
    /// `$PDVIM_TITLE`: the document's title, or its file's name.
    pub title: Option<String>,
    /// `$PDVIM_SELECTION`: the text selected in visual mode, or else the
    /// text last selected in this document.
    pub selection: Option<String>,
    /// `$PDVIM_SEARCH`: what was last searched for in this document.
    pub search: Option<String>,
}

impl Context {
    /// Every variable, with its value when it has one.
    fn variables(&self) -> [(&'static str, Option<std::ffi::OsString>); 7] {
        let text = |value: Option<&String>| value.map(|value| without_nul(value).into());
        let number = |value: Option<u32>| value.map(|value| value.to_string().into());
        let selection = self.selection.as_deref().map(|selection| {
            let mut end = selection.len().min(MOST_SELECTION);
            while !selection.is_char_boundary(end) {
                end = end.saturating_sub(1);
            }
            without_nul(selection.get(..end).unwrap_or_default()).into()
        });
        [
            ("PDVIM_FILE", self.file.clone().map(PathBuf::into_os_string)),
            ("PDVIM_PAGE", number(self.page)),
            ("PDVIM_PAGES", number(self.pages)),
            ("PDVIM_PAGE_LABEL", text(self.label.as_ref())),
            ("PDVIM_TITLE", text(self.title.as_ref())),
            ("PDVIM_SELECTION", selection),
            ("PDVIM_SEARCH", text(self.search.as_ref())),
        ]
    }
}

/// Text with the NUL characters taken out, which no variable can hold.
fn without_nul(text: &str) -> String {
    text.replace('\0', "")
}

/// The command with each `%` replaced by the file's path, quoted for the
/// shell; `\%` is a `%` itself.
pub fn expand(command: &str, file: Option<&Path>) -> Result<String, Error> {
    let mut expanded = String::with_capacity(command.len());
    let mut characters = command.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' if characters.peek() == Some(&'%') => {
                characters.next();
                expanded.push('%');
            }
            '%' => {
                let file = file.ok_or(Error::NoFile)?;
                // A path that is not Unicode would be written with
                // replacement characters, and so name some other file.
                let path = file
                    .to_str()
                    .ok_or_else(|| Error::NotUnicode(file.display().to_string()))?;
                expanded.push_str(&quote(path));
            }
            _ => expanded.push(character),
        }
    }
    Ok(expanded)
}

/// A word the shell takes as it is, whatever is in it.
#[cfg(not(windows))]
fn quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// A word cmd takes as it is; paths on Windows cannot hold `"`.
#[cfg(windows)]
fn quote(word: &str) -> String {
    format!("\"{word}\"")
}

/// Runs a command in the shell, with nothing to read on standard input
/// and where the reader is in its environment, and answers once it has
/// finished.
pub fn run(command: String, context: Context) -> impl Future<Output = Result<Ran, Error>> {
    let (reply, answer) = oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("pdvim-shell".to_owned())
        .spawn(move || {
            let _: Result<(), _> = reply.send(run_now(&command, &context));
        });
    async move {
        spawned.map_err(|error| Error::NotRun(error.to_string()))?;
        answer
            .await
            .unwrap_or_else(|_| Err(Error::NotRun("the command was lost".to_owned())))
    }
}

/// Runs a command in the shell and waits for it.
fn run_now(command: &str, context: &Context) -> Result<Ran, Error> {
    #[cfg(windows)]
    let mut shell = {
        let mut shell = Command::new("cmd");
        shell.arg("/C").arg(command);
        shell
    };
    #[cfg(not(windows))]
    let mut shell = {
        let mut shell = Command::new("sh");
        shell.arg("-c").arg(command);
        shell
    };
    for (name, value) in context.variables() {
        match value {
            Some(value) => shell.env(name, value),
            None => shell.env_remove(name),
        };
    }
    let output = shell
        .stdin(Stdio::null())
        .output()
        .map_err(|error| Error::NotRun(format!("cannot run sh: {error}")))?;
    let last_line = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes)
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .map(str::to_owned)
    };
    Ok(Ran {
        succeeded: output.status.success(),
        code: output.status.code(),
        last_line: last_line(&output.stdout).or_else(|| last_line(&output.stderr)),
    })
}

/// What the status bar says once a command has finished, and whether it
/// is an error.
pub fn report(command: &str, ran: &Ran) -> (String, bool) {
    match (&ran.last_line, ran.succeeded, ran.code) {
        (Some(line), true, _) => (line.clone(), false),
        (None, true, _) => (format!("!{command}: done"), false),
        (Some(line), false, Some(code)) => (format!("!{command}: exit {code}: {line}"), true),
        (None, false, Some(code)) => (format!("!{command}: exit {code}"), true),
        (Some(line), false, None) => (format!("!{command}: killed: {line}"), true),
        (None, false, None) => (format!("!{command}: killed"), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Quoted for sh, and run in it.
    #[cfg(not(windows))]
    #[test]
    fn percent_is_the_file_quoted() {
        let file = Path::new("/books/it's here.pdf");
        assert_eq!(
            expand("cp % /tmp/", Some(file)),
            Ok(r"cp '/books/it'\''s here.pdf' /tmp/".to_owned())
        );
        assert_eq!(
            expand(r"printf 100\% %", Some(Path::new("a.pdf"))),
            Ok("printf 100% 'a.pdf'".to_owned())
        );
        assert_eq!(expand("ls", None), Ok("ls".to_owned()));
        assert_eq!(expand("ls %", None), Err(Error::NoFile));
    }

    #[cfg(unix)]
    #[test]
    fn a_file_whose_name_is_not_unicode_is_not_guessed_at() {
        use std::os::unix::ffi::OsStrExt;
        let file = Path::new(std::ffi::OsStr::from_bytes(b"/books/caf\xe9.pdf"));
        assert!(matches!(
            expand("cp % /tmp/", Some(file)),
            Err(Error::NotUnicode(_))
        ));
        assert_eq!(
            expand(r"echo 100\%", Some(file)),
            Ok("echo 100%".to_owned())
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn a_command_runs_and_says_how_it_ended() {
        let none = Context::default();
        let ran = run_now("echo one; echo two", &none).unwrap();
        assert_eq!(
            ran,
            Ran {
                succeeded: true,
                code: Some(0),
                last_line: Some("two".to_owned())
            }
        );
        let failed = run_now("echo nope >&2; exit 3", &none).unwrap();
        assert_eq!(
            report("false", &failed),
            ("!false: exit 3: nope".to_owned(), true)
        );
        // A quoted path reaches the command as one word, quote and all.
        let file = Path::new("it's a file.pdf");
        let echoed = run_now(&expand(r"printf '\%s\n' %", Some(file)).unwrap(), &none).unwrap();
        assert_eq!(echoed.last_line.as_deref(), Some("it's a file.pdf"));
    }

    #[cfg(not(windows))]
    #[test]
    fn the_command_is_told_where_the_reader_is() {
        let context = Context {
            file: Some(PathBuf::from("/books/a b.pdf")),
            page: Some(12),
            pages: Some(300),
            label: Some("xii".to_owned()),
            title: Some("A Book".to_owned()),
            selection: Some("two\nlines\0".to_owned()),
            search: Some("fox".to_owned()),
        };
        let told = |command: &str, context: &Context| {
            run_now(command, context)
                .unwrap()
                .last_line
                .unwrap_or_default()
        };
        let all = concat!(
            r#"printf '%s|' "$PDVIM_FILE" "$PDVIM_PAGE" "$PDVIM_PAGES" "#,
            r#""$PDVIM_PAGE_LABEL" "$PDVIM_TITLE" "$PDVIM_SEARCH"; "#,
            r#"printf '%s' "$PDVIM_SELECTION" | tr '\n' ' '"#,
        );
        assert_eq!(
            told(all, &context),
            "/books/a b.pdf|12|300|xii|A Book|fox|two lines"
        );
        // What is not known is not set at all, so that `${…-default}`
        // works.
        let unset = Context::default();
        assert_eq!(
            told(
                r#"echo "${PDVIM_PAGE-unset} ${PDVIM_SELECTION-unset}""#,
                &unset
            ),
            "unset unset"
        );
    }

    #[test]
    fn a_long_selection_is_cut_short_between_characters() {
        let context = Context {
            selection: Some("é".repeat(MOST_SELECTION)),
            ..Context::default()
        };
        let selection = context
            .variables()
            .into_iter()
            .find_map(|(name, value)| (name == "PDVIM_SELECTION").then_some(value))
            .flatten()
            .and_then(|value| value.into_string().ok())
            .unwrap();
        assert!(selection.len() <= MOST_SELECTION, "{}", selection.len());
        assert!(
            selection.len() > MOST_SELECTION - 2,
            "cut no shorter than it must"
        );
        assert!(selection.chars().all(|character| character == 'é'));
    }
}
