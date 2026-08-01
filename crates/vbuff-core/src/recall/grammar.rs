//! Unified query grammar: one AST, one facet registry, one parser.
//!
//! # Why this module exists
//!
//! vbuff ships two independent query parsers with disjoint vocabularies:
//!
//! * [`crate::recall::parse_natural_query`] understands
//!   `app|kind|tag|device|before|after`, quoted values, relative dates and
//!   bare-word sugar (`urls from Chrome last week`);
//! * `vbuff-store`'s `parse_query` understands `host|color|lang|iso_date`,
//!   without quoting or validation.
//!
//! Each parser silently swallows the other's facets as free text, and adding
//! one facet means editing roughly eight places across three files. On top of
//! that the two sides normalize values differently (`to_ascii_lowercase` in
//! the recall parser, Unicode `to_lowercase` in the recall matcher), so a
//! non-ASCII `source_app` never matches an `app:` filter.
//!
//! This module is the single description both sides are meant to be rebuilt
//! on:
//!
//! * [`REGISTRY`] is a table where one row fully describes a facet: its
//!   spelling, value syntax, repeat and empty-value policy, the in-memory
//!   subject it reads and the SQL binding it compiles to. Adding a facet is
//!   one row.
//! * [`parse_query`] is the only parser, driven by that table.
//! * [`normalize_lookup`] is the only string normalization used for
//!   comparisons, on both sides.
//!
//! # Status
//!
//! Design step only: no live search path calls this module yet, and it
//! changes no behaviour. Replacing both parsers with it is the migration step
//! tracked as T1 in `docs/solid-dry-review-2026-07-26.md`.
//!
//! # Deliberate non-goals
//!
//! * **No negation, no disjunction.** Neither of today's grammars has `-app:x`
//!   or `OR`, so neither is introduced here; the AST is a conjunction of
//!   constraints plus free text.
//! * **No new facets.** The registry is exactly the union of the two live
//!   vocabularies. In particular `has_payment_number`, which
//!   [`crate::facets::extract_facets`] writes into `clip_facets`, is
//!   deliberately *not* a query facet: it is unreachable today, and deciding
//!   between "expose it" and "stop writing it" is a product decision, not a
//!   refactor.

use chrono::{DateTime, Duration, NaiveDate, TimeZone as _, Utc};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization as _;
use vbuff_types::ContentKind;

/// Largest accepted raw query, in bytes (mirrors the recall parser).
pub const MAX_QUERY_BYTES: usize = 4 * 1_024;
/// Largest accepted token count (mirrors the recall parser).
pub const MAX_QUERY_TOKENS: usize = 64;
/// Largest accepted facet value, in bytes, measured before normalization.
pub const MAX_FACET_VALUE_BYTES: usize = 512;

/// Table alias the SQL fragments in [`SqlPredicate`] assume for `clips`.
pub const CLIP_ALIAS: &str = "c";

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

/// The one string normalization used for every equality/containment
/// comparison in search, on both the in-memory and the SQL side.
///
/// Today there are three competing rules: `to_ascii_lowercase` in the recall
/// parser, Unicode `to_lowercase` in the recall matcher, and
/// `to_lowercase` again in the store parser. The mismatch is a live bug: an
/// `app:` value folded with ASCII rules can never equal a `source_app` folded
/// with Unicode rules.
///
/// The pipeline is:
///
/// 1. `trim`;
/// 2. NFKC, so decomposed (`e` + combining acute), precomposed (`é`),
///    full-width and ligature spellings collapse onto one form. This matters
///    in practice because macOS pasteboard strings are frequently
///    decomposed while typed queries are precomposed;
/// 3. uppercase-then-lowercase. `std` has no Unicode case folding and the
///    workspace has no case-folding crate; the round trip is the closest
///    locale-independent approximation and is what makes `STRASSE`/`straße`
///    and `ΣΣ`/`σσ` compare equal;
/// 4. NFKC again, because case mapping can leave the result denormalized.
///
/// Known limitation: `İ` (U+0130) lowercases to `i` + combining dot above and
/// therefore does not equal `i`. Fixing that needs real case folding.
///
/// The function is idempotent: `normalize_lookup(normalize_lookup(x)) ==
/// normalize_lookup(x)`.
///
/// It allocates, so executors should normalize the query side once (the
/// parser already does) and, where a subject value is compared repeatedly,
/// cache the normalized form rather than recomputing it per clip.
pub fn normalize_lookup(value: &str) -> String {
    let composed: String = value.trim().nfkc().collect();
    let folded = composed.to_uppercase().to_lowercase();
    folded.nfkc().collect()
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Which of today's parsers owns a facet. Kept so the guard test can assert
/// the registry is exactly the union of the two live vocabularies, and so the
/// migration can retire one grammar at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grammar {
    /// `crates/vbuff-core/src/recall/query.rs`.
    Recall,
    /// `crates/vbuff-store/src/search.rs`.
    Store,
}

/// How a facet's value text is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueSyntax {
    /// Free text, stored [`normalize_lookup`]-folded.
    Text,
    /// A [`ContentKind`]: the UI synonyms in [`KIND_SYNONYMS`] layered over
    /// the canonical `ContentKind` slugs.
    Kind,
    /// An instant: `YYYY-MM-DD`, `today`, `yesterday` or `last-<duration>`.
    Instant,
}

/// How a constraint value is compared with the subject value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    /// Normalized equality.
    Equals,
    /// Normalized substring containment: the query value occurs in the
    /// subject value.
    Contains,
    /// Subject instant strictly before the constraint value.
    Before,
    /// Subject instant at or after the constraint value.
    AtOrAfter,
}

/// Where the in-memory executor reads the value being constrained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    /// `Clip::meta.source_app`.
    SourceApp,
    /// `Clip::meta.lineage.origin_device`.
    OriginDevice,
    /// `Clip::meta.kind`.
    Kind,
    /// `Clip::meta.created_at`.
    CreatedAt,
    /// Labels from the `ClipTags` side index.
    Tag,
    /// A value derived from the clip text by
    /// [`crate::facets::extract_facets`], keyed by `clip_facets.key`.
    Derived(&'static str),
}

/// Why a facet has no SQL representation yet. Each variant is concrete
/// migration work for `vbuff-store`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaGap {
    /// The value is stored, but in an un-normalized form that cannot be
    /// compared with a [`normalize_lookup`]-folded query value: SQLite's
    /// `lower()`/`LIKE` fold ASCII only. Needs a normalized column or a
    /// `clip_facets` row written at insert time.
    UnnormalizedSource(&'static str),
    /// Nothing in SQLite stores this at all.
    NotPersisted(&'static str),
}

/// How a facet compiles to SQL against `clips` (aliased [`CLIP_ALIAS`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlBinding {
    /// A column on `clips` whose stored form is directly comparable with the
    /// constraint value.
    Column(&'static str),
    /// `EXISTS` against `clip_facets(clip_id, key, value)` with this key.
    FacetRow(&'static str),
    /// Not expressible against today's schema.
    Missing(SchemaGap),
}

/// What a second constraint on the same facet means.
///
/// The two variants exist because today's parsers disagree, and the
/// disagreement should be visible in the table instead of buried in two
/// implementations. The migration is expected to collapse this column once a
/// single answer is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatPolicy {
    /// A second constraint on this facet is a parse error (today's
    /// `recall/query.rs`: one `Option` slot per facet).
    Reject,
    /// Repeats accumulate and are ANDed (today's `store/search.rs`: a flat
    /// facet list appended to the `WHERE` clause).
    Conjunction,
}

/// What `name:` with a blank value means.
///
/// Same rationale as [`RepeatPolicy`]: today's parsers disagree, so the
/// divergence is recorded in the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyValue {
    /// A parse error (`recall/query.rs`).
    Reject,
    /// Not a facet at all: the whole token stays a free-text term
    /// (`store/search.rs`).
    Text,
}

/// One row of the facet table: everything needed to parse, normalize, match
/// in memory and compile to SQL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FacetSpec {
    /// Canonical spelling, used in `name:value` and as the `clip_facets` key
    /// where one applies.
    pub name: &'static str,
    /// Additional accepted `name:` spellings. Empty today; kept so a rename
    /// can keep the old spelling working instead of forking the parser.
    pub aliases: &'static [&'static str],
    /// How the value text is read.
    pub value: ValueSyntax,
    /// Whether a bare token (no `name:` prefix) that matches this facet's
    /// synonym vocabulary becomes a constraint. Only meaningful with
    /// [`ValueSyntax::Kind`], and deliberately limited to [`KIND_SYNONYMS`]:
    /// today a bare `urls` filters by kind while a bare `html` does not.
    pub bare_synonyms: bool,
    /// Comparison applied between constraint value and subject value.
    pub compare: Compare,
    /// Where the in-memory executor reads the subject value.
    pub subject: Subject,
    /// How the facet compiles to SQL.
    pub sql: SqlBinding,
    /// Meaning of a repeated constraint.
    pub repeat: RepeatPolicy,
    /// Meaning of a blank value.
    pub empty_value: EmptyValue,
    /// Which live parser owns this facet today.
    pub grammar: Grammar,
}

impl FacetSpec {
    /// Whether `key` (already [`normalize_lookup`]-folded) names this facet.
    pub fn answers_to(&self, key: &str) -> bool {
        self.name == key || self.aliases.contains(&key)
    }
}

/// The facet table. Adding a facet is one row here plus, if it needs a new
/// storage location, one [`SqlBinding`] target.
///
/// Ordering is by grammar, then by the order the facets appear in that
/// grammar's parser, so the table reads as a diff against today's code.
pub const REGISTRY: &[FacetSpec] = &[
    FacetSpec {
        name: "app",
        aliases: &[],
        value: ValueSyntax::Text,
        bare_synonyms: false,
        compare: Compare::Contains,
        subject: Subject::SourceApp,
        sql: SqlBinding::Missing(SchemaGap::UnnormalizedSource("clips.source_app")),
        repeat: RepeatPolicy::Reject,
        empty_value: EmptyValue::Reject,
        grammar: Grammar::Recall,
    },
    FacetSpec {
        name: "kind",
        aliases: &[],
        value: ValueSyntax::Kind,
        bare_synonyms: true,
        compare: Compare::Equals,
        subject: Subject::Kind,
        sql: SqlBinding::Column("kind"),
        repeat: RepeatPolicy::Reject,
        empty_value: EmptyValue::Reject,
        grammar: Grammar::Recall,
    },
    FacetSpec {
        name: "tag",
        aliases: &[],
        value: ValueSyntax::Text,
        bare_synonyms: false,
        compare: Compare::Equals,
        subject: Subject::Tag,
        sql: SqlBinding::Missing(SchemaGap::NotPersisted(
            "clip tags live only in the in-memory ClipTags index",
        )),
        repeat: RepeatPolicy::Reject,
        empty_value: EmptyValue::Reject,
        grammar: Grammar::Recall,
    },
    FacetSpec {
        name: "device",
        aliases: &[],
        value: ValueSyntax::Text,
        bare_synonyms: false,
        compare: Compare::Equals,
        subject: Subject::OriginDevice,
        sql: SqlBinding::Missing(SchemaGap::UnnormalizedSource(
            "json_extract(clips.metadata_json, '$.lineage.origin_device')",
        )),
        repeat: RepeatPolicy::Reject,
        empty_value: EmptyValue::Reject,
        grammar: Grammar::Recall,
    },
    FacetSpec {
        name: "before",
        aliases: &[],
        value: ValueSyntax::Instant,
        bare_synonyms: false,
        compare: Compare::Before,
        subject: Subject::CreatedAt,
        sql: SqlBinding::Column("created_at"),
        repeat: RepeatPolicy::Reject,
        empty_value: EmptyValue::Reject,
        grammar: Grammar::Recall,
    },
    FacetSpec {
        name: "after",
        aliases: &[],
        value: ValueSyntax::Instant,
        bare_synonyms: false,
        compare: Compare::AtOrAfter,
        subject: Subject::CreatedAt,
        sql: SqlBinding::Column("created_at"),
        repeat: RepeatPolicy::Reject,
        empty_value: EmptyValue::Reject,
        grammar: Grammar::Recall,
    },
    FacetSpec {
        name: "host",
        aliases: &[],
        value: ValueSyntax::Text,
        bare_synonyms: false,
        compare: Compare::Equals,
        subject: Subject::Derived("host"),
        sql: SqlBinding::FacetRow("host"),
        repeat: RepeatPolicy::Conjunction,
        empty_value: EmptyValue::Text,
        grammar: Grammar::Store,
    },
    FacetSpec {
        name: "color",
        aliases: &[],
        value: ValueSyntax::Text,
        bare_synonyms: false,
        compare: Compare::Equals,
        subject: Subject::Derived("color"),
        sql: SqlBinding::FacetRow("color"),
        repeat: RepeatPolicy::Conjunction,
        empty_value: EmptyValue::Text,
        grammar: Grammar::Store,
    },
    FacetSpec {
        name: "lang",
        aliases: &[],
        value: ValueSyntax::Text,
        bare_synonyms: false,
        compare: Compare::Equals,
        subject: Subject::Derived("lang"),
        sql: SqlBinding::FacetRow("lang"),
        repeat: RepeatPolicy::Conjunction,
        empty_value: EmptyValue::Text,
        grammar: Grammar::Store,
    },
    FacetSpec {
        name: "iso_date",
        aliases: &[],
        value: ValueSyntax::Text,
        bare_synonyms: false,
        compare: Compare::Equals,
        subject: Subject::Derived("iso_date"),
        sql: SqlBinding::FacetRow("iso_date"),
        repeat: RepeatPolicy::Conjunction,
        empty_value: EmptyValue::Text,
        grammar: Grammar::Store,
    },
];

/// UI synonyms for [`ContentKind`], accepted both as `kind:` values and as
/// bare words. Layered over (not replacing) the canonical `ContentKind`
/// slugs, which `kind:` also accepts.
pub const KIND_SYNONYMS: &[(&str, ContentKind)] = &[
    ("url", ContentKind::Url),
    ("urls", ContentKind::Url),
    ("link", ContentKind::Url),
    ("links", ContentKind::Url),
    ("image", ContentKind::Image),
    ("images", ContentKind::Image),
    ("picture", ContentKind::Image),
    ("pictures", ContentKind::Image),
    ("code", ContentKind::Code),
    ("snippet", ContentKind::Code),
    ("snippets", ContentKind::Code),
    ("file", ContentKind::File),
    ("files", ContentKind::File),
    ("color", ContentKind::Color),
    ("colors", ContentKind::Color),
];

/// Look a facet up by `name:` key. `key` may be in any case or width; it is
/// folded with [`normalize_lookup`] first.
pub fn facet_spec(key: &str) -> Option<&'static FacetSpec> {
    let key = normalize_lookup(key);
    REGISTRY.iter().find(|spec| spec.answers_to(&key))
}

// ---------------------------------------------------------------------------
// Bare-word sugar
// ---------------------------------------------------------------------------

/// A fixed calendar window expressed by a sugar word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarWindow {
    /// `[start of today, ∞)`.
    Today,
    /// `[start of yesterday, start of today)`.
    Yesterday,
    /// `[start of today, today 12:00 UTC)`.
    UntilLunch,
}

/// What a sugar word does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SugarRule {
    /// The next token is the value of the named facet (`from Chrome`).
    ValueFollows { facet: &'static str },
    /// The next token is a relative duration; the named facet gets
    /// `now - duration` (`last week`).
    DurationFollows { facet: &'static str },
    /// The rule fires only when the next token equals `qualifier`
    /// (`before lunch`); otherwise the word is ordinary text.
    QualifiedWindow {
        qualifier: &'static str,
        window: CalendarWindow,
    },
    /// A standalone window word (`today`, `yesterday`).
    Window(CalendarWindow),
}

/// One sugar word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SugarSpec {
    /// The lead token, already in normalized form.
    pub word: &'static str,
    /// What it expands to.
    pub rule: SugarRule,
}

/// Bare-word sugar of the recall grammar. Exactly today's set: no word is
/// added, none is dropped.
pub const SUGAR: &[SugarSpec] = &[
    SugarSpec {
        word: "from",
        rule: SugarRule::ValueFollows { facet: "app" },
    },
    SugarSpec {
        word: "last",
        rule: SugarRule::DurationFollows { facet: "after" },
    },
    SugarSpec {
        word: "today",
        rule: SugarRule::Window(CalendarWindow::Today),
    },
    SugarSpec {
        word: "yesterday",
        rule: SugarRule::Window(CalendarWindow::Yesterday),
    },
    SugarSpec {
        word: "before",
        rule: SugarRule::QualifiedWindow {
            qualifier: "lunch",
            window: CalendarWindow::UntilLunch,
        },
    },
];

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

/// A parsed facet value. Text values are already [`normalize_lookup`]-folded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FacetValue {
    Text(String),
    Kind(ContentKind),
    Instant(DateTime<Utc>),
}

impl FacetValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_kind(&self) -> Option<ContentKind> {
        match self {
            Self::Kind(kind) => Some(*kind),
            _ => None,
        }
    }

    pub fn as_instant(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Instant(instant) => Some(*instant),
            _ => None,
        }
    }
}

/// One facet constraint: a registry row plus the value it was given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    spec: &'static FacetSpec,
    value: FacetValue,
}

impl Constraint {
    pub const fn spec(&self) -> &'static FacetSpec {
        self.spec
    }

    pub const fn value(&self) -> &FacetValue {
        &self.value
    }
}

/// A parsed query: free text plus a conjunction of facet constraints.
///
/// Constraints keep source order, and a facet may appear more than once when
/// its [`RepeatPolicy`] allows it; consumers must not assume at most one
/// constraint per facet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    text: String,
    constraints: Vec<Constraint>,
}

impl Query {
    /// The free-text terms, joined by single spaces, in source order and in
    /// their original case (matching today's parsers, which fold case at
    /// match time rather than at parse time).
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    pub fn has_constraints(&self) -> bool {
        !self.constraints.is_empty()
    }

    /// Every constraint on the named facet, in source order. The name is
    /// resolved through [`facet_spec`], so aliases and casing work here too;
    /// a name outside the registry yields nothing.
    pub fn constraints_for<'a>(&'a self, facet: &str) -> impl Iterator<Item = &'a Constraint> + 'a {
        let name = facet_spec(facet).map(|spec| spec.name);
        self.constraints
            .iter()
            .filter(move |constraint| Some(constraint.spec.name) == name)
    }

    /// First value of the named facet, for the single-valued facets that the
    /// current call sites expect.
    pub fn first(&self, facet: &str) -> Option<&FacetValue> {
        self.constraints_for(facet).next().map(Constraint::value)
    }

    /// Stable identity of the query, used to key query pins.
    ///
    /// Order-independent across constraints (`app:a kind:url` and
    /// `kind:url app:a` agree) and domain-separated from the legacy
    /// `NaturalQuery` fingerprint, which uses `vbuff-natural-query-v1`.
    /// Pins are in-memory only, so the domain change costs nothing across
    /// restarts, but the domain string must be bumped again if the encoding
    /// below ever changes.
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut encoded = self
            .constraints
            .iter()
            .map(encode_constraint)
            .collect::<Vec<_>>();
        encoded.sort_unstable();
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"vbuff-query-ast-v1");
        update_framed(&mut hasher, self.text.as_bytes());
        hasher.update(&(encoded.len() as u32).to_be_bytes());
        for entry in &encoded {
            update_framed(&mut hasher, entry);
        }
        *hasher.finalize().as_bytes()
    }
}

fn update_framed(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn encode_constraint(constraint: &Constraint) -> Vec<u8> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(constraint.spec.name.as_bytes());
    encoded.push(0);
    match &constraint.value {
        FacetValue::Text(value) => {
            encoded.push(b'T');
            encoded.extend_from_slice(value.as_bytes());
        }
        FacetValue::Kind(kind) => {
            encoded.push(b'K');
            encoded.extend_from_slice(&kind.stored_discriminant().to_be_bytes());
        }
        FacetValue::Instant(instant) => {
            encoded.push(b'I');
            encoded.extend_from_slice(&instant.timestamp_millis().to_be_bytes());
        }
    }
    encoded
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("query is too large")]
    TooLarge,
    #[error("query syntax is invalid")]
    InvalidSyntax,
    #[error("query filter is invalid")]
    InvalidFilter,
}

/// Parse a raw query into the AST, driven entirely by [`REGISTRY`] and
/// [`SUGAR`].
///
/// # Unknown facets
///
/// A `key:value` token whose key is not in the registry stays a **free-text
/// term**, exactly as both of today's parsers behave. This is a deliberate
/// choice, not an oversight:
///
/// * rejecting unknown keys would break ordinary pasted text, since `:` is
///   common in it. `https://example.test` tokenizes as key `https`, and the
///   recall parser has a pinned test asserting it survives as text;
/// * falling back to text is the conservative direction here: the text path
///   already refuses to look inside sensitive payloads, so a mistyped facet
///   degrades into a search that finds nothing rather than one that widens
///   access;
/// * once both parsers share this registry, the failure mode that motivated
///   the review disappears anyway: a facet is unknown only if it is unknown
///   to *all* of search, not merely to the engine that happened to run.
///
/// A *known* facet with an invalid value (`kind:unknown`, `before:garbage`)
/// is still an error: the user clearly meant a filter, so failing loudly
/// beats silently searching for the literal text.
pub fn parse_query(raw: &str, now: DateTime<Utc>) -> Result<Query, ParseError> {
    if raw.len() > MAX_QUERY_BYTES || raw.contains('\0') {
        return Err(ParseError::TooLarge);
    }
    let tokens = tokenize(raw)?;
    if tokens.len() > MAX_QUERY_TOKENS {
        return Err(ParseError::TooLarge);
    }

    let mut text: Vec<&str> = Vec::new();
    let mut constraints: Vec<Constraint> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].as_str();

        let mut consumed = 0;
        if let Some((key, value)) = token.split_once(':')
            && let Some(spec) = facet_spec(key)
            && let Some(parsed) = parse_facet_value(spec, value, now)?
        {
            push_constraint(&mut constraints, spec, parsed)?;
            consumed = 1;
        }
        if consumed == 0
            && let Some(used) = apply_sugar(&mut constraints, &tokens, index, now)?
        {
            consumed = used;
        }
        if consumed == 0
            && let Some(used) = apply_bare_kind(&mut constraints, token)?
        {
            consumed = used;
        }
        if consumed == 0 {
            text.push(token);
            consumed = 1;
        }
        index += consumed;
    }

    check_time_window(&constraints)?;
    Ok(Query {
        text: text.join(" "),
        constraints,
    })
}

/// Split on whitespace, with `"` toggling quoting so a facet value or a text
/// term can contain spaces. Quotes are removed wherever they appear, so
/// `tag:"release notes"` yields the single token `tag:release notes`. An
/// unterminated quote is a syntax error.
///
/// This is the recall tokenizer. The store parser used bare
/// `split_whitespace`, so unifying gives store-side facets quoting they did
/// not have: a widening, and the only one in this module.
fn tokenize(raw: &str) -> Result<Vec<String>, ParseError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in raw.chars() {
        match character {
            '"' => quoted = !quoted,
            _ if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if quoted {
        return Err(ParseError::InvalidSyntax);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

/// `Ok(None)` means "this is not a facet after all, keep the token as text",
/// which only happens for a blank value on a facet whose [`EmptyValue`]
/// policy is `Text`.
fn parse_facet_value(
    spec: &FacetSpec,
    raw: &str,
    now: DateTime<Utc>,
) -> Result<Option<FacetValue>, ParseError> {
    if raw.len() > MAX_FACET_VALUE_BYTES || raw.chars().any(char::is_control) {
        return Err(ParseError::InvalidFilter);
    }
    if raw.trim().is_empty() {
        return match spec.empty_value {
            EmptyValue::Reject => Err(ParseError::InvalidFilter),
            EmptyValue::Text => Ok(None),
        };
    }
    let normalized = normalize_lookup(raw);
    let value = match spec.value {
        ValueSyntax::Text => FacetValue::Text(normalized),
        ValueSyntax::Kind => {
            FacetValue::Kind(parse_kind(&normalized).ok_or(ParseError::InvalidFilter)?)
        }
        ValueSyntax::Instant => FacetValue::Instant(parse_instant(&normalized, now)?),
    };
    Ok(Some(value))
}

fn push_constraint(
    constraints: &mut Vec<Constraint>,
    spec: &'static FacetSpec,
    value: FacetValue,
) -> Result<(), ParseError> {
    if spec.repeat == RepeatPolicy::Reject
        && constraints
            .iter()
            .any(|constraint| constraint.spec.name == spec.name)
    {
        return Err(ParseError::InvalidFilter);
    }
    constraints.push(Constraint { spec, value });
    Ok(())
}

/// Returns the number of tokens consumed, or `None` when the token is not
/// sugar (or the sugar's follow-up token is missing or wrong, in which case
/// the word is ordinary text, which is today's behaviour for a bare `before`
/// that is not followed by `lunch`).
fn apply_sugar(
    constraints: &mut Vec<Constraint>,
    tokens: &[String],
    index: usize,
    now: DateTime<Utc>,
) -> Result<Option<usize>, ParseError> {
    let word = normalize_lookup(&tokens[index]);
    let Some(sugar) = SUGAR.iter().find(|entry| entry.word == word) else {
        return Ok(None);
    };
    match sugar.rule {
        SugarRule::ValueFollows { facet } => {
            let Some(next) = tokens.get(index + 1) else {
                return Ok(None);
            };
            let spec = facet_spec(facet).ok_or(ParseError::InvalidFilter)?;
            let Some(value) = parse_facet_value(spec, next, now)? else {
                return Ok(None);
            };
            push_constraint(constraints, spec, value)?;
            Ok(Some(2))
        }
        SugarRule::DurationFollows { facet } => {
            let Some(next) = tokens.get(index + 1) else {
                return Ok(None);
            };
            let spec = facet_spec(facet).ok_or(ParseError::InvalidFilter)?;
            let duration = parse_relative_duration(&normalize_lookup(next))?;
            push_constraint(constraints, spec, FacetValue::Instant(now - duration))?;
            Ok(Some(2))
        }
        SugarRule::QualifiedWindow { qualifier, window } => {
            let qualified = tokens
                .get(index + 1)
                .is_some_and(|next| normalize_lookup(next) == qualifier);
            if !qualified {
                return Ok(None);
            }
            push_window(constraints, window, now)?;
            Ok(Some(2))
        }
        SugarRule::Window(window) => {
            push_window(constraints, window, now)?;
            Ok(Some(1))
        }
    }
}

fn push_window(
    constraints: &mut Vec<Constraint>,
    window: CalendarWindow,
    now: DateTime<Utc>,
) -> Result<(), ParseError> {
    let midnight = start_of_day(now);
    let (after, before) = match window {
        CalendarWindow::Today => (Some(midnight), None),
        CalendarWindow::Yesterday => (Some(midnight - Duration::days(1)), Some(midnight)),
        CalendarWindow::UntilLunch => (Some(midnight), Some(midnight + Duration::hours(12))),
    };
    for (facet, instant) in [("after", after), ("before", before)] {
        let Some(instant) = instant else { continue };
        let spec = facet_spec(facet).ok_or(ParseError::InvalidFilter)?;
        push_constraint(constraints, spec, FacetValue::Instant(instant))?;
    }
    Ok(())
}

fn apply_bare_kind(
    constraints: &mut Vec<Constraint>,
    token: &str,
) -> Result<Option<usize>, ParseError> {
    let word = normalize_lookup(token);
    let Some(kind) = kind_synonym(&word) else {
        return Ok(None);
    };
    let Some(spec) = REGISTRY
        .iter()
        .find(|spec| spec.bare_synonyms && spec.value == ValueSyntax::Kind)
    else {
        return Ok(None);
    };
    push_constraint(constraints, spec, FacetValue::Kind(kind))?;
    Ok(Some(1))
}

/// Derived from the registry rather than from hard-coded facet names: every
/// `CreatedAt` upper bound must be strictly later than every lower bound, or
/// the query can never match.
fn check_time_window(constraints: &[Constraint]) -> Result<(), ParseError> {
    let bound = |compare: Compare| {
        constraints
            .iter()
            .filter(move |constraint| {
                constraint.spec.subject == Subject::CreatedAt && constraint.spec.compare == compare
            })
            .filter_map(|constraint| constraint.value.as_instant())
    };
    let upper = bound(Compare::Before).min();
    let lower = bound(Compare::AtOrAfter).max();
    if let (Some(upper), Some(lower)) = (upper, lower)
        && upper <= lower
    {
        return Err(ParseError::InvalidFilter);
    }
    Ok(())
}

fn kind_synonym(value: &str) -> Option<ContentKind> {
    KIND_SYNONYMS
        .iter()
        .find(|(word, _)| *word == value)
        .map(|(_, kind)| *kind)
}

/// UI synonyms layered over the canonical [`ContentKind`] slugs.
fn parse_kind(normalized: &str) -> Option<ContentKind> {
    kind_synonym(normalized).or_else(|| normalized.parse::<ContentKind>().ok())
}

/// `today`, `yesterday`, `last-<duration>` or `YYYY-MM-DD` (UTC midnight).
///
/// Unlike the recall parser, the `last-` prefix is recognized on the
/// normalized value, so `LAST-2H` works like `last-2h`. That inconsistency in
/// today's code (the match folded case but the `starts_with` guard did not)
/// is fixed here rather than reproduced.
fn parse_instant(normalized: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>, ParseError> {
    match normalized {
        "today" => Ok(start_of_day(now)),
        "yesterday" => Ok(start_of_day(now) - Duration::days(1)),
        _ => {
            if let Some(rest) = normalized.strip_prefix("last-") {
                return Ok(now - parse_relative_duration(rest)?);
            }
            let date = NaiveDate::parse_from_str(normalized, "%Y-%m-%d")
                .map_err(|_| ParseError::InvalidFilter)?;
            let midnight = date.and_hms_opt(0, 0, 0).ok_or(ParseError::InvalidFilter)?;
            Ok(Utc.from_utc_datetime(&midnight))
        }
    }
}

fn parse_relative_duration(normalized: &str) -> Result<Duration, ParseError> {
    match normalized {
        "hour" => return Ok(Duration::hours(1)),
        "day" => return Ok(Duration::days(1)),
        "week" => return Ok(Duration::days(7)),
        _ => {}
    }
    let split = normalized
        .find(|character: char| !character.is_ascii_digit())
        .ok_or(ParseError::InvalidFilter)?;
    let amount = normalized[..split]
        .parse::<i64>()
        .map_err(|_| ParseError::InvalidFilter)?;
    if !(1..=365).contains(&amount) {
        return Err(ParseError::InvalidFilter);
    }
    match &normalized[split..] {
        "m" | "min" => Ok(Duration::minutes(amount)),
        "h" | "hr" => Ok(Duration::hours(amount)),
        "d" | "day" | "days" => Ok(Duration::days(amount)),
        "w" | "week" | "weeks" => Ok(Duration::weeks(amount)),
        _ => Err(ParseError::InvalidFilter),
    }
}

fn start_of_day(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.from_utc_datetime(
        &now.date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("UTC midnight always exists"),
    )
}

// ---------------------------------------------------------------------------
// In-memory execution
// ---------------------------------------------------------------------------

/// What an in-memory executor must expose so the registry can be evaluated
/// against a clip.
///
/// The GUI projection and `search_recall` are expected to implement this over
/// `Clip` plus their side indices (`ClipTags` for [`Subject::Tag`],
/// [`crate::facets::extract_facets`] for [`Subject::Derived`]).
pub trait ClipFacetView {
    fn kind(&self) -> ContentKind;

    fn created_at(&self) -> DateTime<Utc>;

    /// Every value the clip exposes for a string subject, un-normalized; the
    /// matcher folds them with [`normalize_lookup`]. Only called for the
    /// string subjects (`SourceApp`, `OriginDevice`, `Tag`, `Derived`).
    fn values(&self, subject: Subject) -> Vec<String>;
}

impl Constraint {
    /// Evaluate this constraint against a clip.
    ///
    /// Fail-closed: a combination the registry cannot produce (a text value
    /// on a timestamp subject, say) evaluates to `false` rather than being
    /// ignored, so a corrupted table narrows results instead of widening
    /// them.
    pub fn matches<V: ClipFacetView + ?Sized>(&self, view: &V) -> bool {
        match (&self.value, self.spec.subject) {
            (FacetValue::Kind(kind), Subject::Kind) => view.kind() == *kind,
            (FacetValue::Instant(instant), Subject::CreatedAt) => match self.spec.compare {
                Compare::Before => view.created_at() < *instant,
                Compare::AtOrAfter => view.created_at() >= *instant,
                Compare::Equals | Compare::Contains => false,
            },
            (FacetValue::Text(value), subject) => {
                matches!(
                    subject,
                    Subject::SourceApp | Subject::OriginDevice | Subject::Tag | Subject::Derived(_)
                ) && view.values(subject).iter().any(|candidate| {
                    let candidate = normalize_lookup(candidate);
                    match self.spec.compare {
                        Compare::Equals => candidate == *value,
                        Compare::Contains => candidate.contains(value.as_str()),
                        Compare::Before | Compare::AtOrAfter => false,
                    }
                })
            }
            _ => false,
        }
    }
}

/// Whether every constraint of the query holds for a clip. Free text is not
/// evaluated here: scoring stays the executor's job.
pub fn matches_constraints<V: ClipFacetView + ?Sized>(query: &Query, view: &V) -> bool {
    query
        .constraints
        .iter()
        .all(|constraint| constraint.matches(view))
}

// ---------------------------------------------------------------------------
// SQL execution
// ---------------------------------------------------------------------------

/// A bound parameter for a [`SqlPredicate`], deliberately free of any
/// `rusqlite` type so the mapping lives in the pure crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlBind {
    Text(String),
    Integer(i64),
}

/// A `WHERE`-clause fragment plus its bind values, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlPredicate {
    pub fragment: String,
    pub binds: Vec<SqlBind>,
}

impl Constraint {
    /// Compile this constraint to a SQL fragment against `clips` aliased
    /// [`CLIP_ALIAS`].
    ///
    /// Containment uses `instr(...) > 0` rather than `LIKE '%…%'`: no wildcard
    /// escaping is needed, so a user value containing `%` or `_` cannot widen
    /// the match.
    ///
    /// Fails with the registry's [`SchemaGap`] when the facet has no SQL
    /// representation yet, so the store can decide per query whether to
    /// degrade or refuse instead of silently dropping a filter.
    pub fn sql_predicate(&self) -> Result<SqlPredicate, SchemaGap> {
        let bind = match &self.value {
            FacetValue::Text(value) => SqlBind::Text(value.clone()),
            FacetValue::Kind(kind) => SqlBind::Integer(kind.stored_discriminant()),
            FacetValue::Instant(instant) => SqlBind::Integer(instant.timestamp_millis()),
        };
        let operator = match self.spec.compare {
            Compare::Equals => "=",
            Compare::Before => "<",
            Compare::AtOrAfter => ">=",
            Compare::Contains => "",
        };
        match self.spec.sql {
            SqlBinding::Column(column) => {
                let fragment = if self.spec.compare == Compare::Contains {
                    format!("instr({CLIP_ALIAS}.{column}, ?) > 0")
                } else {
                    format!("{CLIP_ALIAS}.{column} {operator} ?")
                };
                Ok(SqlPredicate {
                    fragment,
                    binds: vec![bind],
                })
            }
            SqlBinding::FacetRow(key) => {
                let comparison = if self.spec.compare == Compare::Contains {
                    "instr(f.value, ?) > 0".to_owned()
                } else {
                    format!("f.value {operator} ?")
                };
                Ok(SqlPredicate {
                    fragment: format!(
                        "EXISTS (SELECT 1 FROM clip_facets f \
                         WHERE f.clip_id = {CLIP_ALIAS}.id AND f.key = ? AND {comparison})"
                    ),
                    binds: vec![SqlBind::Text(key.to_owned()), bind],
                })
            }
            SqlBinding::Missing(gap) => Err(gap),
        }
    }
}

/// Compile every constraint of a query, ANDed.
///
/// Returns the first [`SchemaGap`] encountered: partially applying a filter
/// set would return rows the user did not ask for, which is the failure mode
/// this module exists to remove.
pub fn sql_predicates(query: &Query) -> Result<Vec<SqlPredicate>, SchemaGap> {
    query
        .constraints
        .iter()
        .map(Constraint::sql_predicate)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap()
    }

    fn now() -> DateTime<Utc> {
        at(2026, 7, 21, 18)
    }

    fn text_of(query: &Query, facet: &str) -> Option<String> {
        query
            .first(facet)
            .and_then(FacetValue::as_text)
            .map(str::to_owned)
    }

    #[derive(Default)]
    struct FakeClip {
        kind: ContentKind,
        created_at: Option<DateTime<Utc>>,
        source_app: Option<String>,
        device: Option<String>,
        tags: Vec<String>,
        derived: Vec<(String, String)>,
    }

    impl ClipFacetView for FakeClip {
        fn kind(&self) -> ContentKind {
            self.kind
        }

        fn created_at(&self) -> DateTime<Utc> {
            self.created_at.unwrap_or_else(|| at(2026, 7, 21, 12))
        }

        fn values(&self, subject: Subject) -> Vec<String> {
            match subject {
                Subject::SourceApp => self.source_app.clone().into_iter().collect(),
                Subject::OriginDevice => self.device.clone().into_iter().collect(),
                Subject::Tag => self.tags.clone(),
                Subject::Derived(key) => self
                    .derived
                    .iter()
                    .filter(|(candidate, _)| candidate == key)
                    .map(|(_, value)| value.clone())
                    .collect(),
                Subject::Kind | Subject::CreatedAt => Vec::new(),
            }
        }
    }

    // -- registry guards ----------------------------------------------------

    #[test]
    fn registry_covers_both_live_grammars_and_nothing_else() {
        // The literal lists are the vocabularies of the two parsers this
        // module replaces: `recall/query.rs` and `vbuff-store/src/search.rs`.
        // Adding a facet to the registry without a decision fails here.
        let recall = ["app", "kind", "tag", "device", "before", "after"];
        let store = ["host", "color", "lang", "iso_date"];

        let named = |grammar: Grammar| {
            REGISTRY
                .iter()
                .filter(|spec| spec.grammar == grammar)
                .map(|spec| spec.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(named(Grammar::Recall), recall);
        assert_eq!(named(Grammar::Store), store);
        assert_eq!(REGISTRY.len(), recall.len() + store.len());

        for name in recall.iter().chain(store.iter()) {
            assert!(facet_spec(name).is_some(), "{name} is not resolvable");
            assert!(
                facet_spec(&name.to_uppercase()).is_some(),
                "{name} is case sensitive"
            );
        }
        // `has_payment_number` is written by `facets::extract_facets` but is
        // deliberately unreachable from the grammar; see the module docs.
        assert!(facet_spec("has_payment_number").is_none());
    }

    #[test]
    fn registry_rows_are_internally_consistent() {
        for spec in REGISTRY {
            match spec.value {
                ValueSyntax::Instant => assert!(
                    matches!(spec.compare, Compare::Before | Compare::AtOrAfter)
                        && spec.subject == Subject::CreatedAt,
                    "{} is an instant facet with a non-instant comparison",
                    spec.name
                ),
                ValueSyntax::Kind => assert!(
                    spec.compare == Compare::Equals && spec.subject == Subject::Kind,
                    "{} is a kind facet with a non-kind comparison",
                    spec.name
                ),
                ValueSyntax::Text => assert!(
                    matches!(spec.compare, Compare::Equals | Compare::Contains),
                    "{} is a text facet with a non-text comparison",
                    spec.name
                ),
            }
            assert!(
                !spec.bare_synonyms || spec.value == ValueSyntax::Kind,
                "{} claims bare synonyms without a synonym vocabulary",
                spec.name
            );
            if let SqlBinding::FacetRow(key) = spec.sql {
                assert_eq!(
                    spec.subject,
                    Subject::Derived(key),
                    "{} disagrees with itself about its facet key",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn sugar_targets_resolve_and_do_not_shadow_kind_synonyms() {
        for entry in SUGAR {
            assert_eq!(
                entry.word,
                normalize_lookup(entry.word),
                "sugar word {} is not in normalized form",
                entry.word
            );
            match entry.rule {
                SugarRule::ValueFollows { facet } | SugarRule::DurationFollows { facet } => {
                    assert!(facet_spec(facet).is_some(), "sugar targets unknown {facet}");
                }
                SugarRule::QualifiedWindow { .. } | SugarRule::Window(_) => {}
            }
            assert!(
                kind_synonym(entry.word).is_none(),
                "sugar word {} also parses as a kind",
                entry.word
            );
        }
        for (word, _) in KIND_SYNONYMS {
            assert_eq!(*word, normalize_lookup(word));
        }
    }

    // -- normalization ------------------------------------------------------

    #[test]
    fn normalize_lookup_is_unicode_aware_and_idempotent() {
        // The bug this replaces: ASCII folding never matches Unicode folding.
        assert_eq!(normalize_lookup("БРАУЗЕР"), "браузер");
        assert_eq!(normalize_lookup("ЧАТ"), normalize_lookup("чат"));
        // Decomposed (macOS pasteboard) vs precomposed spelling.
        assert_eq!(normalize_lookup("cafe\u{301}"), normalize_lookup("café"));
        // Compatibility forms: full-width and ligature.
        assert_eq!(normalize_lookup("\u{ff23}\u{ff28}"), "ch");
        assert_eq!(normalize_lookup("\u{fb01}le"), "file");
        // Poor-man's case folding.
        assert_eq!(normalize_lookup("straße"), normalize_lookup("STRASSE"));
        assert_eq!(normalize_lookup("ΣΣ"), normalize_lookup("σσ"));
        assert_eq!(normalize_lookup("  Chrome  "), "chrome");

        for sample in [
            "Chrome",
            "БРАУЗЕР",
            "cafe\u{301}",
            "straße",
            "ΣΣ",
            "\u{ff23}\u{ff28}",
            "  spaced  ",
            "",
        ] {
            let once = normalize_lookup(sample);
            assert_eq!(
                normalize_lookup(&once),
                once,
                "not idempotent for {sample:?}"
            );
        }
    }

    // -- parsing: recall vocabulary ----------------------------------------

    #[test]
    fn parses_every_recall_facet() {
        let query = parse_query(
            "app:Chrome kind:link tag:Work device:Laptop after:2026-07-01 before:2026-07-20",
            now(),
        )
        .unwrap();
        assert_eq!(text_of(&query, "app").as_deref(), Some("chrome"));
        assert_eq!(
            query.first("kind").and_then(FacetValue::as_kind),
            Some(ContentKind::Url)
        );
        assert_eq!(text_of(&query, "tag").as_deref(), Some("work"));
        assert_eq!(text_of(&query, "device").as_deref(), Some("laptop"));
        assert_eq!(
            query.first("after").and_then(FacetValue::as_instant),
            Some(at(2026, 7, 1, 0))
        );
        assert_eq!(
            query.first("before").and_then(FacetValue::as_instant),
            Some(at(2026, 7, 20, 0))
        );
        assert!(query.text().is_empty());
    }

    #[test]
    fn parses_every_store_facet() {
        let query = parse_query(
            "sqlite host:Docs.RS lang:Rust color:#FF8800 iso_date:2026-07-21",
            now(),
        )
        .unwrap();
        assert_eq!(query.text(), "sqlite");
        assert_eq!(text_of(&query, "host").as_deref(), Some("docs.rs"));
        assert_eq!(text_of(&query, "lang").as_deref(), Some("rust"));
        assert_eq!(text_of(&query, "color").as_deref(), Some("#ff8800"));
        assert_eq!(text_of(&query, "iso_date").as_deref(), Some("2026-07-21"));
    }

    #[test]
    fn recall_sugar_still_parses() {
        let query = parse_query("urls from Chrome last week", now()).unwrap();
        assert_eq!(
            query.first("kind").and_then(FacetValue::as_kind),
            Some(ContentKind::Url)
        );
        assert_eq!(text_of(&query, "app").as_deref(), Some("chrome"));
        assert_eq!(
            query.first("after").and_then(FacetValue::as_instant),
            Some(now() - Duration::days(7))
        );
        assert!(query.text().is_empty());

        let today = parse_query("today", now()).unwrap();
        assert_eq!(
            today.first("after").and_then(FacetValue::as_instant),
            Some(at(2026, 7, 21, 0))
        );
        assert!(today.first("before").is_none());

        let yesterday = parse_query("yesterday", now()).unwrap();
        assert_eq!(
            yesterday.first("after").and_then(FacetValue::as_instant),
            Some(at(2026, 7, 20, 0))
        );
        assert_eq!(
            yesterday.first("before").and_then(FacetValue::as_instant),
            Some(at(2026, 7, 21, 0))
        );

        let lunch = parse_query("\"release note\" before lunch", now()).unwrap();
        assert_eq!(lunch.text(), "release note");
        assert_eq!(
            lunch.first("after").and_then(FacetValue::as_instant),
            Some(at(2026, 7, 21, 0))
        );
        assert_eq!(
            lunch.first("before").and_then(FacetValue::as_instant),
            Some(at(2026, 7, 21, 12))
        );

        // `before` without `lunch` is plain text, as today.
        let bare = parse_query("before noon", now()).unwrap();
        assert_eq!(bare.text(), "before noon");
        assert!(!bare.has_constraints());
    }

    #[test]
    fn relative_instants_are_case_insensitive() {
        let relative = parse_query("after:LAST-2H", now()).unwrap();
        assert_eq!(
            relative.first("after").and_then(FacetValue::as_instant),
            Some(now() - Duration::hours(2))
        );
        let today = parse_query("after:Today", now()).unwrap();
        assert_eq!(
            today.first("after").and_then(FacetValue::as_instant),
            Some(at(2026, 7, 21, 0))
        );
        let yesterday = parse_query("before:YESTERDAY", now()).unwrap();
        assert_eq!(
            yesterday.first("before").and_then(FacetValue::as_instant),
            Some(at(2026, 7, 20, 0))
        );
    }

    // -- parsing: shared rules ---------------------------------------------

    #[test]
    fn quotes_group_text_and_facet_values() {
        let query = parse_query("\"release note\" tag:\"in review\"", now()).unwrap();
        assert_eq!(query.text(), "release note");
        assert_eq!(text_of(&query, "tag").as_deref(), Some("in review"));
        assert_eq!(
            parse_query("\"unterminated", now()),
            Err(ParseError::InvalidSyntax)
        );
    }

    #[test]
    fn facet_names_and_values_are_case_and_width_insensitive() {
        let query = parse_query("APP:CHROME KIND:URL HOST:Docs.RS", now()).unwrap();
        assert_eq!(text_of(&query, "app").as_deref(), Some("chrome"));
        assert_eq!(
            query.first("kind").and_then(FacetValue::as_kind),
            Some(ContentKind::Url)
        );
        assert_eq!(text_of(&query, "host").as_deref(), Some("docs.rs"));

        let wide = parse_query("\u{ff41}\u{ff50}\u{ff50}:Chrome", now()).unwrap();
        assert_eq!(text_of(&wide, "app").as_deref(), Some("chrome"));
    }

    #[test]
    fn non_ascii_facet_values_survive_parsing() {
        let query = parse_query("app:Браузер device:Ноутбук", now()).unwrap();
        assert_eq!(text_of(&query, "app").as_deref(), Some("браузер"));
        assert_eq!(text_of(&query, "device").as_deref(), Some("ноутбук"));
    }

    #[test]
    fn unknown_facets_stay_text_but_bad_known_values_fail() {
        // The documented decision: an unknown key is not a facet.
        let query = parse_query("unknown:needle https://example.test", now()).unwrap();
        assert_eq!(query.text(), "unknown:needle https://example.test");
        assert!(!query.has_constraints());

        // A known facet with a bad value is still an error.
        assert_eq!(
            parse_query("kind:nonsense", now()),
            Err(ParseError::InvalidFilter)
        );
        assert_eq!(
            parse_query("before:garbage", now()),
            Err(ParseError::InvalidFilter)
        );
    }

    #[test]
    fn empty_values_follow_the_registry_policy() {
        // Recall facets reject a blank value ...
        assert_eq!(parse_query("app:", now()), Err(ParseError::InvalidFilter));
        assert_eq!(parse_query("kind:", now()), Err(ParseError::InvalidFilter));
        // ... store facets degrade the whole token to text.
        let query = parse_query("host: sqlite", now()).unwrap();
        assert_eq!(query.text(), "host: sqlite");
        assert!(!query.has_constraints());
        // A quoted blank value is blank too.
        let blank = parse_query("host:\"   \"", now()).unwrap();
        assert!(!blank.has_constraints());
    }

    #[test]
    fn repeat_policies_follow_the_registry() {
        // Recall facets are single-slot ...
        assert_eq!(
            parse_query("app:a app:b", now()),
            Err(ParseError::InvalidFilter)
        );
        assert_eq!(
            parse_query("kind:url urls", now()),
            Err(ParseError::InvalidFilter)
        );
        // ... store facets accumulate and are ANDed.
        let query = parse_query("host:a host:b", now()).unwrap();
        assert_eq!(query.constraints_for("host").count(), 2);
        assert_eq!(
            query
                .constraints_for("host")
                .filter_map(|constraint| constraint.value().as_text())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn impossible_time_windows_are_rejected() {
        assert_eq!(
            parse_query("after:2026-07-20 before:2026-07-10", now()),
            Err(ParseError::InvalidFilter)
        );
        assert!(parse_query("after:2026-07-10 before:2026-07-20", now()).is_ok());
    }

    #[test]
    fn oversized_and_control_input_is_rejected() {
        let long = "a".repeat(MAX_QUERY_BYTES + 1);
        assert_eq!(parse_query(&long, now()), Err(ParseError::TooLarge));
        assert_eq!(parse_query("a\0b", now()), Err(ParseError::TooLarge));

        let many = vec!["x"; MAX_QUERY_TOKENS + 1].join(" ");
        assert_eq!(parse_query(&many, now()), Err(ParseError::TooLarge));

        let oversized_value = format!("app:{}", "a".repeat(MAX_FACET_VALUE_BYTES + 1));
        assert_eq!(
            parse_query(&oversized_value, now()),
            Err(ParseError::InvalidFilter)
        );
        assert_eq!(
            parse_query("app:a\u{1}b", now()),
            Err(ParseError::InvalidFilter)
        );
    }

    #[test]
    fn fingerprint_ignores_constraint_order_but_not_content() {
        let left = parse_query("app:chrome kind:url", now()).unwrap();
        let right = parse_query("kind:url app:chrome", now()).unwrap();
        assert_eq!(left.fingerprint(), right.fingerprint());

        let other = parse_query("app:editor kind:url", now()).unwrap();
        assert_ne!(left.fingerprint(), other.fingerprint());

        let with_text = parse_query("notes app:chrome kind:url", now()).unwrap();
        assert_ne!(left.fingerprint(), with_text.fingerprint());
    }

    // -- in-memory execution -----------------------------------------------

    #[test]
    fn in_memory_matcher_honours_every_subject() {
        let clip = FakeClip {
            kind: ContentKind::Url,
            created_at: Some(at(2026, 7, 21, 9)),
            source_app: Some("Google Chrome".into()),
            device: Some("Ноутбук".into()),
            tags: vec!["work".into()],
            derived: vec![("host".into(), "docs.rs".into())],
        };

        // `app` is containment, `device`/`tag`/`host` are equality.
        let query = parse_query(
            "app:chrome device:НОУТБУК tag:Work host:Docs.RS kind:urls after:2026-07-21 before:2026-07-22",
            now(),
        )
        .unwrap();
        assert!(matches_constraints(&query, &clip));

        let miss = parse_query("app:editor", now()).unwrap();
        assert!(!matches_constraints(&miss, &clip));

        // The non-ASCII case the two normalizers used to disagree about.
        let cyrillic = parse_query("device:ноутбук", now()).unwrap();
        assert!(matches_constraints(&cyrillic, &clip));

        // Absent subject values never match.
        let empty = FakeClip::default();
        assert!(!matches_constraints(
            &parse_query("app:chrome", now()).unwrap(),
            &empty
        ));
        assert!(!matches_constraints(
            &parse_query("host:docs.rs", now()).unwrap(),
            &empty
        ));
    }

    #[test]
    fn instant_bounds_are_half_open() {
        let clip = FakeClip {
            created_at: Some(at(2026, 7, 21, 0)),
            ..FakeClip::default()
        };
        assert!(matches_constraints(
            &parse_query("after:2026-07-21", now()).unwrap(),
            &clip
        ));
        assert!(!matches_constraints(
            &parse_query("before:2026-07-21", now()).unwrap(),
            &clip
        ));
    }

    // -- SQL execution ------------------------------------------------------

    #[test]
    fn materialized_facets_compile_to_sql() {
        let query = parse_query("kind:url after:2026-07-01 host:docs.rs", now()).unwrap();
        let predicates = sql_predicates(&query).unwrap();
        assert_eq!(predicates.len(), 3);

        assert_eq!(predicates[0].fragment, "c.kind = ?");
        assert_eq!(
            predicates[0].binds,
            vec![SqlBind::Integer(ContentKind::Url.stored_discriminant())]
        );

        assert_eq!(predicates[1].fragment, "c.created_at >= ?");
        assert_eq!(
            predicates[1].binds,
            vec![SqlBind::Integer(at(2026, 7, 1, 0).timestamp_millis())]
        );

        assert_eq!(
            predicates[2].fragment,
            "EXISTS (SELECT 1 FROM clip_facets f WHERE f.clip_id = c.id AND f.key = ? AND f.value = ?)"
        );
        assert_eq!(
            predicates[2].binds,
            vec![
                SqlBind::Text("host".into()),
                SqlBind::Text("docs.rs".into())
            ]
        );
    }

    #[test]
    fn unmaterialized_facets_report_the_schema_gap() {
        let app = parse_query("app:chrome", now()).unwrap();
        assert_eq!(
            sql_predicates(&app),
            Err(SchemaGap::UnnormalizedSource("clips.source_app"))
        );

        let tag = parse_query("tag:work", now()).unwrap();
        assert!(matches!(
            sql_predicates(&tag),
            Err(SchemaGap::NotPersisted(_))
        ));

        // Exactly three facets need store-side work before the migration can
        // route `Store::search_page` through this AST.
        let gaps = REGISTRY
            .iter()
            .filter(|spec| matches!(spec.sql, SqlBinding::Missing(_)))
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert_eq!(gaps, ["app", "tag", "device"]);
    }
}
