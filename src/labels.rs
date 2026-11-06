//! Label data, sheet layout, Code 128 barcodes, fraction clean-up and list files.
//!
//! Every measurement here is copied from First_Rack.odg so the PDF lines up with
//! the label sheets that were already being printed.

use std::io::Read;
use std::path::Path;

/// One label: what the user types in, plus any text they added themselves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub name: String,
    pub barcode: String,
    pub buildto: String,
    /// Extra text boxes placed on this label in the sheet editor.
    pub extras: Vec<TextBox>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// A piece of the user's own text on a label. Position and width are in cm,
/// measured from the top-left corner of the label's box.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextBox {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    /// Font size in points.
    pub size: f32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub align: TextAlign,
}

impl TextBox {
    /// A new box across the bottom of the label (the empty strip under the barcode number).
    pub fn new(text: &str) -> Self {
        TextBox { text: text.into(), x: 0.3, y: 2.85, w: 5.5, size: 7.0, bold: false, align: TextAlign::Center }
    }
}

impl Item {
    pub fn new(name: &str, barcode: &str, buildto: &str) -> Self {
        Item { name: name.into(), barcode: barcode.into(), buildto: buildto.into(), extras: vec![] }
    }
    /// A blank row leaves an empty spot on the sheet.
    pub fn is_blank(&self) -> bool {
        self.name.trim().is_empty()
            && self.barcode.trim().is_empty()
            && self.buildto.trim().is_empty()
            && self.extras.iter().all(|t| t.text.trim().is_empty())
    }
}

/// Size of a label box (cm): the space a text box can sit in.
pub fn cell_size(col: usize, row: usize) -> (f32, f32) {
    (CELL_X[col].1 - CELL_X[col].0, CELL_Y[row].1 - CELL_Y[row].0)
}

// --------------------------------------------------------------------------
// Layout (centimetres, measured from the top-left corner of a US Letter page)
// --------------------------------------------------------------------------




/// x position of each piece of a label, per column.
pub struct Col { pub title: f32, pub build: f32, pub bar: f32, pub num: f32, pub cap: f32 }
/// y position of each piece of a label, per row.
pub struct Row { pub title: f32, pub build: f32, pub bar: f32, pub cap: f32 }

pub const COLUMNS: [Col; 3] = [
    Col { title: 0.991, build: 1.321, bar: 3.105, num: 1.530, cap: 2.845 },
    Col { title: 7.747, build: 8.077, bar: 9.861, num: 8.286, cap: 9.601 },
    Col { title: 14.554, build: 14.884, bar: 16.668, num: 15.094, cap: 16.408 },
];
pub const ROWS: [Row; 7] = [
    Row { title: 1.118, build: 2.159, bar: 2.794, cap: 3.251 },
    Row { title: 5.004, build: 6.045, bar: 6.680, cap: 7.137 },
    Row { title: 8.890, build: 9.931, bar: 10.566, cap: 11.024 },
    Row { title: 12.776, build: 13.818, bar: 14.453, cap: 14.910 },
    Row { title: 16.637, build: 17.678, bar: 18.313, cap: 18.771 },
    Row { title: 20.523, build: 21.565, bar: 22.200, cap: 22.657 },
    Row { title: 24.384, build: 25.425, bar: 26.060, cap: 26.518 },
];
pub const LABELS_PER_PAGE: usize = 21;

/// (width, height) of each text box, in cm.
pub const TITLE_SIZE: (f32, f32) = (5.893, 0.813);
pub const BUILD_SIZE: (f32, f32) = (5.588, 0.559);
pub const NUM_SIZE: (f32, f32) = (1.397, 0.457);
pub const CAP_SIZE: (f32, f32) = (2.540, 0.356);
pub const BAR_HEIGHT_CM: f32 = 0.457;
/// Width of a 6-character barcode; the bar positions above are for this width.
pub const REF_BAR_WIDTH_CM: f32 = 2.02;
/// Width of the thinnest bar.
pub const MODULE_CM: f32 = 0.02;
pub const MAX_BAR_WIDTH_CM: f32 = 5.0;

/// Font sizes (points) used on the label.
pub const TITLE_PT: f32 = 8.0;
pub const NUM_PT: f32 = 5.0;
pub const CAP_PT: f32 = 6.0;

/// The physical label edges (the guide lines in First_Rack.odg), in cm: (left, right) and (top, bottom).
pub const CELL_X: [(f32, f32); 3] = [(0.864, 6.985), (7.620, 13.767), (14.427, 20.574)];
pub const CELL_Y: [(f32, f32); 7] = [
    (0.787, 4.039), (4.674, 7.899), (8.572, 11.760), (12.421, 15.646),
    (16.281, 19.533), (20.168, 23.368), (24.028, 27.280),
];

/// Where label number `i` (0-based) lands: (page, row, column), all 0-based.
pub fn spot(i: usize) -> (usize, usize, usize) {
    let page = i / LABELS_PER_PAGE;
    let slot = i % LABELS_PER_PAGE;
    (page, slot / 3, slot % 3)
}

// --------------------------------------------------------------------------
// Code 128 (set B) - the barcode type used on the existing labels
// --------------------------------------------------------------------------

const C128: [&str; 107] = [
    "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212", "221213",
    "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221", "223211", "221132",
    "221231", "213212", "223112", "312131", "311222", "321122", "321221", "312212", "322112", "322211",
    "212123", "212321", "232121", "111323", "131123", "131321", "112313", "132113", "132311", "211313",
    "231113", "231311", "112133", "112331", "132131", "113123", "113321", "133121", "313121", "211331",
    "231131", "213113", "213311", "213131", "311123", "311321", "331121", "312113", "312311", "332111",
    "314111", "221411", "431111", "111224", "111422", "121124", "121421", "141122", "141221", "112214",
    "112412", "122114", "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111",
    "111242", "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
    "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311", "113141",
    "114131", "311141", "411131", "211412", "211214", "211232", "2331112",
];
const START_B: usize = 104;
const STOP: usize = 106;

/// The bars of a Code 128 B barcode: `true` = black module.
pub fn code128_modules(code: &str) -> Vec<bool> {
    let mut vals = vec![START_B];
    vals.extend(code.chars().map(|c| c as usize - 32));
    let check = (vals[0] + vals.iter().skip(1).enumerate().map(|(i, v)| v * (i + 1)).sum::<usize>()) % 103;
    vals.push(check);
    vals.push(STOP);
    let mut mods = Vec::new();
    for v in vals {
        let mut bar = true;
        for w in C128[v].bytes() {
            for _ in 0..(w - b'0') {
                mods.push(bar);
            }
            bar = !bar;
        }
    }
    mods
}

pub fn code128_width_cm(code: &str) -> f32 {
    (11 * (code.chars().count() + 3) + 2) as f32 * MODULE_CM
}

/// Why a barcode can't be printed, or `None` if it's fine (blank is fine too).
pub fn barcode_problem(code: &str) -> Option<String> {
    if code.is_empty() {
        return None;
    }
    let mut bad: Vec<char> = code.chars().filter(|c| !(' '..='~').contains(c)).collect();
    bad.sort();
    bad.dedup();
    if !bad.is_empty() {
        let list: Vec<String> = bad.iter().map(|c| format!("'{c}'")).collect();
        return Some(format!("Barcode can't contain: {}", list.join(" ")));
    }
    if code128_width_cm(code) > MAX_BAR_WIDTH_CM {
        return Some("Barcode is too long to fit on the label".into());
    }
    None
}

// --------------------------------------------------------------------------
// Fractions: 1/2 -> ½ (only the ones the label font can print)
// --------------------------------------------------------------------------

pub const FRACTIONS: [(&str, char); 9] = [
    ("1/2", '½'), ("1/3", '⅓'), ("2/3", '⅔'), ("1/4", '¼'), ("3/4", '¾'),
    ("1/8", '⅛'), ("3/8", '⅜'), ("5/8", '⅝'), ("7/8", '⅞'),
];

fn fraction_char(s: &str) -> Option<char> {
    FRACTIONS.iter().find(|(k, _)| *k == s).map(|(_, c)| *c)
}

/// Turn 1/2, 3/4 ... into ½, ¾ ... and "2-1/2" into "2 ½".
///
/// `cursor` is a character index that is kept in the same place in the text.
/// With `finished == false` (while typing) a fraction is only converted once
/// something is typed after it, so "1/16" isn't caught early as "1/1…".
/// Returns the new text and cursor.
pub fn fractionize(text: &str, cursor: Option<usize>, finished: bool) -> (String, Option<usize>) {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut cur: Option<usize> = None;
    let mut i = 0;
    while i < chars.len() {
        if cursor == Some(i) {
            cur = Some(out.len());
        }
        // a fraction starts at a digit that isn't preceded by a digit, '/' or '.'
        let starts = chars[i].is_ascii_digit()
            && (i == 0 || !(chars[i - 1].is_ascii_digit() || chars[i - 1] == '/' || chars[i - 1] == '.'));
        if starts {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j < chars.len() && chars[j] == '/' {
                let mut k = j + 1;
                while k < chars.len() && chars[k].is_ascii_digit() {
                    k += 1;
                }
                let has_denominator = k > j + 1;
                let next_ok = match chars.get(k) {
                    None => finished,
                    Some(c) => !(c.is_ascii_digit() || *c == '/'),
                };
                if has_denominator && next_ok {
                    let token: String = chars[i..k].iter().collect();
                    if let Some(sym) = fraction_char(&token) {
                        out.push(sym);
                        if let Some(c) = cursor {
                            if i < c && c < k {
                                cur = Some(out.len()); // cursor was inside the fraction
                            }
                        }
                        i = k;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    if cursor.is_some_and(|c| c >= chars.len()) {
        cur = Some(out.len());
    }
    // "2-½" -> "2 ½"
    for n in 1..out.len().saturating_sub(1) {
        if out[n] == '-' && out[n - 1].is_ascii_digit() && FRACTIONS.iter().any(|(_, c)| *c == out[n + 1]) {
            out[n] = ' ';
        }
    }
    (out.into_iter().collect(), cur)
}

pub fn fix_fractions(s: &str) -> String {
    fractionize(s, None, true).0
}

// --------------------------------------------------------------------------
// CSV lists
// --------------------------------------------------------------------------

fn header_key(h: &str) -> Option<usize> {
    match h.trim().to_lowercase().as_str() {
        "name" | "item" | "item name" | "description" => Some(0),
        "barcode" | "bar code" | "code" | "number" | "item number" => Some(1),
        "build-to" | "build to" | "buildto" | "build-to's" | "build to's" | "build" | "par" => Some(2),
        "extra text" | "extra text (made in the sheet editor)" => Some(3),
        _ => None,
    }
}

pub fn read_csv_str(data: &str) -> Vec<Item> {
    let data = data.trim_start_matches('\u{feff}');
    let first = data.lines().next().unwrap_or("");
    let delim = [b',', b';', b'\t']
        .into_iter()
        .max_by_key(|d| first.bytes().filter(|b| b == d).count())
        .unwrap_or(b',');
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delim)
        .from_reader(data.as_bytes());
    let rows: Vec<Vec<String>> = rdr
        .records()
        .filter_map(|r| r.ok())
        .map(|r| r.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        .filter(|r| r.iter().any(|c| !c.trim().is_empty()))
        .collect();
    if rows.is_empty() {
        return vec![];
    }
    let mut order: Vec<Option<usize>> = vec![Some(0), Some(1), Some(2)];
    let mut body = &rows[..];
    let head: Vec<Option<usize>> = rows[0].iter().map(|h| header_key(h)).collect();
    if head.iter().any(|h| h.is_some()) {
        order = head;
        body = &rows[1..];
    }
    body.iter()
        .map(|r| {
            let mut it = Item::default();
            for (key, val) in order.iter().zip(r.iter()) {
                match key {
                    Some(0) => it.name = fix_fractions(val.trim()),
                    Some(1) => it.barcode = val.trim().to_string(),
                    Some(2) => it.buildto = fix_fractions(val.trim()),
                    Some(3) if !val.trim().is_empty() => {
                        it.extras = serde_json::from_str(val.trim()).unwrap_or_default();
                    }
                    _ => {}
                }
            }
            it
        })
        .collect()
}

/// CSV with a byte-order mark so Excel on Windows reads ½ correctly.
pub fn write_csv(items: &[Item], path: &Path) -> Result<(), String> {
    let mut wtr = csv::Writer::from_writer(Vec::new());
    let e = |e: csv::Error| e.to_string();
    // the extra column only appears when someone has added their own text
    let extras = items.iter().any(|i| !i.extras.is_empty());
    if extras {
        wtr.write_record(["Name", "Barcode", "Build-To", "Extra text (made in the sheet editor)"]).map_err(e)?;
    } else {
        wtr.write_record(["Name", "Barcode", "Build-To"]).map_err(e)?;
    }
    for it in items {
        if extras {
            let json = if it.extras.is_empty() { String::new() } else { serde_json::to_string(&it.extras).unwrap_or_default() };
            wtr.write_record([&it.name, &it.barcode, &it.buildto, &json]).map_err(e)?;
        } else {
            wtr.write_record([&it.name, &it.barcode, &it.buildto]).map_err(e)?;
        }
    }
    let mut bytes = "\u{feff}".as_bytes().to_vec();
    bytes.extend(wtr.into_inner().map_err(|e| e.to_string())?);
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

// --------------------------------------------------------------------------
// Existing OpenDocument drawing sheets (.odg) - pull the items back out
// --------------------------------------------------------------------------

pub fn read_odg(path: &Path) -> Result<Vec<Item>, String> {
    use quick_xml::events::Event;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Not an OpenDocument drawing ({e})"))?;
    let mut xml = String::new();
    zip.by_name("content.xml")
        .map_err(|e| e.to_string())?
        .read_to_string(&mut xml)
        .map_err(|e| e.to_string())?;

    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut items = Vec::new();
    let mut in_group = 0usize;
    // per group
    let (mut name, mut build, mut num, mut cap) = (String::new(), String::new(), String::new(), String::new());
    // per frame
    let mut frame_style: Option<String> = None;
    let mut text = String::new();
    let mut in_box = false;

    let attr = |e: &quick_xml::events::BytesStart, key: &str| -> Option<String> {
        e.attributes()
            .filter_map(|a| a.ok())
            .find(|a| a.key.as_ref() == key)
            .and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.to_string()))
    };

    loop {
        match reader.read_event() {
            Err(e) => return Err(e.to_string()),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => match e.name().as_ref() {
                "draw:g" => {
                    in_group += 1;
                    if in_group == 1 {
                        name.clear(); build.clear(); num.clear(); cap.clear();
                    }
                }
                "draw:frame" => {
                    frame_style = attr(&e, "draw:text-style-name");
                    text.clear();
                }
                "draw:text-box" => in_box = true,
                "text:p" if in_box && !text.is_empty() => text.push(' '),
                _ => {}
            },
            Ok(Event::Empty(e)) if in_box => match e.name().as_ref() {
                "text:s" => {
                    let n = attr(&e, "text:c").and_then(|v| v.parse().ok()).unwrap_or(1usize);
                    text.push_str(&" ".repeat(n));
                }
                "text:tab" | "text:line-break" => text.push(' '),
                _ => {}
            },
            Ok(Event::Text(t)) if in_box => text.push_str(&t.xml10_content()),
            Ok(Event::GeneralRef(r)) if in_box => {
                let ent = format!("&{};", r.into_inner());
                if let Ok(c) = quick_xml::escape::unescape(&ent) {
                    text.push_str(&c);
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                "draw:text-box" => in_box = false,
                "draw:frame" => {
                    let t = text.trim().to_string();
                    if !t.is_empty() {
                        if t.to_lowercase().starts_with("build-to:") {
                            build = t[9..].trim().to_string();
                        } else if frame_style.as_deref() == Some("P7") {
                            cap = t;
                        } else if frame_style.as_deref() == Some("P5") {
                            num = t;
                        } else if name.is_empty() {
                            name = t;
                        }
                    }
                    frame_style = None;
                }
                "draw:g" => {
                    in_group = in_group.saturating_sub(1);
                    if in_group == 0 && !(name.is_empty() && build.is_empty() && cap.is_empty() && num.is_empty()) {
                        let code = if cap.is_empty() { num.clone() } else { cap.clone() };
                        items.push(Item::new(&name, &code, &build));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    Ok(items)
}

/// Open a list: .csv/.txt, or an existing .odg label sheet.
pub fn load_items(path: &Path) -> Result<Vec<Item>, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext == "odg" {
        return read_odg(path);
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = match String::from_utf8(bytes) {
        Ok(s) => s,
        // old Excel CSVs are often Windows-1252; keep the ASCII and map ½ ¼ ¾
        Err(e) => e
            .into_bytes()
            .iter()
            .map(|&b| match b {
                0xBD => '½',
                0xBC => '¼',
                0xBE => '¾',
                b if b < 0x80 => b as char,
                b => char::from_u32(b as u32).unwrap_or('?'),
            })
            .collect(),
    };
    Ok(read_csv_str(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions() {
        let f = |s: &str| fix_fractions(s);
        assert_eq!(f("Pull at 1/2 Container"), "Pull at ½ Container");
        assert_eq!(f("2 1/2 Bags"), "2 ½ Bags");
        assert_eq!(f("2-1/2 Bags"), "2 ½ Bags");
        assert_eq!(f("1/16 pan"), "1/16 pan");
        assert_eq!(f("1/5 tray"), "1/5 tray");
        assert_eq!(f("10/8 date"), "10/8 date");
        assert_eq!(f("0.1/2"), "0.1/2");
        assert_eq!(f("Half 1/3 and 2/3"), "Half ⅓ and ⅔");
        // while typing, the last fraction waits for the next character
        assert_eq!(fractionize("2 3/4", None, false).0, "2 3/4");
        assert_eq!(fractionize("2 3/4 ", Some(6), false), ("2 ¾ ".to_string(), Some(4)));
        assert_eq!(fractionize("1/2 ", Some(4), false), ("½ ".to_string(), Some(2)));
        assert_eq!(fractionize("1/2 and 3/4 x", Some(13), false), ("½ and ¾ x".to_string(), Some(9)));
        assert_eq!(fractionize("1/2 and 3/4 x", Some(5), false), ("½ and ¾ x".to_string(), Some(3)));
    }

    #[test]
    fn barcode_matches_first_rack() {
        // module pattern decoded from First_Rack.odg for 132954 (101 modules wide)
        let m = code128_modules("132954");
        assert_eq!(m.len(), 101);
        assert!((code128_width_cm("132954") - 2.02).abs() < 1e-5);
        assert!((code128_width_cm("60771") - 1.80).abs() < 1e-5);
        let runs: String = {
            let mut s = String::new();
            let mut n = 1;
            for w in m.windows(2) {
                if w[0] == w[1] { n += 1 } else { s.push_str(&n.to_string()); n = 1 }
            }
            s.push_str(&n.to_string());
            s
        };
        assert!(runs.starts_with("211214")); // start B
        assert!(runs.ends_with("2331112")); // stop
        assert_eq!(barcode_problem("ABCé").unwrap(), "Barcode can't contain: 'é'");
        assert!(barcode_problem(&"1".repeat(20)).is_some());
    }

    #[test]
    fn extras_roundtrip() {
        let mut a = Item::new("ii A", "111", "1 Pack");
        let mut tb = TextBox::new("Thaw 24 hrs, \"cold\"");
        tb.bold = true;
        tb.align = TextAlign::Left;
        a.extras.push(tb);
        let b = Item::new("ii B", "222", "");
        let dir = std::env::temp_dir().join("slg_extras_test.csv");
        write_csv(&[a.clone(), b.clone()], &dir).unwrap();
        let back = load_items(&dir).unwrap();
        assert_eq!(back, vec![a, b]);
        // a label with only custom text is not blank
        let mut only = Item::default();
        only.extras.push(TextBox::new("Note"));
        assert!(!only.is_blank());
    }

    #[test]
    fn csv_roundtrip() {
        let items = read_csv_str("\u{feff}Name,Barcode,Build-To\n\"ii Meat - Patty, Slider\",132954,2 Bags of 6\nii Beans,97402,1/2 Pack\n");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "ii Meat - Patty, Slider");
        assert_eq!(items[1].buildto, "½ Pack");
        let other_order = read_csv_str("Build-To,Name,Barcode\n1 Pack,ii X,123\n");
        assert_eq!(other_order[0], Item::new("ii X", "123", "1 Pack"));
        let no_header = read_csv_str("ii Y,456,2 Bags\n");
        assert_eq!(no_header[0], Item::new("ii Y", "456", "2 Bags"));
    }
}
