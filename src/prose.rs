//! Lines of a page made into running text, for copying: a PDF breaks its
//! lines where they fill the column, not where the writer did, so the
//! lines of a paragraph are joined with spaces, a word broken over two
//! lines with a hyphen is made whole, and paragraphs are kept apart by a
//! blank line.

/// The lines joined: each with whether it starts a paragraph. Runs of
/// spaces in a line become one; an empty line ends the paragraph.
pub fn join<'a>(lines: impl IntoIterator<Item = (&'a str, bool)>) -> String {
    let mut text = String::new();
    // Whether the next line starts a paragraph whatever it says, after an
    // empty line.
    let mut broken = false;
    for (line, starts) in lines {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            broken = !text.is_empty();
            continue;
        }
        if text.is_empty() {
            text = line;
        } else if starts || broken {
            text.push_str("\n\n");
            text.push_str(&line);
        } else if let Some(whole) = mended(&text, &line) {
            text.truncate(whole);
            text.push_str(&line);
        } else {
            text.push(' ');
            text.push_str(&line);
        }
        broken = false;
    }
    text
}

/// Where the text so far is cut, to join the next line straight on, when
/// it ends with a hyphen at the end of a word. A word broken by a soft
/// hyphen, or by a hyphen after a letter with the next line going on in
/// small letters, loses the hyphen; one before a capital or a number, as
/// in pre-Columbian or 3-4, is a hyphen of the text, and stays.
fn mended(text: &str, next: &str) -> Option<usize> {
    if let Some(before) = text.strip_suffix('\u{ad}') {
        return Some(before.len());
    }
    let before = text.strip_suffix('-')?;
    let last = before.chars().next_back().filter(|c| !c.is_whitespace())?;
    let goes_on = next.chars().next().is_some_and(char::is_lowercase);
    Some(if last.is_alphabetic() && goes_on {
        before.len()
    } else {
        text.len()
    })
}

/// Whether a line, the last of a page, ends a sentence, so that the next
/// page starting with a capital starts a paragraph of its own, rather than
/// going on with this one.
pub fn ends_sentence(line: &str) -> bool {
    line.trim_end()
        .trim_end_matches(['"', '\'', ')', '\u{201d}', '\u{2019}'])
        .ends_with(['.', '!', '?', ':'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_of_a_paragraph_run_on() {
        assert_eq!(
            join([("The quick   brown", true), ("fox jumps.", false)]),
            "The quick brown fox jumps."
        );
    }

    #[test]
    fn paragraphs_are_kept_apart() {
        assert_eq!(
            join([
                ("One line", true),
                ("ends.", false),
                ("Another starts", true),
                ("", false),
                ("and a third.", false),
            ]),
            "One line ends.\n\nAnother starts\n\nand a third."
        );
    }

    #[test]
    fn words_broken_over_lines_are_made_whole() {
        assert_eq!(join([("an exam-", false), ("ple", false)]), "an example");
        assert_eq!(
            join([("an exam\u{ad}", false), ("Ple", false)]),
            "an examPle"
        );
        // A hyphen before a capital or a number is one of the text, and
        // stays; a dash with a space before it is not a broken word.
        assert_eq!(
            join([("pre-", false), ("Columbian", false)]),
            "pre-Columbian"
        );
        assert_eq!(join([("pages 3-", false), ("4", false)]), "pages 3-4");
        assert_eq!(
            join([("a dash -", false), ("here", false)]),
            "a dash - here"
        );
    }

    #[test]
    fn a_sentence_ends_at_its_stop_whatever_closes_it() {
        assert!(ends_sentence("It ended."));
        assert!(ends_sentence("\u{201c}Did it?\u{201d} "));
        assert!(ends_sentence("As follows:"));
        assert!(!ends_sentence("and it went on"));
        assert!(!ends_sentence("e.g. this,"));
    }
}
