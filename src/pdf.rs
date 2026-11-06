//! Lays out the label sheet and writes it to PDF - same layout as First_Rack.odg.
//!
//! The layout step turns a page into simple pieces (text runs, bars, outlines).
//! The PDF writer draws those pieces, and so does the sheet editor on screen,
//! so what you see while editing is what prints.

use crate::labels::*;
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo};
use pdf_writer::{Content, Filter, Name, Pdf, Rect, Ref, Str, TextStr};
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub static REGULAR_TTF: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
pub static BOLD_TTF: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Bold.ttf");

pub const PAGE_W_PT: f32 = 612.0;
pub const PAGE_H_PT: f32 = 792.0;

pub fn pt(cm: f32) -> f32 {
    cm * 72.0 / 2.54
}

// --------------------------------------------------------------------------
// Font measurements
// --------------------------------------------------------------------------

pub struct Metrics {
    face: ttf_parser::Face<'static>,
}

impl Metrics {
    fn new(data: &'static [u8]) -> Self {
        Metrics { face: ttf_parser::Face::parse(data, 0).expect("bundled font") }
    }
    fn upem(&self) -> f32 {
        self.face.units_per_em() as f32
    }
    fn gid(&self, c: char) -> u16 {
        self.face.glyph_index(c).map(|g| g.0).unwrap_or(0)
    }
    fn advance(&self, c: char) -> f32 {
        self.face.glyph_hor_advance(ttf_parser::GlyphId(self.gid(c))).unwrap_or(0) as f32 / self.upem()
    }
    /// Pair kerning between two characters (fraction of an em, usually negative).
    pub fn kern(&self, a: char, b: char) -> f32 {
        let (l, r) = (ttf_parser::GlyphId(self.gid(a)), ttf_parser::GlyphId(self.gid(b)));
        if let Some(table) = self.face.tables().kern {
            for st in table.subtables {
                if st.horizontal && !st.variable {
                    if let Some(k) = st.glyphs_kerning(l, r) {
                        return k as f32 / self.upem();
                    }
                }
            }
        }
        0.0
    }
    /// Width of `s` in points, kerning included (as the original sheet does).
    pub fn width(&self, s: &str, size: f32) -> f32 {
        let chars: Vec<char> = s.chars().collect();
        let adv: f32 = chars.iter().map(|c| self.advance(*c)).sum();
        let kern: f32 = chars.windows(2).map(|w| self.kern(w[0], w[1])).sum();
        (adv + kern) * size
    }
    /// How far each character moves the pen (points), kerning with the next one included.
    #[allow(dead_code)] // used by Windows printing
    pub fn advances(&self, s: &str, size: f32) -> Vec<f32> {
        let chars: Vec<char> = s.chars().collect();
        (0..chars.len())
            .map(|i| {
                let k = chars.get(i + 1).map(|n| self.kern(chars[i], *n)).unwrap_or(0.0);
                (self.advance(chars[i]) + k) * size
            })
            .collect()
    }
    pub fn ascent(&self) -> f32 {
        self.face.ascender() as f32 / self.upem()
    }
    pub fn descent(&self) -> f32 {
        -(self.face.descender() as f32) / self.upem()
    }
}

// --------------------------------------------------------------------------
// Layout pieces. Coordinates are points from the TOP-LEFT of the page.
// --------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Prim {
    /// One run of text in one font, starting at `x` on the baseline `y`.
    Text { x: f32, y: f32, size: f32, bold: bool, text: String },
    /// A filled black rectangle (barcode bar).
    Bar { x: f32, y: f32, w: f32, h: f32 },
    /// A thin grey outline (label edge on test prints).
    Outline { x: f32, y: f32, w: f32, h: f32 },
}

#[derive(Clone, Copy, PartialEq)]
pub enum VAlign {
    /// The original sheet's text boxes centre their text vertically.
    Middle,
    /// The user's own text boxes hang from their top edge.
    Top,
}

/// A piece of text in one font.
pub struct Run {
    pub text: String,
    pub bold: bool,
}

pub struct Layout {
    pub reg: Metrics,
    pub bold: Metrics,
}

/// The shared layout engine (fonts are only parsed once).
pub fn layout() -> &'static Layout {
    static L: OnceLock<Layout> = OnceLock::new();
    L.get_or_init(|| Layout { reg: Metrics::new(REGULAR_TTF), bold: Metrics::new(BOLD_TTF) })
}

/// The original sheet stores font sizes in twentieths of a point and prints them at these sizes.
fn lo_size(size: f32) -> f32 {
    match size {
        s if s == 8.0 => 7.994,
        s if s == 6.0 => 6.009,
        s if s == 5.0 => 4.989,
        s => s,
    }
}

impl Layout {
    pub fn font(&self, bold: bool) -> &Metrics {
        if bold { &self.bold } else { &self.reg }
    }

    pub fn line_height(&self, size: f32) -> f32 {
        (self.reg.ascent() + self.reg.descent()) * size
    }

    /// Lay `runs` out inside a box (cm, from the page's top-left), wrapping on spaces and
    /// at line breaks. Returns the height of the text block in points.
    #[allow(clippy::too_many_arguments)]
    pub fn text_box(
        &self, runs: &[Run], size: f32, x: f32, y: f32, w: f32, h: f32,
        align: TextAlign, valign: VAlign, out: &mut Vec<Prim>,
    ) -> f32 {
        // split into words, keeping each word's font; a word owns the space after it;
        // a line break is its own word ("\n")
        let mut words: Vec<(String, bool)> = Vec::new();
        for r in runs {
            let mut cur = String::new();
            for c in r.text.chars() {
                if c == '\n' {
                    if !cur.is_empty() {
                        words.push((std::mem::take(&mut cur), r.bold));
                    }
                    words.push(("\n".into(), r.bold));
                    continue;
                }
                cur.push(c);
                if c == ' ' {
                    words.push((std::mem::take(&mut cur), r.bold));
                }
            }
            if !cur.is_empty() {
                words.push((cur, r.bold));
            }
        }
        let box_w = pt(w);
        let mut lines: Vec<Vec<(String, bool)>> = vec![vec![]];
        let mut line_w = 0.0;
        for (word, bold) in words {
            if word == "\n" {
                lines.push(vec![]);
                line_w = 0.0;
                continue;
            }
            let ww = self.font(bold).width(&word, size);
            let trimmed = self.font(bold).width(word.trim_end(), size);
            let last = lines.last_mut().unwrap();
            if !last.is_empty() && line_w + trimmed > box_w + 0.01 {
                lines.push(vec![(word, bold)]);
                line_w = ww;
            } else {
                last.push((word, bold));
                line_w += ww;
            }
        }

        let asc = self.reg.ascent() * size;
        let line_h = self.line_height(size);
        let block = line_h * lines.len() as f32;
        let top = match valign {
            VAlign::Middle => pt(y) + (pt(h) - block) / 2.0,
            VAlign::Top => pt(y),
        };
        for (n, line) in lines.iter().enumerate() {
            // join neighbouring words in the same font; a trailing space doesn't count
            let mut segs: Vec<(String, bool)> = Vec::new();
            for (t, b) in line {
                match segs.last_mut() {
                    Some(last) if last.1 == *b => last.0.push_str(t),
                    _ => segs.push((t.clone(), *b)),
                }
            }
            if let Some(last) = segs.last_mut() {
                last.0 = last.0.trim_end().to_string();
            }
            let total: f32 = segs.iter().map(|(t, b)| self.font(*b).width(t, size)).sum();
            let mut cx = match align {
                TextAlign::Left => pt(x),
                TextAlign::Center => pt(x) + (box_w - total) / 2.0,
                TextAlign::Right => pt(x) + box_w - total,
            };
            let baseline = top + asc + line_h * n as f32;
            for (t, b) in segs {
                if t.is_empty() {
                    continue;
                }
                let adv = self.font(b).width(&t, size);
                out.push(Prim::Text { x: cx, y: baseline, size, bold: b, text: t });
                cx += adv;
            }
        }
        block
    }

    fn barcode(&self, code: &str, x: f32, y: f32, w: f32, out: &mut Vec<Prim>) {
        // The original sheet draws the barcode picture slightly inside its box (0.13 mm at the sides,
        // 0.12 mm top and bottom); do the same so bars land exactly where they always have.
        let (inset_x, inset_y) = (0.013, 0.012);
        let mods = code128_modules(code);
        let h = pt(BAR_HEIGHT_CM - 2.0 * inset_y);
        let top = pt(y + inset_y);
        let x = x + inset_x;
        let m = (w - 2.0 * inset_x) / mods.len() as f32; // cm per module
        // bar edges snap to 0.01 mm, like the original sheet's drawing grid
        let snap = |cm: f32| (cm * 1000.0).round() / 1000.0;
        let mut i = 0;
        while i < mods.len() {
            if mods[i] {
                let start = i;
                while i < mods.len() && mods[i] {
                    i += 1;
                }
                let left = snap(x + start as f32 * m);
                let right = snap(x + i as f32 * m);
                out.push(Prim::Bar { x: pt(left), y: top, w: pt(right - left), h });
            } else {
                i += 1;
            }
        }
    }

    /// Where one of the user's text boxes sits on the page: (x, y, w, h) in points.
    pub fn extra_rect(&self, tb: &TextBox, slot: usize) -> (f32, f32, f32, f32) {
        let (col, row) = (slot % 3, slot / 3);
        let x = CELL_X[col].0 + tb.x;
        let y = CELL_Y[row].0 + tb.y;
        let mut scratch = Vec::new();
        let text = if tb.text.is_empty() { " " } else { &tb.text };
        let h = self.text_box(&[Run { text: text.into(), bold: tb.bold }], tb.size, x, y, tb.w, 0.0,
            tb.align, VAlign::Top, &mut scratch);
        (pt(x), pt(y), pt(tb.w), h)
    }

    /// Everything printed on one label spot (0..21 on its page).
    pub fn label(&self, it: &Item, slot: usize, out: &mut Vec<Prim>) {
        let (col, row) = (&COLUMNS[slot % 3], &ROWS[slot / 3]);
        let name = fix_fractions(it.name.trim());
        let build = fix_fractions(it.buildto.trim());
        let code = it.barcode.trim();
        let has_main = !(name.is_empty() && build.is_empty() && code.is_empty());
        if has_main {
            self.text_box(&[Run { text: name, bold: false }], lo_size(TITLE_PT),
                col.title, row.title, TITLE_SIZE.0, TITLE_SIZE.1, TextAlign::Center, VAlign::Middle, out);
            let mut runs = vec![Run { text: "Build-To: ".into(), bold: false }];
            if !build.is_empty() {
                runs.push(Run { text: build, bold: true });
            }
            self.text_box(&runs, lo_size(TITLE_PT), col.build, row.build, BUILD_SIZE.0, BUILD_SIZE.1,
                TextAlign::Center, VAlign::Middle, out);
        }
        if !code.is_empty() && barcode_problem(code).is_none() {
            let bw = (code128_width_cm(code) * 1000.0).round() / 1000.0;
            let bx = ((col.bar + (REF_BAR_WIDTH_CM - bw) / 2.0) * 1000.0).round() / 1000.0;
            let nx = ((col.num + (bx - col.bar)) * 1000.0).round() / 1000.0;
            self.barcode(code, bx, row.bar, bw, out);
            self.text_box(&[Run { text: code.into(), bold: false }], lo_size(NUM_PT),
                nx, row.bar, NUM_SIZE.0, NUM_SIZE.1, TextAlign::Right, VAlign::Middle, out);
            self.text_box(&[Run { text: code.into(), bold: false }], lo_size(CAP_PT),
                col.cap, row.cap, CAP_SIZE.0, CAP_SIZE.1, TextAlign::Center, VAlign::Middle, out);
        }
        // the user's own text
        let (cx, cy) = (CELL_X[slot % 3].0, CELL_Y[slot / 3].0);
        for tb in &it.extras {
            let text = fix_fractions(&tb.text);
            if text.trim().is_empty() {
                continue;
            }
            self.text_box(&[Run { text, bold: tb.bold }], tb.size, cx + tb.x, cy + tb.y, tb.w, 0.0,
                tb.align, VAlign::Top, out);
        }
    }

    /// The label edges.
    pub fn outlines(&self, out: &mut Vec<Prim>) {
        for (x0, x1) in CELL_X {
            for (y0, y1) in CELL_Y {
                out.push(Prim::Outline { x: pt(x0), y: pt(y0), w: pt(x1 - x0), h: pt(y1 - y0) });
            }
        }
    }

    /// Everything on page `page` (0-based).
    pub fn page(&self, items: &[Item], page: usize, outlines: bool) -> Vec<Prim> {
        let mut out = Vec::new();
        if outlines {
            self.outlines(&mut out);
        }
        for (slot, it) in items.iter().skip(page * LABELS_PER_PAGE).take(LABELS_PER_PAGE).enumerate() {
            if !it.is_blank() {
                self.label(it, slot, &mut out);
            }
        }
        out
    }
}

// --------------------------------------------------------------------------
// PDF output
// --------------------------------------------------------------------------

/// One embedded font, tracking which glyphs the sheet uses so only those get embedded.
struct PdfFont {
    data: &'static [u8],
    metrics: &'static Metrics,
    remap: subsetter::GlyphRemapper,
    /// new glyph id -> (original glyph id, character)
    used: BTreeMap<u16, (u16, char)>,
    res_name: &'static [u8],
    ps_name: &'static str,
}

impl PdfFont {
    fn new(data: &'static [u8], metrics: &'static Metrics, res_name: &'static [u8], ps_name: &'static str) -> Self {
        let mut remap = subsetter::GlyphRemapper::new();
        remap.remap(0); // .notdef keeps id 0
        PdfFont { data, metrics, remap, used: BTreeMap::new(), res_name, ps_name }
    }
    fn encode(&mut self, c: char) -> [u8; 2] {
        let old = self.metrics.gid(c);
        let new = self.remap.remap(old);
        self.used.entry(new).or_insert((old, c));
        new.to_be_bytes()
    }
}

fn draw_prims(prims: &[Prim], content: &mut Content, reg: &mut PdfFont, bold: &mut PdfFont) {
    let flip = |y: f32| PAGE_H_PT - y;
    // outlines first (under everything)
    let outlines: Vec<&Prim> = prims.iter().filter(|p| matches!(p, Prim::Outline { .. })).collect();
    if !outlines.is_empty() {
        content.save_state();
        content.set_stroke_gray(0.6);
        content.set_line_width(0.5);
        for p in outlines {
            if let Prim::Outline { x, y, w, h } = p {
                content.rect(*x, flip(y + h), *w, *h);
            }
        }
        content.stroke();
        content.restore_state();
    }
    let mut bars = false;
    for p in prims {
        if let Prim::Bar { x, y, w, h } = p {
            content.rect(*x, flip(y + h), *w, *h);
            bars = true;
        }
    }
    if bars {
        content.set_fill_gray(0.0);
        content.fill_nonzero();
    }
    for p in prims {
        let Prim::Text { x, y, size, bold: is_bold, text } = p else { continue };
        let font = if *is_bold { &mut *bold } else { &mut *reg };
        // glyphs, with a kerning adjustment wherever a pair needs one
        let chars: Vec<char> = text.chars().collect();
        let mut pieces: Vec<(Vec<u8>, f32)> = vec![(vec![], 0.0)];
        for (k, c) in chars.iter().enumerate() {
            let bytes = font.encode(*c);
            pieces.last_mut().unwrap().0.extend(bytes);
            if let Some(next) = chars.get(k + 1) {
                let kern = font.metrics.kern(*c, *next);
                if kern != 0.0 {
                    pieces.last_mut().unwrap().1 = -kern * 1000.0;
                    pieces.push((vec![], 0.0));
                }
            }
        }
        content.begin_text();
        content.set_font(Name(font.res_name), *size);
        content.set_text_matrix([1.0, 0.0, 0.0, 1.0, *x, flip(*y)]);
        {
            let mut tj = content.show_positioned();
            let mut items = tj.items();
            for (bytes, adj) in &pieces {
                if !bytes.is_empty() {
                    items.show(Str(bytes));
                }
                if *adj != 0.0 {
                    items.adjust(*adj);
                }
            }
        }
        content.end_text();
    }
}

pub struct Options {
    /// Draw the label edges - handy for a test print on plain paper.
    pub outlines: bool,
    pub title: String,
}

/// Labels to print: trailing blank rows are dropped (blank rows in the middle stay as gaps).
pub fn trimmed(items: &[Item]) -> Vec<Item> {
    let mut items: Vec<Item> = items.to_vec();
    while items.last().is_some_and(|i| i.is_blank()) {
        items.pop();
    }
    items
}

/// Build the PDF. Problems with a barcode are reported, not printed.
pub fn build_pdf(items: &[Item], opts: &Options) -> Result<(Vec<u8>, usize, usize), String> {
    let items = trimmed(items);
    for it in &items {
        if let Some(p) = barcode_problem(it.barcode.trim()) {
            let who = if it.name.trim().is_empty() { it.barcode.clone() } else { it.name.clone() };
            return Err(format!("{who}: {p}"));
        }
    }
    let pages = items.len().div_ceil(LABELS_PER_PAGE).max(1);
    let lay = layout();
    let mut reg = PdfFont::new(REGULAR_TTF, &lay.reg, b"F1", "LiberationSans");
    let mut bold = PdfFont::new(BOLD_TTF, &lay.bold, b"F2", "LiberationSans-Bold");

    let mut alloc = Ref::new(1);
    let catalog_id = alloc.bump();
    let pages_id = alloc.bump();
    let info_id = alloc.bump();
    let mut page_ids = Vec::new();
    let mut contents: Vec<Vec<u8>> = Vec::new();
    for p in 0..pages {
        let prims = lay.page(&items, p, opts.outlines);
        let mut content = Content::new();
        draw_prims(&prims, &mut content, &mut reg, &mut bold);
        contents.push(content.finish().to_vec());
        page_ids.push((alloc.bump(), alloc.bump()));
    }

    let mut pdf = Pdf::new();
    {
        let mut cat = pdf.catalog(catalog_id);
        cat.pages(pages_id);
        // ask PDF viewers to print at actual size, so labels line up with the sheet
        cat.viewer_preferences().pair(Name(b"PrintScaling"), Name(b"None"));
    }
    pdf.document_info(info_id)
        .title(TextStr(&opts.title))
        .creator(TextStr("Kincaid Label Generator - developed by Adam Kincaid"))
        .producer(TextStr("Kincaid Label Generator"));
    pdf.pages(pages_id).kids(page_ids.iter().map(|p| p.0)).count(pages as i32);

    let font_ids: Vec<Ref> = (0..2).map(|_| alloc.bump()).collect();
    for (pid, cid) in &page_ids {
        let mut page = pdf.page(*pid);
        page.media_box(Rect::new(0.0, 0.0, PAGE_W_PT, PAGE_H_PT)); // US Letter
        page.parent(pages_id);
        page.contents(*cid);
        let mut res = page.resources();
        let mut fonts = res.fonts();
        fonts.pair(Name(reg.res_name), font_ids[0]);
        fonts.pair(Name(bold.res_name), font_ids[1]);
    }
    for ((_, cid), c) in page_ids.iter().zip(&contents) {
        let z = miniz_oxide::deflate::compress_to_vec_zlib(c, 6);
        pdf.stream(*cid, &z).filter(Filter::FlateDecode);
    }

    for (font, type0_id) in [(&reg, font_ids[0]), (&bold, font_ids[1])] {
        embed_font(&mut pdf, &mut alloc, font, type0_id)?;
    }
    Ok((pdf.finish(), items.iter().filter(|i| !i.is_blank()).count(), pages))
}

fn embed_font(pdf: &mut Pdf, alloc: &mut Ref, font: &PdfFont, type0_id: Ref) -> Result<(), String> {
    let cid_id = alloc.bump();
    let desc_id = alloc.bump();
    let file_id = alloc.bump();
    let cmap_id = alloc.bump();
    let face = &font.metrics.face;

    // subset tag: six letters derived from the glyphs used
    let mut h: u32 = 2166136261;
    for g in font.used.keys() {
        h = (h ^ *g as u32).wrapping_mul(16777619);
    }
    let tag: String = (0..6).map(|i| (b'A' + ((h >> (i * 5)) % 26) as u8) as char).collect();
    let base = format!("{tag}+{}", font.ps_name);

    let subset = subsetter::subset(font.data, 0, &font.remap).map_err(|e| format!("font: {e}"))?;
    let upem = font.metrics.upem();
    let scale = |v: f32| v * 1000.0 / upem;

    pdf.type0_font(type0_id)
        .base_font(Name(base.as_bytes()))
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(cid_id)
        .to_unicode(cmap_id);

    let info = SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 };
    {
        let mut cid = pdf.cid_font(cid_id);
        cid.subtype(CidFontType::Type2);
        cid.base_font(Name(base.as_bytes()));
        cid.system_info(info);
        cid.font_descriptor(desc_id);
        cid.default_width(0.0);
        cid.cid_to_gid_map_predefined(Name(b"Identity"));
        let n = font.remap.num_gids();
        let mut widths = vec![0.0f32; n as usize];
        widths[0] = scale(face.glyph_hor_advance(ttf_parser::GlyphId(0)).unwrap_or(0) as f32);
        for (new, (old, _)) in &font.used {
            widths[*new as usize] = scale(face.glyph_hor_advance(ttf_parser::GlyphId(*old)).unwrap_or(0) as f32);
        }
        cid.widths().consecutive(0, widths);
    }

    let bb = face.global_bounding_box();
    pdf.font_descriptor(desc_id)
        .name(Name(base.as_bytes()))
        .flags(FontFlags::NON_SYMBOLIC)
        .bbox(Rect::new(scale(bb.x_min as f32), scale(bb.y_min as f32), scale(bb.x_max as f32), scale(bb.y_max as f32)))
        .italic_angle(0.0)
        .ascent(scale(face.ascender() as f32))
        .descent(scale(face.descender() as f32))
        .cap_height(scale(face.capital_height().unwrap_or(1409) as f32))
        .stem_v(if font.res_name == b"F2" { 140.0 } else { 80.0 })
        .font_file2(file_id);

    let z = miniz_oxide::deflate::compress_to_vec_zlib(&subset, 9);
    pdf.stream(file_id, &z).filter(Filter::FlateDecode);

    let mut cmap = pdf_writer::types::UnicodeCmap::new(Name(b"Custom"), info);
    for (new, (_, c)) in &font.used {
        cmap.pair(*new, *c);
    }
    let data = cmap.finish();
    pdf.cmap(cmap_id, &data).writing_mode(pdf_writer::writers::WMode::Horizontal);
    Ok(())
}
