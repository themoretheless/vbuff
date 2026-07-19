//! Temporary pin boards, queues, collectors, baskets, and action ranking.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use thiserror::Error;
use vbuff_types::{ClipId, ContentKind};

const MAX_BOARD_SLOTS: usize = 9;
const MAX_ROUTED_BOARDS: usize = 64;
const MAX_WORKING_SET: usize = 256;
const MAX_COLLECTOR_ITEMS: usize = 128;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_LABEL_BYTES: usize = 80;
const MAX_CONTEXT_BYTES: usize = 256;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum BoardError {
    #[error("board slot is outside the supported range")]
    InvalidSlot,
    #[error("board or working set is full")]
    Full,
    #[error("board item was not found")]
    NotFound,
    #[error("board label or context is invalid")]
    InvalidText,
    #[error("collector has already completed")]
    CollectorComplete,
    #[error("collector output exceeds the byte limit")]
    TooLarge,
}

#[derive(Clone, PartialEq, Eq)]
pub struct BoardItem {
    pub clip_id: ClipId,
    pub label: String,
}

impl fmt::Debug for BoardItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoardItem")
            .field("clip_id", &self.clip_id)
            .field(
                "label",
                &format_args!("[redacted; {} bytes]", self.label.len()),
            )
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinBoard {
    pub id: String,
    slots: [Option<BoardItem>; MAX_BOARD_SLOTS],
}

impl PinBoard {
    pub fn new(id: impl Into<String>) -> Result<Self, BoardError> {
        let id = id.into();
        validate_text(&id, MAX_LABEL_BYTES)?;
        Ok(Self {
            id,
            slots: std::array::from_fn(|_| None),
        })
    }

    pub fn assign(&mut self, slot: u8, item: BoardItem) -> Result<Option<BoardItem>, BoardError> {
        validate_text(&item.label, MAX_LABEL_BYTES)?;
        let index = slot_index(slot)?;
        Ok(self.slots[index].replace(item))
    }

    pub fn remove(&mut self, slot: u8) -> Result<Option<BoardItem>, BoardError> {
        let index = slot_index(slot)?;
        Ok(self.slots[index].take())
    }

    pub fn get(&self, slot: u8) -> Result<Option<&BoardItem>, BoardError> {
        let index = slot_index(slot)?;
        Ok(self.slots[index].as_ref())
    }

    pub fn occupied(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BoardContext {
    pub app_id: Option<String>,
    pub window_class: Option<String>,
    pub kind: ContentKind,
}

impl fmt::Debug for BoardContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoardContext")
            .field("app_id", &self.app_id.as_ref().map(|_| "[redacted]"))
            .field(
                "window_class",
                &self.window_class.as_ref().map(|_| "[redacted]"),
            )
            .field("kind", &self.kind)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BoardMatcher {
    pub app_id: Option<String>,
    pub window_class: Option<String>,
    pub kind: Option<ContentKind>,
}

impl fmt::Debug for BoardMatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoardMatcher")
            .field("app_id", &self.app_id.as_ref().map(|_| "[redacted]"))
            .field(
                "window_class",
                &self.window_class.as_ref().map(|_| "[redacted]"),
            )
            .field("kind", &self.kind)
            .finish()
    }
}

impl BoardMatcher {
    pub fn validate(&self) -> Result<(), BoardError> {
        for value in [self.app_id.as_deref(), self.window_class.as_deref()]
            .into_iter()
            .flatten()
        {
            validate_text(value, MAX_CONTEXT_BYTES)?;
        }
        if self.app_id.is_none() && self.window_class.is_none() && self.kind.is_none() {
            return Err(BoardError::InvalidText);
        }
        Ok(())
    }

    fn score(&self, context: &BoardContext) -> Option<u8> {
        if self
            .app_id
            .as_ref()
            .is_some_and(|expected| context.app_id.as_ref() != Some(expected))
            || self
                .window_class
                .as_ref()
                .is_some_and(|expected| context.window_class.as_ref() != Some(expected))
            || self.kind.is_some_and(|expected| context.kind != expected)
        {
            return None;
        }
        Some(
            u8::from(self.app_id.is_some()) * 4
                + u8::from(self.window_class.is_some()) * 2
                + u8::from(self.kind.is_some()),
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardRouter {
    entries: Vec<(BoardMatcher, PinBoard)>,
}

impl BoardRouter {
    pub fn add(&mut self, matcher: BoardMatcher, board: PinBoard) -> Result<(), BoardError> {
        matcher.validate()?;
        if self.entries.len() >= MAX_ROUTED_BOARDS {
            return Err(BoardError::Full);
        }
        self.entries.push((matcher, board));
        Ok(())
    }

    pub fn resolve(&self, context: &BoardContext) -> Option<&PinBoard> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, (matcher, board))| {
                matcher.score(context).map(|score| (score, index, board))
            })
            .max_by_key(|(score, index, _)| (*score, std::cmp::Reverse(*index)))
            .map(|(_, _, board)| board)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsumeQueue {
    items: VecDeque<BoardItem>,
    consumed: Vec<BoardItem>,
}

impl ConsumeQueue {
    pub fn push(&mut self, item: BoardItem) -> Result<(), BoardError> {
        validate_text(&item.label, MAX_LABEL_BYTES)?;
        if self.items.len() + self.consumed.len() >= MAX_WORKING_SET {
            return Err(BoardError::Full);
        }
        self.items.push_back(item);
        Ok(())
    }

    pub fn consume(&mut self) -> Option<BoardItem> {
        let item = self.items.pop_front()?;
        self.consumed.push(item.clone());
        Some(item)
    }

    pub fn undo(&mut self) -> Option<&BoardItem> {
        let item = self.consumed.pop()?;
        self.items.push_front(item);
        self.items.front()
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.consumed.len(), self.consumed.len() + self.items.len())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum CollectorJoiner {
    Newline,
    BlankLine,
    Space,
    Custom(String),
}

impl fmt::Debug for CollectorJoiner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Newline => formatter.write_str("Newline"),
            Self::BlankLine => formatter.write_str("BlankLine"),
            Self::Space => formatter.write_str("Space"),
            Self::Custom(value) => formatter
                .debug_tuple("Custom")
                .field(&format_args!("[redacted; {} bytes]", value.len()))
                .finish(),
        }
    }
}

impl CollectorJoiner {
    fn value(&self) -> &str {
        match self {
            Self::Newline => "\n",
            Self::BlankLine => "\n\n",
            Self::Space => " ",
            Self::Custom(value) => value,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct CaptureCollector {
    target: usize,
    joiner: CollectorJoiner,
    parts: Vec<String>,
    output_bytes: usize,
}

impl fmt::Debug for CaptureCollector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CaptureCollector")
            .field("target", &self.target)
            .field("joiner", &self.joiner)
            .field("part_count", &self.parts.len())
            .field("output_bytes", &self.output_bytes)
            .finish()
    }
}

impl CaptureCollector {
    pub fn new(target: usize, joiner: CollectorJoiner) -> Result<Self, BoardError> {
        if target == 0 || target > MAX_COLLECTOR_ITEMS || joiner.value().len() > MAX_LABEL_BYTES {
            return Err(BoardError::InvalidText);
        }
        if let CollectorJoiner::Custom(value) = &joiner
            && (value.is_empty() || value.chars().any(char::is_control))
        {
            return Err(BoardError::InvalidText);
        }
        Ok(Self {
            target,
            joiner,
            parts: Vec::new(),
            output_bytes: 0,
        })
    }

    pub fn append(&mut self, value: impl Into<String>) -> Result<bool, BoardError> {
        if self.complete() {
            return Err(BoardError::CollectorComplete);
        }
        let value = value.into();
        let separator = usize::from(!self.parts.is_empty()) * self.joiner.value().len();
        let next = self
            .output_bytes
            .checked_add(separator)
            .and_then(|bytes| bytes.checked_add(value.len()))
            .ok_or(BoardError::TooLarge)?;
        if next > MAX_TEXT_BYTES {
            return Err(BoardError::TooLarge);
        }
        self.parts.push(value);
        self.output_bytes = next;
        Ok(self.complete())
    }

    pub fn complete(&self) -> bool {
        self.parts.len() == self.target
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.parts.len(), self.target)
    }

    pub fn output(&self) -> Option<String> {
        self.complete()
            .then(|| self.parts.join(self.joiner.value()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionBasket {
    ids: Vec<ClipId>,
}

impl SessionBasket {
    pub fn add(&mut self, id: ClipId) -> Result<bool, BoardError> {
        if self.ids.contains(&id) {
            return Ok(false);
        }
        if self.ids.len() >= MAX_WORKING_SET {
            return Err(BoardError::Full);
        }
        self.ids.push(id);
        Ok(true)
    }

    pub fn remove(&mut self, id: ClipId) -> bool {
        self.ids
            .iter()
            .position(|candidate| *candidate == id)
            .is_some_and(|index| {
                self.ids.remove(index);
                true
            })
    }

    pub fn promote(&mut self) -> Vec<ClipId> {
        std::mem::take(&mut self.ids)
    }

    pub fn ids(&self) -> &[ClipId] {
        &self.ids
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Checklist {
    items: Vec<(ClipId, bool)>,
}

impl Checklist {
    pub fn add(&mut self, id: ClipId) -> Result<(), BoardError> {
        if self.items.iter().any(|(candidate, _)| *candidate == id) {
            return Ok(());
        }
        if self.items.len() >= MAX_WORKING_SET {
            return Err(BoardError::Full);
        }
        self.items.push((id, false));
        Ok(())
    }

    pub fn set_done(&mut self, id: ClipId, done: bool) -> Result<(), BoardError> {
        self.items
            .iter_mut()
            .find(|(candidate, _)| *candidate == id)
            .ok_or(BoardError::NotFound)?
            .1 = done;
        Ok(())
    }

    pub fn pasted(&mut self, id: ClipId, check_on_paste: bool) -> Result<(), BoardError> {
        if check_on_paste {
            self.set_done(id, true)
        } else if self.items.iter().any(|(candidate, _)| *candidate == id) {
            Ok(())
        } else {
            Err(BoardError::NotFound)
        }
    }

    pub fn progress(&self) -> (usize, usize) {
        (
            self.items.iter().filter(|(_, done)| *done).count(),
            self.items.len(),
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NamedSlots {
    slots: BTreeMap<char, ClipId>,
}

impl NamedSlots {
    pub fn assign(&mut self, slot: char, id: ClipId) -> Result<Option<ClipId>, BoardError> {
        let slot = slot.to_ascii_uppercase();
        if !('A'..='I').contains(&slot) {
            return Err(BoardError::InvalidSlot);
        }
        Ok(self.slots.insert(slot, id))
    }

    pub fn get(&self, slot: char) -> Option<ClipId> {
        self.slots.get(&slot.to_ascii_uppercase()).copied()
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ActionCandidate {
    pub id: String,
    pub destination_apps: BTreeSet<String>,
    pub supported_kinds: BTreeSet<ContentKind>,
    pub usage_count: u32,
    pub last_used_ms: u64,
}

impl fmt::Debug for ActionCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActionCandidate")
            .field("id", &self.id)
            .field("destination_app_count", &self.destination_apps.len())
            .field("supported_kinds", &self.supported_kinds)
            .field("usage_count", &self.usage_count)
            .field("last_used_ms", &self.last_used_ms)
            .finish()
    }
}

pub fn rank_actions(
    candidates: &[ActionCandidate],
    context: &BoardContext,
) -> Result<Vec<String>, BoardError> {
    let mut scored = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        validate_text(&candidate.id, MAX_LABEL_BYTES)?;
        if candidate
            .destination_apps
            .iter()
            .any(|value| validate_text(value, MAX_CONTEXT_BYTES).is_err())
        {
            return Err(BoardError::InvalidText);
        }
        let destination = context
            .app_id
            .as_ref()
            .is_some_and(|app| candidate.destination_apps.contains(app));
        let kind = candidate.supported_kinds.contains(&context.kind);
        let score = u64::from(destination) * 1_000_000_000
            + u64::from(kind) * 100_000_000
            + u64::from(candidate.usage_count.min(1_000_000)) * 1_000
            + candidate.last_used_ms.min(999);
        scored.push((score, candidate.id.clone()));
    }
    scored.sort_by(|left, right| right.cmp(left));
    Ok(scored.into_iter().map(|(_, id)| id).collect())
}

fn slot_index(slot: u8) -> Result<usize, BoardError> {
    if (1..=MAX_BOARD_SLOTS as u8).contains(&slot) {
        Ok(usize::from(slot - 1))
    } else {
        Err(BoardError::InvalidSlot)
    }
}

fn validate_text(value: &str, max_bytes: usize) -> Result<(), BoardError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        Err(BoardError::InvalidText)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u128) -> ClipId {
        ClipId(ulid::Ulid::from(value))
    }

    #[test]
    fn routed_pin_boards_are_stable_and_muscle_memory_sized() {
        let mut general = PinBoard::new("general").unwrap();
        general
            .assign(
                1,
                BoardItem {
                    clip_id: id(1),
                    label: "private address".into(),
                },
            )
            .unwrap();
        assert_eq!(general.get(1).unwrap().unwrap().clip_id, id(1));
        assert!(!format!("{general:?}").contains("private address"));

        let mut router = BoardRouter::default();
        router
            .add(
                BoardMatcher {
                    app_id: Some("dev.editor".into()),
                    window_class: None,
                    kind: Some(ContentKind::Code),
                },
                general,
            )
            .unwrap();
        let context = BoardContext {
            app_id: Some("dev.editor".into()),
            window_class: Some("Editor".into()),
            kind: ContentKind::Code,
        };
        assert_eq!(router.resolve(&context).unwrap().id, "general");
        assert!(!format!("{context:?}").contains("dev.editor"));
    }

    #[test]
    fn consuming_queue_advances_and_undo_restores_the_front() {
        let mut queue = ConsumeQueue::default();
        for value in 1..=3 {
            queue
                .push(BoardItem {
                    clip_id: id(value),
                    label: format!("item {value}"),
                })
                .unwrap();
        }
        assert_eq!(queue.consume().unwrap().clip_id, id(1));
        assert_eq!(queue.progress(), (1, 3));
        assert_eq!(queue.undo().unwrap().clip_id, id(1));
        assert_eq!(queue.progress(), (0, 3));
    }

    #[test]
    fn collector_basket_checklist_and_slots_are_bounded_working_sets() {
        let mut collector = CaptureCollector::new(2, CollectorJoiner::Newline).unwrap();
        assert!(!collector.append("first private quote").unwrap());
        assert!(collector.append("second private quote").unwrap());
        assert_eq!(
            collector.output().as_deref(),
            Some("first private quote\nsecond private quote")
        );
        assert!(!format!("{collector:?}").contains("private"));

        let mut basket = SessionBasket::default();
        assert!(basket.add(id(1)).unwrap());
        assert!(!basket.add(id(1)).unwrap());
        assert_eq!(basket.promote(), vec![id(1)]);

        let mut checklist = Checklist::default();
        checklist.add(id(2)).unwrap();
        checklist.pasted(id(2), true).unwrap();
        assert_eq!(checklist.progress(), (1, 1));

        let mut slots = NamedSlots::default();
        slots.assign('a', id(3)).unwrap();
        assert_eq!(slots.get('A'), Some(id(3)));
        assert_eq!(slots.assign('Z', id(4)), Err(BoardError::InvalidSlot));
    }

    #[test]
    fn destination_and_kind_dominate_action_ranking() {
        let context = BoardContext {
            app_id: Some("dev.editor".into()),
            window_class: None,
            kind: ContentKind::Code,
        };
        let candidates = [
            ActionCandidate {
                id: "popular".into(),
                destination_apps: BTreeSet::new(),
                supported_kinds: BTreeSet::new(),
                usage_count: 100,
                last_used_ms: 999,
            },
            ActionCandidate {
                id: "editor-code".into(),
                destination_apps: BTreeSet::from(["dev.editor".into()]),
                supported_kinds: BTreeSet::from([ContentKind::Code]),
                usage_count: 0,
                last_used_ms: 0,
            },
        ];
        assert_eq!(
            rank_actions(&candidates, &context).unwrap()[0],
            "editor-code"
        );
    }
}
