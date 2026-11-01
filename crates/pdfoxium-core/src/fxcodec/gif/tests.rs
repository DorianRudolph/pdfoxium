// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
fn context(input: &[u8]) -> CfxGifContext {
    let mut result = CfxGifContext::default();
    result.set_input_buffer(CfxCodecMemory::from_bytes(input.to_vec()));
    result
}
#[test]
fn set_input_buffer() {
    for input in [&[][..], &[0, 1, 2][..0], &[0, 1, 2][..]] {
        let context = context(input);
        assert_eq!(context.input_buffer.size(), input.len());
        assert_eq!(context.input_buffer.position(), 0);
    }
}
#[test]
fn read_all_or_none() {
    assert!(!context(&[]).read_all_or_none(&mut []));
    let source = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    let mut output = [0; 10];
    let mut reader = context(&source[..1]);
    assert!(!reader.read_all_or_none(&mut output));
    assert_eq!(reader.input_buffer.position(), 0);
    assert!(reader.read_all_or_none(&mut output[..1]));
    assert_eq!(output[0], source[0]);
    let mut reader = context(&source);
    assert!(reader.read_all_or_none(&mut output));
    assert_eq!(output, source);
    let mut reader = context(&source);
    for byte in source {
        assert!(reader.read_all_or_none(&mut output[..1]));
        assert_eq!(output[0], byte);
    }
}
#[test]
fn read_gif_signature() {
    for input in [&[][..], b"GIF"] {
        let mut reader = context(input);
        assert_eq!(reader.read_gif_signature(), Status::Continue);
        assert_eq!(reader.input_buffer.position(), 0);
    }
    for input in [b"NOTGIF", b"GIF80a", b"GIF92a", b"GIF87a", b"GIF89a"] {
        let mut reader = context(input);
        assert_eq!(
            reader.read_gif_signature(),
            if input == b"GIF87a" || input == b"GIF89a" {
                Status::Success
            } else {
                Status::Error
            }
        );
        assert_eq!(reader.input_buffer.position(), 6);
    }
}
#[test]
fn read_local_screen_descriptor() {
    assert_eq!(
        context(&[]).read_logical_screen_descriptor(),
        Status::Continue
    );
    let mut zero = context(&[0; 7]);
    assert_eq!(zero.read_logical_screen_descriptor(), Status::Success);
    assert_eq!(zero.input_buffer.position(), 7);
    assert_eq!((zero.width, zero.height, zero.background_index), (0, 0, 0));
    let mut plain = context(&[10, 0, 0, 15, 0, 1, 2]);
    assert_eq!(plain.read_logical_screen_descriptor(), Status::Success);
    assert_eq!(
        (plain.width, plain.height, plain.background_index),
        (10, 0x0f00, 0)
    );
    assert_eq!(plain.input_buffer.position(), 7);
    let mut missing = context(&[10, 0, 0, 15, 0x80, 1, 2]);
    assert_eq!(missing.read_logical_screen_descriptor(), Status::Continue);
    assert_eq!(missing.input_buffer.position(), 0);
    let mut reader = context(&[10, 0, 0, 15, 0xa9, 1, 2, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1]);
    assert_eq!(reader.read_logical_screen_descriptor(), Status::Success);
    assert_eq!(reader.input_buffer.position(), 19);
    assert_eq!(
        (
            reader.width,
            reader.height,
            reader.background_index,
            reader.palette_exp,
            reader.sort,
            reader.color_resolution
        ),
        (10, 0xf00, 1, 1, true, 2)
    );
    assert_eq!(
        reader.global_palette,
        [0xff000000, 0xff010101, 0xff010000, 0xff000001]
    );
}
#[test]
fn read_header_native() {
    let mut invalid = context(b"NOTGIF\x0a\x00\x00\x0f\x00\x01\x02");
    assert_eq!(invalid.read_header(), Status::Error);
    assert_eq!(invalid.input_buffer.position(), 6);
    let mut short = context(b"GIF87a");
    assert_eq!(short.read_header(), Status::Continue);
    assert_eq!(short.input_buffer.position(), 6);
    let mut plain = context(b"GIF87a\x0a\x00\x00\x0f\x00\x01\x02");
    assert_eq!(plain.read_header(), Status::Success);
    assert_eq!(plain.input_buffer.position(), 13);
    assert_eq!(
        (plain.width, plain.height, plain.background_index),
        (10, 0xf00, 0)
    );
    let mut missing = context(b"GIF87a\x0a\x00\x00\x0f\x80\x01\x02");
    assert_eq!(missing.read_header(), Status::Continue);
    assert_eq!(missing.input_buffer.position(), 6);
    let mut palette = context(
        b"GIF87a\x0a\x00\x00\x0f\xa9\x01\x02\x00\x00\x00\x01\x01\x01\x01\x00\x00\x00\x00\x01",
    );
    assert_eq!(palette.read_header(), Status::Success);
    assert_eq!(palette.input_buffer.position(), 25);
    assert_eq!(
        (
            palette.width,
            palette.height,
            palette.background_index,
            palette.palette_exp,
            palette.sort,
            palette.color_resolution
        ),
        (10, 0xf00, 1, 1, true, 2)
    );
    assert_eq!(
        palette.global_palette,
        [0xff000000, 0xff010101, 0xff010000, 0xff000001]
    );
}
#[test]
fn frames_extensions_transparency_local_palette_and_interlace() {
    let mut source = b"GIF89a\x01\x00\x02\x00\x80\x00\x00\x00\x00\x00\xff\x00\x00".to_vec();
    source.extend_from_slice(&[
        0x21, 0xfe, 3, b'a', b'b', b'c', 0, 0x21, 0xf9, 4, 1, 5, 0, 0, 0, 0x2c, 0, 0, 0, 0, 1, 0,
        2, 0, 0xc0, 0, 0, 0, 0, 255, 0, 2, 2, 0x44, 0x0a, 0, 0x3b,
    ]);
    let header = read_header(&source).unwrap();
    let frames = get_frames(&source, &header).unwrap();
    assert_eq!(frames.len(), 1);
    assert!(frames[0].interlaced);
    assert_eq!(frames[0].control.as_ref().unwrap().delay, 5);
    let image = decode_frame(&source, &header, &frames[0], true).unwrap();
    assert_eq!(image.info.palette, [0, 0xff00ff00]);
    assert_eq!(image.pixels, [0, 1]);
    source.pop();
    source.extend_from_slice(&[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 1, 0, 0x3b]);
    assert_eq!(
        get_frames(&source, &read_header(&source).unwrap())
            .unwrap()
            .len(),
        2
    );
}
