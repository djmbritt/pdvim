//! Jumping to text by typing it, as flash.nvim does: each match of what is
//! typed in the lines in view gets a label, and typing a label jumps there.
//!
//! Labels never start with a letter that could continue what is typed, so
//! a letter typed is always either more of the search or a label. The
//! matches nearest where the search started get labels first, of one
//! letter while there are few, and of two when one letter is not enough.

use crate::backend::TextLine;
use crate::visual::Spot;

/// The letters labels are made of, the easiest to reach first.
pub const LABELS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// A match of what is typed, and the label to jump to it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Its first character.
    pub start: Spot,
    /// How many characters it runs to.
    pub len: usize,
    /// Empty when there are not enough labels to go round.
    pub label: String,
}

/// What a key comes to, as the app reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Jump to the nearest match.
    Submit,
    Cancel,
    RubOut,
    Char(char),
    Other,
}

/// What a key typed comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    Jump(Spot),
    Cancel,
}

/// A search being typed, and its labelled matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flash {
    pattern: String,
    /// The first letter of a label of two, typed.
    half: Option<char>,
    targets: Vec<Target>,
}

impl Flash {
    pub const fn new() -> Self {
        Self {
            pattern: String::new(),
            half: None,
            targets: Vec::new(),
        }
    }

    /// What has been typed, for the status bar.
    pub fn typed(&self) -> String {
        self.half.map_or_else(
            || self.pattern.clone(),
            |half| format!("{} [{half}]", self.pattern),
        )
    }

    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    /// Takes a key. `lines` are the lines in view, in reading order, each
    /// with its page and its place on the page; `from` is where distances
    /// are measured from.
    pub fn feed(&mut self, press: Press, lines: &[(u32, usize, &TextLine)], from: Spot) -> Outcome {
        match press {
            Press::Cancel => Outcome::Cancel,
            Press::Submit => self
                .targets
                .first()
                .map_or(Outcome::Stay, |target| Outcome::Jump(target.start)),
            Press::RubOut => {
                if self.half.take().is_some() {
                    return Outcome::Stay;
                }
                if self.pattern.pop().is_none() {
                    return Outcome::Cancel;
                }
                self.search(lines, from);
                Outcome::Stay
            }
            Press::Char(typed) => {
                if let Some(half) = self.half.take() {
                    let label: String = [half, typed].iter().collect();
                    return self
                        .targets
                        .iter()
                        .find(|target| target.label == label)
                        .map_or(Outcome::Stay, |target| Outcome::Jump(target.start));
                }
                let starting: Vec<&Target> = self
                    .targets
                    .iter()
                    .filter(|target| target.label.starts_with(typed))
                    .collect();
                match starting.as_slice() {
                    [] => {
                        self.pattern.push(typed);
                        self.search(lines, from);
                        Outcome::Stay
                    }
                    [only] if only.label.chars().count() == 1 => Outcome::Jump(only.start),
                    _ => {
                        self.half = Some(typed);
                        Outcome::Stay
                    }
                }
            }
            Press::Other => Outcome::Stay,
        }
    }

    /// Finds and labels the matches of the pattern in the lines.
    fn search(&mut self, lines: &[(u32, usize, &TextLine)], from: Spot) {
        self.targets.clear();
        let pattern: Vec<char> = self.pattern.chars().collect();
        if pattern.is_empty() {
            return;
        }
        // As vim's smartcase: case counts only once a capital is typed.
        let exact = pattern.iter().any(|character| character.is_uppercase());
        let same = |one: char, other: char| {
            if one.is_whitespace() && other.is_whitespace() {
                true
            } else if exact {
                one == other
            } else {
                lower(one) == lower(other)
            }
        };
        let from_rank = lines
            .iter()
            .position(|(page, line, _)| *page == from.page && *line == from.line)
            .unwrap_or(0);
        let mut found = Vec::new();
        let mut continuing = Vec::new();
        for (rank, (page, line, text)) in lines.iter().enumerate() {
            let characters: Vec<char> = text.chars.iter().map(|text| text.character).collect();
            let last = characters.len().saturating_sub(pattern.len());
            for index in 0..=last {
                let Some(window) = characters.get(index..index.saturating_add(pattern.len()))
                else {
                    continue;
                };
                if window.len() != pattern.len()
                    || !window.iter().zip(&pattern).all(|(one, other)| same(*one, *other))
                {
                    continue;
                }
                if let Some(next) = characters.get(index.saturating_add(pattern.len())) {
                    continuing.push(lower(*next));
                }
                let distance = (rank.abs_diff(from_rank), index.abs_diff(from.index));
                found.push((
                    distance,
                    Spot {
                        page: *page,
                        line: *line,
                        index,
                    },
                ));
            }
        }
        found.sort_by_key(|(distance, spot)| (*distance, *spot));
        let firsts: Vec<char> = LABELS
            .chars()
            .filter(|letter| !continuing.contains(letter))
            .collect();
        let labels = labels(&firsts, found.len());
        self.targets = found
            .into_iter()
            .enumerate()
            .map(|(index, (_, start))| Target {
                start,
                len: pattern.len(),
                label: labels.get(index).cloned().unwrap_or_default(),
            })
            .collect();
    }
}

/// A character in lower case, as one character.
fn lower(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

/// Labels for so many matches, nearest first: one letter each while there
/// are letters enough, and else two, the first from `firsts`.
fn labels(firsts: &[char], count: usize) -> Vec<String> {
    if count <= firsts.len() {
        return firsts.iter().take(count).map(char::to_string).collect();
    }
    firsts
        .iter()
        .flat_map(|first| LABELS.chars().map(move |second| [*first, second].iter().collect()))
        .take(count)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TextChar;
    use crate::layout::Rect;

    fn line(content: &str) -> TextLine {
        TextLine {
            chars: content
                .chars()
                .map(|character| TextChar {
                    character,
                    area: Rect {
                        left: 0.0,
                        top: 0.0,
                        right: 0.0,
                        bottom: 0.0,
                    },
                })
                .collect(),
        }
    }

    fn spot(line: usize, index: usize) -> Spot {
        Spot {
            page: 0,
            line,
            index,
        }
    }

    /// Types a string, one key at a time, and says what the last key came to.
    fn typing(flash: &mut Flash, lines: &[TextLine], keys: &str) -> Outcome {
        let lines: Vec<(u32, usize, &TextLine)> = lines
            .iter()
            .enumerate()
            .map(|(index, line)| (0, index, line))
            .collect();
        let mut outcome = Outcome::Stay;
        for key in keys.chars() {
            outcome = flash.feed(Press::Char(key), &lines, spot(0, 0));
        }
        outcome
    }

    #[test]
    fn matches_are_labelled_nearest_first_and_a_label_jumps() {
        let lines = [line("the cat sat on the mat"), line("then the end")];
        let mut flash = Flash::new();
        assert_eq!(typing(&mut flash, &lines, "th"), Outcome::Stay);
        let starts: Vec<Spot> = flash.targets().iter().map(|target| target.start).collect();
        assert_eq!(starts, vec![spot(0, 0), spot(0, 15), spot(1, 0), spot(1, 5)]);
        // "e" follows every match, so no label starts with it.
        assert!(flash.targets().iter().all(|target| !target.label.starts_with('e')));
        assert_eq!(flash.targets()[0].label, "a");
        assert_eq!(flash.targets()[2].label, "d");
        assert_eq!(typing(&mut flash, &lines, "d"), Outcome::Jump(spot(1, 0)));
    }

    #[test]
    fn typing_on_narrows_the_matches() {
        let lines = [line("the cat sat on the mat"), line("then the end")];
        let mut flash = Flash::new();
        typing(&mut flash, &lines, "then");
        assert_eq!(flash.targets().len(), 1);
        let lines: Vec<(u32, usize, &TextLine)> = vec![(0, 0, &lines[0]), (0, 1, &lines[1])];
        assert_eq!(
            flash.feed(Press::Submit, &lines, spot(0, 0)),
            Outcome::Jump(spot(1, 0))
        );
        // Rubbing out widens them again.
        flash.feed(Press::RubOut, &lines, spot(0, 0));
        flash.feed(Press::RubOut, &lines, spot(0, 0));
        assert_eq!(flash.targets().len(), 4);
    }

    #[test]
    fn case_counts_only_once_a_capital_is_typed() {
        let lines = [line("Alice and alice")];
        let mut flash = Flash::new();
        typing(&mut flash, &lines, "ali");
        assert_eq!(flash.targets().len(), 2);
        let mut flash = Flash::new();
        typing(&mut flash, &lines, "Ali");
        assert_eq!(flash.targets().len(), 1);
    }

    #[test]
    fn many_matches_get_labels_of_two_letters() {
        let lines: Vec<TextLine> = (0..40).map(|_| line("x")).collect();
        let mut flash = Flash::new();
        typing(&mut flash, &lines, "x");
        assert_eq!(flash.targets().len(), 40);
        assert!(flash.targets().iter().all(|target| target.label.chars().count() == 2));
        let label = flash.targets()[30].label.clone();
        let jumped = typing(&mut flash, &lines, &label);
        assert_eq!(jumped, Outcome::Jump(spot(30, 0)));
    }

    #[test]
    fn rubbing_out_an_empty_search_gives_up() {
        let mut flash = Flash::new();
        assert_eq!(flash.feed(Press::RubOut, &[], spot(0, 0)), Outcome::Cancel);
    }
}
