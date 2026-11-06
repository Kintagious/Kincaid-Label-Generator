// No console window on Windows when started by double-click.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod labels;
mod pdf;
mod print;
mod sheet;

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Command line:  kincaid-label-generator LIST.csv -o OUT.pdf [--outlines]
    if let Some(o) = args.iter().position(|a| a == "-o" || a == "--out") {
        let input = args.iter().find(|a| !a.starts_with('-') && Some(*a) != args.get(o + 1));
        let (Some(input), Some(out)) = (input, args.get(o + 1)) else {
            eprintln!("usage: kincaid-label-generator LIST.csv -o OUT.pdf [--outlines]");
            std::process::exit(2);
        };
        let items = labels::load_items(&PathBuf::from(input)).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        });
        let opts = pdf::Options { outlines: args.iter().any(|a| a == "--outlines"), title: "Labels".into() };
        match pdf::build_pdf(&items, &opts) {
            Ok((bytes, n, pages)) => {
                std::fs::write(out, bytes).expect("write PDF");
                println!("Wrote {n} labels on {pages} page(s) to {out}");
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1)
            }
        }
        return;
    }

    // Otherwise open the window (optionally with a list: kincaid-label-generator LIST.csv)
    let start = args.iter().find(|a| !a.starts_with('-')).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(app::APP_NAME)
            .with_app_id("kincaid-label-generator")
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([780.0, 520.0])
            .with_icon(std::sync::Arc::new(app::icon()))
            .with_drag_and_drop(true),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        app::APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, start)))),
    ) {
        eprintln!("Couldn't open the window: {e}");
        std::process::exit(1);
    }
}
