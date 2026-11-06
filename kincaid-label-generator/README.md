# Kincaid Label Generator

*Developed by Adam Kincaid*

A desktop program for Windows, Mac and Linux that prints Build-To label sheets straight to your printer,
or saves them as **PDF** files. No other software needed.
The layout matches `First_Rack.odg`: 3 labels across and 7 down (21 per page) on US Letter paper, with the same
fonts, sizes and Code 128 barcodes. Lists longer than 21 labels continue onto more pages.

Written in Rust. It's a single program file with the fonts, logo and icon built in.

## Using it

There are two views, switched with **List** and **Sheet** in the toolbar.

### List

- **Add a label:** type the Name, Barcode and Build-To, then press **Enter**. Fractions fix themselves:
  `1/2` becomes ½ (also ⅓ ⅔ ¼ ¾ ⅛ ⅜ ⅝ ⅞), and `2 1/2` or `2-1/2` becomes `2 ½`.
- **Edit a label:** click a row, change the fields, then press **Enter** (or **Update**).
- **Select several rows:** Ctrl-click (Cmd-click on a Mac) picks individual rows. Shift-click picks a range.
  **Delete**, **Duplicate** and **Move up/down** work on everything selected. The Delete key removes rows too.
- **Insert blank spot:** leaves an empty label, for skipping labels already used on a sheet.
- **Page · Spot:** shows where each label will print (R = row, C = column).
- **Open… / Save list:** lists are saved as `.csv` files, which you can also edit in Excel or Calc. Open… also
  reads your old drawing sheets (`.odg`, such as First_Rack.odg) so you can carry them over. You can also
  drag a file onto the window.
- **Print…:** prints the sheet. Pick the printer, number of copies and which pages; it always
  prints at actual size (100%) on US Letter.
- **Save PDF…:** saves the sheet as a PDF if you want a file instead.
- **Options:**
  - **Print label outlines:** draws the label edges, for a test print on plain paper held up to a label sheet.
  - **Open PDF after saving:** opens each saved PDF automatically.
- **Shortcuts:** Ctrl (Cmd on a Mac) + **N** new, **O** open, **S** save list, **P** print, **E** save PDF,
  **Z** undo, **Y** (or Shift+Z) redo.
- **Light and dark mode:** follows your computer's setting.

### Sheet: the built-in editor

The Sheet view shows the actual page: every label box outlined and numbered, a ¼ cm grid behind it
(turn it off with **Grid**), and each label drawn exactly as it will print.

- **Edit a label:** click it. Its Name, Barcode and Build-To appear on the right, and changes show up
  on the page as you type.
- **Start a new label in a particular spot:** click an empty box.
- **Add your own text:** with a label selected, click **+ Add text** and type. Use Enter for a second line.
  - **Style:** set the size, Bold, alignment (Left / Center / Right) and width.
  - **Move it:** drag it on the page. With **Snap to grid** on, it lines up on the grid. Drag the blue
    handle on its right edge to change the width.
  - **Keyboard:** arrow keys nudge the selected text, and Delete removes it.
  - **Copy to every label:** puts the same text in the same place on all your labels, for things like
    "Thaw 24 hrs" or "Use by ____".
- **Pages:** **‹ Prev / Next ›** move between pages. **− / + / %** zoom (click the % to fit the width).
- **Undo:** **Undo** / **Redo** in the toolbar, or Ctrl+Z / Ctrl+Y, take back any change.
- **Saving:** your own text is saved with the list in an extra CSV column, and the List view marks
  those labels with "(+1 text)".

### Printing

**Print…** sends the labels straight to the printer at actual size:

- **Windows:** the program draws each label directly on the printer page using Windows' own
  printing, placing every letter and bar exactly where the PDF puts it.
- **Mac and Linux:** it uses the computer's print system (CUPS) with scaling turned off.

If you print a saved PDF yourself instead, print it at **Actual size / 100%**, not "Fit to page", or the labels won't line up. The PDF asks viewers
to do this, but some ignore it, so check the print dialog the first time.

## Getting the program

### Easiest: let GitHub build it

1. Put this folder in a GitHub repository.
2. Open the **Actions** tab, choose **Build**, and click **Run workflow**.
3. When it finishes (about 10 minutes), download from the run page:
   - **Windows:** `Kincaid Label Generator.exe`. It runs on its own; copy it anywhere.
   - **Mac:** `Kincaid Label Generator.app`, which runs on both Apple Silicon and Intel Macs.
     The first time, right-click it and choose **Open**, since it isn't signed by Apple.
   - **Linux:** `kincaid-label-generator-linux-x86_64.tar.gz` for regular PCs and
     `kincaid-label-generator-linux-arm64.tar.gz` for 64-bit ARM boards such as a Raspberry Pi

### Publishing a release (for the website's Download buttons)

Push a version tag:

```
git tag v2.2.1
git push origin v2.2.1
```

The same workflow builds everything and attaches it to a GitHub Release. The file names never change, so
`https://github.com/<you>/<repo>/releases/latest/download/<file>` always gives the newest version. Those are
the links the website's Download section uses.

### Build it yourself

First install Rust from <https://rustup.rs>.

| System  | Steps |
|---------|-------|
| Windows | Double-click `packaging\windows\build.bat`. The Rust installer offers to install the Microsoft C++ build tools it needs. |
| Mac     | Run `xcode-select --install` once, then `bash packaging/macos/make-app.sh`. This gives you `target/Kincaid Label Generator.app`. |
| Linux   | Run `sudo apt install libgtk-3-dev` (or your distribution's equivalent), then `./packaging/linux/install.sh`. This also adds it to your app menu. |

Or just run `cargo run --release` on any of them.

## Command line

```
kincaid-label-generator Sample_List.csv -o Sample_List.pdf           # make a PDF without opening the window
kincaid-label-generator Sample_List.csv -o test.pdf --outlines        # with label edges drawn
kincaid-label-generator Sample_List.csv                                # open the window with a list loaded
```

To force light or dark mode, set `LABEL_THEME=light` or `LABEL_THEME=dark`.

## How the PDF matches the original sheets

- **Measurements:** every position and size comes from `First_Rack.odg`.
- **Comparison:** the PDF of First_Rack was checked against a PDF of the original sheet.
  All 576 barcode bars land within 0.01 mm of the original, and every line of text within about 0.1 mm.
- **Text:** uses the same font (Liberation Sans, the font in the original sheet), the same kerning, and
  the original's exact font sizes.
- **Barcodes:** drawn as sharp vector bars rather than pictures, with the same small inset the original
  uses. They're Code 128 (set B), the same as before.
- **Fonts in the file:** only the letters actually used are embedded, so a sheet is about 30 KB.

## Project layout

```
src/main.rs     starts the window, or the command line
src/app.rs      the window
src/pdf.rs      lays out the page (shared by the Sheet view) and writes the PDF
src/sheet.rs    the Sheet view (page preview, grid, your own text)
src/print.rs    printing (CUPS on Mac/Linux, Windows GDI)
src/labels.rs   layout measurements, barcodes, fractions, CSV and .odg reading (with tests)
assets/         Kincaid Solutions logos, the app icon (icon.svg is its source), the bundled Liberation Sans fonts (SIL Open Font License), a made-up sample list (Sample_List.csv)
packaging/      build scripts for Windows, Mac and Linux
```

The Liberation Sans fonts are © Red Hat and others, under the SIL Open Font License 1.1
(see `assets/fonts/LICENSE-OFL.txt`), which allows bundling them with the program.
