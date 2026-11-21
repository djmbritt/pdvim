//! `:!`: running a shell command, as vim does, with `%` standing for the
//! file of the document in view.
//!
//! The command runs in `sh -c`, or `cmd /C` on Windows, on a thread of its
//! own, so that pdvim goes on while it runs: `:!xdg-open %` hands the
//! document to another program, and `:!cp % ~/papers/` copies it. What the
//! command prints is not shown whole, only its last line, in the status
//! bar.

use std::path::Path;
use std::process::{Command, Stdio};

use iced::futures::channel::oneshot;

/// What stops a command from being run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("no file for % to stand for")]
    NoFile,
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
            '%' => expanded.push_str(&quote(&file.ok_or(Error::NoFile)?.to_string_lossy())),
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

/// Runs a command in the shell, with nothing to read on standard input,
/// and answers once it has finished.
pub fn run(command: String) -> impl Future<Output = Result<Ran, Error>> {
    let (reply, answer) = oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("pdvim-shell".to_owned())
        .spawn(move || {
            let _ = reply.send(run_now(&command));
        });
    async move {
        spawned.map_err(|error| Error::NotRun(error.to_string()))?;
        answer
            .await
            .unwrap_or_else(|_| Err(Error::NotRun("the command was lost".to_owned())))
    }
}

/// Runs a command in the shell and waits for it.
fn run_now(command: &str) -> Result<Ran, Error> {
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

    #[cfg(not(windows))]
    #[test]
    fn a_command_runs_and_says_how_it_ended() {
        let ran = run_now("echo one; echo two").unwrap();
        assert_eq!(
            ran,
            Ran {
                succeeded: true,
                code: Some(0),
                last_line: Some("two".to_owned())
            }
        );
        let failed = run_now("echo nope >&2; exit 3").unwrap();
        assert_eq!(
            report("false", &failed),
            ("!false: exit 3: nope".to_owned(), true)
        );
        // A quoted path reaches the command as one word, quote and all.
        let file = Path::new("it's a file.pdf");
        let echoed = run_now(&expand(r"printf '\%s\n' %", Some(file)).unwrap()).unwrap();
        assert_eq!(echoed.last_line.as_deref(), Some("it's a file.pdf"));
    }
}
