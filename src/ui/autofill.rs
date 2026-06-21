#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoFillItem {
    pub label: String,
    pub insert: String,
    pub detail: Option<String>,
}

impl AutoFillItem {
    pub fn new(label: impl Into<String>, insert: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            insert: insert.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoFillMenu {
    pub title: String,
    pub replacement_start: usize,
    pub replacement_end: usize,
    pub items: Vec<AutoFillItem>,
    pub selected: usize,
}

impl AutoFillMenu {
    pub fn new(
        title: impl Into<String>,
        replacement_start: usize,
        replacement_end: usize,
        items: Vec<AutoFillItem>,
    ) -> Self {
        Self {
            title: title.into(),
            replacement_start,
            replacement_end,
            items,
            selected: 0,
        }
    }

    pub fn with_selected(mut self, selected: usize) -> Self {
        self.selected = selected.min(self.items.len().saturating_sub(1));
        self
    }

    pub fn selected_index(&self) -> Option<usize> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.selected.min(self.items.len() - 1))
        }
    }

    pub fn previous_index(&self) -> usize {
        self.selected_index().unwrap_or(0).saturating_sub(1)
    }

    pub fn next_index(&self) -> usize {
        self.selected_index()
            .map(|idx| (idx + 1).min(self.items.len().saturating_sub(1)))
            .unwrap_or(0)
    }

    pub fn apply(&self, input: &str) -> Option<String> {
        let item = self.items.get(self.selected_index()?)?;
        if self.replacement_start > self.replacement_end || self.replacement_end > input.len() {
            return None;
        }
        if !input.is_char_boundary(self.replacement_start)
            || !input.is_char_boundary(self.replacement_end)
        {
            return None;
        }

        let mut out = String::with_capacity(
            input.len() - (self.replacement_end - self.replacement_start) + item.insert.len(),
        );
        out.push_str(&input[..self.replacement_start]);
        out.push_str(&item.insert);
        out.push_str(&input[self.replacement_end..]);
        Some(out)
    }
}
