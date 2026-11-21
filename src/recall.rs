//! Going back through the lines typed at a prompt before, as vi's command
//! line does: up recalls the last line that starts as the one being
//! typed, up again the one before, and down comes back, at last to what
//! was being typed.

/// How many lines each prompt remembers.
pub const KEPT: usize = 500;

/// The lines typed at a prompt, the oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lines(Vec<String>);

impl Lines {
    pub const fn new(lines: Vec<String>) -> Self {
        Self(lines)
    }

    /// Remembers a line as the newest, once: typed again, it moves to the
    /// end. Empty lines are not remembered.
    pub fn add(&mut self, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        self.0.retain(|kept| kept != line);
        self.0.push(line.to_owned());
        let over = self.0.len().saturating_sub(KEPT);
        self.0.drain(..over);
    }

    #[cfg(test)]
    pub fn all(&self) -> &[String] {
        &self.0
    }
}

/// A walk through the lines, from the newest back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recall {
    /// What was typed when the walk began: the lines recalled start with
    /// it, and it comes back at the end.
    typed: String,
    /// The line recalled, by its place in the lines; none while back at
    /// what was typed.
    at: Option<usize>,
}

impl Recall {
    pub const fn new(typed: String) -> Self {
        Self { typed, at: None }
    }

    /// The line before the one recalled that starts as what was typed;
    /// none when there is no older one, and the walk stays where it is.
    pub fn older(&mut self, lines: &Lines) -> Option<String> {
        let before = self.at.unwrap_or(lines.0.len());
        let found = lines
            .0
            .get(..before)?
            .iter()
            .rposition(|line| line.starts_with(&self.typed) && *line != self.typed)?;
        self.at = Some(found);
        lines.0.get(found).cloned()
    }

    /// The line after the one recalled that starts as what was typed, or
    /// what was typed once past the newest; none when the walk is already
    /// back at what was typed.
    pub fn newer(&mut self, lines: &Lines) -> Option<String> {
        let after = self.at?.saturating_add(1);
        let found = lines
            .0
            .get(after..)
            .and_then(|newer| {
                newer
                    .iter()
                    .position(|line| line.starts_with(&self.typed) && *line != self.typed)
            })
            .map(|offset| after.saturating_add(offset));
        self.at = found;
        Some(
            found
                .and_then(|index| lines.0.get(index).cloned())
                .unwrap_or_else(|| self.typed.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(all: &[&str]) -> Lines {
        Lines::new(all.iter().map(|line| (*line).to_owned()).collect())
    }

    #[test]
    fn up_goes_back_and_down_comes_back_to_what_was_typed() {
        let lines = lines(&["open a.pdf", "zoom 150", "theme catppuccin-mocha"]);
        let mut recall = Recall::new(String::new());
        assert_eq!(
            recall.older(&lines).as_deref(),
            Some("theme catppuccin-mocha")
        );
        assert_eq!(recall.older(&lines).as_deref(), Some("zoom 150"));
        assert_eq!(recall.older(&lines).as_deref(), Some("open a.pdf"));
        // Nothing older: it stays.
        assert_eq!(recall.older(&lines), None);
        assert_eq!(recall.newer(&lines).as_deref(), Some("zoom 150"));
        assert_eq!(
            recall.newer(&lines).as_deref(),
            Some("theme catppuccin-mocha")
        );
        assert_eq!(recall.newer(&lines).as_deref(), Some(""));
        assert_eq!(recall.newer(&lines), None);
    }

    #[test]
    fn only_lines_starting_as_what_was_typed_are_recalled() {
        let lines = lines(&["zoom 120", "open a.pdf", "zoom 150", "zoom"]);
        let mut recall = Recall::new("zoom".to_owned());
        // The line that is just what was typed is skipped.
        assert_eq!(recall.older(&lines).as_deref(), Some("zoom 150"));
        assert_eq!(recall.older(&lines).as_deref(), Some("zoom 120"));
        assert_eq!(recall.older(&lines), None);
        assert_eq!(recall.newer(&lines).as_deref(), Some("zoom 150"));
        assert_eq!(recall.newer(&lines).as_deref(), Some("zoom"));
    }

    #[test]
    fn a_line_typed_again_moves_to_the_end_and_only_so_many_are_kept() {
        let mut lines = lines(&["one", "two"]);
        lines.add("one");
        lines.add("  ");
        assert_eq!(lines.all(), ["two", "one"]);
        for number in 0..KEPT {
            lines.add(&number.to_string());
        }
        assert_eq!(lines.all().len(), KEPT);
        assert_eq!(lines.all().first().map(String::as_str), Some("0"));
    }

    #[test]
    fn with_nothing_remembered_up_and_down_do_nothing() {
        let lines = Lines::default();
        let mut recall = Recall::new("x".to_owned());
        assert_eq!(recall.older(&lines), None);
        assert_eq!(recall.newer(&lines), None);
    }
}
