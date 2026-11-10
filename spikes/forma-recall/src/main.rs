//! Spike: vbuff's Recall screen drawn with Forma instead of egui.
//!
//! The view model sits on the same seam the egui popup uses: it reads a
//! `vbuff_gui::SharedState` snapshot and emits `vbuff_gui::UiAction`s. Nothing
//! here touches the store, the clipboard or the paste backends.
//!
//! Modes:
//!   cargo run                      open the window
//!   cargo run -- --shot out.png    render one frame without a window
//!   cargo run -- --bench           time filtering and painting, no window
//! Optional: `--query <text>` and `--clips <n>` (default 2000).

use forma::binding::Property;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex},
};
use vbuff_gui::{AppState, SharedState, UiAction};
use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

mod window;

/// Rows drawn at once. The list is windowed by hand: a Forma scene is limited
/// to 256 controls, so the history can never be mounted whole.
const VISIBLE_ROWS: usize = 10;

#[derive(Clone, PartialEq)]
pub struct RecallRow {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub place: String,
    pub badge: String,
    pub selected: bool,
}

pub struct RecallVm {
    pub query: Property<String>,
    pub rows: Property<Vec<RecallRow>>,
    pub heading: Property<String>,
    pub preview: Property<String>,
    pub meta: Property<String>,
    pub status: Property<String>,
    pub action: Property<String>,
    pub target: Property<String>,
    shared: SharedState,
    matches: RefCell<Vec<ClipId>>,
    selected: Cell<usize>,
    top: Cell<usize>,
    pub actions: RefCell<Vec<UiAction>>,
}

impl RecallVm {
    pub fn new(shared: SharedState) -> Rc<Self> {
        let vm = Rc::new(Self {
            query: Property::new(String::new()),
            rows: Property::new(Vec::new()),
            heading: Property::new(String::new()),
            preview: Property::new(String::new()),
            meta: Property::new(String::new()),
            status: Property::new(String::new()),
            action: Property::new(String::new()),
            // The spike has no summon path, so no paste target was captured.
            target: Property::new("Copy only · no paste target".into()),
            shared,
            matches: RefCell::new(Vec::new()),
            selected: Cell::new(0),
            top: Cell::new(0),
            actions: RefCell::new(Vec::new()),
        });
        vm.refresh(true);
        vm
    }

    /// Recompute matches for the current query and republish the visible rows.
    pub fn refresh(&self, reset_selection: bool) {
        let needle = self.query.get().trim().to_lowercase();
        let state = self.shared.lock().unwrap();
        let clips = &state.clips;
        let matched: Vec<&Clip> = clips
            .iter()
            .filter(|clip| needle.is_empty() || haystack(clip).contains(&needle))
            .collect();
        if reset_selection {
            self.selected.set(0);
            self.top.set(0);
        }
        let selected = self.selected.get().min(matched.len().saturating_sub(1));
        self.selected.set(selected);
        let mut top = self.top.get().min(selected);
        if selected >= top + VISIBLE_ROWS {
            top = selected + 1 - VISIBLE_ROWS;
        }
        self.top.set(top);

        let rows = matched
            .iter()
            .enumerate()
            .skip(top)
            .take(VISIBLE_ROWS)
            .map(|(index, clip)| RecallRow {
                id: clip.id.to_string_repr(),
                kind: kind_label(clip.meta.kind).into(),
                title: clip.preview(80),
                place: place(clip),
                badge: if index == selected {
                    "•".into()
                } else {
                    String::new()
                },
                selected: index == selected,
            })
            .collect();
        self.rows.set(rows);

        self.heading.set(if needle.is_empty() {
            format!("RECENT · {} CLIPS", clips.len())
        } else {
            format!("{} OF {} · WHOLE HISTORY SEARCHED", matched.len(), clips.len())
        });
        match matched.get(selected) {
            Some(clip) => {
                self.preview.set(preview_text(clip));
                self.meta.set(format!(
                    "{} · {} bytes · {}",
                    clip.meta.source_app.as_deref().unwrap_or("source unknown"),
                    clip.meta.byte_size,
                    if clip.pinned { "pinned" } else { "on disk" }
                ));
                self.action.set("Enter Copy    Esc Close    ↑↓ Select".into());
            }
            None => {
                self.preview.set(String::new());
                self.meta.set(String::new());
                self.action.set("Esc Clear search".into());
            }
        }
        self.status.set(if state.paused {
            "Capture paused".into()
        } else {
            "Capturing · storage not encrypted".into()
        });
        *self.matches.borrow_mut() = matched.iter().map(|clip| clip.id).collect();
    }

    pub fn move_selection(&self, delta: isize) {
        let count = self.matches.borrow().len();
        if count == 0 {
            return;
        }
        let next = (self.selected.get() as isize + delta).clamp(0, count as isize - 1);
        self.selected.set(next as usize);
        self.refresh(false);
    }

    pub fn select_id(&self, id: &str) {
        let index = self
            .matches
            .borrow()
            .iter()
            .position(|clip| clip.to_string_repr() == id);
        if let Some(index) = index {
            self.selected.set(index);
            self.refresh(false);
        }
    }

    /// Emit the same action the egui popup emits on Enter.
    pub fn activate_selected(&self) -> bool {
        let id = self.matches.borrow().get(self.selected.get()).copied();
        match id {
            Some(id) => {
                self.actions.borrow_mut().push(UiAction::Paste(id));
                true
            }
            None => false,
        }
    }
}

fn haystack(clip: &Clip) -> String {
    let mut text = clip.primary_text().unwrap_or_default().to_lowercase();
    if let Some(app) = &clip.meta.source_app {
        text.push(' ');
        text.push_str(&app.to_lowercase());
    }
    text
}

fn kind_label(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Text => "TXT",
        ContentKind::Url => "URL",
        ContentKind::Code => "CODE",
        ContentKind::Image => "IMG",
        ContentKind::Color => "CLR",
        _ => "···",
    }
}

fn place(clip: &Clip) -> String {
    let minutes = (chrono::Utc::now() - clip.meta.updated_at).num_minutes().max(0);
    let age = match minutes {
        0 => "now".to_string(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    };
    match clip.meta.source_app.as_deref() {
        Some(app) => format!("{app} · {age}"),
        None => format!("unknown · {age}"),
    }
}

fn preview_text(clip: &Clip) -> String {
    match clip.primary_text() {
        Some(text) => text.lines().take(18).collect::<Vec<_>>().join("\n"),
        None => clip.preview(80),
    }
}

include!(concat!(env!("OUT_DIR"), "/recall.rs"));

fn text_clip(text: &str, kind: ContentKind, app: Option<&str>, minutes_ago: i64) -> Clip {
    let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
    let mut meta = ClipMeta::now(kind, text.len() as u64, app.map(str::to_owned));
    meta.updated_at -= chrono::Duration::minutes(minutes_ago);
    meta.created_at = meta.updated_at;
    Clip {
        id: ClipId::new(),
        content_hash: vbuff_core::content_hash_from_flavors(&flavors),
        flavors,
        meta,
        pinned: false,
        favorite: false,
    }
}

/// Local sample history: a few realistic clips, then filler up to `count`.
fn sample_history(count: usize) -> Vec<Clip> {
    let mut clips = vec![
        text_clip(
            "cargo test --workspace --all-features --locked",
            ContentKind::Code,
            Some("iTerm2"),
            0,
        ),
        text_clip(
            "error[E0502]: cannot borrow `store` as mutable because it is also borrowed as immutable\n   --> src/history_search.rs\n    |\n    |     let rows = store.candidates(&query);\n    |                ----- immutable borrow occurs here\n    |     store.touch(id);\n    |     ^^^^^^^^^^^^^^^ mutable borrow occurs here",
            ContentKind::Text,
            Some("iTerm2"),
            2,
        ),
        text_clip(
            "https://docs.rs/arboard/latest/arboard/",
            ContentKind::Url,
            Some("Safari"),
            6,
        ),
        text_clip(
            "{\"backend\":\"duckdb\",\"strict_security_mode\":true,\n  \"history_cap\":5000}",
            ContentKind::Code,
            Some("VS Code"),
            160,
        ),
        text_clip(
            "SELECT kind, count(*)\nFROM clips\nWHERE captured_at > now() - INTERVAL 1 DAY\nGROUP BY kind;",
            ContentKind::Code,
            Some("VS Code"),
            195,
        ),
        text_clip(
            "Release notes and next actions\n- ship DuckDB migration\n- re-run storage benchmark",
            ContentKind::Text,
            None,
            1500,
        ),
    ];
    let apps = [Some("iTerm2"), Some("VS Code"), Some("Safari"), None];
    for index in clips.len()..count {
        let (text, kind) = match index % 4 {
            0 => (format!("git checkout -b feature/item-{index}"), ContentKind::Code),
            1 => (
                format!("https://example.com/docs/page-{index}"),
                ContentKind::Url,
            ),
            2 => (format!("cargo run --example sample_{index}"), ContentKind::Code),
            _ => (format!("Meeting note {index}: follow up on storage"), ContentKind::Text),
        };
        clips.push(text_clip(&text, kind, apps[index % 4], 1500 + index as i64 * 7));
    }
    clips
}

pub struct Spike {
    pub form: Recall,
    pub vm: Rc<RecallVm>,
}

pub fn build(clips: usize) -> Result<Spike, String> {
    let shared: SharedState = Arc::new(Mutex::new(AppState::with_clips(sample_history(clips))));
    let vm = RecallVm::new(shared);
    let mut form = Recall::new(vm.clone())?;
    form.on_query_changed(|event| event.context.refresh(true));
    form.on_picked(|event| event.context.select_id(&event.id));
    let search = {
        let runtime = form.runtime()?;
        (0..runtime.control_count()).find(|&i| runtime.control_key(i).ends_with("search"))
    };
    if let Some(search) = search {
        form.update(|runtime| runtime.focus_control(search))?;
    }
    Ok(Spike { form, vm })
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1).cloned())
}

fn shot(spike: &mut Spike, path: &str) -> Result<(), String> {
    let (width, height, scale) = (900_u32, 580_u32, 2.0_f32);
    // `pixels` takes the physical size; `scale` maps logical units onto it.
    let (w, h) = (
        (width as f32 * scale) as u32,
        (height as f32 * scale) as u32,
    );
    let pixels = spike.form.runtime()?.pixels(w, h, scale);
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&pixels))
        .map_err(|e| e.to_string())?;
    println!("wrote {path} ({w}x{h})");
    Ok(())
}

fn bench(spike: &mut Spike) -> Result<(), String> {
    use std::time::Instant;
    let controls = spike.form.runtime()?.control_count();
    println!("controls in scene: {controls}");
    for ch in "cargo".chars() {
        let started = Instant::now();
        spike
            .form
            .update(|runtime| runtime.text_insert(&ch.to_string()))?;
        let typed = started.elapsed();
        let started = Instant::now();
        let bytes = spike.form.runtime()?.pixels(1800, 1160, 2.0).len();
        println!(
            "type '{ch}': update {:.2} ms, paint {:.2} ms ({bytes} bytes), heading: {}",
            typed.as_secs_f64() * 1000.0,
            started.elapsed().as_secs_f64() * 1000.0,
            spike.vm.heading.get()
        );
    }
    let started = Instant::now();
    for _ in 0..50 {
        spike.vm.move_selection(1);
        spike.form.sync()?;
    }
    println!(
        "arrow down x50: {:.2} ms per step",
        started.elapsed().as_secs_f64() * 1000.0 / 50.0
    );
    Ok(())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let clips = arg_value(&args, "--clips")
        .and_then(|value| value.parse().ok())
        .unwrap_or(2000);
    let mut spike = build(clips)?;
    if args.iter().any(|arg| arg == "--bench") {
        return bench(&mut spike);
    }
    if let Some(query) = arg_value(&args, "--query") {
        spike.form.update(|runtime| runtime.text_insert(&query))?;
    }
    if let Some(steps) = arg_value(&args, "--down").and_then(|value| value.parse().ok()) {
        spike.vm.move_selection(steps);
        spike.form.sync()?;
    }
    if let Some(path) = arg_value(&args, "--shot") {
        return shot(&mut spike, &path);
    }
    window::run(spike)
}
