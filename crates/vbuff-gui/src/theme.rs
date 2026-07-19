//! A tiny design-system: named spacing/size constants instead of magic
//! numbers scattered across the rendering code.
//!
//! This is intentionally small (a handful of constants, not a theming
//! engine) - the goal is that every hardcoded `6.0`, `40.0`, `52.0` in the
//! popup rendering has one named, documented source of truth, so a future
//! visual pass changes one number instead of grepping for float literals.

/// Extra-small spacing: tight gaps between an icon and adjacent text.
pub(crate) const SPACING_XS: f32 = 4.0;

/// Small spacing: row inner margin, gaps between inline meta segments.
pub(crate) const SPACING_SM: f32 = 6.0;

/// Height of a single list row, including its inner margin.
pub(crate) const ROW_HEIGHT: f32 = 52.0;

/// Size of the kind icon / color swatch / image thumbnail square.
pub(crate) const THUMBNAIL_SIZE: f32 = 40.0;

/// Font size for the kind-icon glyph shown when no thumbnail is available.
pub(crate) const ICON_FONT_SIZE: f32 = 20.0;

/// How many leading rows get a quick-pick number badge (and a Cmd/Ctrl+N
/// binding) - digits 1-9.
pub(crate) const QUICK_PICK_SLOTS: usize = 9;

/// Fixed width reserved for the quick-pick digit column on every row (even
/// rows without a badge), so the icon/thumbnail column stays aligned instead
/// of shifting left on rows past [`QUICK_PICK_SLOTS`].
pub(crate) const QUICK_PICK_BADGE_WIDTH: f32 = 14.0;
