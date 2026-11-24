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
        } else if runs_on(&text, &line) {
            text.push_str(&line);
        } else {
            text.push(' ');
            text.push_str(&line);
        }
        broken = false;
    }
    text
}

/// Whether the next line goes on from the text with no space between, as
/// in Chinese, Japanese and Thai, which put none between words.
fn runs_on(text: &str, next: &str) -> bool {
    let last = text.chars().next_back();
    let first = next.chars().next();
    last.and_then(unspaced).is_some() || first.and_then(unspaced).is_some()
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

/// The characters that end a sentence, in the scripts that have one: the
/// full stop, question and exclamation marks and the ellipsis; the
/// ideographic and full-width ones of Chinese and Japanese, which need no
/// space after them; the danda of Hindi and other Indian scripts; the
/// Ethiopic full stop; and the Arabic question mark and the Urdu full
/// stop.
pub const SENTENCE_ENDS: &[char] = &[
    '.', '!', '?', '\u{2026}', '\u{3002}', '\u{ff01}', '\u{ff1f}', '\u{ff0e}', '\u{ff61}',
    '\u{964}', '\u{965}', '\u{1362}', '\u{61f}', '\u{6d4}',
];

/// Whether a character ends a sentence with no space needed after it, as
/// the ideographic full stop does.
pub const fn ends_without_space(character: char) -> bool {
    matches!(
        character,
        '\u{3002}' | '\u{ff01}' | '\u{ff1f}' | '\u{ff0e}' | '\u{ff61}'
    )
}

/// Whether a character is a mark that belongs to the letter before it,
/// rather than a letter of its own: an accent, a vowel sign or other mark
/// of an Indian or South-East Asian script, Hebrew and Arabic vowel
/// points, the Japanese voicing marks, and the joiners and selectors that
/// shape what comes before them. The cursor goes over a letter and its
/// marks as one. The ranges are those of the scripts' marks, without the
/// whole of Unicode's tables.
pub fn is_mark(character: char) -> bool {
    matches!(u32::from(character),
        0x0300..=0x036f | 0x0483..=0x0489 // accents; Cyrillic
        | 0x0591..=0x05bd | 0x05bf | 0x05c1 | 0x05c2 | 0x05c4 | 0x05c5 | 0x05c7 // Hebrew
        | 0x0610..=0x061a | 0x064b..=0x065f | 0x0670 | 0x06d6..=0x06dc | 0x06df..=0x06e4
        | 0x06e7 | 0x06e8 | 0x06ea..=0x06ed // Arabic
        | 0x0900..=0x0903 | 0x093a..=0x093c | 0x093e..=0x094f | 0x0951..=0x0957
        | 0x0962 | 0x0963 // Devanagari
        | 0x0981..=0x0983 | 0x09bc | 0x09be..=0x09cd | 0x09d7 | 0x09e2 | 0x09e3 // Bengali
        | 0x0a01..=0x0a03 | 0x0a3c..=0x0a51 | 0x0a70 | 0x0a71 | 0x0a75 // Gurmukhi
        | 0x0a81..=0x0a83 | 0x0abc | 0x0abe..=0x0acd | 0x0ae2 | 0x0ae3 // Gujarati
        | 0x0b01..=0x0b03 | 0x0b3c | 0x0b3e..=0x0b57 | 0x0b62 | 0x0b63 // Odia
        | 0x0b82 | 0x0bbe..=0x0bcd | 0x0bd7 // Tamil
        | 0x0c00..=0x0c04 | 0x0c3c | 0x0c3e..=0x0c56 | 0x0c62 | 0x0c63 // Telugu
        | 0x0c81..=0x0c83 | 0x0cbc | 0x0cbe..=0x0cd6 | 0x0ce2 | 0x0ce3 // Kannada
        | 0x0d00..=0x0d03 | 0x0d3b | 0x0d3c | 0x0d3e..=0x0d4d | 0x0d57
        | 0x0d62 | 0x0d63 // Malayalam
        | 0x0d81..=0x0d83 | 0x0dca..=0x0ddf | 0x0df2 | 0x0df3 // Sinhala
        | 0x0e31 | 0x0e34..=0x0e3a | 0x0e47..=0x0e4e // Thai
        | 0x0eb1 | 0x0eb4..=0x0ebc | 0x0ec8..=0x0ece // Lao
        | 0x0f71..=0x0f84 | 0x0f8d..=0x0fbc // Tibetan
        | 0x102b..=0x103e | 0x1056..=0x1059 // Myanmar
        | 0x17b4..=0x17d3 // Khmer
        | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f // more accents
        | 0x200c | 0x200d | 0xfe00..=0xfe0f | 0xe0100..=0xe01ef // joiners, selectors
        | 0x3099 | 0x309a // Japanese voicing
        | 0x1f3fb..=0x1f3ff // skin tones
    )
}

/// The scripts written without spaces between words, each a kind of
/// character apart: where one kind gives way to another, a word ends, as
/// vim's motions take it. Chinese characters, hiragana, katakana and the
/// ideographic punctuation are kinds of their own; Thai, Lao, Khmer and
/// Myanmar, whose words only a dictionary can tell apart, are one each.
/// None for the scripts that put spaces between words.
pub fn unspaced(character: char) -> Option<u8> {
    Some(match u32::from(character) {
        // Ideographic and full-width punctuation.
        0x3000..=0x303f | 0xff00..=0xff0f | 0xff1a..=0xff20 | 0xff3b..=0xff40 | 0xff5b..=0xff65 => {
            1
        }
        0x3040..=0x309f => 2,
        0x30a0..=0x30ff | 0x31f0..=0x31ff | 0xff66..=0xff9f => 3,
        // Chinese characters, as Japanese uses them too.
        0x2e80..=0x2fdf
        | 0x3400..=0x4dbf
        | 0x4e00..=0x9fff
        | 0xf900..=0xfaff
        | 0x20000..=0x3134f => 4,
        0x0e00..=0x0e7f => 5,
        0x0e80..=0x0eff => 6,
        0x1780..=0x17ff => 7,
        0x1000..=0x109f => 8,
        _ => return None,
    })
}

/// Whether a line, the last of a page, ends a sentence, so that the next
/// page starting with a capital starts a paragraph of its own, rather than
/// going on with this one.
pub fn ends_sentence(line: &str) -> bool {
    let line = line
        .trim_end()
        .trim_end_matches(['"', '\'', ')', '\u{201d}', '\u{2019}']);
    line.ends_with(':') || line.ends_with(SENTENCE_ENDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_script_without_spaces_is_a_kind_of_its_own() {
        // Punctuation, hiragana, katakana, Chinese, Thai, Lao, Khmer and
        // Myanmar, full-width forms with their scripts.
        let kinds: Vec<Option<u8>> = ['。', '！', 'ひ', 'カ', 'ｶ', '字', 'ก', 'ກ', 'ក', 'က']
            .into_iter()
            .map(unspaced)
            .collect();
        let expected = [1, 1, 2, 3, 3, 4, 5, 6, 7, 8].map(Some);
        assert_eq!(kinds, expected);
        for spaced in ['a', 'é', 'Ж', 'ا', 'अ', '한', ' ', '.', '1'] {
            assert_eq!(unspaced(spaced), None, "{spaced}");
        }
    }

    #[test]
    fn a_line_runs_on_without_a_space_when_either_side_is_unspaced() {
        assert!(runs_on("用", "字"));
        assert!(runs_on("打开 PDF", "文件"));
        assert!(runs_on("文件", "PDF 打开"));
        assert!(!runs_on("open the", "file"));
    }

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
