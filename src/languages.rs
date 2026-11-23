//! The text handling tried in many languages and scripts: finding
//! sentences, copying, searching, and moving the cursor, each over the
//! same sample in every language. Most of pdvim's tests are in English; a
//! rule that only holds for it, such as that a sentence starts with a
//! capital, shows up here as a language it fails in.
//!
//! The samples are Article 1 of the Universal Declaration of Human
//! Rights, the text most often translated, in a language of each kind of
//! script: alphabets with and without capitals, abjads written right to
//! left, abugidas whose vowels are marks on consonants, and Chinese,
//! Japanese and Thai, which put no spaces between words. Each test checks
//! every language and lists all that fail, so that one run shows where
//! pdvim stands.

use std::collections::HashMap;

use crate::backend::{self, Needle, TextChar, TextLine};
use crate::bindings::Keymaps;
use crate::input::Key;
use crate::layout::Rect;
use crate::visual::{Kind, Spot, Text, Visual};

/// A language's sample: its sentences, and whether it puts spaces between
/// words, and so between sentences and at the ends of lines.
struct Sample {
    language: &'static str,
    sentences: &'static [&'static str],
    spaced: bool,
}

const SAMPLES: &[Sample] = &[
    Sample {
        language: "English",
        sentences: &[
            "All human beings are born free and equal in dignity and rights.",
            "They are endowed with reason and conscience and should act towards one another in a spirit of brotherhood.",
        ],
        spaced: true,
    },
    Sample {
        language: "German",
        sentences: &[
            "Alle Menschen sind frei und gleich an Würde und Rechten geboren.",
            "Sie sind mit Vernunft und Gewissen begabt und sollen einander im Geist der Brüderlichkeit begegnen.",
        ],
        spaced: true,
    },
    Sample {
        language: "Turkish",
        sentences: &[
            "Bütün insanlar hür, haysiyet ve haklar bakımından eşit doğarlar.",
            "Akıl ve vicdana sahiptirler ve birbirlerine karşı kardeşlik zihniyeti ile hareket etmelidirler.",
        ],
        spaced: true,
    },
    Sample {
        language: "Greek",
        sentences: &[
            "Όλοι οι άνθρωποι γεννιούνται ελεύθεροι και ίσοι στην αξιοπρέπεια και τα δικαιώματα.",
            "Είναι προικισμένοι με λογική και συνείδηση, και οφείλουν να συμπεριφέρονται μεταξύ τους με πνεύμα αδελφοσύνης.",
        ],
        spaced: true,
    },
    Sample {
        language: "Russian",
        sentences: &[
            "Все люди рождаются свободными и равными в своем достоинстве и правах.",
            "Они наделены разумом и совестью и должны поступать в отношении друг друга в духе братства.",
        ],
        spaced: true,
    },
    Sample {
        language: "Vietnamese",
        sentences: &[
            "Tất cả mọi người sinh ra đều được tự do và bình đẳng về nhân phẩm và quyền.",
            "Mọi con người đều được tạo hóa ban cho lý trí và lương tâm và cần phải đối xử với nhau trong tình bằng hữu.",
        ],
        spaced: true,
    },
    // The same language with each accent a mark of its own after its
    // letter, as some PDFs give it.
    Sample {
        language: "Vietnamese, accents apart",
        sentences: &[
            "Ta\u{302}\u{301}t ca\u{309} mo\u{323}i ngu\u{31b}o\u{31b}\u{300}i \u{111}e\u{302}\u{300}u bi\u{300}nh \u{111}a\u{306}\u{309}ng.",
            "Ho\u{323} co\u{301} ly\u{301} tri\u{301}.",
        ],
        spaced: true,
    },
    Sample {
        language: "Arabic",
        sentences: &[
            "يولد جميع الناس أحراراً متساوين في الكرامة والحقوق.",
            "وقد وهبوا عقلاً وضميراً وعليهم أن يعامل بعضهم بعضاً بروح الإخاء.",
        ],
        spaced: true,
    },
    Sample {
        language: "Hebrew",
        sentences: &[
            "כל בני האדם נולדו בני חורין ושווים בערכם ובזכויותיהם.",
            "כולם חוננו בתבונה ובמצפון, לפיכך חובה עליהם לנהוג איש ברעהו ברוח של אחוה.",
        ],
        spaced: true,
    },
    Sample {
        language: "Hindi",
        sentences: &[
            "सभी मनुष्यों को गौरव और अधिकारों के मामले में जन्मजात स्वतन्त्रता और समानता प्राप्त है।",
            "उन्हें बुद्धि और अन्तरात्मा की देन प्राप्त है और परस्पर उन्हें भाईचारे के भाव से बर्ताव करना चाहिए।",
        ],
        spaced: true,
    },
    Sample {
        language: "Tamil",
        sentences: &[
            "மனிதப் பிறவியினர் சகலரும் சுதந்திரமாகவே பிறக்கின்றனர்; அவர்கள் மதிப்பிலும், உரிமைகளிலும் சமமானவர்கள்.",
            "அவர்கள் ஒருவருடனொருவர் சகோதர உணர்வுப் பாங்கில் நடந்துகொள்ளல் வேண்டும்.",
        ],
        spaced: true,
    },
    Sample {
        language: "Amharic",
        sentences: &[
            "የሰው ልጅ ሁሉ ሲወለድ ነጻና በክብርና በመብትም እኩልነት ያለው ነው።",
            "የተፈጥሮ ማስተዋልና ሕሊና ስላለው አንዱ ሌላውን በወንድማማችነት መንፈስ መመልከት ይገባዋል።",
        ],
        spaced: true,
    },
    Sample {
        language: "Korean",
        sentences: &[
            "모든 인간은 태어날 때부터 자유로우며 그 존엄과 권리에 있어 동등하다.",
            "인간은 천부적으로 이성과 양심을 부여받았으며 서로 형제애의 정신으로 행동하여야 한다.",
        ],
        spaced: true,
    },
    Sample {
        language: "Chinese",
        sentences: &[
            "人人生而自由，在尊严和权利上一律平等。",
            "他们赋有理性和良心，并应以兄弟关系的精神相对待。",
        ],
        spaced: false,
    },
    Sample {
        language: "Japanese",
        sentences: &[
            "すべての人間は、生れながらにして自由であり、かつ、尊厳と権利とについて平等である。",
            "人間は、理性と良心とを授けられており、互いに同胞の精神をもって行動しなければならない。",
        ],
        spaced: false,
    },
    // Thai ends a sentence with a space, not a stop: one sentence here.
    Sample {
        language: "Thai",
        sentences: &[
            "มนุษย์ทั้งหลายเกิดมามีอิสระและเสมอภาคกันในเกียรติศักดิ์และสิทธิ ต่างมีเหตุผลและมโนธรรม และควรปฏิบัติต่อกันด้วยเจตนารมณ์แห่งภราดรภาพ",
        ],
        spaced: false,
    },
];

impl Sample {
    /// The whole text, as written.
    fn text(&self) -> String {
        self.sentences.join(if self.spaced { " " } else { "" })
    }

    /// The text broken into lines as a narrow column would break it:
    /// between words where there are spaces, anywhere where there are
    /// not.
    fn lines(&self) -> Vec<String> {
        let text = self.text();
        if !self.spaced {
            // Sixteen characters to a line, but never a mark apart from
            // the letter it is on.
            let mut lines: Vec<String> = Vec::new();
            for character in text.chars() {
                match lines.last_mut() {
                    Some(line) if line.chars().count() < 16 || is_mark(character) => {
                        line.push(character);
                    }
                    _ => lines.push(character.to_string()),
                }
            }
            return lines;
        }
        let mut lines: Vec<String> = Vec::new();
        for word in text.split(' ') {
            match lines.last_mut() {
                Some(line) if line.chars().count().saturating_add(word.chars().count()) < 28 => {
                    line.push(' ');
                    line.push_str(word);
                }
                _ => lines.push(word.to_owned()),
            }
        }
        lines
    }

    /// The page the text is on: its lines one under another, each spread
    /// over the same width, as justified text is, so that no line looks
    /// like the end of a paragraph.
    fn page(&self) -> Text {
        let lines = self
            .lines()
            .iter()
            .enumerate()
            .map(|(row, line)| {
                let count = line.chars().count();
                let width = 400.0 / f32::from(u16::try_from(count).unwrap());
                let top = f32::from(u16::try_from(row).unwrap()) * 14.0;
                TextLine {
                    chars: line
                        .chars()
                        .enumerate()
                        .map(|(column, character)| {
                            let left = f32::from(u16::try_from(column).unwrap()) * width;
                            TextChar {
                                character,
                                area: Rect {
                                    left,
                                    top,
                                    right: left + width,
                                    bottom: top + 12.0,
                                },
                            }
                        })
                        .collect(),
                }
            })
            .collect();
        HashMap::from([(0, lines)])
    }
}

/// Feeds keys to visual mode, as typed.
fn keys(visual: &mut Visual, text: &Text, typed: &str) {
    let keymap = Keymaps::default().visual;
    for key in typed.chars() {
        visual.feed(Key::Char(key), &keymap, text);
    }
}

/// The last character of the page.
fn end(text: &Text) -> Spot {
    let lines = &text[&0];
    let line = lines.len().saturating_sub(1);
    Spot {
        page: 0,
        line,
        index: lines[line].chars.len().saturating_sub(1),
    }
}

/// The character at a spot.
fn character(text: &Text, spot: Spot) -> char {
    text[&0][spot.line].chars[spot.index].character
}

/// Whether a character is a mark that belongs to the letter before it: an
/// accent, or a vowel sign of an abugida. The cursor stopping on one has
/// split a letter in two.
fn is_mark(character: char) -> bool {
    matches!(u32::from(character),
        0x0300..=0x036f // accents
        | 0x0591..=0x05c7 // Hebrew points
        | 0x064b..=0x065f | 0x0670 // Arabic vowel marks
        | 0x0900..=0x0903 | 0x093a..=0x094f | 0x0951..=0x0957 | 0x0962 | 0x0963 // Devanagari
        | 0x0bbe..=0x0bcd | 0x0bd7 // Tamil
        | 0x0e31 | 0x0e34..=0x0e3a | 0x0e47..=0x0e4e // Thai
    )
}

/// Fails, listing every language a check failed in and how. Each
/// failure starts with its language's name.
fn report(check: &str, failures: &[String]) {
    let mut languages: Vec<&str> = failures
        .iter()
        .filter_map(|failure| failure.split(':').next())
        .collect();
    languages.dedup();
    assert!(
        failures.is_empty(),
        "{check} fails in {} of {} languages:\n  {}",
        languages.len(),
        SAMPLES.len(),
        failures.join("\n  ")
    );
}

/// The text without its spaces, to compare where text is split apart from
/// how it is spaced.
fn unspaced(text: &str) -> String {
    text.chars().filter(|character| !character.is_whitespace()).collect()
}

#[test]
fn a_sentence_is_selected_whole() {
    let mut failures = Vec::new();
    for sample in SAMPLES {
        let text = sample.page();
        // From the start, and from partway along the second sentence.
        let mut first = Visual::new(Kind::Characters, Spot::line_start(0, 0));
        keys(&mut first, &text, "is");
        let got = unspaced(&first.text(&text));
        let wanted = unspaced(sample.sentences[0]);
        if got != wanted {
            failures.push(format!("{}: `is` at the start selects {got:?}", sample.language));
        }
        if let Some(second) = sample.sentences.get(1) {
            let mut last = Visual::new(Kind::Characters, end(&text));
            keys(&mut last, &text, "is");
            let got = unspaced(&last.text(&text));
            if got != unspaced(second) {
                failures.push(format!("{}: `is` at the end selects {got:?}", sample.language));
            }
        }
    }
    report("Selecting a sentence", &failures);
}

#[test]
fn text_copied_is_the_text_written() {
    let mut failures = Vec::new();
    for sample in SAMPLES {
        let text = sample.page();
        let mut all = Visual::new(Kind::Characters, Spot::line_start(0, 0));
        all.move_to(end(&text));
        let copied = all.text(&text);
        let written = sample.text();
        if copied != written {
            failures.push(format!("{}: copied as {copied:?}", sample.language));
        }
    }
    report("Copying", &failures);
}

#[test]
fn every_sentence_is_found_in_capitals_with_case_ignored() {
    let mut failures = Vec::new();
    for sample in SAMPLES {
        let text = sample.page();
        for sentence in sample.sentences {
            let needle = Needle {
                text: sentence.to_uppercase(),
                match_case: false,
                whole_word: false,
            };
            if backend::text_hits(&text[&0], 0, &needle).is_empty() {
                failures.push(format!("{}: {:?} is not found", sample.language, needle.text));
            }
        }
    }
    report("Searching", &failures);
}

#[test]
fn the_cursor_stops_at_every_word_and_never_inside_a_letter() {
    let mut failures = Vec::new();
    for sample in SAMPLES {
        let text = sample.page();
        let last = end(&text);
        // Where `w` stops, from the start to the end.
        let mut cursor = Visual::cursor_at(Spot::line_start(0, 0));
        let mut stops = vec![cursor.cursor()];
        while cursor.cursor() != last && stops.len() < 500 {
            keys(&mut cursor, &text, "w");
            if stops.last() == Some(&cursor.cursor()) {
                break;
            }
            stops.push(cursor.cursor());
        }
        if let Some(&inside) = stops.iter().find(|&&spot| is_mark(character(&text, spot))) {
            failures.push(format!(
                "{}: `w` stops on the mark {:?} at {inside:?}",
                sample.language,
                character(&text, inside)
            ));
        }
        if sample.spaced {
            // Every word after a space starts where `w` stops.
            let lines = &text[&0];
            for (line, row) in lines.iter().enumerate() {
                for index in 1..row.chars.len() {
                    let after_space = row.chars[index - 1].character == ' ';
                    let spot = Spot { page: 0, line, index };
                    if after_space && !stops.contains(&spot) {
                        failures.push(format!(
                            "{}: `w` passes over the word at {spot:?}",
                            sample.language
                        ));
                    }
                }
            }
        } else if stops.len() <= text[&0].len() {
            failures.push(format!(
                "{}: `w` stops only {} times in {} lines",
                sample.language,
                stops.len(),
                text[&0].len()
            ));
        }
        // `l` along the first line.
        let mut right = Visual::cursor_at(Spot::line_start(0, 0));
        for _ in 0..text[&0][0].chars.len() {
            keys(&mut right, &text, "l");
            let spot = right.cursor();
            if is_mark(character(&text, spot)) {
                failures.push(format!(
                    "{}: `l` stops on the mark {:?} at {spot:?}",
                    sample.language,
                    character(&text, spot)
                ));
                break;
            }
        }
    }
    report("Moving the cursor", &failures);
}
