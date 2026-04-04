use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

pub struct CommandEntry {
    pub command: String,
    pub description: String,
}

pub struct Picker {
    pub query: String,
    pub entries: Vec<CommandEntry>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    matcher: Matcher,
}

impl Picker {
    pub fn new(entries: Vec<CommandEntry>) -> Self {
        let filtered: Vec<usize> = (0..entries.len()).collect();
        Self {
            query: String::new(),
            entries,
            filtered,
            selected: 0,
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    pub fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            let pattern = Pattern::new(
                &self.query,
                CaseMatching::Smart,
                Normalization::Smart,
                AtomKind::Fuzzy,
            );
            let mut buf = Vec::new();
            let mut scored: Vec<(usize, u32)> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, entry)| {
                    let haystack = Utf32Str::new(&entry.command, &mut buf);
                    let score = pattern.score(haystack, &mut self.matcher)?;
                    Some((i, score))
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered = scored.into_iter().map(|(i, _)| i).collect();
        }
        self.selected = 0;
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1).min(self.filtered.len() - 1);
        }
    }

    pub fn selected_command(&self) -> Option<&str> {
        let idx = *self.filtered.get(self.selected)?;
        Some(&self.entries[idx].command)
    }

    pub fn type_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_filter();
    }
}

/// Build picker entries from inspect.providers response.
pub fn build_entries(data: &serde_json::Value) -> Vec<CommandEntry> {
    let mut entries = Vec::new();

    let Some(providers) = data.as_array() else {
        return entries;
    };

    for p in providers {
        let name = p["name"].as_str().unwrap_or("?");

        if let Some(topics) = p["topics"].as_array() {
            for t in topics {
                if let Some(topic) = t.as_str() {
                    entries.push(CommandEntry {
                        command: format!("get {topic}"),
                        description: format!("{name} topic"),
                    });
                    entries.push(CommandEntry {
                        command: format!("sub {topic}"),
                        description: format!("subscribe to {name}"),
                    });
                }
            }
        }

        if let Some(methods) = p["methods"].as_array() {
            for m in methods {
                if let Some(method) = m.as_str() {
                    entries.push(CommandEntry {
                        command: format!("call {method}"),
                        description: format!("{name} method"),
                    });
                }
            }
        }
    }

    // Add built-in commands.
    entries.push(CommandEntry {
        command: "inspect".into(),
        description: "list providers".into(),
    });
    entries.push(CommandEntry {
        command: "clear".into(),
        description: "clear messages".into(),
    });
    entries.push(CommandEntry {
        command: "quit".into(),
        description: "exit".into(),
    });

    entries
}
