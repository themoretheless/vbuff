//! Pure composition state for paste stacks, form-fill plans, and merges.

use std::fmt;

use thiserror::Error;

const MAX_STACK_ITEMS: usize = 128;
const MAX_ITEM_BYTES: usize = 1024 * 1024;
const MAX_SLOT_NAME_BYTES: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PasteStackItemId(u64);

#[derive(Clone, PartialEq, Eq)]
pub struct PasteStackItem {
    pub id: PasteStackItemId,
    pub label: String,
    pub text: String,
}

impl fmt::Debug for PasteStackItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasteStackItem")
            .field("id", &self.id)
            .field(
                "label",
                &format_args!("[redacted; {} bytes]", self.label.len()),
            )
            .field(
                "text",
                &format_args!("[redacted; {} bytes]", self.text.len()),
            )
            .finish()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PasteStack {
    items: Vec<PasteStackItem>,
    next_id: u64,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ComposeError {
    #[error("composition item is empty")]
    Empty,
    #[error("composition item exceeds the byte limit")]
    TooLarge,
    #[error("paste stack is full")]
    StackFull,
    #[error("paste stack item was not found")]
    NotFound,
    #[error("form slot name is invalid")]
    InvalidSlotName,
}

impl PasteStack {
    pub fn items(&self) -> &[PasteStackItem] {
        &self.items
    }

    pub fn item(&self, id: PasteStackItemId) -> Result<&PasteStackItem, ComposeError> {
        Ok(&self.items[self.index(id)?])
    }

    pub fn add(
        &mut self,
        label: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<PasteStackItemId, ComposeError> {
        if self.items.len() >= MAX_STACK_ITEMS {
            return Err(ComposeError::StackFull);
        }
        let label = label.into();
        let text = text.into();
        validate_label(&label)?;
        validate_text(&text)?;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let id = PasteStackItemId(self.next_id);
        self.items.push(PasteStackItem { id, label, text });
        Ok(id)
    }

    pub fn edit(
        &mut self,
        id: PasteStackItemId,
        text: impl Into<String>,
    ) -> Result<(), ComposeError> {
        let text = text.into();
        validate_text(&text)?;
        self.item_mut(id)?.text = text;
        Ok(())
    }

    pub fn rename(
        &mut self,
        id: PasteStackItemId,
        label: impl Into<String>,
    ) -> Result<(), ComposeError> {
        let label = label.into();
        validate_label(&label)?;
        self.item_mut(id)?.label = label;
        Ok(())
    }

    pub fn remove(&mut self, id: PasteStackItemId) -> Result<PasteStackItem, ComposeError> {
        let index = self.index(id)?;
        Ok(self.items.remove(index))
    }

    pub fn duplicate(&mut self, id: PasteStackItemId) -> Result<PasteStackItemId, ComposeError> {
        if self.items.len() >= MAX_STACK_ITEMS {
            return Err(ComposeError::StackFull);
        }
        let index = self.index(id)?;
        let source = self.items[index].clone();
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let new_id = PasteStackItemId(self.next_id);
        self.items.insert(
            index + 1,
            PasteStackItem {
                id: new_id,
                label: source.label,
                text: source.text,
            },
        );
        Ok(new_id)
    }

    pub fn move_up(&mut self, id: PasteStackItemId) -> Result<(), ComposeError> {
        let index = self.index(id)?;
        if index > 0 {
            self.items.swap(index, index - 1);
        }
        Ok(())
    }

    pub fn move_down(&mut self, id: PasteStackItemId) -> Result<(), ComposeError> {
        let index = self.index(id)?;
        if index + 1 < self.items.len() {
            self.items.swap(index, index + 1);
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    fn index(&self, id: PasteStackItemId) -> Result<usize, ComposeError> {
        self.items
            .iter()
            .position(|item| item.id == id)
            .ok_or(ComposeError::NotFound)
    }

    fn item_mut(&mut self, id: PasteStackItemId) -> Result<&mut PasteStackItem, ComposeError> {
        let index = self.index(id)?;
        Ok(&mut self.items[index])
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct FormSlot {
    pub name: String,
    pub value: String,
}

impl fmt::Debug for FormSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FormSlot")
            .field(
                "name",
                &format_args!("[redacted; {} bytes]", self.name.len()),
            )
            .field(
                "value",
                &format_args!("[redacted; {} bytes]", self.value.len()),
            )
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormFillPlan {
    slots: Vec<FormSlot>,
    cursor: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FormStep<'a> {
    Paste { name: &'a str, value: &'a str },
    AdvanceFocus,
    Complete,
}

impl fmt::Debug for FormStep<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Paste { name, value } => formatter
                .debug_struct("Paste")
                .field("name", &format_args!("[redacted; {} bytes]", name.len()))
                .field("value", &format_args!("[redacted; {} bytes]", value.len()))
                .finish(),
            Self::AdvanceFocus => formatter.write_str("AdvanceFocus"),
            Self::Complete => formatter.write_str("Complete"),
        }
    }
}

impl FormFillPlan {
    pub fn new(slots: Vec<FormSlot>) -> Result<Self, ComposeError> {
        if slots.len() > MAX_STACK_ITEMS {
            return Err(ComposeError::StackFull);
        }
        for slot in &slots {
            validate_label(&slot.name)?;
            validate_text(&slot.value)?;
        }
        Ok(Self { slots, cursor: 0 })
    }

    pub fn slots(&self) -> &[FormSlot] {
        &self.slots
    }

    /// Returns one explicit operation at a time. The caller must acknowledge
    /// each operation; this type never injects Tab or paste on its own.
    pub fn current_step(&self) -> FormStep<'_> {
        let slot_index = self.cursor / 2;
        if slot_index >= self.slots.len() {
            return FormStep::Complete;
        }
        if self.cursor.is_multiple_of(2) {
            let slot = &self.slots[slot_index];
            FormStep::Paste {
                name: &slot.name,
                value: &slot.value,
            }
        } else {
            FormStep::AdvanceFocus
        }
    }

    pub fn acknowledge(&mut self) {
        if !matches!(self.current_step(), FormStep::Complete) {
            self.cursor = self.cursor.saturating_add(1);
        }
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MergeTemplate {
    #[default]
    Bullets,
    NumberedCitations,
    CsvRows,
    MarkdownTable,
}

pub fn merge_text(
    items: &[impl AsRef<str>],
    template: MergeTemplate,
) -> Result<String, ComposeError> {
    if merge_output_len(items, template).is_none_or(|bytes| bytes > MAX_ITEM_BYTES) {
        return Err(ComposeError::TooLarge);
    }
    Ok(match template {
        MergeTemplate::Bullets => items
            .iter()
            .map(|item| format!("- {}", item.as_ref().trim()))
            .collect::<Vec<_>>()
            .join("\n"),
        MergeTemplate::NumberedCitations => items
            .iter()
            .enumerate()
            .map(|(index, item)| format!("[{}] {}", index + 1, item.as_ref().trim()))
            .collect::<Vec<_>>()
            .join("\n"),
        MergeTemplate::CsvRows => items
            .iter()
            .map(|item| csv_cell(item.as_ref().trim()))
            .collect::<Vec<_>>()
            .join("\n"),
        MergeTemplate::MarkdownTable => {
            let mut rows = vec!["| # | Clip |".to_string(), "|---:|---|".to_string()];
            rows.extend(items.iter().enumerate().map(|(index, item)| {
                let escaped = item
                    .as_ref()
                    .trim()
                    .replace('|', "\\|")
                    .replace('\n', "<br>");
                format!("| {} | {} |", index + 1, escaped)
            }));
            rows.join("\n")
        }
    })
}

fn merge_output_len(items: &[impl AsRef<str>], template: MergeTemplate) -> Option<usize> {
    let separators = items.len().saturating_sub(1);
    match template {
        MergeTemplate::Bullets => items
            .iter()
            .try_fold(0_usize, |total, item| {
                total
                    .checked_add(2)?
                    .checked_add(item.as_ref().trim().len())
            })?
            .checked_add(separators),
        MergeTemplate::NumberedCitations => items
            .iter()
            .enumerate()
            .try_fold(0_usize, |total, (index, item)| {
                total
                    .checked_add(decimal_digits(index + 1))?
                    .checked_add(3)?
                    .checked_add(item.as_ref().trim().len())
            })?
            .checked_add(separators),
        MergeTemplate::CsvRows => items
            .iter()
            .try_fold(0_usize, |total, item| {
                let value = item.as_ref().trim();
                let cell_len = if value.contains([',', '"', '\n', '\r']) {
                    value
                        .len()
                        .checked_add(value.bytes().filter(|byte| *byte == b'"').count())?
                        .checked_add(2)?
                } else {
                    value.len()
                };
                total.checked_add(cell_len)
            })?
            .checked_add(separators),
        MergeTemplate::MarkdownTable => {
            const HEADER_BYTES: usize = "| # | Clip |\n|---:|---|".len();
            items
                .iter()
                .enumerate()
                .try_fold(HEADER_BYTES, |total, (index, item)| {
                    let value = item.as_ref().trim();
                    let escaped_len = value
                        .len()
                        .checked_add(value.bytes().filter(|byte| *byte == b'|').count())?
                        .checked_add(
                            value
                                .bytes()
                                .filter(|byte| *byte == b'\n')
                                .count()
                                .checked_mul(3)?,
                        )?;
                    total
                        .checked_add(8)?
                        .checked_add(decimal_digits(index + 1))?
                        .checked_add(escaped_len)
                })
        }
    }
}

fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn validate_text(text: &str) -> Result<(), ComposeError> {
    if text.is_empty() {
        Err(ComposeError::Empty)
    } else if text.len() > MAX_ITEM_BYTES {
        Err(ComposeError::TooLarge)
    } else {
        Ok(())
    }
}

fn validate_label(label: &str) -> Result<(), ComposeError> {
    if label.trim().is_empty()
        || label.len() > MAX_SLOT_NAME_BYTES
        || label.chars().any(char::is_control)
    {
        Err(ComposeError::InvalidSlotName)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_stack_edits_reorders_duplicates_and_redacts_debug() {
        let mut stack = PasteStack::default();
        let first = stack.add("one", "private one").unwrap();
        let second = stack.add("two", "private two").unwrap();
        stack.move_up(second).unwrap();
        let duplicate = stack.duplicate(second).unwrap();
        stack.edit(duplicate, "edited").unwrap();
        stack.move_down(second).unwrap();
        stack.remove(first).unwrap();
        assert_eq!(stack.items().len(), 2);
        assert_eq!(stack.items()[0].text, "edited");
        assert!(!format!("{:?}", stack.items()[1]).contains("private"));
    }

    #[test]
    fn form_fill_requires_explicit_step_acknowledgement() {
        let mut plan = FormFillPlan::new(vec![
            FormSlot {
                name: "Email".into(),
                value: "a@example.test".into(),
            },
            FormSlot {
                name: "Team".into(),
                value: "vbuff".into(),
            },
        ])
        .unwrap();
        assert!(matches!(
            plan.current_step(),
            FormStep::Paste { name: "Email", .. }
        ));
        plan.acknowledge();
        assert_eq!(plan.current_step(), FormStep::AdvanceFocus);
        plan.acknowledge();
        assert!(matches!(
            plan.current_step(),
            FormStep::Paste { name: "Team", .. }
        ));
        assert!(!format!("{plan:?}").contains("a@example.test"));
        assert!(!format!("{:?}", plan.current_step()).contains("vbuff"));
    }

    #[test]
    fn merge_templates_escape_their_output_formats() {
        let items = ["alpha", "comma, quote \"", "pipe |\nnext"];
        for template in [
            MergeTemplate::Bullets,
            MergeTemplate::NumberedCitations,
            MergeTemplate::CsvRows,
            MergeTemplate::MarkdownTable,
        ] {
            let output = merge_text(&items, template).unwrap();
            assert_eq!(merge_output_len(&items, template), Some(output.len()));
        }
        assert_eq!(
            merge_text(&items, MergeTemplate::Bullets)
                .unwrap()
                .lines()
                .count(),
            4
        );
        assert!(
            merge_text(&items, MergeTemplate::CsvRows)
                .unwrap()
                .contains("\"comma, quote \"\"\"")
        );
        let markdown = merge_text(&items, MergeTemplate::MarkdownTable).unwrap();
        assert!(markdown.contains("pipe \\|<br>next"));
        assert!(markdown.starts_with("| # | Clip |"));

        let huge = ["x".repeat(MAX_ITEM_BYTES), "y".into()];
        assert_eq!(
            merge_text(&huge, MergeTemplate::Bullets),
            Err(ComposeError::TooLarge)
        );
        let mut stack = PasteStack::default();
        assert_eq!(stack.add("", "text"), Err(ComposeError::InvalidSlotName));
    }
}
