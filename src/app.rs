//! The Kincaid Label Generator window.

use crate::labels::{self, Item};
use crate::pdf;
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Key, KeyboardShortcut, Layout, Modifiers, RichText,
    Stroke, TextEdit, Vec2,
};
use egui_extras::{Column, TableBuilder};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const APP_NAME: &str = "Kincaid Label Generator";
pub const APP_AUTHOR: &str = "Adam Kincaid";
/// Kincaid Solutions blue and cyan (from the logo)
pub(crate) const BRAND_BLUE: Color32 = Color32::from_rgb(0x2f, 0x6b, 0xff);
pub(crate) const BRAND_CYAN: Color32 = Color32::from_rgb(0x3e, 0xe6, 0xff);

static KINCAID_LIGHT_PNG: &[u8] = include_bytes!("../assets/kincaid_light.png");
static KINCAID_DARK_PNG: &[u8] = include_bytes!("../assets/kincaid_dark.png");
pub static ICON_PNG: &[u8] = include_bytes!("../assets/icon_256.png");
static UI_REGULAR: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
static UI_BOLD: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Bold.ttf");

/// Something waiting on "save changes first?".
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum View {
    List,
    Sheet,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Pending {
    New,
    Open,
    Quit,
}

/// The "Print labels" window.
struct PrintDialog {
    printers: Vec<crate::print::Printer>,
    sel: usize,
    copies: u32,
    all: bool,
    from: usize,
    to: usize,
    pages: usize,
}

pub struct App {
    pub(crate) items: Vec<Item>,
    pub(crate) selected: BTreeSet<usize>,
    pub(crate) anchor: Option<usize>,
    pub(crate) name: String,
    pub(crate) code: String,
    pub(crate) build: String,
    pub(crate) list_path: Option<PathBuf>,
    pub(crate) dirty: bool,
    pub(crate) open_after: bool,
    pub(crate) outlines: bool,
    pub(crate) message: Option<(String, bool)>, // (text, is_error) shown in the status bar
    pub(crate) alert: Option<String>,           // pop-up error
    pub(crate) pending: Option<Pending>,
    pub(crate) allow_close: bool,
    kincaid_light: egui::TextureHandle,
    kincaid_dark: egui::TextureHandle,
    print_dlg: Option<PrintDialog>,
    printing: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    last_printer: Option<String>,
    pub(crate) scroll_to: Option<usize>,
    pub(crate) focus_name: bool,
    pub(crate) last_title: String,
    pub(crate) view: View,
    pub(crate) sheet: crate::sheet::SheetState,
    undo: Vec<Vec<Item>>,
    redo: Vec<Vec<Item>>,
    last_items: Vec<Item>,
    last_change: f64,
    new_step: bool,
}

fn load_png(bytes: &[u8]) -> egui::ColorImage {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .expect("bundled image")
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw())
}

pub fn icon() -> egui::IconData {
    let img = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
        .expect("bundled icon")
        .to_rgba8();
    egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("liberation".into(), Arc::new(egui::FontData::from_static(UI_REGULAR)));
    fonts.font_data.insert("liberation-bold".into(), Arc::new(egui::FontData::from_static(UI_BOLD)));
    // Liberation Sans first (it has ½ ⅓ ¼ ⅛ ...), egui's own fonts as fallback
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "liberation".into());
    let mut bold = vec!["liberation-bold".to_string()];
    bold.extend(fonts.families[&FontFamily::Proportional].iter().skip(1).cloned());
    fonts.families.insert(FontFamily::Name("bold".into()), bold);
    ctx.set_fonts(fonts);
}

pub(crate) fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

/// Open a file with whatever the computer normally uses (PDF viewer, etc.).
pub fn open_with_system(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(path).spawn();
    r.is_ok()
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start_file: Option<PathBuf>) -> Self {
        setup_fonts(&cc.egui_ctx);
        cc.egui_ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = Vec2::new(8.0, 6.0);
            style.spacing.button_padding = Vec2::new(10.0, 5.0);
            style.spacing.interact_size.y = 26.0;
            style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
            style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
            style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(12.0));
            style.visuals.selection.bg_fill = BRAND_BLUE;
            style.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
            style.visuals.hyperlink_color = BRAND_BLUE;
            // table text isn't selectable, so shift-click selects a range of rows
            style.interaction.selectable_labels = false;
        });
        // follows the computer's light/dark setting; LABEL_THEME=light or dark forces one
        match std::env::var("LABEL_THEME").unwrap_or_default().to_lowercase().as_str() {
            "light" => cc.egui_ctx.set_theme(egui::Theme::Light),
            "dark" => cc.egui_ctx.set_theme(egui::Theme::Dark),
            _ => {}
        }
        // mipmaps keep the small "Solutions" text crisp when the logo is drawn smaller than its image
        let logo_opts = egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear));
        let kincaid_light = cc.egui_ctx.load_texture("kincaid_light", load_png(KINCAID_LIGHT_PNG), logo_opts);
        let kincaid_dark = cc.egui_ctx.load_texture("kincaid_dark", load_png(KINCAID_DARK_PNG), logo_opts);
        let mut app = App {
            items: vec![],
            selected: BTreeSet::new(),
            anchor: None,
            name: String::new(),
            code: String::new(),
            build: String::new(),
            list_path: None,
            dirty: false,
            open_after: true,
            outlines: false,
            message: None,
            alert: None,
            pending: None,
            allow_close: false,
            kincaid_light,
            kincaid_dark,
            print_dlg: None,
            printing: None,
            last_printer: None,
            scroll_to: None,
            focus_name: true,
            last_title: String::new(),
            view: View::List,
            sheet: Default::default(),
            undo: vec![],
            redo: vec![],
            last_items: vec![],
            last_change: 0.0,
            new_step: false,
        };
        if let Some(p) = start_file {
            app.open_path(&p);
        }
        app
    }

    // ---------------------------------------------------------------- actions

    fn editor_item(&self) -> Item {
        Item::new(
            labels::fix_fractions(self.name.trim()).as_str(),
            self.code.trim(),
            labels::fix_fractions(self.build.trim()).as_str(),
        )
    }

    fn valid(&mut self, it: &Item) -> bool {
        if it.name.is_empty() {
            self.message = Some(("Type a name for the label.".into(), true));
            self.focus_name = true;
            return false;
        }
        if let Some(p) = labels::barcode_problem(&it.barcode) {
            self.message = Some((p, true));
            return false;
        }
        true
    }

    fn sel(&self) -> Vec<usize> {
        self.selected.iter().copied().collect()
    }

    fn select_only(&mut self, i: usize) {
        self.selected.clear();
        if i < self.items.len() {
            self.selected.insert(i);
            self.anchor = Some(i);
            self.scroll_to = Some(i);
        }
    }

    pub(crate) fn load_editor_from_selection(&mut self) {
        if self.selected.len() == 1 {
            let it = &self.items[*self.selected.iter().next().unwrap()];
            self.name = it.name.clone();
            self.code = it.barcode.clone();
            self.build = it.buildto.clone();
        }
    }

    fn clear_editor(&mut self) {
        self.name.clear();
        self.code.clear();
        self.build.clear();
        self.selected.clear();
        self.focus_name = true;
    }

    fn add(&mut self) {
        let it = self.editor_item();
        if !self.valid(&it) {
            return;
        }
        let pos = self.sel().last().map(|i| i + 1).unwrap_or(self.items.len());
        self.items.insert(pos, it);
        self.dirty = true;
        self.name.clear();
        self.code.clear();
        self.build.clear();
        self.selected.clear();
        self.scroll_to = Some(pos);
        self.focus_name = true;
        self.message = None;
    }

    fn update(&mut self) {
        let s = self.sel();
        if s.len() != 1 {
            self.message = Some(("Pick one label in the list to update.".into(), true));
            return;
        }
        let mut it = self.editor_item();
        if !self.valid(&it) {
            return;
        }
        it.extras = std::mem::take(&mut self.items[s[0]].extras); // keep text added in the sheet editor
        self.items[s[0]] = it;
        self.dirty = true;
        self.message = None;
        self.select_only(s[0]);
        self.load_editor_from_selection();
    }

    fn enter(&mut self) {
        if self.selected.len() == 1 { self.update() } else { self.add() }
    }

    pub(crate) fn delete(&mut self) {
        let s = self.sel();
        if s.is_empty() {
            return;
        }
        for i in s.iter().rev() {
            self.items.remove(*i);
        }
        self.dirty = true;
        let next = s[0].min(self.items.len().saturating_sub(1));
        self.selected.clear();
        if !self.items.is_empty() {
            self.select_only(next);
            self.load_editor_from_selection();
        }
    }

    pub(crate) fn duplicate(&mut self) {
        let s = self.sel();
        let Some(&last) = s.last() else { return };
        let copies: Vec<Item> = s.iter().map(|i| self.items[*i].clone()).collect();
        let n = copies.len();
        for (k, c) in copies.into_iter().enumerate() {
            self.items.insert(last + 1 + k, c);
        }
        self.selected = (last + 1..last + 1 + n).collect();
        self.scroll_to = Some(last + n);
        self.dirty = true;
    }

    fn move_by(&mut self, up: bool) {
        let s = self.sel();
        if s.is_empty() || (up && s[0] == 0) || (!up && *s.last().unwrap() + 1 >= self.items.len()) {
            return;
        }
        if up {
            for &i in &s {
                self.items.swap(i, i - 1);
            }
            self.selected = s.iter().map(|i| i - 1).collect();
        } else {
            for &i in s.iter().rev() {
                self.items.swap(i, i + 1);
            }
            self.selected = s.iter().map(|i| i + 1).collect();
        }
        self.scroll_to = self.selected.iter().next().copied();
        self.dirty = true;
    }

    fn blank_spot(&mut self) {
        let pos = self.sel().last().map(|i| i + 1).unwrap_or(self.items.len());
        self.items.insert(pos, Item::default());
        self.select_only(pos);
        self.dirty = true;
    }

    fn open_path(&mut self, path: &Path) {
        match labels::load_items(path) {
            Ok(items) => {
                let n = items.len();
                self.items = items;
                let is_list = !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("odg"));
                self.list_path = if is_list { Some(path.to_path_buf()) } else { None };
                self.dirty = !is_list;
                self.clear_editor();
                self.reset_history();
                self.message = Some((format!("Opened {n} labels from {}", file_name(path)), false));
            }
            Err(e) => self.alert = Some(format!("Couldn't read that file:\n{e}")),
        }
    }

    fn do_new(&mut self) {
        self.items.clear();
        self.list_path = None;
        self.dirty = false;
        self.clear_editor();
        self.reset_history();
        self.message = None;
    }

    // ---------------------------------------------------------------- undo

    /// Start a fresh history (after New / Open) without marking the list changed.
    fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_items = self.items.clone();
        self.sheet.sel_box = None;
        self.sheet.page = 0;
    }

    /// Notice changes made this frame. Each click or key like Enter starts a new undo step;
    /// continuous typing and dragging undo together.
    fn track_changes(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let typing = ctx.memory(|m| m.focused().is_some());
        let step = ctx.input(|i| {
            i.pointer.any_pressed()
                || i.key_pressed(Key::Enter)
                || i.key_pressed(Key::Tab)
                || (!typing && (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)))
        });
        if step {
            self.new_step = true;
        }
        if self.items != self.last_items {
            if self.new_step || now - self.last_change > 1.0 || self.undo.is_empty() {
                self.new_step = false;
                self.undo.push(std::mem::take(&mut self.last_items));
                if self.undo.len() > 200 {
                    self.undo.remove(0);
                }
            }
            self.redo.clear();
            self.last_items = self.items.clone();
            self.last_change = now;
            self.dirty = true;
        }
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.items, prev));
            self.after_history_jump("Undone");
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.items, next));
            self.after_history_jump("Redone");
        }
    }

    fn after_history_jump(&mut self, what: &str) {
        self.last_items = self.items.clone();
        self.last_change = 0.0;
        self.dirty = true;
        self.selected.retain(|i| *i < self.items.len());
        self.sheet.sel_box = None;
        self.load_editor_from_selection();
        self.message = Some((what.into(), false));
    }

    fn do_open(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Open a list or an existing label sheet")
            .add_filter("Lists and label sheets", &["csv", "txt", "odg"])
            .add_filter("CSV list", &["csv"])
            .add_filter("OpenDocument drawing", &["odg"])
            .pick_file();
        if let Some(p) = picked {
            self.open_path(&p);
        }
    }

    /// Returns true if saved.
    fn save_list(&mut self, save_as: bool) -> bool {
        let path = match (&self.list_path, save_as) {
            (Some(p), false) if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv")) => p.clone(),
            _ => {
                let mut d = rfd::FileDialog::new().set_title("Save list").add_filter("CSV list", &["csv"]);
                d = d.set_file_name(format!("{}.csv", self.base_name()));
                match d.save_file() {
                    Some(mut p) => {
                        if p.extension().is_none() {
                            p.set_extension("csv");
                        }
                        p
                    }
                    None => return false,
                }
            }
        };
        match labels::write_csv(&self.items, &path) {
            Ok(()) => {
                self.message = Some((format!("Saved list to {}", path.display()), false));
                self.list_path = Some(path);
                self.dirty = false;
                true
            }
            Err(e) => {
                self.alert = Some(format!("Couldn't save:\n{e}"));
                false
            }
        }
    }

    fn base_name(&self) -> String {
        self.list_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Labels".into())
    }

    fn make_pdf(&mut self) {
        if !self.ready_to_output() {
            return;
        }
        let mut d = rfd::FileDialog::new().set_title("Save label sheet (PDF)").add_filter("PDF", &["pdf"]);
        if let Some(dir) = self.list_path.as_ref().and_then(|p| p.parent()) {
            d = d.set_directory(dir);
        }
        let Some(mut path) = d.set_file_name(format!("{}.pdf", self.base_name())).save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("pdf");
        }
        let opts = pdf::Options { outlines: self.outlines, title: self.base_name() };
        match pdf::build_pdf(&self.items, &opts).and_then(|(bytes, n, pages)| {
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            Ok((n, pages))
        }) {
            Ok((n, pages)) => {
                self.message = Some((
                    format!("Saved {n} labels on {pages} page{} to {}", if pages == 1 { "" } else { "s" }, path.display()),
                    false,
                ));
                if self.open_after && !open_with_system(&path) {
                    self.alert = Some(format!("Saved, but couldn't open it automatically:\n{}", path.display()));
                }
            }
            Err(e) => self.alert = Some(format!("Couldn't make the PDF:\n{e}")),
        }
    }

    /// Labels are ready to print/save? Shows what's wrong if not.
    fn ready_to_output(&mut self) -> bool {
        if !self.items.iter().any(|i| !i.is_blank()) {
            self.message = Some(("Add some labels first.".into(), true));
            return false;
        }
        let bad: Vec<usize> =
            (0..self.items.len()).filter(|&i| labels::barcode_problem(self.items[i].barcode.trim()).is_some()).collect();
        if !bad.is_empty() {
            self.selected = bad.iter().copied().collect();
            self.scroll_to = bad.first().copied();
            self.message = Some(("Fix the barcodes shown in red first.".into(), true));
            return false;
        }
        true
    }

    fn open_print(&mut self) {
        if !self.ready_to_output() {
            return;
        }
        match crate::print::printers() {
            Ok((printers, default)) if !printers.is_empty() => {
                let want = self.last_printer.clone().or(default);
                let sel = want.and_then(|w| printers.iter().position(|p| p.name == w)).unwrap_or(0);
                let pages = crate::print::page_count(&self.items);
                self.print_dlg = Some(PrintDialog { printers, sel, copies: 1, all: true, from: 1, to: pages, pages });
            }
            Ok(_) => {
                self.alert = Some(
                    "No printers are set up on this computer.\nAdd one in your system settings, or use Save PDF… instead.".into(),
                )
            }
            Err(e) => self.alert = Some(e),
        }
    }

    fn print_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.print_dlg.take() else { return };
        let mut go = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("print_dialog")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.label(RichText::new("Print labels").font(bold(17.0)));
            ui.add_space(8.0);
            egui::Grid::new("print_grid").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                ui.label("Printer");
                let current = d.printers[d.sel].label.clone();
                egui::ComboBox::from_id_salt("printer_combo").width(290.0).selected_text(current).show_ui(ui, |ui| {
                    for (k, p) in d.printers.iter().enumerate() {
                        ui.selectable_value(&mut d.sel, k, &p.label);
                    }
                });
                ui.end_row();
                ui.label("Copies");
                ui.add(egui::DragValue::new(&mut d.copies).range(1..=99).speed(0.1));
                ui.end_row();
                if d.pages > 1 {
                    ui.label("Pages");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut d.all, true, format!("All {}", d.pages));
                        ui.radio_value(&mut d.all, false, "");
                        ui.add_enabled(!d.all, egui::DragValue::new(&mut d.from).range(1..=d.pages));
                        ui.label("to");
                        ui.add_enabled(!d.all, egui::DragValue::new(&mut d.to).range(1..=d.pages));
                    });
                    ui.end_row();
                }
                ui.label("");
                ui.checkbox(&mut self.outlines, "Print label outlines (test sheet on plain paper)");
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Prints at actual size (100%) on US Letter paper.").weak().size(12.0));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let print = egui::Button::new(RichText::new("Print").font(bold(14.0)).color(Color32::WHITE))
                    .fill(BRAND_BLUE)
                    .min_size(Vec2::new(90.0, 28.0));
                if ui.add(print).clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    go = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if cancel {
            return;
        }
        if !go {
            self.print_dlg = Some(d);
            return;
        }
        let (from, to) = if d.all { (0, d.pages - 1) } else { (d.from.min(d.to) - 1, d.from.max(d.to) - 1) };
        let printer = d.printers[d.sel].name.clone();
        self.last_printer = Some(printer.clone());
        let job = crate::print::Job { printer, copies: d.copies, pages: (from, to), outlines: self.outlines, title: self.base_name() };
        let items = self.items.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::print::print(&items, &job));
        });
        self.printing = Some(rx);
        self.message = Some((format!("Printing to {}…", d.printers[d.sel].label), false));
    }

    /// Run `p` now, or ask about unsaved changes first.
    fn request(&mut self, p: Pending, ctx: &egui::Context) {
        if self.dirty {
            self.pending = Some(p);
        } else {
            self.run_pending(p, ctx);
        }
    }

    fn run_pending(&mut self, p: Pending, ctx: &egui::Context) {
        match p {
            Pending::New => self.do_new(),
            Pending::Open => self.do_open(),
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    // ---------------------------------------------------------------- drawing

    fn header(&mut self, ui: &mut egui::Ui) {
        let dark = ui.visuals().dark_mode;
        // white header in light mode; in dark mode it matches the window so the logo blends in
        let bg = if dark { ui.visuals().panel_fill } else { Color32::WHITE };
        let fg = if dark { Color32::from_gray(235) } else { Color32::from_gray(45) };
        let muted = if dark { Color32::from_gray(150) } else { Color32::from_gray(120) };
        egui::Panel::top("header")
            .frame(egui::Frame::NONE.fill(bg).inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 10 }))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Kincaid Solutions logo: the version made for this background
                    let tex = if dark { &self.kincaid_dark } else { &self.kincaid_light };
                    let lh = 56.0;
                    let ls = tex.size_vec2();
                    ui.add(egui::Image::new((tex.id(), Vec2::new(ls.x * lh / ls.y, lh))))
                        .on_hover_text("Kincaid Solutions");
                    ui.add_space(12.0);
                    ui.label(RichText::new("Label Generator").font(bold(28.0)).color(fg));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(format!("Developed by {APP_AUTHOR}")).italics().size(12.0).color(muted));
                    });
                });
            });
        // brand line under the header: logo blue fading to cyan
        egui::Panel::top("brandline")
            .exact_size(4.0)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| {
                let r = ui.max_rect();
                let mut m = egui::Mesh::default();
                m.colored_vertex(r.left_top(), BRAND_BLUE);
                m.colored_vertex(r.right_top(), BRAND_CYAN);
                m.colored_vertex(r.right_bottom(), BRAND_CYAN);
                m.colored_vertex(r.left_bottom(), BRAND_BLUE);
                m.add_triangle(0, 1, 2);
                m.add_triangle(0, 2, 3);
                ui.painter().add(m);
            });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 14, right: 14, top: 10, bottom: 8 }))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let ctx = ui.ctx().clone();
                    if ui.button("New").on_hover_text(shortcut_text("N")).clicked() {
                        self.request(Pending::New, &ctx);
                    }
                    if ui.button("Open…").on_hover_text(shortcut_text("O")).clicked() {
                        self.request(Pending::Open, &ctx);
                    }
                    if ui.button("Save list").on_hover_text(shortcut_text("S")).clicked() {
                        self.save_list(false);
                    }
                    if ui.button("Save list as…").clicked() {
                        self.save_list(true);
                    }
                    ui.separator();
                    let undo_tip = if cfg!(target_os = "macos") { "Undo (Cmd+Z)" } else { "Undo (Ctrl+Z)" };
                    if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).on_hover_text(undo_tip).clicked() {
                        self.undo();
                    }
                    let redo_tip = if cfg!(target_os = "macos") { "Redo (Shift+Cmd+Z)" } else { "Redo (Ctrl+Y)" };
                    if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).on_hover_text(redo_tip).clicked() {
                        self.redo();
                    }
                    ui.separator();
                    // List / Sheet tabs
                    for (v, t, tip) in [
                        (View::List, "List", "Type labels in a list"),
                        (View::Sheet, "Sheet", "See the page with its label grid; edit labels and add your own text"),
                    ] {
                        let on = self.view == v;
                        let b = egui::Button::selectable(on, RichText::new(t).font(if on { bold(14.0) } else { FontId::proportional(14.0) }));
                        if ui.add(b).on_hover_text(tip).clicked() && !on {
                            self.view = v;
                            if v == View::Sheet {
                                if let Some(i) = self.selected.iter().next() {
                                    self.sheet.page = i / labels::LABELS_PER_PAGE;
                                }
                            }
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let printing = self.printing.is_some();
                        let print = egui::Button::new(
                            RichText::new(if printing { "Printing…" } else { "Print…" }).font(bold(15.0)).color(Color32::WHITE),
                        )
                        .fill(BRAND_BLUE)
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(100.0, 30.0));
                        if ui.add_enabled(!printing, print).on_hover_text(format!("Print the labels ({})", shortcut_text("P"))).clicked() {
                            self.open_print();
                        }
                        if ui.add(egui::Button::new("Save PDF…").min_size(Vec2::new(0.0, 30.0)))
                            .on_hover_text(format!("Save the sheet as a PDF ({})", shortcut_text("E")))
                            .clicked()
                        {
                            self.make_pdf();
                        }
                        ui.menu_button("Options", |ui| {
                            ui.checkbox(&mut self.outlines, "Print label outlines")
                                .on_hover_text("Draws the label edges - for a test print on plain paper");
                            ui.checkbox(&mut self.open_after, "Open PDF after saving");
                        });
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 14, right: 14, top: 4, bottom: 8 }))
            .show_separator_line(false)
            .show(ui, |ui| {
                let n = self.items.len();
                let pages = if n == 0 { 0 } else { n.div_ceil(labels::LABELS_PER_PAGE) };
                let mut msg = format!("{n} label{} · {pages} page{}", plural(n), plural(pages));
                let empty = (labels::LABELS_PER_PAGE - n % labels::LABELS_PER_PAGE) % labels::LABELS_PER_PAGE;
                if n > 0 && empty > 0 {
                    msg += &format!(" · {empty} empty spot{} on the last page", plural(empty));
                }
                let bad = self.items.iter().filter(|i| row_is_bad(i)).count();
                if bad > 0 {
                    msg += &format!(" · {bad} need fixing (red)");
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(msg).weak());
                    if let Some((m, err)) = &self.message {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let color = if *err { error_color(ui) } else { ui.visuals().weak_text_color() };
                            ui.add(egui::Label::new(RichText::new(m).color(color)).truncate());
                        });
                    }
                });
            });
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("editor")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 14, right: 14, top: 6, bottom: 4 }))
            .show_separator_line(false)
            .show(ui, |ui| {
                egui::Frame::group(ui.style())
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Label").font(bold(14.0)));
                        ui.add_space(2.0);
                        let total = ui.available_width();
                        let gap = 10.0;
                        let w_name = (total - 2.0 * gap) * 0.47;
                        let w_code = (total - 2.0 * gap) * 0.18;
                        let w_build = total - 2.0 * gap - w_name - w_code;
                        let mut enter = false;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for (label, w) in [("Name", w_name), ("Barcode", w_code), ("Build-To", w_build)] {
                                ui.allocate_ui(Vec2::new(w, 18.0), |ui| {
                                    ui.set_width(w);
                                    ui.label(RichText::new(label).weak());
                                });
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            let name_id = egui::Id::unique("name_field");
                            enter |= fraction_field(ui, &mut self.name, name_id, w_name, "e.g. ii Meat - Taco Beef", true);
                            if self.focus_name {
                                ui.ctx().memory_mut(|m| m.request_focus(name_id));
                                self.focus_name = false;
                            }
                            enter |= fraction_field(ui, &mut self.code, egui::Id::unique("code_field"), w_code, "128568", false);
                            enter |= fraction_field(ui, &mut self.build, egui::Id::unique("build_field"), w_build, "e.g. 2 1/2 Bags", true);
                        });
                        let hint = labels::barcode_problem(self.code.trim());
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(hint.unwrap_or_else(|| "Fractions like 1/2 turn into ½ by themselves.".into()))
                                .size(12.0)
                                .color(if labels::barcode_problem(self.code.trim()).is_some() {
                                    error_color(ui)
                                } else {
                                    ui.visuals().weak_text_color()
                                }),
                        );
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            let one = self.selected.len() == 1;
                            let any = !self.selected.is_empty();
                            if ui.button("Add").on_hover_text("Enter").clicked() {
                                self.add();
                            }
                            if ui.add_enabled(one, egui::Button::new("Update")).clicked() {
                                self.update();
                            }
                            if ui.add_enabled(any, egui::Button::new("Delete")).clicked() {
                                self.delete();
                            }
                            if ui.add_enabled(any, egui::Button::new("Duplicate")).clicked() {
                                self.duplicate();
                            }
                            if ui.add_enabled(any, egui::Button::new("Move up")).clicked() {
                                self.move_by(true);
                            }
                            if ui.add_enabled(any, egui::Button::new("Move down")).clicked() {
                                self.move_by(false);
                            }
                            if ui.button("Insert blank spot").on_hover_text("Leaves an empty label - e.g. to skip one already used").clicked() {
                                self.blank_spot();
                            }
                            if ui.button("Clear").clicked() {
                                self.clear_editor();
                            }
                        });
                        if enter {
                            self.enter();
                        }
                    });
            });
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 14, right: 14, top: 0, bottom: 0 }))
            .show(ui, |ui| {
                let err = error_color(ui);
                let mut clicked: Option<(usize, Modifiers)> = None;
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .sense(egui::Sense::click())
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(96.0))
                    .column(Column::remainder().at_least(220.0).clip(true))
                    .column(Column::exact(110.0))
                    .column(Column::initial(240.0).at_least(120.0).clip(true))
                    .min_scrolled_height(0.0)
                    .max_scroll_height(f32::INFINITY);
                if let Some(i) = self.scroll_to.take() {
                    table = table.scroll_to_row(i, Some(Align::Center));
                }
                table
                    .header(26.0, |mut h| {
                        for t in ["Page · Spot", "Name", "Barcode", "Build-To"] {
                            h.col(|ui| {
                                ui.label(RichText::new(t).font(bold(14.0)));
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(26.0, self.items.len(), |mut row| {
                            let i = row.index();
                            let it = &self.items[i];
                            row.set_selected(self.selected.contains(&i));
                            let (page, r, c) = labels::spot(i);
                            let color = if row_is_bad(it) { Some(err) } else { None };
                            let cell = |s: &str| {
                                let mut t = RichText::new(s);
                                if let Some(c) = color {
                                    t = t.color(c);
                                }
                                t
                            };
                            row.col(|ui| {
                                ui.label(RichText::new(format!("{} · R{} C{}", page + 1, r + 1, c + 1)).weak());
                            });
                            let blank = it.is_blank();
                            row.col(|ui| {
                                if blank {
                                    ui.label(RichText::new("(blank spot)").italics().weak());
                                } else {
                                    let mut job = cell(&it.name);
                                    if !it.extras.is_empty() {
                                        job = RichText::new(format!("{}   (+{} text)", it.name, it.extras.len()));
                                        if let Some(c) = color {
                                            job = job.color(c);
                                        }
                                    }
                                    ui.add(egui::Label::new(job).truncate());
                                }
                            });
                            row.col(|ui| {
                                ui.label(cell(&it.barcode));
                            });
                            row.col(|ui| {
                                ui.add(egui::Label::new(cell(&it.buildto)).truncate());
                            });
                            if row.response().clicked() {
                                clicked = Some((i, row.response().ctx.input(|inp| inp.modifiers)));
                            }
                        });
                    });
                if let Some((i, m)) = clicked {
                    if m.command {
                        if !self.selected.remove(&i) {
                            self.selected.insert(i);
                        }
                        self.anchor = Some(i);
                    } else if m.shift && self.anchor.is_some() {
                        let a = self.anchor.unwrap();
                        self.selected = (a.min(i)..=a.max(i)).collect();
                    } else {
                        self.selected.clear();
                        self.selected.insert(i);
                        self.anchor = Some(i);
                    }
                    self.load_editor_from_selection();
                }
                if self.items.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No labels yet").size(18.0).weak());
                        ui.label(RichText::new("Type a Name, Barcode and Build-To below and press Enter, or Open… a list.").weak());
                    });
                }
            });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(p) = self.pending {
            let mut choice: Option<u8> = None;
            egui::Modal::new(egui::Id::unique("unsaved")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.label(RichText::new("Save changes to the list first?").font(bold(16.0)));
                ui.add_space(4.0);
                ui.label("Your changes will be lost if you don't save them.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(0);
                    }
                    if ui.button("Don't Save").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        choice = Some(2);
                    }
                });
            });
            match choice {
                Some(0) => {
                    self.pending = None;
                    if self.save_list(false) {
                        self.run_pending(p, ctx);
                    }
                }
                Some(1) => {
                    self.pending = None;
                    self.dirty = false;
                    self.run_pending(p, ctx);
                }
                Some(2) => self.pending = None,
                _ => {}
            }
        }
        if let Some(text) = self.alert.clone() {
            let mut close = false;
            egui::Modal::new(egui::Id::unique("alert")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new(APP_NAME).font(bold(15.0)));
                ui.add_space(6.0);
                ui.label(text);
                ui.add_space(10.0);
                if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Escape)) {
                    close = true;
                }
            });
            if close {
                self.alert = None;
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let sc = |k: Key| KeyboardShortcut::new(Modifiers::COMMAND, k);
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Key::S))) {
            self.save_list(false);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Key::O))) {
            self.request(Pending::Open, ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Key::N))) {
            self.request(Pending::New, ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Key::E))) {
            self.make_pdf();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Key::P))) && self.printing.is_none() {
            self.open_print();
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        // undo / redo (a text box being typed in handles its own)
        if !typing {
            let redo = ctx.input_mut(|i| {
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z))
                    || i.consume_shortcut(&sc(Key::Y))
            });
            if redo {
                self.redo();
            } else if ctx.input_mut(|i| i.consume_shortcut(&sc(Key::Z))) {
                self.undo();
            }
        }
        if self.view == View::Sheet && self.sheet_keys(ctx) {
            return;
        }
        // Delete / Backspace removes selected rows when no text box is being typed in
        if !typing && !self.selected.is_empty() && ctx.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
            self.delete();
        }
        if self.view == View::List && !typing && ctx.input(|i| i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)) && !self.items.is_empty() {
            let down = ctx.input(|i| i.key_pressed(Key::ArrowDown));
            let cur = self.anchor.unwrap_or(0);
            let next = if down { (cur + 1).min(self.items.len() - 1) } else { cur.saturating_sub(1) };
            self.select_only(next);
            self.load_editor_from_selection();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // window title shows the list name and * when there are unsaved changes
        let title = format!(
            "{}{} - {APP_NAME}",
            if self.dirty { "*" } else { "" },
            self.list_path.as_deref().map(file_name).unwrap_or_else(|| "Untitled".into())
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        // closing the window with unsaved changes asks first
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }

        // files dropped onto the window open as a list
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf())) {
            if self.dirty {
                self.alert = Some("Save or clear the current list before dropping in another one.".into());
            } else {
                self.open_path(&path);
            }
        }

        if self.pending.is_none() && self.alert.is_none() && self.print_dlg.is_none() {
            self.shortcuts(&ctx);
        }
        // a print job finishing in the background
        if let Some(rx) = &self.printing {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    self.message = Some((msg, false));
                    self.printing = None;
                }
                Ok(Err(e)) => {
                    self.alert = Some(format!("Couldn't print:\n{e}"));
                    self.message = None;
                    self.printing = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
                Err(_) => self.printing = None,
            }
        }

        let bg = ctx.global_style().visuals.panel_fill;
        egui::Frame::NONE.fill(bg).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            self.header(ui);
            self.toolbar(ui);
            self.status_bar(ui);
            match self.view {
                View::List => {
                    self.editor(ui);
                    self.table(ui);
                }
                View::Sheet => self.sheet_view(ui),
            }
        });
        self.dialogs(&ctx);
        self.print_dialog(&ctx);

        // blank spots left at the very end (e.g. an empty box clicked and left) aren't kept
        while self.items.last().is_some_and(|it| it.is_blank()) && !self.selected.contains(&(self.items.len() - 1)) {
            self.items.pop();
        }
        self.track_changes(&ctx);
    }
}

/// A one-line text box that turns 1/2 into ½ as you type. Returns true when Enter is pressed.
pub(crate) fn fraction_field(ui: &mut egui::Ui, text: &mut String, id: egui::Id, width: f32, hint: &str, fractions: bool) -> bool {
    let resp = ui.add(
        TextEdit::singleline(text)
            .id(id)
            .desired_width(width)
            .hint_text(hint)
            .margin(egui::Margin::symmetric(6, 5)),
    );
    if fractions && (resp.changed() || resp.lost_focus()) {
        let finished = resp.lost_focus();
        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        let cursor: Option<usize> = state.cursor.char_range().map(|r| r.primary.index.into());
        let (new, cur) = labels::fractionize(text, cursor, finished);
        if new != *text {
            *text = new;
            if let Some(c) = cur {
                let cc = egui::text::CCursor::new(c);
                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(cc)));
                state.store(ui.ctx(), id);
            }
        }
    }
    resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
}

fn row_is_bad(it: &Item) -> bool {
    !it.is_blank() && (it.name.trim().is_empty() || labels::barcode_problem(it.barcode.trim()).is_some())
}

pub(crate) fn error_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode { Color32::from_rgb(0xff, 0x6b, 0x6b) } else { Color32::from_rgb(0xb0, 0x00, 0x20) }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

fn shortcut_text(key: &str) -> String {
    if cfg!(target_os = "macos") { format!("Cmd+{key}") } else { format!("Ctrl+{key}") }
}
