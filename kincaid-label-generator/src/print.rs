//! Printing straight to a printer, at actual size (100%), so the labels line up.
//!
//! - Mac and Linux: the sheet goes to the computer's print system (CUPS) as a PDF, with
//!   scaling turned off.
//! - Windows: the sheet is drawn directly on the printer page with Windows' own printing
//!   (GDI), using the same positions as the PDF.

use crate::labels::{Item, LABELS_PER_PAGE};

#[derive(Clone, Debug)]
pub struct Printer {
    /// The name the system uses for it.
    pub name: String,
    /// What to show in the list.
    pub label: String,
}

pub struct Job {
    pub printer: String,
    pub copies: u32,
    /// first and last page to print, 0-based, inclusive
    pub pages: (usize, usize),
    pub outlines: bool,
    pub title: String,
}

/// Number of printed pages for this list.
pub fn page_count(items: &[Item]) -> usize {
    crate::pdf::trimmed(items).len().div_ceil(LABELS_PER_PAGE).max(1)
}

/// The printers on this computer and the default one.
pub fn printers() -> Result<(Vec<Printer>, Option<String>), String> {
    #[cfg(windows)]
    {
        win::printers()
    }
    #[cfg(not(windows))]
    {
        cups::printers()
    }
}

/// Print. Returns a short message for the status bar.
pub fn print(items: &[Item], job: &Job) -> Result<String, String> {
    let items = crate::pdf::trimmed(items);
    for it in &items {
        if let Some(p) = crate::labels::barcode_problem(it.barcode.trim()) {
            return Err(format!("{}: {p}", if it.name.trim().is_empty() { &it.barcode } else { &it.name }));
        }
    }
    #[cfg(windows)]
    {
        win::print(&items, job)
    }
    #[cfg(not(windows))]
    {
        cups::print(&items, job)
    }
}

// --------------------------------------------------------------------------
// Mac and Linux: CUPS (the lp / lpstat commands)
// --------------------------------------------------------------------------

#[cfg(not(windows))]
mod cups {
    use super::*;
    use std::process::Command;

    const NO_CUPS: &str = "This computer's print system (CUPS) wasn't found.\nUse Save PDF… and print the PDF instead.";

    fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
        let out = Command::new(cmd).args(args).env("LC_ALL", "C").output().map_err(|_| NO_CUPS.to_string())?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if err.is_empty() { format!("{cmd} failed") } else { err });
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    pub fn printers() -> Result<(Vec<Printer>, Option<String>), String> {
        // names: `lpstat -e` (newer CUPS), falling back to `lpstat -a`
        let mut names: Vec<String> = run("lpstat", &["-e"])
            .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
            .unwrap_or_default();
        if names.is_empty() {
            match run("lpstat", &["-a"]) {
                Ok(s) => names = s.lines().filter_map(|l| l.split_whitespace().next().map(String::from)).collect(),
                Err(e) if e == NO_CUPS => return Err(e),
                Err(_) => {}
            }
        }
        // friendly descriptions from `lpstat -l -p`
        let mut descr = std::collections::HashMap::new();
        if let Ok(s) = run("lpstat", &["-l", "-p"]) {
            let mut cur: Option<String> = None;
            for line in s.lines() {
                if let Some(rest) = line.strip_prefix("printer ") {
                    cur = rest.split_whitespace().next().map(String::from);
                } else if let (Some(c), Some(d)) = (&cur, line.trim().strip_prefix("Description:")) {
                    let d = d.trim();
                    if !d.is_empty() {
                        descr.insert(c.clone(), d.to_string());
                    }
                }
            }
        }
        let default = run("lpstat", &["-d"]).ok().and_then(|s| {
            s.split(':').nth(1).map(|d| d.trim().to_string()).filter(|d| !d.is_empty() && !d.contains(' '))
        });
        let printers = names
            .into_iter()
            .map(|n| Printer { label: descr.get(&n).cloned().unwrap_or_else(|| n.replace('_', " ")), name: n })
            .collect();
        Ok((printers, default))
    }

    pub fn print(items: &[Item], job: &Job) -> Result<String, String> {
        let opts = crate::pdf::Options { outlines: job.outlines, title: job.title.clone() };
        let (bytes, _, _) = crate::pdf::build_pdf(items, &opts)?;
        let path = std::env::temp_dir().join(format!("kincaid-labels-{}.pdf", std::process::id()));
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        let copies = job.copies.max(1).to_string();
        let range = format!("page-ranges={}-{}", job.pages.0 + 1, job.pages.1 + 1);
        let p = path.to_string_lossy().to_string();
        let result = run(
            "lp",
            &[
                "-d", &job.printer, "-n", &copies, "-t", &job.title,
                // actual size on US Letter - never "fit to page"
                "-o", "media=Letter", "-o", "print-scaling=none", "-o", "fit-to-page=false",
                "-o", &range, &p,
            ],
        );
        let _ = std::fs::remove_file(&path);
        result.map(|out| {
            let id = out.trim().strip_prefix("request id is ").and_then(|s| s.split_whitespace().next()).unwrap_or("");
            if id.is_empty() { format!("Sent to {}", job.printer) } else { format!("Sent to {} ({id})", job.printer) }
        })
    }
}

// --------------------------------------------------------------------------
// Windows: draw the page with GDI
// --------------------------------------------------------------------------

#[cfg(any(windows, wincheck))]
mod win {
    use super::*;
    use crate::pdf::{self, Prim};
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::Graphics::Printing::{EnumPrintersW, GetDefaultPrinterW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W};
    use windows_sys::Win32::Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn from_wide(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut n = 0;
        while unsafe { *p.add(n) } != 0 {
            n += 1;
        }
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) })
    }

    pub fn printers() -> Result<(Vec<Printer>, Option<String>), String> {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let mut needed = 0u32;
        let mut count = 0u32;
        let mut list = Vec::new();
        unsafe {
            EnumPrintersW(flags, std::ptr::null(), 4, std::ptr::null_mut(), 0, &mut needed, &mut count);
            if needed > 0 {
                let mut buf = vec![0u8; needed as usize];
                if EnumPrintersW(flags, std::ptr::null(), 4, buf.as_mut_ptr(), needed, &mut needed, &mut count) != 0 {
                    let infos = std::slice::from_raw_parts(buf.as_ptr() as *const PRINTER_INFO_4W, count as usize);
                    for i in infos {
                        let name = from_wide(i.pPrinterName);
                        if !name.is_empty() {
                            list.push(Printer { label: name.clone(), name });
                        }
                    }
                }
            }
        }
        let mut size = 0u32;
        let default = unsafe {
            GetDefaultPrinterW(std::ptr::null_mut(), &mut size);
            if size > 0 {
                let mut buf = vec![0u16; size as usize];
                if GetDefaultPrinterW(buf.as_mut_ptr(), &mut size) != 0 { Some(from_wide(buf.as_ptr())) } else { None }
            } else {
                None
            }
        };
        Ok((list, default))
    }

    /// The bundled Liberation Sans fonts, made available to this program only.
    fn load_fonts() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| unsafe {
            for data in [pdf::REGULAR_TTF, pdf::BOLD_TTF] {
                let mut n = 0u32;
                AddFontMemResourceEx(data.as_ptr() as *const _, data.len() as u32, std::ptr::null(), &mut n);
            }
        });
    }

    pub fn print(items: &[Item], job: &Job) -> Result<String, String> {
        load_fonts();
        let lay = pdf::layout();
        let pages = page_count(items);
        let (first, last) = (job.pages.0.min(pages - 1), job.pages.1.min(pages - 1));
        let printer = wide(&job.printer);
        let driver = wide("WINSPOOL");
        let title = wide(&job.title);
        let face = wide("Liberation Sans");
        unsafe {
            let hdc = CreateDCW(driver.as_ptr(), printer.as_ptr(), std::ptr::null(), std::ptr::null());
            if hdc.is_null() {
                return Err(format!("Couldn't open the printer \"{}\".", job.printer));
            }
            let di = DOCINFOW {
                cbSize: std::mem::size_of::<DOCINFOW>() as i32,
                lpszDocName: title.as_ptr(),
                lpszOutput: std::ptr::null(),
                lpszDatatype: std::ptr::null(),
                fwType: 0,
            };
            if StartDocW(hdc, &di) <= 0 {
                DeleteDC(hdc);
                return Err("The printer didn't accept the job.".into());
            }
            // device dots per inch, and the unprintable margin the page origin starts at
            let dpi_x = GetDeviceCaps(hdc, LOGPIXELSX as i32) as f32;
            let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY as i32) as f32;
            let off_x = GetDeviceCaps(hdc, PHYSICALOFFSETX as i32) as f32;
            let off_y = GetDeviceCaps(hdc, PHYSICALOFFSETY as i32) as f32;
            let px = |x: f32| x / 72.0 * dpi_x - off_x;
            let py = |y: f32| y / 72.0 * dpi_y - off_y;

            let mut fonts: HashMap<(bool, i32), HFONT> = HashMap::new();
            let black = GetStockObject(BLACK_BRUSH) as HBRUSH;
            let pen = CreatePen(PS_SOLID, (dpi_x / 144.0).max(1.0) as i32, 0x0099_9999);
            let mut ok = true;
            'copies: for _ in 0..job.copies.max(1) {
                for p in first..=last {
                    if StartPage(hdc) <= 0 {
                        ok = false;
                        break 'copies;
                    }
                    SetBkMode(hdc, TRANSPARENT as i32);
                    SetTextAlign(hdc, TA_BASELINE | TA_LEFT);
                    SetTextColor(hdc, 0);
                    for prim in lay.page(items, p, job.outlines) {
                        match prim {
                            Prim::Outline { x, y, w, h } => {
                                let old_pen = SelectObject(hdc, pen);
                                let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
                                Rectangle(hdc, px(x).round() as i32, py(y).round() as i32, px(x + w).round() as i32, py(y + h).round() as i32);
                                SelectObject(hdc, old_pen);
                                SelectObject(hdc, old_brush);
                            }
                            Prim::Bar { x, y, w, h } => {
                                let r = RECT {
                                    left: px(x).round() as i32,
                                    top: py(y).round() as i32,
                                    right: px(x + w).round() as i32,
                                    bottom: py(y + h).round() as i32,
                                };
                                FillRect(hdc, &r, black);
                            }
                            Prim::Text { x, y, size, bold, text } => {
                                let height = (size / 72.0 * dpi_y).round() as i32;
                                let font = *fonts.entry((bold, height)).or_insert_with(|| {
                                    CreateFontW(
                                        -height, 0, 0, 0,
                                        if bold { FW_BOLD as i32 } else { FW_NORMAL as i32 },
                                        0, 0, 0,
                                        DEFAULT_CHARSET as u32, OUT_TT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32,
                                        PROOF_QUALITY as u32, DEFAULT_PITCH as u32,
                                        face.as_ptr(),
                                    )
                                });
                                let old = SelectObject(hdc, font);
                                // place every letter exactly where the PDF puts it
                                let adv = lay.font(bold).advances(&text, size);
                                let mut units: Vec<u16> = Vec::new();
                                let mut dx: Vec<i32> = Vec::new();
                                let mut pos = x;
                                for (c, a) in text.chars().zip(adv) {
                                    let start = px(pos).round() as i32;
                                    pos += a;
                                    let end = px(pos).round() as i32;
                                    let mut buf = [0u16; 2];
                                    let enc = c.encode_utf16(&mut buf);
                                    for (k, u) in enc.iter().enumerate() {
                                        units.push(*u);
                                        dx.push(if k == 0 { end - start } else { 0 });
                                    }
                                }
                                ExtTextOutW(hdc, px(x).round() as i32, py(y).round() as i32, 0, std::ptr::null(),
                                    units.as_ptr(), units.len() as u32, dx.as_ptr());
                                SelectObject(hdc, old);
                            }
                        }
                    }
                    if EndPage(hdc) <= 0 {
                        ok = false;
                        break 'copies;
                    }
                }
            }
            if ok {
                EndDoc(hdc);
            } else {
                AbortDoc(hdc);
            }
            for f in fonts.values() {
                DeleteObject(*f);
            }
            DeleteObject(pen);
            DeleteDC(hdc);
            if ok { Ok(format!("Sent to {}", job.printer)) } else { Err("Printing stopped - the printer reported a problem.".into()) }
        }
    }
}
