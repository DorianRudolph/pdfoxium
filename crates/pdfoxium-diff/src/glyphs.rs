// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Diagnostic crops use native text boxes only; the Rust text-extraction API
//! is deliberately not involved. Compare unshifted rendering in shared boxes.
use super::*;
use image::{imageops, Rgba};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

struct Glyph {
    index: i32,
    unicode: char,
    font: String,
    size: f64,
    rect: (u32, u32, u32, u32),
}

pub(super) fn audit(case: &Case) -> Result<()> {
    let bindings = Pdfium::bind_to_library(library_path("native"))?;
    // SAFETY: Only native PDFium is initialized in this parent process. Page,
    // text-page and document handles close in reverse order on every result.
    unsafe {
        bindings.FPDF_InitLibrary();
        let result = (|| {
            let path = project_root().join(&case.path);
            let document = bindings.FPDF_LoadDocument(
                path.to_str().ok_or("non-UTF8 PDF path")?,
                case.password.as_deref(),
            );
            if document.is_null() {
                return Err("glyph audit: cannot open PDF".into());
            }
            let result = (|| {
                for index in selected_pages(case, bindings.FPDF_GetPageCount(document) as usize)? {
                    let page = bindings.FPDF_LoadPage(document, index as i32);
                    if page.is_null() {
                        return Err("glyph audit: cannot open page".into());
                    }
                    let text = bindings.FPDFText_LoadPage(page);
                    let result = if text.is_null() {
                        Err("glyph audit: cannot load native text page".into())
                    } else {
                        let result = (|| {
                            let (width, height) =
                                page_dimensions(bindings.as_ref(), page, case.dpi)?;
                            let mut glyphs = Vec::new();
                            let mut seen = BTreeSet::new();
                            for char_index in 0..bindings.FPDFText_CountChars(text) {
                                let Some(unicode) =
                                    char::from_u32(bindings.FPDFText_GetUnicode(text, char_index))
                                else {
                                    continue;
                                };
                                if unicode.is_whitespace() || unicode.is_control() {
                                    continue;
                                }
                                let size = bindings.FPDFText_GetFontSize(text, char_index);
                                let mut font = [0u8; 256];
                                let mut flags = 0;
                                let length = bindings.FPDFText_GetFontInfo(
                                    text,
                                    char_index,
                                    font.as_mut_ptr().cast(),
                                    font.len() as _,
                                    &mut flags,
                                );
                                let font = if length > 0 && length <= font.len() as _ {
                                    String::from_utf8_lossy(&font[..length as usize - 1])
                                        .into_owned()
                                } else {
                                    "unknown".to_owned()
                                };
                                let key = (font.clone(), (size * 64.0).round() as i32, unicode);
                                if seen.contains(&key) {
                                    continue;
                                }
                                let (mut left, mut right, mut bottom, mut top) =
                                    (0.0, 0.0, 0.0, 0.0);
                                if bindings.FPDFText_GetCharBox(
                                    text,
                                    char_index,
                                    &mut left,
                                    &mut right,
                                    &mut bottom,
                                    &mut top,
                                ) == 0
                                {
                                    continue;
                                }
                                let (mut x0, mut y0, mut x1, mut y1) = (0, 0, 0, 0);
                                if bindings.FPDF_PageToDevice(
                                    page, 0, 0, width, height, 0, left, top, &mut x0, &mut y0,
                                ) == 0
                                    || bindings.FPDF_PageToDevice(
                                        page, 0, 0, width, height, 0, right, bottom, &mut x1,
                                        &mut y1,
                                    ) == 0
                                {
                                    continue;
                                }
                                let padding = (case.dpi / 72).max(2) as i32;
                                let x = (x0.min(x1).saturating_sub(padding)).clamp(0, width);
                                let y = (y0.min(y1).saturating_sub(padding)).clamp(0, height);
                                let end_x = (x0.max(x1).saturating_add(padding)).clamp(0, width);
                                let end_y = (y0.max(y1).saturating_add(padding)).clamp(0, height);
                                if end_x <= x || end_y <= y {
                                    continue;
                                }
                                seen.insert(key);
                                glyphs.push(Glyph {
                                    index: char_index,
                                    unicode,
                                    font,
                                    size,
                                    rect: (x as _, y as _, (end_x - x) as _, (end_y - y) as _),
                                });
                            }
                            // Round-robin font/size groups so a long body-text
                            // run cannot crowd out a rare script or italic face.
                            let mut groups: BTreeMap<_, VecDeque<_>> = BTreeMap::new();
                            for glyph in glyphs {
                                groups
                                    .entry((glyph.font.clone(), (glyph.size * 64.0).round() as i32))
                                    .or_default()
                                    .push_back(glyph);
                            }
                            let mut selected = Vec::new();
                            while selected.len() < 64 {
                                let before = selected.len();
                                for group in groups.values_mut() {
                                    if let Some(glyph) = group.pop_front() {
                                        selected.push(glyph);
                                    }
                                    if selected.len() == 64 {
                                        break;
                                    }
                                }
                                if selected.len() == before {
                                    break;
                                }
                            }
                            write_crops(case, index, &selected)
                        })();
                        bindings.FPDFText_ClosePage(text);
                        result
                    };
                    bindings.FPDF_ClosePage(page);
                    result?;
                }
                Ok(())
            })();
            bindings.FPDF_CloseDocument(document);
            result
        })();
        bindings.FPDF_DestroyLibrary();
        result
    }
}

fn ink(pixel: &Rgba<u8>) -> u8 {
    255 - pixel[0].min(pixel[1]).min(pixel[2])
}

// Dark pixels unsupported by any dark pixel within a 1-pixel neighborhood of
// the other render. Diagnostic only: nearby letters and clipping may contribute.
fn unsupported_ink(source: &RgbaImage, other: &RgbaImage) -> usize {
    let mut count = 0;
    for (x, y, pixel) in source.enumerate_pixels() {
        if ink(pixel) < 192 {
            continue;
        }
        let nearby = (y.saturating_sub(1)..=(y + 1).min(other.height() - 1)).any(|sy| {
            (x.saturating_sub(1)..=(x + 1).min(other.width() - 1))
                .any(|sx| ink(other.get_pixel(sx, sy)) >= 64)
        });
        count += usize::from(!nearby);
    }
    count
}

fn write_crops(case: &Case, page: usize, glyphs: &[Glyph]) -> Result<()> {
    let native = image::open(png(&output("native", case), page))?.into_rgba8();
    let rust = image::open(png(&output("rust", case), page))?.into_rgba8();
    if native.dimensions() != rust.dimensions() {
        return Err("glyph audit: image dimensions differ".into());
    }
    let directory = output("glyphs", case).join(format!("glyphs-{page}"));
    if directory.exists() {
        fs::remove_dir_all(&directory)?;
    }
    fs::create_dir_all(&directory)?;
    let mut csv = String::from("sample,char_index,unicode,font,size_pt,x,y,width,height,changed_pixels,mean_channel_difference,native_unsupported_dark,rust_unsupported_dark\n");
    // Each tile contains native | Rust | amplified difference, nearest-neighbor
    // magnified equally, with no alignment correction or interpolated smoothing.
    let cell_width = 96;
    let cell_height = 120;
    let columns = 4;
    let mut sheet = RgbaImage::from_pixel(
        columns * 3 * cell_width,
        (glyphs.len() as u32).div_ceil(columns).max(1) * cell_height,
        Rgba([230, 230, 230, 255]),
    );
    for (i, glyph) in glyphs.iter().enumerate() {
        let (x, y, width, height) = glyph.rect;
        let a = imageops::crop_imm(&native, x, y, width, height).to_image();
        let b = imageops::crop_imm(&rust, x, y, width, height).to_image();
        let (diff, changed, total) = difference(&a, &b);
        let mean = total as f64 / (f64::from(width) * f64::from(height) * 4.0);
        let prefix = format!("{i:02}-U{:04X}", glyph.unicode as u32);
        a.save(directory.join(format!("{prefix}-native.png")))?;
        b.save(directory.join(format!("{prefix}-rust.png")))?;
        diff.save(directory.join(format!("{prefix}-diff.png")))?;
        let mut amplified = diff;
        for pixel in amplified.pixels_mut() {
            for channel in &mut pixel.0 {
                *channel = channel.saturating_mul(16);
            }
        }
        let diff = image::DynamicImage::ImageRgb8(amplified).into_rgba8();
        let factor = ((cell_width - 4) / width)
            .min((cell_height - 4) / height)
            .clamp(1, 8);
        for (j, image) in [&a, &b, &diff].into_iter().enumerate() {
            let image = imageops::resize(
                image,
                (width * factor).min(cell_width - 4),
                (height * factor).min(cell_height - 4),
                imageops::FilterType::Nearest,
            );
            let offset_x = (i as u32 % columns) * 3 * cell_width + j as u32 * cell_width + 2;
            let offset_y = (i as u32 / columns) * cell_height + 2;
            imageops::overlay(&mut sheet, &image, i64::from(offset_x), i64::from(offset_y));
        }
        csv.push_str(&format!(
            "{i},{},U+{:04X},\"{}\",{:.4},{x},{y},{width},{height},{changed},{mean:.9},{},{}\n",
            glyph.index,
            glyph.unicode as u32,
            glyph.font.replace('"', "\"\""),
            glyph.size,
            unsupported_ink(&a, &b),
            unsupported_ink(&b, &a)
        ));
    }
    fs::write(directory.join("glyphs.csv"), csv)?;
    sheet.save(directory.join("contact-sheet.png"))?;
    println!(
        "{}/{page}: audited {} unique font/size/character samples at {} DPI; {}",
        case.id,
        glyphs.len(),
        case.dpi,
        directory.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_stroke_check_distinguishes_one_pixel_edge_shift_from_lost_ink() {
        let mut a = RgbaImage::from_pixel(8, 8, Rgba([255; 4]));
        let mut b = a.clone();
        for y in 1..7 {
            a.put_pixel(2, y, Rgba([0, 0, 0, 255]));
            b.put_pixel(3, y, Rgba([0, 0, 0, 255]));
        }
        assert_eq!(unsupported_ink(&a, &b), 0);
        assert_eq!(unsupported_ink(&b, &a), 0);
        let blank = RgbaImage::from_pixel(8, 8, Rgba([255; 4]));
        assert_eq!(unsupported_ink(&a, &blank), 6);
    }
}
