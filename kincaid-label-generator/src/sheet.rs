//! The Sheet view: a picture of the printed page with the label grid, where you can
//! click a label to edit it and add, drag and style your own text.

use crate::app::{bold, error_color, fraction_field, App, BRAND_BLUE};
use crate::labels::{self, Item, TextAlign, TextBox, CELL_X, CELL_Y, LABELS_PER_PAGE};
use crate::pdf::{self, Prim, PAGE_H_PT, PAGE_W_PT};
use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Key, Layout, Pos2, Rect, RichText, Sense,
    Shape, Stroke, StrokeKind, TextEdit, Vec2,
};

/// Grid spacing in cm (and the step text snaps to).
const GRID_CM: f32 = 0.25;

/// What a drag on the page is doing.
#[derive(Clone, Copy, PartialEq)]
pub enum Drag {
    /// moving text box `bx` of label `label`; `start` = its (x, y) when the drag began, `acc` = pointer travel so far (cm)
    Move { label: usize, bx: usize, start: (f32, f32), acc: (f32, f32) },
    /// changing its width
    Width { label: usize, bx: usize, start: f32, acc: f32 },
}

pub struct SheetState {
    pub page: usize,
    pub sel_box: Option<usize>,
    pub show_grid: bool,
    pub snap: bool,
    pub zoom: f32,
    pub drag: Option<Drag>,
    pub focus_text: Option<(usize, usize)>,
    pub focus_name: bool,
}

impl Default for SheetState {
    fn default() -> Self {
        SheetState { page: 0, sel_box: None, show_grid: true, snap: true, zoom: 1.0, drag: None, focus_text: None, focus_name: false }
    }
}

fn cm(p: f32) -> f32 {
    p * 2.54 / 72.0
}

fn snap_cm(v: f32, on: bool) -> f32 {
    if on { (v / GRID_CM).round() * GRID_CM } else { (v * 100.0).round() / 100.0 }
}

impl App {
    /// Index of the single selected label, if exactly one is selected.
    pub(crate) fn one_selected(&self) -> Option<usize> {
        if self.selected.len() == 1 { self.selected.iter().next().copied() } else { None }
    }

    /// Pages shown in the sheet view: always room for one more label.
    fn sheet_pages(&self) -> usize {
        self.items.len() / LABELS_PER_PAGE + 1
    }

    pub(crate) fn sheet_view(&mut self, ui: &mut egui::Ui) {
        // keep the page in range and on the selected label
        let pages = self.sheet_pages();
        if self.sheet.page >= pages {
            self.sheet.page = pages - 1;
        }

        self.inspector(ui);

        egui::Panel::top("sheet_bar")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 14, right: 14, top: 0, bottom: 6 }))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.sheet.page > 0, egui::Button::new("‹ Prev")).clicked() {
                        self.sheet.page -= 1;
                    }
                    ui.label(format!("Page {} of {}", self.sheet.page + 1, pages));
                    if ui.add_enabled(self.sheet.page + 1 < pages, egui::Button::new("Next ›")).clicked() {
                        self.sheet.page += 1;
                    }
                    ui.separator();
                    if ui.button("−").on_hover_text("Zoom out").clicked() {
                        self.sheet.zoom = (self.sheet.zoom / 1.25).max(0.5);
                    }
                    if ui.button(format!("{:.0}%", self.sheet.zoom * 100.0)).on_hover_text("Fit the page width").clicked() {
                        self.sheet.zoom = 1.0;
                    }
                    if ui.button("+").on_hover_text("Zoom in").clicked() {
                        self.sheet.zoom = (self.sheet.zoom * 1.25).min(5.0);
                    }
                    ui.separator();
                    ui.checkbox(&mut self.sheet.show_grid, "Grid");
                    ui.checkbox(&mut self.sheet.snap, "Snap to grid")
                        .on_hover_text("Text you drag lines up on the ¼ cm grid");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add(egui::Label::new(
                            RichText::new("Click a label to edit it · drag your own text to move it").weak().size(12.0),
                        ).truncate());
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 14, right: 6, top: 0, bottom: 0 }))
            .show(ui, |ui| {
                let avail = ui.available_width() - 18.0;
                let s = (avail / PAGE_W_PT).max(0.3) * self.sheet.zoom; // screen points per PDF point
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    self.page_canvas(ui, s);
                });
            });
    }

    fn page_canvas(&mut self, ui: &mut egui::Ui, s: f32) {
        let margin = 8.0;
        let size = Vec2::new(PAGE_W_PT * s + 2.0 * margin, PAGE_H_PT * s + 2.0 * margin);
        // when the page is smaller than the view (zoomed out), center it
        let view = ui.available_size();
        let area = Vec2::new(
            if view.x.is_finite() { size.x.max(view.x) } else { size.x },
            if view.y.is_finite() { size.y.max(view.y) } else { size.y },
        );
        let (resp, painter) = ui.allocate_painter(area, Sense::click());
        let centering = ((area - size) / 2.0).max(Vec2::ZERO);
        let origin = resp.rect.min + centering + Vec2::splat(margin);
        let to_screen = |x: f32, y: f32| Pos2::new(origin.x + x * s, origin.y + y * s);
        let page_rect = Rect::from_min_size(origin, Vec2::new(PAGE_W_PT * s, PAGE_H_PT * s));
        let dark = ui.visuals().dark_mode;

        // paper
        painter.rect_filled(page_rect.translate(Vec2::new(2.0, 3.0)), 2.0, Color32::from_black_alpha(if dark { 120 } else { 40 }));
        painter.rect_filled(page_rect, 0.0, Color32::WHITE);

        // fine grid
        if self.sheet.show_grid {
            let step = pdf::pt(GRID_CM);
            let n_x = (PAGE_W_PT / step) as i32;
            let n_y = (PAGE_H_PT / step) as i32;
            for i in 1..=n_x {
                let major = i % 4 == 0;
                let x = origin.x + i as f32 * step * s;
                let c = if major { Color32::from_rgb(205, 215, 230) } else { Color32::from_rgb(232, 237, 245) };
                if major || s > 0.8 {
                    painter.line_segment([Pos2::new(x, page_rect.top()), Pos2::new(x, page_rect.bottom())], Stroke::new(1.0, c));
                }
            }
            for i in 1..=n_y {
                let major = i % 4 == 0;
                let y = origin.y + i as f32 * step * s;
                let c = if major { Color32::from_rgb(205, 215, 230) } else { Color32::from_rgb(232, 237, 245) };
                if major || s > 0.8 {
                    painter.line_segment([Pos2::new(page_rect.left(), y), Pos2::new(page_rect.right(), y)], Stroke::new(1.0, c));
                }
            }
        }

        let first = self.sheet.page * LABELS_PER_PAGE;
        let sel = self.one_selected();
        let mut clicked_cell: Option<(usize, bool)> = None;

        // label boxes: always outlined so you can see where each label is
        for slot in 0..LABELS_PER_PAGE {
            let (col, row) = (slot % 3, slot / 3);
            let r = Rect::from_min_max(
                to_screen(pdf::pt(CELL_X[col].0), pdf::pt(CELL_Y[row].0)),
                to_screen(pdf::pt(CELL_X[col].1), pdf::pt(CELL_Y[row].1)),
            );
            let idx = first + slot;
            let id = egui::Id::unique(("cell", idx));
            let cell = ui.interact(r, id, Sense::click());
            let exists = idx < self.items.len();
            let selected = self.selected.contains(&idx);
            let bad = exists && !self.items[idx].is_blank() && labels::barcode_problem(self.items[idx].barcode.trim()).is_some();
            if selected {
                painter.rect_filled(r, 6.0, Color32::from_rgba_unmultiplied(0x2f, 0x6b, 0xff, 18));
            } else if cell.hovered() {
                painter.rect_filled(r, 6.0, Color32::from_rgba_unmultiplied(0x3e, 0xa8, 0xff, 12));
            }
            let stroke = if selected {
                Stroke::new(2.0, BRAND_BLUE)
            } else if bad {
                Stroke::new(1.5, Color32::from_rgb(0xd0, 0x40, 0x40))
            } else {
                Stroke::new(1.0, Color32::from_rgb(150, 160, 175))
            };
            painter.rect_stroke(r, 6.0, stroke, StrokeKind::Inside);
            // label number in the corner
            painter.text(
                r.min + Vec2::new(5.0, 4.0),
                egui::Align2::LEFT_TOP,
                format!("{}", idx + 1),
                FontId::proportional((9.0 * s).clamp(8.0, 13.0)),
                Color32::from_rgb(160, 168, 180),
            );
            if (!exists || self.items[idx].is_blank()) && cell.hovered() {
                painter.text(r.center(), egui::Align2::CENTER_CENTER, "+ Click to add a label here",
                    FontId::proportional((8.0 * s).clamp(9.0, 14.0)), Color32::from_rgb(120, 130, 150));
            }
            if cell.clicked() || cell.double_clicked() {
                clicked_cell = Some((idx, cell.double_clicked()));
            }
        }

        // the printed content, exactly as the PDF draws it
        let lay = pdf::layout();
        let page_items: Vec<Item> = self.items.iter().skip(first).take(LABELS_PER_PAGE).cloned().collect();
        let prims = lay.page(&page_items, 0, false);
        for p in &prims {
            match p {
                Prim::Bar { x, y, w, h } => {
                    painter.rect_filled(Rect::from_min_size(to_screen(*x, *y), Vec2::new(w * s, h * s)), 0.0, Color32::BLACK);
                }
                Prim::Text { x, y, size, bold: b, text } => {
                    let family = if *b { FontFamily::Name("bold".into()) } else { FontFamily::Proportional };
                    let galley = painter.layout_no_wrap(text.clone(), FontId::new(size * s, family), Color32::BLACK);
                    let baseline = galley.rows.first().and_then(|r| r.glyphs.first().map(|g| r.pos.y + g.pos.y))
                        .unwrap_or(size * s * 0.9);
                    let pos = to_screen(*x, *y) - Vec2::new(0.0, baseline);
                    painter.galley(pos, galley, Color32::BLACK);
                }
                Prim::Outline { .. } => {}
            }
        }

        // the user's own text boxes on the selected label: outlined, draggable
        if let Some(i) = sel.filter(|i| *i >= first && *i < first + LABELS_PER_PAGE && *i < self.items.len()) {
            let slot = i - first;
            let (col, row) = (slot % 3, slot / 3);
            let (cw, ch) = labels::cell_size(col, row);
            for k in 0..self.items[i].extras.len() {
                let tb = self.items[i].extras[k].clone();
                let (x, y, w, h) = lay.extra_rect(&tb, slot);
                let r = Rect::from_min_size(to_screen(x, y), Vec2::new(w * s, h.max(4.0) * s)).expand(2.0);
                let is_sel = self.sheet.sel_box == Some(k);
                let resp = ui.interact(r, egui::Id::unique(("extra", i, k)), Sense::click_and_drag());
                let color = if is_sel { BRAND_BLUE } else { Color32::from_rgb(0x2a, 0xa8, 0xc8) };
                if is_sel || resp.hovered() {
                    painter.rect_stroke(r, 2.0, Stroke::new(if is_sel { 1.5 } else { 1.0 }, color), StrokeKind::Inside);
                } else {
                    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                    painter.extend(Shape::dashed_line(&pts, Stroke::new(1.0, color), 4.0, 3.0));
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::Move);
                }
                if resp.clicked() || resp.drag_started() {
                    self.sheet.sel_box = Some(k);
                }
                if resp.double_clicked() {
                    self.sheet.focus_text = Some((i, k));
                }
                if resp.drag_started() {
                    self.sheet.drag = Some(Drag::Move { label: i, bx: k, start: (tb.x, tb.y), acc: (0.0, 0.0) });
                }
                // width handle on the right edge of the selected box
                if is_sel {
                    let hr = Rect::from_center_size(Pos2::new(r.right(), r.center().y), Vec2::splat(9.0));
                    painter.rect_filled(hr, 2.0, BRAND_BLUE);
                    let hresp = ui.interact(hr.expand(2.0), egui::Id::unique(("extra_w", i, k)), Sense::drag());
                    if hresp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                    }
                    if hresp.drag_started() {
                        self.sheet.drag = Some(Drag::Width { label: i, bx: k, start: tb.w, acc: 0.0 });
                    }
                }
            }
            // apply the drag in progress
            let delta = ui.input(|inp| inp.pointer.delta());
            let dragging = ui.input(|inp| inp.pointer.primary_down());
            match self.sheet.drag {
                Some(Drag::Move { label, bx, start, mut acc }) if label == i && bx < self.items[i].extras.len() && dragging => {
                    acc.0 += cm(delta.x / s);
                    acc.1 += cm(delta.y / s);
                    let tb = &mut self.items[i].extras[bx];
                    // snap the absolute position to the page grid, then clamp inside the label
                    let ax = snap_cm(CELL_X[col].0 + start.0 + acc.0, self.sheet.snap) - CELL_X[col].0;
                    let ay = snap_cm(CELL_Y[row].0 + start.1 + acc.1, self.sheet.snap) - CELL_Y[row].0;
                    tb.x = ax.clamp(0.0, (cw - tb.w).max(0.0));
                    tb.y = ay.clamp(0.0, (ch - 0.3).max(0.0));
                    self.sheet.drag = Some(Drag::Move { label, bx, start, acc });
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                }
                Some(Drag::Width { label, bx, start, mut acc }) if label == i && bx < self.items[i].extras.len() && dragging => {
                    acc += cm(delta.x / s);
                    let tb = &mut self.items[i].extras[bx];
                    let max_w = (cw - tb.x).max(0.5);
                    tb.w = snap_cm(start + acc, self.sheet.snap).clamp(0.5, max_w);
                    self.sheet.drag = Some(Drag::Width { label, bx, start, acc });
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                }
                Some(_) if !dragging => self.sheet.drag = None,
                _ => {}
            }
        }

        if let Some((idx, double)) = clicked_cell {
            if self.sheet.drag.is_none() {
                self.select_cell(idx);
                if double || idx >= self.items.len() || self.items[idx].is_blank() {
                    self.sheet.focus_name = true;
                }
            }
        }
        // clicking the margin around the labels clears the selection
        if resp.clicked() && clicked_cell.is_none() {
            self.selected.clear();
            self.sheet.sel_box = None;
        }
    }

    /// Select label spot `idx`, creating blank spots up to it if it's past the end of the list.
    pub(crate) fn select_cell(&mut self, idx: usize) {
        while self.items.len() <= idx {
            self.items.push(Item::default());
        }
        if self.one_selected() != Some(idx) {
            self.sheet.sel_box = None;
        }
        self.selected.clear();
        self.selected.insert(idx);
        self.anchor = Some(idx);
        self.scroll_to = Some(idx);
        self.load_editor_from_selection();
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("inspector")
            .exact_size(330.0)
            .resizable(false)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 6, right: 14, top: 0, bottom: 6 }))
            .show_separator_line(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Frame::group(ui.style()).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match self.one_selected().filter(|i| *i < self.items.len()) {
                            None if self.selected.len() > 1 => {
                                ui.label(RichText::new(format!("{} labels selected", self.selected.len())).font(bold(15.0)));
                                ui.label(RichText::new("Pick one label to edit it.").weak());
                            }
                            None => {
                                ui.label(RichText::new("Sheet editor").font(bold(15.0)));
                                ui.add_space(4.0);
                                ui.label("Click any label on the page to change its text or add your own.");
                                ui.add_space(6.0);
                                ui.label(RichText::new("• Click an empty box to start a new label there.\n• Drag your own text to move it; drag its blue handle to make it wider.\n• Arrow keys nudge the selected text; Delete removes it.\n• Undo with Ctrl+Z (Cmd+Z on a Mac).").weak());
                            }
                            Some(i) => self.label_inspector(ui, i),
                        }
                    });
                });
            });
    }

    fn label_inspector(&mut self, ui: &mut egui::Ui, i: usize) {
        let (page, r, c) = labels::spot(i);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Label {}", i + 1)).font(bold(15.0)));
            ui.label(RichText::new(format!("page {} · row {} · column {}", page + 1, r + 1, c + 1)).weak().size(12.0));
        });
        ui.add_space(6.0);
        let w = ui.available_width();
        let name_id = egui::Id::unique("sheet_name");
        ui.label(RichText::new("Name").weak());
        fraction_field(ui, &mut self.items[i].name, name_id, w, "e.g. ii Meat - Taco Beef", true);
        if self.sheet.focus_name {
            ui.ctx().memory_mut(|m| m.request_focus(name_id));
            self.sheet.focus_name = false;
        }
        ui.horizontal(|ui| {
            let half = (w - 8.0) / 2.0;
            ui.vertical(|ui| {
                ui.set_width(half);
                ui.label(RichText::new("Barcode").weak());
                fraction_field(ui, &mut self.items[i].barcode, egui::Id::unique("sheet_code"), half, "128568", false);
            });
            ui.vertical(|ui| {
                ui.set_width(half);
                ui.label(RichText::new("Build-To").weak());
                fraction_field(ui, &mut self.items[i].buildto, egui::Id::unique("sheet_build"), half, "2 1/2 Bags", true);
            });
        });
        if let Some(p) = labels::barcode_problem(self.items[i].barcode.trim()) {
            ui.label(RichText::new(p).color(error_color(ui)).size(12.0));
        }

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Your text").font(bold(14.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("+ Add text").clicked() {
                    self.items[i].extras.push(TextBox::new(""));
                    let k = self.items[i].extras.len() - 1;
                    self.sheet.sel_box = Some(k);
                    self.sheet.focus_text = Some((i, k));
                }
            });
        });
        if self.items[i].extras.is_empty() {
            ui.label(RichText::new("Add notes like \"Thaw 24 hrs\" or \"Use by ____\" anywhere on this label.").weak().size(12.0));
        }

        let mut remove: Option<usize> = None;
        let mut copy_all: Option<usize> = None;
        for k in 0..self.items[i].extras.len() {
            let is_sel = self.sheet.sel_box == Some(k);
            let frame = egui::Frame::NONE
                .stroke(Stroke::new(if is_sel { 1.5 } else { 1.0 }, if is_sel { BRAND_BLUE } else { ui.visuals().widgets.noninteractive.bg_stroke.color }))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8));
            let resp = frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                let text_id = egui::Id::unique(("sheet_text", i, k));
                let tb = &mut self.items[i].extras[k];
                let te = ui.add(TextEdit::multiline(&mut tb.text).id(text_id).desired_rows(2)
                    .desired_width(f32::INFINITY).hint_text("Type your text (Enter for a new line)"));
                if te.changed() || te.lost_focus() {
                    let mut state = egui::text_edit::TextEditState::load(ui.ctx(), text_id).unwrap_or_default();
                    let cursor: Option<usize> = state.cursor.char_range().map(|r| r.primary.index.into());
                    let (new, cur) = labels::fractionize(&tb.text, cursor, te.lost_focus());
                    if new != tb.text {
                        tb.text = new;
                        if let Some(c) = cur {
                            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(c))));
                            state.store(ui.ctx(), text_id);
                        }
                    }
                }
                if te.gained_focus() || te.clicked() {
                    self.sheet.sel_box = Some(k);
                }
                if self.sheet.focus_text == Some((i, k)) {
                    ui.ctx().memory_mut(|m| m.request_focus(text_id));
                    self.sheet.focus_text = None;
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new("Size").weak());
                    ui.add(egui::DragValue::new(&mut tb.size).range(4.0..=24.0).speed(0.1).fixed_decimals(1).suffix(" pt"))
                        .on_hover_text("Text size - drag or click to type");
                    ui.toggle_value(&mut tb.bold, RichText::new("Bold").font(bold(13.0)));
                    ui.label(RichText::new("Width").weak());
                    ui.add(egui::DragValue::new(&mut tb.w).range(0.5..=6.1).speed(0.02).fixed_decimals(1).suffix(" cm"))
                        .on_hover_text("Width of the text box");
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (a, t) in [(TextAlign::Left, "Left"), (TextAlign::Center, "Center"), (TextAlign::Right, "Right")] {
                        if ui.selectable_label(tb.align == a, t).clicked() {
                            tb.align = a;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.small_button("Copy to every label").on_hover_text("Puts this text in the same place on every label").clicked() {
                        copy_all = Some(k);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            remove = Some(k);
                        }
                    });
                });
            });
            // clicking anywhere in the card selects that text (without blocking its buttons)
            let clicked_here = ui.input(|inp| {
                inp.pointer.primary_clicked() && inp.pointer.interact_pos().is_some_and(|p| resp.response.rect.contains(p))
            });
            if clicked_here {
                self.sheet.sel_box = Some(k);
            }
            ui.add_space(4.0);
        }
        if let Some(k) = remove {
            self.items[i].extras.remove(k);
            self.sheet.sel_box = None;
        }
        if let Some(k) = copy_all {
            let tb = self.items[i].extras[k].clone();
            let mut n = 0;
            for (j, it) in self.items.iter_mut().enumerate() {
                if j != i && !it.is_blank() && !it.extras.iter().any(|e| e.text == tb.text && e.x == tb.x && e.y == tb.y) {
                    it.extras.push(tb.clone());
                    n += 1;
                }
            }
            self.message = Some((format!("Added that text to {n} other label{}", if n == 1 { "" } else { "s" }), false));
        }

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Duplicate label").clicked() {
                self.duplicate();
            }
            if ui.button("Delete label").clicked() {
                self.delete();
                self.sheet.sel_box = None;
            }
        });
    }

    /// Keys that act on the sheet: arrows nudge the selected text (or move between labels), Delete removes text.
    pub(crate) fn sheet_keys(&mut self, ctx: &egui::Context) -> bool {
        let typing = ctx.memory(|m| m.focused().is_some());
        if typing {
            return false;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.sheet.sel_box.take().is_none() {
                self.selected.clear();
            }
            return true;
        }
        let Some(i) = self.one_selected().filter(|i| *i < self.items.len()) else { return false };
        let (dx, dy) = ctx.input(|inp| {
            let mut d = (0.0f32, 0.0f32);
            if inp.key_pressed(Key::ArrowLeft) { d.0 -= 1.0 }
            if inp.key_pressed(Key::ArrowRight) { d.0 += 1.0 }
            if inp.key_pressed(Key::ArrowUp) { d.1 -= 1.0 }
            if inp.key_pressed(Key::ArrowDown) { d.1 += 1.0 }
            d
        });
        if let Some(k) = self.sheet.sel_box.filter(|k| *k < self.items[i].extras.len()) {
            if ctx.input(|inp| inp.key_pressed(Key::Delete) || inp.key_pressed(Key::Backspace)) {
                self.items[i].extras.remove(k);
                self.sheet.sel_box = None;
                return true;
            }
            if dx != 0.0 || dy != 0.0 {
                let step = if self.sheet.snap { GRID_CM } else { 0.05 };
                let slot = i % LABELS_PER_PAGE;
                let (cw, ch) = labels::cell_size(slot % 3, slot / 3);
                let tb = &mut self.items[i].extras[k];
                tb.x = (tb.x + dx * step).clamp(0.0, (cw - tb.w).max(0.0));
                tb.y = (tb.y + dy * step).clamp(0.0, (ch - 0.3).max(0.0));
                return true;
            }
            return false;
        }
        // no text selected: arrows move between labels like on the sheet
        if dx != 0.0 || dy != 0.0 {
            let next = i as i64 + dx as i64 + 3 * dy as i64;
            if next >= 0 {
                let next = next as usize;
                self.select_cell(next.min(self.items.len()));
                self.sheet.page = next / LABELS_PER_PAGE;
            }
            return true;
        }
        false
    }
}
