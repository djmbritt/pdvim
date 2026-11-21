//! Fonts a document needs but neither holds nor gets from MuPDF, found
//! among the fonts installed on the system.
//!
//! The mupdf crate's own lookup asks fontconfig to read every installed
//! font again for each font it looks for. MuPDF looks for one each time it
//! meets a character its fonts lack, and remembers only the fonts it finds,
//! so a book with such characters on every line took seconds a page. Here
//! the fonts installed are read once on each thread, and every answer is
//! kept, fonts not found included.
//!
//! Nor did it find fonts for Chinese, Japanese or Korean, which MuPDF asks
//! for by script and language rather than by name, so a book in those was
//! blank. Here the families such text is usually set in are tried.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once, PoisonError};

use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::source::SystemSource;
use mupdf::{CjkFontOrdering, Font, FontHints, FontLoader};

/// A font found on the system: its family, which face of the file it is,
/// and the file's bytes.
#[derive(Clone)]
struct Found {
    family: String,
    index: i32,
    data: Arc<Vec<u8>>,
}

/// What was looked for: the name, and whether bold and italic.
type Wanted = (String, bool, bool);

/// The answer to every lookup made so far.
static FOUND: Mutex<Option<HashMap<Wanted, Option<Found>>>> = Mutex::new(None);

thread_local! {
    /// The fonts installed, as read by this thread.
    static SOURCE: SystemSource = SystemSource::new();
}

/// Has MuPDF look for fonts here from now on. Done once, however often it
/// is called.
pub fn install() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| mupdf::set_font_loader(SystemFonts));
}

struct SystemFonts;

impl FontLoader for SystemFonts {
    fn load_font(&self, name: &str, hints: FontHints) -> Option<Font> {
        let wanted = (name.to_owned(), hints.bold, hints.italic);
        let known = FOUND
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .and_then(|found| found.get(&wanted).cloned());
        let found = known.unwrap_or_else(|| {
            // Looked for without holding the lock, which a slow lookup on
            // one thread would keep from the others.
            let found = find(name, hints);
            FOUND
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert_with(HashMap::new)
                .insert(wanted, found.clone());
            found
        })?;
        let font = Font::from_bytes_with_index(&found.family, found.index, &found.data).ok()?;
        let lacking = (hints.bold && !font.is_bold()) || (hints.italic && !font.is_italic());
        (!(hints.needs_exact_metrics && lacking)).then_some(font)
    }

    fn load_cjk_font(&self, _name: &str, ordering: CjkFontOrdering, serif: bool) -> Option<Font> {
        self.first_of(cjk_families(ordering, serif))
    }

    fn load_fallback_font(&self, script: u32, language: u32, hints: FontHints) -> Option<Font> {
        let ordering = cjk_ordering(script, language)?;
        self.first_of(cjk_families(ordering, hints.serif))
    }
}

impl SystemFonts {
    /// The first of these families installed, in its plain face.
    fn first_of(&self, families: [&str; 6]) -> Option<Font> {
        families
            .iter()
            .find_map(|family| self.load_font(family, FontHints::default()))
    }
}

/// Scripts as MuPDF numbers them, after UCDN.
const HANGUL: u32 = 24;
const HIRAGANA: u32 = 32;
const KATAKANA: u32 = 33;
const BOPOMOFO: u32 = 34;
const HAN: u32 = 35;

/// Languages as MuPDF numbers them: each letter from 1 for `a`, in base 27.
const KOREAN: u32 = 416;
const SIMPLIFIED_CHINESE: u32 = 14_093;
const TRADITIONAL_CHINESE: u32 = 14_822;

/// The kind of CJK font text of a script in a language is set in: by the
/// script where it says, else by the language, else Japanese, as MuPDF
/// takes Han characters to be. None for other scripts.
const fn cjk_ordering(script: u32, language: u32) -> Option<CjkFontOrdering> {
    Some(match (script, language) {
        (HANGUL, _) | (HAN, KOREAN) => CjkFontOrdering::AdobeKorea,
        (BOPOMOFO, _) | (HAN, TRADITIONAL_CHINESE) => CjkFontOrdering::AdobeCns,
        (HAN, SIMPLIFIED_CHINESE) => CjkFontOrdering::AdobeGb,
        (HIRAGANA | KATAKANA | HAN, _) => CjkFontOrdering::AdobeJapan,
        _ => return None,
    })
}

/// The families text of a kind is usually set in, in serif first or sans
/// first: Noto CJK, Source Han, then others common on Linux.
const fn cjk_families(ordering: CjkFontOrdering, serif: bool) -> [&'static str; 6] {
    let (sans, sans_too, serif_family, serif_too, more, last) = match ordering {
        CjkFontOrdering::AdobeJapan => (
            "Noto Sans CJK JP",
            "Source Han Sans JP",
            "Noto Serif CJK JP",
            "Source Han Serif JP",
            "IPAGothic",
            "IPAMincho",
        ),
        CjkFontOrdering::AdobeKorea => (
            "Noto Sans CJK KR",
            "Source Han Sans KR",
            "Noto Serif CJK KR",
            "Source Han Serif KR",
            "NanumGothic",
            "NanumMyeongjo",
        ),
        CjkFontOrdering::AdobeGb => (
            "Noto Sans CJK SC",
            "Source Han Sans SC",
            "Noto Serif CJK SC",
            "Source Han Serif SC",
            "WenQuanYi Zen Hei",
            "WenQuanYi Micro Hei",
        ),
        CjkFontOrdering::AdobeCns => (
            "Noto Sans CJK TC",
            "Source Han Sans TC",
            "Noto Serif CJK TC",
            "Source Han Serif TC",
            "WenQuanYi Zen Hei",
            "AR PL UMing TW",
        ),
    };
    if serif {
        [serif_family, serif_too, sans, sans_too, last, more]
    } else {
        [sans, sans_too, serif_family, serif_too, more, last]
    }
}

/// Looks for a font among those installed: by its PostScript name, and
/// failing that by family, without the suffixes some PDF makers add.
fn find(name: &str, hints: FontHints) -> Option<Found> {
    SOURCE.with(|source| {
        let handle = source.select_by_postscript_name(name).or_else(|_| {
            let mut family = name;
            for suffix in ["MT", "PS", "IdentityH"] {
                family = family.strip_suffix(suffix).unwrap_or(family);
            }
            let weight = if hints.bold {
                Weight::BOLD
            } else {
                Weight::NORMAL
            };
            let style = if hints.italic {
                Style::Italic
            } else {
                Style::Normal
            };
            source.select_best_match(
                &[FamilyName::Title(family.to_owned())],
                Properties::new().weight(weight).style(style),
            )
        });
        let handle = handle.ok()?;
        let index = match &handle {
            Handle::Path { font_index, .. } | Handle::Memory { font_index, .. } => *font_index,
        };
        let loaded = handle.load().ok()?;
        Some(Found {
            family: loaded.family_name(),
            index: i32::try_from(index).ok()?,
            data: loaded.copy_font_data()?,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn han_is_set_by_its_language_and_kana_as_japanese() {
        let ordering = |script, language| cjk_ordering(script, language);
        assert!(matches!(
            ordering(HAN, KOREAN),
            Some(CjkFontOrdering::AdobeKorea)
        ));
        assert!(matches!(
            ordering(HAN, SIMPLIFIED_CHINESE),
            Some(CjkFontOrdering::AdobeGb)
        ));
        assert!(matches!(
            ordering(HAN, 0),
            Some(CjkFontOrdering::AdobeJapan)
        ));
        assert!(matches!(
            ordering(KATAKANA, KOREAN),
            Some(CjkFontOrdering::AdobeJapan)
        ));
        assert!(matches!(
            ordering(HANGUL, 0),
            Some(CjkFontOrdering::AdobeKorea)
        ));
        // Greek, which other fonts have.
        assert!(ordering(15, 0).is_none());
    }

    #[test]
    fn a_font_not_found_is_looked_for_once() {
        let hints = FontHints::default();
        let name = "pdvim-no-such-font";
        assert!(SystemFonts.load_font(name, hints).is_none());
        let known = FOUND
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|found| found.get(&(name.to_owned(), false, false)).cloned());
        assert!(matches!(known, Some(None)));
        // Asked again, the answer comes from what was kept.
        let start = std::time::Instant::now();
        for _ in 0..100 {
            assert!(SystemFonts.load_font(name, hints).is_none());
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }
}
