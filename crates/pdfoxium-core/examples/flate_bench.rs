// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Isolate the inflater from predictors, color conversion and resampling.
//! Usage: cargo run --release -p pdfoxium-core --example flate_bench -- PDF OBJECT ROW_BYTES [ITERATIONS]

use std::{error::Error, hint::black_box, rc::Rc, time::Instant};

use miniz_oxide::{inflate::stream::InflateState, DataFormat, MZFlush, MZStatus};
use pdfoxium_core::{
    fpdfapi::parser::{
        cpdf_document::CpdfDocument, cpdf_indirect_object_holder::CpdfIndirectObjectHolder,
        cpdf_parser::ParserError, cpdf_stream::CpdfStream,
    },
    fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream,
};
use zlib_rs::{Inflate, InflateFlush, Status};

fn decode(src: &[u8], row_bytes: usize, miniz: bool) -> (Vec<u8>, usize) {
    let mut output = Vec::new();
    let mut row = vec![0; row_bytes];
    let mut consumed = 0;
    if miniz {
        let mut state = InflateState::new_boxed(DataFormat::Zlib);
        loop {
            let mut written = 0;
            let status = loop {
                let result = miniz_oxide::inflate::stream::inflate(
                    &mut state,
                    &src[consumed..],
                    &mut row[written..],
                    MZFlush::Sync,
                );
                consumed += result.bytes_consumed;
                written += result.bytes_written;
                if result.status != Ok(MZStatus::Ok)
                    || written == row.len()
                    || (result.bytes_consumed == 0 && result.bytes_written == 0)
                {
                    break result.status;
                }
            };
            output.extend_from_slice(&row[..written]);
            if status != Ok(MZStatus::Ok) || written == 0 {
                break;
            }
        }
    } else {
        let mut state = Inflate::new(true, 15);
        loop {
            let before = state.total_out();
            let status = state.decompress(&src[consumed..], &mut row, InflateFlush::SyncFlush);
            consumed = state.total_in() as usize;
            let written = (state.total_out() - before) as usize;
            output.extend_from_slice(&row[..written]);
            if status != Ok(Status::Ok) || written == 0 {
                break;
            }
        }
    }
    (output, consumed)
}

fn main() -> Result<(), Box<dyn Error>> {
    if cfg!(debug_assertions) {
        return Err("use --release for benchmarks".into());
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .ok_or("expected PDF OBJECT ROW_BYTES [ITERATIONS]")?;
    let object = args.get(1).ok_or("missing object number")?.parse()?;
    let row_bytes: usize = args.get(2).ok_or("missing row bytes")?.parse()?;
    if row_bytes == 0 {
        return Err("row bytes must be positive".into());
    }
    let iterations: usize = args.get(3).map(|n| n.parse()).transpose()?.unwrap_or(15);
    if iterations == 0 {
        return Err("iterations must be positive".into());
    }
    let mut document = CpdfDocument::new();
    if document.load_doc(
        Rc::new(CfxReadOnlyContainerStream::new(std::fs::read(path)?)),
        b"",
    ) != ParserError::Success
    {
        return Err("could not load PDF".into());
    }
    let object = CpdfIndirectObjectHolder::get_or_parse_indirect_object(&document.holder(), object)
        .ok_or("missing object")?;
    let borrowed = object.borrow();
    let stream = borrowed
        .as_any()
        .downcast_ref::<CpdfStream>()
        .ok_or("object is not a stream")?;
    let src = stream.read_all_raw_data();
    let expected = decode(&src, row_bytes, true);
    assert_eq!(
        decode(&src, row_bytes, false),
        expected,
        "codec output/consumption differs"
    );
    println!(
        "{} compressed bytes, {} decoded bytes, {} bytes per output call",
        src.len(),
        expected.0.len(),
        row_bytes
    );
    // Alternate codec order to reduce systematic cache/order bias. Both get
    // a discarded warmup; allocation, output collection and checksum included.
    let mut samples = [Vec::new(), Vec::new()];
    for round in 0..=iterations {
        for index in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
            let start = Instant::now();
            let actual = black_box(decode(black_box(&src), row_bytes, index == 0));
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(actual, expected);
            if round != 0 {
                samples[index].push(elapsed);
            }
        }
    }
    for (name, times) in ["miniz", "zlib-rs"].into_iter().zip(&mut samples) {
        times.sort_by(f64::total_cmp);
        let median = (times[(times.len() - 1) / 2] + times[times.len() / 2]) / 2.0;
        println!("{name}: median {median:.3} ms ({iterations} iterations)");
    }
    Ok(())
}
