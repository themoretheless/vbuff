//! Declarative conditional and computed fields for GUI-authored snippets.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use chrono::{Duration, NaiveDate};
use thiserror::Error;

const MAX_FIELDS: usize = 64;
const MAX_ID_BYTES: usize = 64;
const MAX_LABEL_BYTES: usize = 128;
const MAX_VALUE_BYTES: usize = 1024 * 1024;
const MAX_CHOICES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Checkbox,
    Choice,
}

#[derive(Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    Bool(bool),
    Choice(String),
    Number(i64),
}

impl FieldValue {
    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(value) | Self::Choice(value) => Some(value),
            Self::Bool(_) | Self::Number(_) => None,
        }
    }
}

impl fmt::Debug for FieldValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(value) => formatter
                .debug_tuple("Text")
                .field(&format_args!("[redacted; {} bytes]", value.len()))
                .finish(),
            Self::Choice(value) => formatter
                .debug_tuple("Choice")
                .field(&format_args!("[redacted; {} bytes]", value.len()))
                .finish(),
            Self::Bool(value) => formatter.debug_tuple("Bool").field(value).finish(),
            Self::Number(value) => formatter.debug_tuple("Number").field(value).finish(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum ValuePredicate {
    IsTrue,
    IsFalse,
    NonEmpty,
    Equals(String),
}

impl fmt::Debug for ValuePredicate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IsTrue => formatter.write_str("IsTrue"),
            Self::IsFalse => formatter.write_str("IsFalse"),
            Self::NonEmpty => formatter.write_str("NonEmpty"),
            Self::Equals(value) => formatter
                .debug_tuple("Equals")
                .field(&format_args!("[redacted; {} bytes]", value.len()))
                .finish(),
        }
    }
}

impl ValuePredicate {
    fn matches(&self, value: &FieldValue) -> bool {
        match self {
            Self::IsTrue => matches!(value, FieldValue::Bool(true)),
            Self::IsFalse => matches!(value, FieldValue::Bool(false)),
            Self::NonEmpty => value.text().is_some_and(|value| !value.trim().is_empty()),
            Self::Equals(expected) => value.text() == Some(expected.as_str()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibilityRule {
    pub source_field: String,
    pub predicate: ValuePredicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputedField {
    Uppercase,
    Lowercase,
    Slugify,
    CharacterCount,
    DateOffsetDays(i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDefinition {
    pub id: String,
    pub label: String,
    pub kind: FieldKind,
    pub choices: Vec<String>,
    pub default: FieldValue,
    pub visibility: Option<VisibilityRule>,
    pub computed_from: Option<(String, ComputedField)>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SnippetError {
    #[error("snippet form has an invalid field count")]
    InvalidFieldCount,
    #[error("snippet field identity is invalid")]
    InvalidIdentity,
    #[error("snippet field value does not match its type")]
    TypeMismatch,
    #[error("snippet field dependency must refer to an earlier field")]
    InvalidDependency,
    #[error("snippet field choice is invalid")]
    InvalidChoice,
    #[error("snippet field value exceeds the byte limit")]
    TooLarge,
    #[error("computed date must use YYYY-MM-DD")]
    InvalidDate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnippetForm {
    fields: Vec<FieldDefinition>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct FormEvaluation {
    pub visible_fields: Vec<String>,
    pub values: BTreeMap<String, FieldValue>,
}

impl fmt::Debug for FormEvaluation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FormEvaluation")
            .field("visible_fields", &self.visible_fields)
            .field("value_count", &self.values.len())
            .finish()
    }
}

impl SnippetForm {
    pub fn new(fields: Vec<FieldDefinition>) -> Result<Self, SnippetError> {
        if fields.is_empty() || fields.len() > MAX_FIELDS {
            return Err(SnippetError::InvalidFieldCount);
        }
        let mut prior = BTreeSet::new();
        for field in &fields {
            validate_identity(&field.id, MAX_ID_BYTES)?;
            validate_identity(&field.label, MAX_LABEL_BYTES)?;
            if prior.contains(&field.id) {
                return Err(SnippetError::InvalidIdentity);
            }
            validate_definition(field)?;
            if field
                .visibility
                .as_ref()
                .is_some_and(|rule| !prior.contains(&rule.source_field))
                || field
                    .computed_from
                    .as_ref()
                    .is_some_and(|(source, _)| !prior.contains(source))
            {
                return Err(SnippetError::InvalidDependency);
            }
            prior.insert(field.id.clone());
        }
        Ok(Self { fields })
    }

    pub fn fields(&self) -> &[FieldDefinition] {
        &self.fields
    }

    pub fn evaluate(
        &self,
        overrides: &BTreeMap<String, FieldValue>,
    ) -> Result<FormEvaluation, SnippetError> {
        if overrides
            .keys()
            .any(|id| !self.fields.iter().any(|field| &field.id == id))
        {
            return Err(SnippetError::InvalidIdentity);
        }
        let mut values = BTreeMap::new();
        let mut visible_fields = Vec::new();
        for field in &self.fields {
            let value = if let Some((source, transform)) = &field.computed_from {
                let source = values.get(source).ok_or(SnippetError::InvalidDependency)?;
                compute(source, *transform)?
            } else {
                overrides
                    .get(&field.id)
                    .cloned()
                    .unwrap_or_else(|| field.default.clone())
            };
            validate_value(field, &value)?;
            let visible = field.visibility.as_ref().is_none_or(|rule| {
                values
                    .get(&rule.source_field)
                    .is_some_and(|source| rule.predicate.matches(source))
            });
            if visible {
                visible_fields.push(field.id.clone());
            }
            values.insert(field.id.clone(), value);
        }
        Ok(FormEvaluation {
            visible_fields,
            values,
        })
    }
}

fn validate_definition(field: &FieldDefinition) -> Result<(), SnippetError> {
    if field.choices.len() > MAX_CHOICES
        || field
            .choices
            .iter()
            .any(|choice| choice.is_empty() || choice.len() > MAX_LABEL_BYTES)
    {
        return Err(SnippetError::InvalidChoice);
    }
    match field.kind {
        FieldKind::Text if !field.choices.is_empty() => Err(SnippetError::InvalidChoice),
        FieldKind::Checkbox if !field.choices.is_empty() => Err(SnippetError::InvalidChoice),
        FieldKind::Choice if field.choices.is_empty() => Err(SnippetError::InvalidChoice),
        _ => validate_value(field, &field.default),
    }
}

fn validate_value(field: &FieldDefinition, value: &FieldValue) -> Result<(), SnippetError> {
    let type_matches = matches!(
        (field.kind, value),
        (FieldKind::Text, FieldValue::Text(_) | FieldValue::Number(_))
            | (FieldKind::Checkbox, FieldValue::Bool(_))
            | (FieldKind::Choice, FieldValue::Choice(_))
    );
    if !type_matches {
        return Err(SnippetError::TypeMismatch);
    }
    if value
        .text()
        .is_some_and(|value| value.len() > MAX_VALUE_BYTES)
    {
        return Err(SnippetError::TooLarge);
    }
    if let FieldValue::Choice(choice) = value
        && !field.choices.iter().any(|candidate| candidate == choice)
    {
        return Err(SnippetError::InvalidChoice);
    }
    Ok(())
}

fn compute(source: &FieldValue, transform: ComputedField) -> Result<FieldValue, SnippetError> {
    let text = source.text().ok_or(SnippetError::TypeMismatch)?;
    let value = match transform {
        ComputedField::Uppercase => FieldValue::Text(text.to_uppercase()),
        ComputedField::Lowercase => FieldValue::Text(text.to_lowercase()),
        ComputedField::Slugify => {
            let mut slug = String::new();
            let mut separator = false;
            for character in text.chars().flat_map(char::to_lowercase) {
                if character.is_alphanumeric() {
                    if separator && !slug.is_empty() {
                        slug.push('-');
                    }
                    slug.push(character);
                    separator = false;
                } else {
                    separator = true;
                }
            }
            FieldValue::Text(slug)
        }
        ComputedField::CharacterCount => FieldValue::Number(text.chars().count() as i64),
        ComputedField::DateOffsetDays(days) => {
            let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map_err(|_| SnippetError::InvalidDate)?;
            let shifted = date
                .checked_add_signed(Duration::days(i64::from(days)))
                .ok_or(SnippetError::InvalidDate)?;
            FieldValue::Text(shifted.format("%Y-%m-%d").to_string())
        }
    };
    if value
        .text()
        .is_some_and(|value| value.len() > MAX_VALUE_BYTES)
    {
        return Err(SnippetError::TooLarge);
    }
    Ok(value)
}

fn validate_identity(value: &str, max_bytes: usize) -> Result<(), SnippetError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        Err(SnippetError::InvalidIdentity)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(id: &str, kind: FieldKind, default: FieldValue) -> FieldDefinition {
        FieldDefinition {
            id: id.into(),
            label: id.into(),
            kind,
            choices: Vec::new(),
            default,
            visibility: None,
            computed_from: None,
        }
    }

    #[test]
    fn conditional_fields_follow_explicit_checkbox_and_choice_values() {
        let enabled = field("include_note", FieldKind::Checkbox, FieldValue::Bool(false));
        let mut note = field("note", FieldKind::Text, FieldValue::Text("private".into()));
        note.visibility = Some(VisibilityRule {
            source_field: "include_note".into(),
            predicate: ValuePredicate::IsTrue,
        });
        let form = SnippetForm::new(vec![enabled, note]).unwrap();
        let hidden = form.evaluate(&BTreeMap::new()).unwrap();
        assert_eq!(hidden.visible_fields, vec!["include_note"]);
        let visible = form
            .evaluate(&BTreeMap::from([(
                "include_note".into(),
                FieldValue::Bool(true),
            )]))
            .unwrap();
        assert_eq!(visible.visible_fields, vec!["include_note", "note"]);
        assert!(!format!("{visible:?}").contains("private"));
    }

    #[test]
    fn computed_fields_are_ordered_bounded_and_deterministic() {
        let title = field(
            "title",
            FieldKind::Text,
            FieldValue::Text("Hello, Clipboard World!".into()),
        );
        let mut slug = field("slug", FieldKind::Text, FieldValue::Text(String::new()));
        slug.computed_from = Some(("title".into(), ComputedField::Slugify));
        let mut count = field("count", FieldKind::Text, FieldValue::Number(0));
        count.computed_from = Some(("title".into(), ComputedField::CharacterCount));
        let form = SnippetForm::new(vec![title, slug, count]).unwrap();
        let values = form.evaluate(&BTreeMap::new()).unwrap().values;
        assert_eq!(
            values.get("slug"),
            Some(&FieldValue::Text("hello-clipboard-world".into()))
        );
        assert_eq!(values.get("count"), Some(&FieldValue::Number(23)));
    }

    #[test]
    fn dependencies_cannot_cycle_or_point_forward() {
        let mut derived = field("derived", FieldKind::Text, FieldValue::Text(String::new()));
        derived.computed_from = Some(("later".into(), ComputedField::Uppercase));
        let later = field("later", FieldKind::Text, FieldValue::Text("x".into()));
        assert_eq!(
            SnippetForm::new(vec![derived, later]),
            Err(SnippetError::InvalidDependency)
        );

        let mut recursive = field(
            "recursive",
            FieldKind::Text,
            FieldValue::Text(String::new()),
        );
        recursive.computed_from = Some(("recursive".into(), ComputedField::Uppercase));
        assert_eq!(
            SnippetForm::new(vec![recursive]),
            Err(SnippetError::InvalidDependency)
        );
    }
}
