// Copyright 2026 The PDFium Authors
// Use of this source code is governed by the BSD-style license in
// ../../../LICENSE-PDFIUM.

//! Independent per-pixel T.88 refinement contexts, without byte shift registers
//! or native's displacement reset. Both signs and the reference-height boundary
//! must retain the encoded offset and the following arithmetic stream position.
use super::*;
use alloc::vec;

fn reference_decode(
    bytes: &[u8],
    reference: &Bitmap,
    dy: i32,
    template: RefinementTemplate,
) -> (Bitmap, usize) {
    let mut output = Bitmap::new_pdfium(37, 7);
    let mut decoder = ArithmeticDecoder::new_with_pdfium(bytes, true);
    let mut contexts = vec![ArithmeticDecoderContext::default(); 1 << template.context_bits()];
    // (reference bitmap?, dx, dy), in context-bit order: T.88 figures 12/13.
    let t0 = [
        (true, 1, 1),
        (true, 0, 1),
        (true, -1, 1),
        (true, 1, 0),
        (true, 0, 0),
        (true, -1, 0),
        (true, 1, -1),
        (true, 0, -1),
        (true, -1, -1),
        (false, -1, 0),
        (false, 1, -1),
        (false, 0, -1),
        (false, -1, -1),
    ];
    let t1 = [
        (true, 1, 1),
        (true, 0, 1),
        (true, 1, 0),
        (true, 0, 0),
        (true, -1, 0),
        (true, 0, -1),
        (false, -1, 0),
        (false, 1, -1),
        (false, 0, -1),
        (false, -1, -1),
    ];
    let offsets = if template == RefinementTemplate::Template0 {
        &t0[..]
    } else {
        &t1[..]
    };
    for y in 0..7 {
        for x in 0..37 {
            let mut context = 0;
            for (bit, &(is_reference, dx, dy_pixel)) in offsets.iter().enumerate() {
                let image = if is_reference { reference } else { &output };
                let px = i64::from(x) + i64::from(dx);
                let py = i64::from(y) + i64::from(dy_pixel)
                    - if is_reference { i64::from(dy) } else { 0 };
                let value = if px >= 0
                    && py >= 0
                    && px < i64::from(image.width)
                    && py < i64::from(image.height)
                {
                    image.get_pixel(px as u32, py as u32)
                } else {
                    0
                };
                context |= usize::from(value) << bit;
            }
            let pixel = decoder.read_bitmap_bit(&mut contexts[context]).unwrap();
            output.set_pixel(x as u32, y as u32, pixel as u8);
        }
    }
    (output, decoder.byte_position())
}

#[test]
fn signed_refinement_offsets_keep_zero_extended_contexts_and_stream_position() {
    let bytes = [0x55; 512];
    let mut reference = Bitmap::new_pdfium(37, 2);
    for y in 0..2 {
        for x in 0..37 {
            reference.set_pixel(x, y, u8::from((11 * x + 17 * y) % 7 < 3));
        }
    }
    let at = [AdaptiveTemplatePixel { x: -1, y: -1 }; 2];
    for template in [RefinementTemplate::Template0, RefinementTemplate::Template1] {
        for dy in [i32::MIN, -7, -3, -2, -1, 0, 1, 2, 3, 7, i32::MAX] {
            let (expected, position) = reference_decode(&bytes, &reference, dy, template);
            let mut actual = Bitmap::new_pdfium(37, 7);
            let mut decoder = ArithmeticDecoder::new_with_pdfium(&bytes, true);
            let mut contexts =
                vec![ArithmeticDecoderContext::default(); 1 << template.context_bits()];
            decode_bitmap(
                &mut decoder,
                &mut contexts,
                &mut actual,
                &reference,
                0,
                dy,
                template,
                &at,
                false,
            )
            .unwrap();
            assert_eq!(actual.data, expected.data, "template={template:?}, dy={dy}");
            assert_eq!(
                decoder.byte_position(),
                position,
                "template={template:?}, dy={dy}"
            );
        }
    }
}
