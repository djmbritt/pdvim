//! The manual pages, built into the program, made into HTML for pdvim to
//! open as a book: `:man` shows pdvim(1), and `:man 5` pdvim(5).
//!
//! Only the part of roff's `man` macros the two pages use is understood:
//! headings, paragraphs, tagged paragraphs, indents, unfilled text, the font
//! macros and escapes, and the named characters they use. Anything else is
//! left out rather than shown as roff.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The pages, by section.
const PAGES: [(u8, &str); 2] = [
    (1, include_str!("../docs/man/pdvim.1")),
    (5, include_str!("../docs/man/pdvim.5")),
];

/// The sections there are pages for, as `:man` takes them.
pub const SECTIONS: [&str; 2] = ["1", "5"];

/// How the pages look: the reader's own type, with room for the tags of
/// tagged paragraphs.
const STYLE: &str = "body { margin: 1em; }
h1 { font-size: 1.4em; }
h2 { font-size: 1.15em; margin-top: 1.2em; }
h3 { font-size: 1em; margin-top: 1em; }
dt { margin-top: 0.6em; }
dd { margin-left: 2.5em; }
.indent { margin-left: 2em; }
pre { font-family: monospace; margin-left: 2em; }";

/// The section of the page `:man` is asked for: the first when none is
/// named.
pub fn section(argument: &str) -> Option<u8> {
    match argument.trim() {
        "" | "1" | "pdvim" | "pdvim(1)" => Some(1),
        "5" | "pdvim(5)" => Some(5),
        _ => None,
    }
}

/// Writes the page of that section as HTML into the directory, unless it is
/// there already as it is now, and gives its path.
pub fn write(section: u8, directory: &Path) -> std::io::Result<PathBuf> {
    let roff = PAGES
        .iter()
        .find(|(number, _)| *number == section)
        .map_or("", |(_, roff)| roff);
    let path = directory.join(format!("pdvim.{section}.html"));
    let page = html(roff);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(page.as_str()) {
        std::fs::create_dir_all(directory)?;
        std::fs::write(&path, page)?;
    }
    Ok(path)
}

/// What is being written at a point of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    /// Nothing: between blocks.
    None,
    /// A paragraph.
    Paragraph,
    /// The tag of a tagged paragraph, on the next line.
    Tag,
    /// The text of a tagged paragraph.
    Tagged,
}

/// A manual page in `man` macros as an HTML document.
pub fn html(roff: &str) -> String {
    let mut page = Page {
        body: String::new(),
        block: Block::None,
        in_list: false,
        unfilled: false,
        title: String::from("pdvim"),
    };
    for line in roff.lines() {
        match line.strip_prefix('.').or_else(|| line.strip_prefix('\'')) {
            Some(request) => {
                let request = request.trim_start();
                let (name, rest) = request
                    .split_once(char::is_whitespace)
                    .unwrap_or((request, ""));
                page.request(name, &arguments(rest));
            }
            None if page.unfilled => {
                page.body.push_str(&inline(line));
                page.body.push('\n');
            }
            None if line.trim().is_empty() => page.close(),
            None => page.text(&inline(line)),
        }
    }
    if page.unfilled {
        page.body.push_str("</pre>\n");
    }
    page.close();
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"/><title>{}</title>\
         <style>{STYLE}</style></head>\n<body>\n{}</body></html>\n",
        escape(&page.title),
        page.body
    )
}

/// A page being written as HTML.
struct Page {
    body: String,
    block: Block,
    /// Whether a list of tagged paragraphs is open.
    in_list: bool,
    /// Whether lines are kept as they are, not filled into paragraphs.
    unfilled: bool,
    title: String,
}

impl Page {
    /// Carries out a request, as `TH` or `B`, with its arguments.
    fn request(&mut self, name: &str, arguments: &[String]) {
        match name {
            "TH" => {
                let name = arguments.first().map_or("PDVIM", String::as_str);
                let number = arguments.get(1).map_or("", String::as_str);
                self.title = format!("{}({number})", name.to_lowercase());
                let _ = writeln!(self.body, "<h1>{}</h1>", escape(&self.title));
            }
            "SH" | "SS" => {
                self.close();
                let level = if name == "SH" { 2 } else { 3 };
                let heading = inline(&arguments.join(" "));
                let _ = writeln!(self.body, "<h{level}>{heading}</h{level}>");
            }
            "PP" | "P" | "LP" => self.close(),
            "TP" | "IP" => self.tagged(name == "IP", arguments),
            // Unfilled text is indented on its own.
            "RS" | "RE" if self.unfilled => {}
            "RS" => {
                self.close();
                self.body.push_str("<div class=\"indent\">\n");
            }
            "RE" => {
                self.close();
                self.body.push_str("</div>\n");
            }
            "nf" | "EX" => {
                self.close();
                self.body.push_str("<pre>");
                self.unfilled = true;
            }
            "fi" | "EE" => {
                if self.unfilled {
                    self.body.push_str("</pre>\n");
                }
                self.unfilled = false;
            }
            "br" => self
                .body
                .push_str(if self.unfilled { "\n" } else { "<br/>" }),
            "B" | "I" | "SM" | "SB" => {
                let font = if name == "I" { 'I' } else { 'B' };
                let words = arguments.join(" ");
                self.text(&inline(&format!("\\f{font}{words}\\fP")));
            }
            "BR" | "RB" | "BI" | "IB" | "IR" | "RI" => {
                let fonts: Vec<char> = name.chars().collect();
                let mut made = String::new();
                for (index, argument) in arguments.iter().enumerate() {
                    let font = fonts.get(index % 2).copied().unwrap_or('R');
                    let _ = write!(made, "\\f{font}{argument}\\fR");
                }
                self.text(&inline(&made));
            }
            // Comments, spacing, hyphenation, adjusting and the like change
            // nothing that matters here.
            _ => {}
        }
    }

    /// Starts a tagged paragraph: its tag is the next line, or for `IP` the
    /// first argument.
    fn tagged(&mut self, tag_given: bool, arguments: &[String]) {
        self.close_block();
        if !self.in_list {
            self.body.push_str("<dl>\n");
            self.in_list = true;
        }
        if tag_given {
            let tag = arguments.first().map_or_default(|tag| inline(tag));
            let _ = write!(self.body, "<dt>{tag}</dt><dd>");
            self.block = Block::Tagged;
        } else {
            self.block = Block::Tag;
        }
    }

    /// Adds a line of text, already made HTML, to the block being written,
    /// starting a paragraph if there is none.
    fn text(&mut self, html: &str) {
        match self.block {
            Block::None => {
                if self.in_list {
                    self.body.push_str("</dl>\n");
                    self.in_list = false;
                }
                self.body.push_str("<p>");
                self.block = Block::Paragraph;
            }
            Block::Tag => {
                let _ = write!(self.body, "<dt>{html}</dt><dd>");
                self.block = Block::Tagged;
                return;
            }
            Block::Paragraph | Block::Tagged => {
                if !self.body.ends_with("<dd>") {
                    self.body.push(' ');
                }
            }
        }
        self.body.push_str(html);
    }

    /// Ends the paragraph or tagged paragraph being written.
    fn close_block(&mut self) {
        self.body.push_str(match self.block {
            Block::Paragraph => "</p>\n",
            Block::Tagged => "</dd>\n",
            Block::Tag => "<dt></dt><dd></dd>\n",
            Block::None => "",
        });
        self.block = Block::None;
    }

    /// Ends the paragraph being written, and the list of tagged paragraphs.
    fn close(&mut self) {
        self.close_block();
        if self.in_list {
            self.body.push_str("</dl>\n");
            self.in_list = false;
        }
    }
}

/// The arguments of a request: words, or text in double quotes, where two
/// double quotes stand for one.
fn arguments(rest: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut chars = rest.chars().peekable();
    loop {
        while chars.next_if(|c| *c == ' ' || *c == '\t').is_some() {}
        let Some(&first) = chars.peek() else {
            break;
        };
        let mut argument = String::new();
        if first == '"' {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '"' {
                    if chars.next_if_eq(&'"').is_some() {
                        argument.push('"');
                    } else {
                        break;
                    }
                } else {
                    argument.push(c);
                }
            }
        } else {
            while let Some(c) = chars.next_if(|c| *c != ' ' && *c != '\t') {
                argument.push(c);
                // An escaped space belongs to the word.
                if c == '\\'
                    && let Some(next) = chars.next()
                {
                    argument.push(next);
                }
            }
        }
        arguments.push(argument);
    }
    arguments
}

/// A line of roff text as HTML: escapes made characters, and font changes
/// made bold and italic. Every font started is ended by the end of the line.
fn inline(roff: &str) -> String {
    let mut out = String::new();
    let mut font = 'R';
    let mut chars = roff.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push_str(&escape_char(c));
            continue;
        }
        match chars.next() {
            Some('f') => {
                let wanted = match chars.next() {
                    Some('B') => 'B',
                    Some('I') => 'I',
                    _ => 'R',
                };
                if wanted != font {
                    out.push_str(end(font));
                    out.push_str(start(wanted));
                    font = wanted;
                }
            }
            Some('(') => {
                let named: String = chars.by_ref().take(2).collect();
                out.push_str(match named.as_str() {
                    "aq" => "'",
                    "ga" => "`",
                    "ti" => "~",
                    "en" => "\u{2013}",
                    "em" => "\u{2014}",
                    "dq" => "&quot;",
                    "lq" => "\u{201c}",
                    "rq" => "\u{201d}",
                    "bu" => "\u{2022}",
                    "co" => "\u{a9}",
                    "ha" => "^",
                    "rs" => "\\",
                    _ => "",
                });
            }
            Some('-') => out.push('-'),
            Some('e' | '\\') => out.push('\\'),
            Some(' ' | '~') => out.push('\u{a0}'),
            Some('.') => out.push('.'),
            // A comment runs to the end of the line.
            Some('"') => break,
            // Zero-width and break-allowing marks show as nothing.
            Some('&' | '|' | '^' | ':' | ')') | None => {}
            Some(other) => out.push_str(&escape_char(other)),
        }
    }
    out.push_str(end(font));
    out
}

/// The tag that starts a font.
const fn start(font: char) -> &'static str {
    match font {
        'B' => "<b>",
        'I' => "<i>",
        _ => "",
    }
}

/// The tag that ends a font.
const fn end(font: char) -> &'static str {
    match font {
        'B' => "</b>",
        'I' => "</i>",
        _ => "",
    }
}

/// A character as it is written in HTML text.
fn escape_char(c: char) -> String {
    match c {
        '&' => "&amp;".to_owned(),
        '<' => "&lt;".to_owned(),
        '>' => "&gt;".to_owned(),
        '"' => "&quot;".to_owned(),
        other => other.to_string(),
    }
}

/// Text as it is written in HTML.
fn escape(text: &str) -> String {
    text.chars().map(escape_char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_and_escapes_become_html() {
        assert_eq!(
            inline(r"\fBbold\fP and \fIitalic\fR"),
            "<b>bold</b> and <i>italic</i>"
        );
        assert_eq!(
            inline(r"scroll\-step \(aqa\(aq <x> & y"),
            "scroll-step 'a' &lt;x&gt; &amp; y"
        );
        assert_eq!(inline(r"\fBnever closed"), "<b>never closed</b>");
    }

    #[test]
    fn quoted_arguments_keep_their_spaces() {
        assert_eq!(
            arguments(r#":tabn[ext] "[n] or" x"#),
            [":tabn[ext]", "[n] or", "x"]
        );
        assert_eq!(arguments(r#""a ""b"" c""#), [r#"a "b" c"#]);
    }

    #[test]
    fn font_macros_alternate() {
        let page = html(".PP\n.BR j \", \" k\n");
        assert!(page.contains("<p><b>j</b>, <b>k</b></p>"), "{page}");
    }

    #[test]
    fn tagged_paragraphs_make_a_list() {
        let page = html(".SH KEYS\n.TP\n.B j\nScroll down.\n.TP\n.B k\nScroll up.\n.PP\nAfter.\n");
        assert!(
            page.contains(
                "<h2>KEYS</h2>\n<dl>\n<dt><b>j</b></dt><dd>Scroll down.</dd>\n\
                 <dt><b>k</b></dt><dd>Scroll up.</dd>\n</dl>\n<p>After.</p>"
            ),
            "{page}"
        );
    }

    #[test]
    fn unfilled_text_keeps_its_lines() {
        let page = html(".nf\n.RS\n[settings]\ntrim = true\n.RE\n.fi\n");
        assert!(
            page.contains("<pre>[settings]\ntrim = true\n</pre>"),
            "{page}"
        );
    }

    #[test]
    fn both_pages_are_whole_and_named() {
        for (section, roff) in PAGES {
            let page = html(roff);
            assert!(page.contains(&format!("<title>pdvim({section})</title>")));
            // No roff is left showing.
            let body = page.split_once("<body>").map_or("", |(_, body)| body);
            for line in body.lines() {
                assert!(!line.starts_with('.'), "{line}");
            }
            assert!(
                !page.contains("\\f") && !page.contains("\\-"),
                "roff left in pdvim({section})"
            );
            // Every list and paragraph is ended.
            assert_eq!(page.matches("<dl>").count(), page.matches("</dl>").count());
            assert_eq!(page.matches("<dd>").count(), page.matches("</dd>").count());
            assert_eq!(page.matches("<p>").count(), page.matches("</p>").count());
            assert_eq!(
                page.matches("<pre>").count(),
                page.matches("</pre>").count()
            );
        }
    }

    #[test]
    fn sections_are_known_by_number_or_name() {
        assert_eq!(section(""), Some(1));
        assert_eq!(section("5"), Some(5));
        assert_eq!(section("pdvim(5)"), Some(5));
        assert_eq!(section("3"), None);
    }

    #[test]
    fn a_page_is_written_once_and_again_only_when_changed() {
        let directory = std::env::temp_dir().join(format!("pdvim-manual-{}", std::process::id()));
        let path = write(5, &directory).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("pdvim(5)"));
        std::fs::write(&path, "old").unwrap();
        write(5, &directory).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("pdvim(5)"));
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
