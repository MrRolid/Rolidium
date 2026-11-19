// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The folder browser used by all file dialogs: places on the left, a lazily loaded folder tree, a table of
//! the folder's contents with sortable columns, a clickable or typed path, filter, new folder and hidden files.
//! Folders are read on worker threads so a slow or network disk never freezes the window.

pub mod listing;
pub mod places;

use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use listing::{Entry, SortKey};
use places::{Place, PlaceKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

enum Msg {
    List(u64, Result<Vec<Entry>, String>),
    Tree(PathBuf, Result<Vec<String>, String>),
}

enum Load {
    Loading,
    Ready,
    Failed(String),
}

#[derive(Default)]
struct Node {
    children: Option<Result<Vec<String>, String>>,
    loading: bool,
    open: bool,
}

/// What happened in the browser this frame.
pub enum Event {
    None,
    /// A file was selected (single click, arrows): its name.
    Picked(String),
    /// A file was opened (double click, Enter).
    Activated(String),
}

pub struct Browser {
    pub dir: PathBuf,
    places: Vec<Place>,
    entries: Vec<Entry>,
    rows: Vec<usize>,
    load: Load,
    generation: u64,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    sort_key: SortKey,
    ascending: bool,
    show_hidden: bool,
    exts: Vec<String>,
    type_label: String,
    all_files: bool,
    selected: Option<String>,
    path_edit: Option<String>,
    path_fresh: bool,
    suggestions: (String, Vec<String>),
    path_error: Option<String>,
    new_folder: Option<(String, Option<String>)>,
    tree: HashMap<PathBuf, Node>,
    type_buf: String,
    type_at: f64,
    scroll_to: Option<usize>,
    pending_select: Option<String>,
}

const ROW_H: f32 = 22.0;
const LEFT_W: f32 = 200.0;

fn paint_folder(p: &egui::Painter, r: Rect, col: Color32) {
    let tab = Rect::from_min_size(r.min, Vec2::new(r.width() * 0.45, 3.0));
    p.rect_filled(tab, 1.0, col);
    p.rect_filled(
        Rect::from_min_max(Pos2::new(r.min.x, r.min.y + 2.0), r.max),
        2.0,
        col,
    );
}

fn paint_file(p: &egui::Painter, r: Rect, col: Color32) {
    let r = Rect::from_min_max(
        Pos2::new(r.min.x + 2.0, r.min.y),
        Pos2::new(r.max.x - 2.0, r.max.y),
    );
    p.rect_stroke(r, 1.0, Stroke::new(1.2, col), egui::StrokeKind::Inside);
    for i in 0..3 {
        let y = r.min.y + 4.0 + 3.0 * i as f32;
        p.line_segment(
            [Pos2::new(r.min.x + 2.5, y), Pos2::new(r.max.x - 2.5, y)],
            Stroke::new(1.0, col),
        );
    }
}

fn paint_triangle(p: &egui::Painter, c: Pos2, open: bool, col: Color32) {
    let pts = if open {
        vec![
            Pos2::new(c.x - 4.0, c.y - 2.0),
            Pos2::new(c.x + 4.0, c.y - 2.0),
            Pos2::new(c.x, c.y + 3.0),
        ]
    } else {
        vec![
            Pos2::new(c.x - 2.0, c.y - 4.0),
            Pos2::new(c.x - 2.0, c.y + 4.0),
            Pos2::new(c.x + 3.0, c.y),
        ]
    };
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

fn place_label(p: &Place) -> String {
    match p.kind {
        PlaceKind::Home
        | PlaceKind::Desktop
        | PlaceKind::Documents
        | PlaceKind::Downloads
        | PlaceKind::Root => tr(&p.name),
        _ => p.name.clone(),
    }
}

/// Ctrl+key, judged by the modifiers the key event itself carried (the state at the end of a frame may already have
/// lost them when the keys are pressed and released quickly).
fn chord(i: &egui::InputState, key: Key) -> bool {
    i.events.iter().any(|e| matches!(e, egui::Event::Key { key: k, pressed: true, modifiers, .. } if *k == key && modifiers.command))
}

impl Browser {
    pub fn new(ctx: &egui::Context, start: &str, exts: &[&str], type_label: &str, recent: &[String]) -> Self {
        let dir = if !start.is_empty() && Path::new(start).is_dir() {
            PathBuf::from(start)
        } else {
            places::home_dir()
                .filter(|h| h.is_dir())
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("/"))
        };
        let (tx, rx) = channel();
        let mut b = Browser {
            dir: PathBuf::new(),
            places: places::collect(recent),
            entries: Vec::new(),
            rows: Vec::new(),
            load: Load::Loading,
            generation: 0,
            tx,
            rx,
            sort_key: SortKey::Name,
            ascending: true,
            show_hidden: false,
            exts: exts.iter().map(|s| s.to_string()).collect(),
            type_label: type_label.to_string(),
            all_files: false,
            selected: None,
            path_edit: None,
            path_fresh: false,
            suggestions: (String::new(), Vec::new()),
            path_error: None,
            new_folder: None,
            tree: HashMap::new(),
            type_buf: String::new(),
            type_at: 0.0,
            scroll_to: None,
            pending_select: None,
        };
        b.go(ctx, dir);
        b
    }

    /// True while a text field of the browser (path, new folder) has the keyboard.
    pub fn editing(&self) -> bool {
        self.path_edit.is_some() || self.new_folder.is_some()
    }

    fn filter(&self) -> Option<&[String]> {
        (!self.all_files).then_some(self.exts.as_slice())
    }

    fn recompute(&mut self) {
        listing::sort(&mut self.entries, self.sort_key, self.ascending);
        self.rows = listing::visible(&self.entries, self.filter(), self.show_hidden);
    }

    fn spawn_list(&mut self, ctx: &egui::Context) {
        self.generation += 1;
        self.load = Load::Loading;
        let (tx, dir, g, ctx) = (self.tx.clone(), self.dir.clone(), self.generation, ctx.clone());
        std::thread::spawn(move || {
            let r = listing::read_entries(&dir);
            let _ = tx.send(Msg::List(g, r));
            ctx.request_repaint();
        });
    }

    fn ensure_children(&mut self, ctx: &egui::Context, path: &Path) {
        let hidden = self.show_hidden;
        let node = self.tree.entry(path.to_path_buf()).or_default();
        if node.children.is_some() || node.loading {
            return;
        }
        node.loading = true;
        let (tx, p, ctx) = (self.tx.clone(), path.to_path_buf(), ctx.clone());
        std::thread::spawn(move || {
            let r = listing::read_subdirs(&p, hidden);
            let _ = tx.send(Msg::Tree(p, r));
            ctx.request_repaint();
        });
    }

    fn roots(&self) -> Vec<(String, PathBuf)> {
        self.places
            .iter()
            .filter(|p| {
                matches!(
                    p.kind,
                    PlaceKind::Home | PlaceKind::Root | PlaceKind::Drive | PlaceKind::Network
                )
            })
            .map(|p| (place_label(p), p.path.clone()))
            .collect()
    }

    /// Opens the folders of the tree down to the current one.
    fn reveal(&mut self, ctx: &egui::Context) {
        for (_, root) in self.roots() {
            if self.dir.starts_with(&root) {
                let mut acc = root.clone();
                let rel: Vec<_> = self
                    .dir
                    .strip_prefix(&root)
                    .map(|r| r.components().map(|c| c.as_os_str().to_owned()).collect())
                    .unwrap_or_default();
                self.tree.entry(acc.clone()).or_default().open = true;
                self.ensure_children(ctx, &acc);
                for c in rel {
                    acc.push(c);
                    if acc != self.dir {
                        self.tree.entry(acc.clone()).or_default().open = true;
                        self.ensure_children(ctx, &acc);
                    }
                }
            }
        }
    }

    pub fn go(&mut self, ctx: &egui::Context, dir: PathBuf) {
        self.dir = dir;
        self.selected = None;
        self.path_edit = None;
        self.path_error = None;
        self.entries.clear();
        self.rows.clear();
        self.spawn_list(ctx);
        self.reveal(ctx);
    }

    pub fn refresh(&mut self, ctx: &egui::Context) {
        self.places = places::collect(&[]);
        self.tree.clear();
        self.spawn_list(ctx);
        self.reveal(ctx);
    }

    fn poll(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::List(g, r) if g == self.generation => match r {
                    Ok(e) => {
                        self.entries = e;
                        self.load = Load::Ready;
                        self.recompute();
                        if let Some(n) = self.pending_select.take() {
                            self.select_name(&n);
                        }
                    }
                    Err(e) => self.load = Load::Failed(e),
                },
                Msg::List(..) => {}
                Msg::Tree(p, r) => {
                    let n = self.tree.entry(p).or_default();
                    n.loading = false;
                    n.children = Some(r);
                }
            }
        }
    }

    fn select_name(&mut self, name: &str) {
        if let Some(i) = self.rows.iter().position(|&i| self.entries[i].name == name) {
            self.selected = Some(name.to_string());
            self.scroll_to = Some(i);
        }
    }

    fn activate(&mut self, ctx: &egui::Context, name: &str) -> Event {
        let Some(e) = self.entries.iter().find(|e| e.name == name) else {
            return Event::None;
        };
        if e.is_dir {
            let d = self.dir.join(name);
            self.go(ctx, d);
            Event::None
        } else {
            Event::Activated(name.to_string())
        }
    }

    fn up(&mut self, ctx: &egui::Context) {
        if let Some(p) = self.dir.parent().map(Path::to_path_buf) {
            let from = self.dir.file_name().map(|n| n.to_string_lossy().into_owned());
            self.go(ctx, p);
            self.pending_select = from;
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) -> Event {
        if ctx.input(|i| chord(i, Key::L)) {
            self.start_path_edit();
            return Event::None;
        }
        if ctx.egui_wants_keyboard_input() || self.editing() {
            return Event::None;
        }
        let mut ev = Event::None;
        let (up, down, enter, back, home, end, f5, ctrl_h, text) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Backspace),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                i.key_pressed(Key::F5),
                chord(i, Key::H),
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Text(t) = e {
                            Some(t.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<String>(),
            )
        });
        let cur = self
            .selected
            .as_ref()
            .and_then(|n| self.rows.iter().position(|&i| &self.entries[i].name == n));
        let mut to: Option<usize> = None;
        if down {
            to = Some(cur.map_or(0, |c| (c + 1).min(self.rows.len().saturating_sub(1))));
        }
        if up {
            to = Some(cur.map_or(0, |c| c.saturating_sub(1)));
        }
        if home {
            to = Some(0);
        }
        if end && !self.rows.is_empty() {
            to = Some(self.rows.len() - 1);
        }
        if !text.is_empty() && !text.chars().all(char::is_control) {
            let now = ctx.input(|i| i.time);
            if now - self.type_at > 1.0 {
                self.type_buf.clear();
            }
            self.type_at = now;
            self.type_buf.push_str(&text.to_lowercase());
            let buf = self.type_buf.clone();
            if let Some(i) = self
                .rows
                .iter()
                .position(|&i| self.entries[i].name.to_lowercase().starts_with(&buf))
            {
                to = Some(i);
            }
        }
        if let Some(i) = to.filter(|_| !self.rows.is_empty()) {
            let e = &self.entries[self.rows[i]];
            self.selected = Some(e.name.clone());
            self.scroll_to = Some(i);
            if !e.is_dir {
                ev = Event::Picked(e.name.clone());
            }
        }
        if enter {
            if let Some(n) = self.selected.clone() {
                ev = self.activate(ctx, &n);
            }
        }
        if back {
            self.up(ctx);
        }
        if f5 {
            self.refresh(ctx);
        }
        if ctrl_h {
            self.show_hidden = !self.show_hidden;
            self.recompute();
        }
        ev
    }

    fn start_path_edit(&mut self) {
        self.path_edit = Some(self.dir.display().to_string());
        self.path_fresh = true;
    }

    fn path_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui
                .button(tr("Up"))
                .on_hover_text(tr("Go to the parent folder (Backspace)"))
                .clicked()
            {
                self.up(ctx);
            }
            if ui.button(tr("Refresh")).on_hover_text("F5").clicked() {
                self.refresh(ctx);
            }
            if let Some(text) = &mut self.path_edit {
                let mut out = egui::TextEdit::singleline(text)
                    .desired_width(ui.available_width() - 70.0)
                    .show(ui);
                let r = out.response.clone();
                if self.path_fresh {
                    // like a browser's address bar: everything is selected so typing replaces it
                    self.path_fresh = false;
                    r.request_focus();
                    let n = text.chars().count();
                    out.state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(n),
                        )));
                    out.state.store(ctx, r.id);
                }
                let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
                let enter = r.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
                if self.suggestions.0 != *text {
                    self.suggestions = (text.clone(), listing::complete(text, self.show_hidden));
                }
                let typed = text.clone();
                if esc {
                    self.path_edit = None;
                    self.path_error = None;
                } else if enter {
                    let p = PathBuf::from(typed.trim());
                    match std::fs::metadata(&p) {
                        Ok(m) if m.is_dir() => self.go(ctx, p),
                        Ok(_) => {
                            // a file path: go to its folder and select it
                            if let (Some(parent), Some(name)) = (p.parent(), p.file_name()) {
                                let name = name.to_string_lossy().into_owned();
                                self.go(ctx, parent.to_path_buf());
                                self.pending_select = Some(name);
                            }
                        }
                        Err(e) => self.path_error = Some(listing::friendly_error(&e)),
                    }
                }
                if ui.button(tr("Cancel")).clicked() {
                    self.path_edit = None;
                }
            } else {
                let crumbs = listing::breadcrumbs(&self.dir);
                let mut target: Option<PathBuf> = None;
                egui::ScrollArea::horizontal()
                    .id_salt("crumbs")
                    .auto_shrink([false, false])
                    .max_width(ui.available_width() - 60.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (i, (label, p)) in crumbs.iter().enumerate() {
                                if i > 1 {
                                    ui.label("/");
                                }
                                if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                    target = Some(p.clone());
                                }
                            }
                        });
                    });
                if ui
                    .button(tr("Path"))
                    .on_hover_text(tr("Type a path (Ctrl+L)"))
                    .clicked()
                {
                    self.start_path_edit();
                }
                if let Some(t) = target {
                    self.go(ctx, t);
                }
            }
        });
        if self.path_edit.is_some() {
            let mut chosen: Option<String> = None;
            for s in self.suggestions.1.iter().take(6) {
                if ui.add(egui::Button::new(s).frame(false)).clicked() {
                    chosen = Some(s.clone());
                }
            }
            if let Some(c) = chosen {
                self.path_edit = Some(c.clone());
                self.suggestions = (c.clone(), listing::complete(&c, self.show_hidden));
            }
            if let Some(e) = &self.path_error {
                ui.colored_label(Color32::from_rgb(255, 120, 110), tr(e));
            }
        }
    }

    fn tools_row(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui.checkbox(&mut self.show_hidden, tr("Hidden files")).changed() {
                self.recompute();
                self.tree.clear();
                self.reveal(ctx);
            }
            if self.new_folder.is_none() {
                if ui.button(tr("New folder")).clicked() {
                    self.new_folder = Some((String::new(), None));
                }
            } else {
                let mut done = false;
                let mut cancel = false;
                if let Some((name, err)) = &mut self.new_folder {
                    let r = ui.add(
                        egui::TextEdit::singleline(name)
                            .desired_width(140.0)
                            .hint_text(tr("Folder name")),
                    );
                    if !r.has_focus() && err.is_none() && !r.lost_focus() {
                        r.request_focus();
                    }
                    let enter = r.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
                    let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
                    if ui.button(tr("Create")).clicked() || enter {
                        let n = name.trim().to_string();
                        if n.is_empty() || n.contains(['/', '\\']) || n == "." || n == ".." {
                            *err = Some(tr("This is not a valid folder name."));
                        } else {
                            match std::fs::create_dir(self.dir.join(&n)) {
                                Ok(()) => {
                                    done = true;
                                    self.pending_select = Some(n);
                                }
                                Err(e) => *err = Some(trf("Cannot create the folder: {}", &[e.to_string()])),
                            }
                        }
                    }
                    if ui.button(tr("Cancel")).clicked() || esc {
                        cancel = true;
                    }
                    if let Some(e) = err {
                        ui.colored_label(Color32::from_rgb(255, 120, 110), e.as_str());
                    }
                }
                if done {
                    self.new_folder = None;
                    self.spawn_list(ctx);
                    self.tree.retain(|p, _| p != &self.dir);
                    self.reveal(ctx);
                } else if cancel {
                    self.new_folder = None;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.all_files {
                    tr("All files")
                } else {
                    tr(&self.type_label)
                };
                egui::ComboBox::from_id_salt("file_type")
                    .selected_text(label)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        let mut changed = false;
                        changed |= ui
                            .selectable_value(&mut self.all_files, false, tr(&self.type_label))
                            .changed();
                        changed |= ui
                            .selectable_value(&mut self.all_files, true, tr("All files"))
                            .changed();
                        if changed {
                            self.recompute();
                        }
                    });
            });
        });
    }

    fn places_and_tree(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut go: Option<PathBuf> = None;
        egui::ScrollArea::vertical()
            .id_salt("left")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(egui::RichText::new(tr("Places")).strong());
                let mut last = None;
                for p in &self.places {
                    if last.is_some_and(|k| k != p.kind)
                        && matches!(
                            p.kind,
                            PlaceKind::Root | PlaceKind::Bookmark | PlaceKind::Recent | PlaceKind::Network
                        )
                    {
                        ui.add_space(4.0);
                        if p.kind == PlaceKind::Recent {
                            ui.label(egui::RichText::new(tr("Recent folders")).weak());
                        }
                    }
                    last = Some(p.kind);
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
                    let sel = p.path == self.dir;
                    if sel {
                        ui.painter()
                            .rect_filled(rect, 3.0, ui.visuals().selection.bg_fill);
                    } else if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
                    }
                    let icon = Rect::from_min_size(
                        Pos2::new(rect.min.x + 4.0, rect.center().y - 6.0),
                        Vec2::new(14.0, 12.0),
                    );
                    let col = match p.kind {
                        PlaceKind::Drive | PlaceKind::Network | PlaceKind::Root => {
                            Color32::from_rgb(160, 190, 230)
                        }
                        _ => Color32::from_rgb(255, 200, 80),
                    };
                    if matches!(p.kind, PlaceKind::Drive | PlaceKind::Network | PlaceKind::Root) {
                        ui.painter().rect_filled(
                            Rect::from_min_size(icon.min + Vec2::new(0.0, 3.0), Vec2::new(14.0, 7.0)),
                            2.0,
                            col,
                        );
                    } else {
                        paint_folder(ui.painter(), icon, col);
                    }
                    ui.painter().text(
                        Pos2::new(rect.min.x + 24.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        place_label(p),
                        egui::FontId::proportional(13.5),
                        ui.visuals().text_color(),
                    );
                    if resp.on_hover_text(p.path.display().to_string()).clicked() {
                        go = Some(p.path.clone());
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new(tr("Folders")).strong());
                // the tree, flattened to the visible rows
                let mut rows: Vec<(usize, PathBuf, String)> = Vec::new();
                fn push(
                    b: &Browser,
                    path: &Path,
                    name: String,
                    depth: usize,
                    out: &mut Vec<(usize, PathBuf, String)>,
                ) {
                    out.push((depth, path.to_path_buf(), name));
                    if let Some(n) = b.tree.get(path) {
                        if n.open {
                            if let Some(Ok(ch)) = &n.children {
                                for c in ch {
                                    push(b, &path.join(c), c.clone(), depth + 1, out);
                                }
                            }
                        }
                    }
                }
                for (name, p) in self.roots() {
                    push(self, &p, name, 0, &mut rows);
                }
                let mut toggle: Option<PathBuf> = None;
                for (depth, path, name) in &rows {
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
                    if *path == self.dir {
                        ui.painter()
                            .rect_filled(rect, 3.0, ui.visuals().selection.bg_fill);
                    } else if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
                    }
                    let x0 = rect.min.x + 4.0 + *depth as f32 * 14.0;
                    let node = self.tree.get(path);
                    let open = node.is_some_and(|n| n.open);
                    let leaf = node.is_some_and(|n| matches!(&n.children, Some(Ok(c)) if c.is_empty()));
                    let arrow = Rect::from_min_size(Pos2::new(x0, rect.min.y), Vec2::new(14.0, ROW_H));
                    if !leaf {
                        paint_triangle(ui.painter(), arrow.center(), open, ui.visuals().text_color());
                    }
                    paint_folder(
                        ui.painter(),
                        Rect::from_min_size(
                            Pos2::new(x0 + 16.0, rect.center().y - 5.0),
                            Vec2::new(13.0, 10.0),
                        ),
                        Color32::from_rgb(255, 200, 80),
                    );
                    ui.painter().text(
                        Pos2::new(x0 + 34.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        name,
                        egui::FontId::proportional(13.5),
                        ui.visuals().text_color(),
                    );
                    if resp.clicked() {
                        let in_arrow = resp
                            .interact_pointer_pos()
                            .is_some_and(|p| arrow.expand2(Vec2::new(2.0, 0.0)).contains(p));
                        if in_arrow {
                            toggle = Some(path.clone());
                        } else {
                            go = Some(path.clone());
                        }
                    }
                    if node.is_some_and(|n| n.loading) {
                        ui.painter().text(
                            Pos2::new(rect.max.x - 6.0, rect.center().y),
                            egui::Align2::RIGHT_CENTER,
                            "…",
                            egui::FontId::proportional(12.0),
                            ui.visuals().weak_text_color(),
                        );
                    }
                }
                if let Some(p) = toggle {
                    let open = !self.tree.get(&p).is_some_and(|n| n.open);
                    self.tree.entry(p.clone()).or_default().open = open;
                    if open {
                        self.ensure_children(ctx, &p);
                    }
                }
            });
        if let Some(p) = go {
            self.go(ctx, p);
        }
    }

    fn table(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> Event {
        let mut ev = Event::None;
        match &self.load {
            Load::Loading => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr("Reading the folder…"));
                });
                return ev;
            }
            Load::Failed(e) => {
                ui.colored_label(Color32::from_rgb(255, 120, 110), tr(e));
                return ev;
            }
            Load::Ready => {}
        }
        if self.rows.is_empty() {
            ui.label(egui::RichText::new(tr("This folder has no matching files.")).weak());
            return ev;
        }
        use egui_extras::{Column, TableBuilder};
        let mut sort_click: Option<SortKey> = None;
        let mut activate: Option<String> = None;
        let mut pick: Option<(String, bool)> = None;
        let scroll = self.scroll_to.take();
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .sense(Sense::click())
            .resizable(true)
            .auto_shrink([false, false])
            .column(Column::remainder().at_least(120.0).clip(true))
            .column(Column::exact(80.0))
            .column(Column::exact(130.0));
        if let Some(i) = scroll {
            table = table.scroll_to_row(i, None);
        }
        let arrow = if self.ascending { "^" } else { "v" };
        let (key, asc_label) = (self.sort_key, arrow);
        table
            .header(ROW_H, |mut h| {
                for (k, label) in [
                    (SortKey::Name, "Name"),
                    (SortKey::Size, "Size"),
                    (SortKey::Modified, "Modified"),
                ] {
                    h.col(|ui| {
                        let text = if key == k {
                            format!("{} {asc_label}", tr(label))
                        } else {
                            tr(label)
                        };
                        if ui
                            .add(egui::Label::new(egui::RichText::new(text).strong()).sense(Sense::click()))
                            .clicked()
                        {
                            sort_click = Some(k);
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_H, self.rows.len(), |mut row| {
                    let e = &self.entries[self.rows[row.index()]];
                    row.set_selected(self.selected.as_deref() == Some(e.name.as_str()));
                    row.col(|ui| {
                        let (r, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width().max(20.0), ROW_H - 2.0),
                            Sense::hover(),
                        );
                        let p = ui.painter().with_clip_rect(r);
                        let icon = Rect::from_min_size(
                            Pos2::new(r.min.x + 1.0, r.center().y - 6.0),
                            Vec2::new(15.0, 12.0),
                        );
                        if e.is_dir {
                            paint_folder(&p, icon, Color32::from_rgb(255, 200, 80));
                        } else {
                            paint_file(&p, icon, ui.visuals().weak_text_color());
                        }
                        p.text(
                            Pos2::new(r.min.x + 22.0, r.center().y),
                            egui::Align2::LEFT_CENTER,
                            &e.name,
                            egui::FontId::proportional(13.5),
                            ui.visuals().text_color(),
                        );
                    });
                    row.col(|ui| {
                        if !e.is_dir {
                            ui.add(egui::Label::new(listing::human_size(e.size)).selectable(false));
                        }
                    });
                    row.col(|ui| {
                        ui.add(egui::Label::new(listing::format_modified(e.modified)).selectable(false));
                    });
                    let r = row.response();
                    if r.double_clicked() {
                        activate = Some(e.name.clone());
                    } else if r.clicked() {
                        pick = Some((e.name.clone(), e.is_dir));
                    }
                });
            });
        if let Some(k) = sort_click {
            if self.sort_key == k {
                self.ascending = !self.ascending;
            } else {
                self.sort_key = k;
                self.ascending = true;
            }
            self.recompute();
        }
        if let Some((n, is_dir)) = pick {
            self.selected = Some(n.clone());
            if !is_dir {
                ev = Event::Picked(n);
            }
        }
        if let Some(n) = activate {
            ev = self.activate(ctx, &n);
        }
        ev
    }

    /// Draws the whole browser in a box `width` wide; `height` is the height of the lists (the caller adds its own rows below).
    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, width: f32, height: f32) -> Event {
        self.poll();
        let mut ev = self.keyboard(ctx);
        self.path_bar(ui, ctx);
        self.tools_row(ui, ctx);
        ui.add_space(2.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(LEFT_W, height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_min_size(Vec2::new(LEFT_W, height));
                    ui.set_max_size(Vec2::new(LEFT_W, height));
                    self.places_and_tree(ui, ctx);
                },
            );
            let (line, _) = ui.allocate_exact_size(Vec2::new(4.0, height), Sense::hover());
            ui.painter().line_segment(
                [
                    Pos2::new(line.center().x, line.min.y),
                    Pos2::new(line.center().x, line.max.y),
                ],
                Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            );
            let w = (width - LEFT_W - 40.0).max(200.0);
            ui.allocate_ui_with_layout(
                Vec2::new(w, height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_min_size(Vec2::new(w, height));
                    ui.set_max_size(Vec2::new(w, height));
                    let e = self.table(ui, ctx);
                    if !matches!(e, Event::None) {
                        ev = e;
                    }
                },
            );
        });
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_ready(b: &mut Browser) {
        let t = Instant::now();
        loop {
            b.poll();
            if !matches!(b.load, Load::Loading) {
                return;
            }
            assert!(
                t.elapsed() < Duration::from_secs(5),
                "the folder was not read in time"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn row_names(b: &Browser) -> Vec<String> {
        b.rows.iter().map(|&i| b.entries[i].name.clone()).collect()
    }

    #[test]
    fn folder_is_read_in_the_background_filtered_sorted_and_hidden_files_toggle() {
        let base = std::env::temp_dir().join(format!("rolidium-browser-{}", std::process::id()));
        std::fs::create_dir_all(base.join("sub")).unwrap();
        std::fs::write(base.join("b.rolidium"), "xx").unwrap();
        std::fs::write(base.join("a.rolidium"), "x").unwrap();
        std::fs::write(base.join("note.txt"), "x").unwrap();
        std::fs::write(base.join(".hidden.rolidium"), "x").unwrap();
        let ctx = egui::Context::default();
        let mut b = Browser::new(
            &ctx,
            &base.display().to_string(),
            &["rolidium"],
            "Rolidium documents (*.rolidium)",
            &[],
        );
        assert!(matches!(b.load, Load::Loading) || matches!(b.load, Load::Ready));
        wait_ready(&mut b);
        assert_eq!(row_names(&b), ["sub", "a.rolidium", "b.rolidium"]);
        b.show_hidden = true;
        b.recompute();
        assert_eq!(
            row_names(&b),
            ["sub", ".hidden.rolidium", "a.rolidium", "b.rolidium"]
        );
        b.all_files = true;
        b.recompute();
        assert!(row_names(&b).contains(&"note.txt".to_string()));
        b.sort_key = SortKey::Size;
        b.ascending = false;
        b.recompute();
        assert_eq!(row_names(&b)[1], "b.rolidium");
        // going into a folder and back up selects the folder we came from
        b.go(&ctx, base.join("sub"));
        wait_ready(&mut b);
        assert!(b.rows.is_empty());
        b.up(&ctx);
        wait_ready(&mut b);
        assert_eq!(b.selected.as_deref(), Some("sub"));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_missing_folder_gives_a_readable_message_and_the_tree_loads_lazily() {
        let ctx = egui::Context::default();
        let base = std::env::temp_dir().join(format!("rolidium-browser2-{}", std::process::id()));
        std::fs::create_dir_all(base.join("one/two")).unwrap();
        let mut b = Browser::new(&ctx, &base.display().to_string(), &["rolidium"], "x", &[]);
        wait_ready(&mut b);
        // nothing below the current folder is loaded until it is opened
        assert!(b.tree.get(&base.join("one")).is_none_or(|n| n.children.is_none()));
        b.ensure_children(&ctx, &base.join("one"));
        let t = Instant::now();
        while b.tree.get(&base.join("one")).is_none_or(|n| n.children.is_none()) {
            b.poll();
            assert!(t.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            b.tree[&base.join("one")].children,
            Some(Ok(vec!["two".to_string()]))
        );
        b.go(&ctx, base.join("gone"));
        wait_ready(&mut b);
        assert!(matches!(&b.load, Load::Failed(m) if m.contains("does not exist")));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
