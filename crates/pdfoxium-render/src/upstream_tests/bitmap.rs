// Upstream pdfium-render 0.9.4 safe API tests; MIT.
use crate::prelude::*;
fn create_sized_buffer(size: usize) -> Vec<u8> {
    vec![0; size]
}
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_from_bytes() -> Result<(), PdfiumError> {
    let _pdfium = test_bind_to_pdfium();

    let test_width = 157;
    let test_height = 300;

    let mut buffer = create_sized_buffer(PdfBitmap::bytes_required_for_size_and_format(
        test_width,
        test_height,
        PdfBitmapFormat::BGR,
    ) as usize);

    let buffer_ptr = buffer.as_ptr();
    let buffer_len = buffer.len();

    let bitmap = PdfBitmap::from_bytes(
        test_width,
        test_height,
        PdfBitmapFormat::BGR,
        buffer.as_mut_slice(),
    )?;

    assert_eq!(bitmap.width(), test_width);
    assert_eq!(bitmap.height(), test_height);
    assert_eq!(bitmap.data.as_ptr() as usize, buffer_ptr as usize);

    // Check that we can copy out of the buffer without a segfault
    let raw_bytes = bitmap.as_raw_bytes();
    assert_eq!(raw_bytes.len(), buffer_len);

    Ok(())
}

#[test]
fn test_from_bytes_errors() {
    test_bind_to_pdfium();

    let mut buffer = create_sized_buffer(10);

    // error: stride is too large
    let result = PdfBitmap::from_bytes(1 << 30, 10, PdfBitmapFormat::BGR, buffer.as_mut_slice());
    assert!(result.is_err());
    drop(result);

    // error: required buffer is too large
    let result = PdfBitmap::from_bytes(10, 1 << 30, PdfBitmapFormat::BGR, buffer.as_mut_slice());
    assert!(result.is_err());
    drop(result);

    // error: provided buffer is too small
    let result = PdfBitmap::from_bytes(1000, 2000, PdfBitmapFormat::BGR, buffer.as_mut_slice());
    assert!(result.is_err());
    drop(result);
}

#[test]
fn test_preferred_stride_bytes() -> () {
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(0, PdfBitmapFormat::Gray),
        Some(0)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(0, PdfBitmapFormat::BGR),
        Some(0)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(0, PdfBitmapFormat::BGRx),
        Some(0)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(0, PdfBitmapFormat::BGRA),
        Some(0)
    );

    assert_eq!(
        PdfBitmap::preferred_stride_bytes(1, PdfBitmapFormat::Gray),
        Some(4)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(1, PdfBitmapFormat::BGR),
        Some(4)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(1, PdfBitmapFormat::BGRx),
        Some(4)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(1, PdfBitmapFormat::BGRA),
        Some(4)
    );

    assert_eq!(
        PdfBitmap::preferred_stride_bytes(2, PdfBitmapFormat::Gray),
        Some(4)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(2, PdfBitmapFormat::BGR),
        Some(8)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(2, PdfBitmapFormat::BGRx),
        Some(8)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(2, PdfBitmapFormat::BGRA),
        Some(8)
    );

    assert_eq!(
        PdfBitmap::preferred_stride_bytes(3, PdfBitmapFormat::Gray),
        Some(4)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(3, PdfBitmapFormat::BGR),
        Some(12)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(3, PdfBitmapFormat::BGRx),
        Some(12)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(3, PdfBitmapFormat::BGRA),
        Some(12)
    );

    assert_eq!(
        PdfBitmap::preferred_stride_bytes(4, PdfBitmapFormat::Gray),
        Some(4)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(4, PdfBitmapFormat::BGR),
        Some(12)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(4, PdfBitmapFormat::BGRx),
        Some(16)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(4, PdfBitmapFormat::BGRA),
        Some(16)
    );

    assert_eq!(
        PdfBitmap::preferred_stride_bytes(5, PdfBitmapFormat::Gray),
        Some(8)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(5, PdfBitmapFormat::BGR),
        Some(16)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(5, PdfBitmapFormat::BGRx),
        Some(20)
    );
    assert_eq!(
        PdfBitmap::preferred_stride_bytes(5, PdfBitmapFormat::BGRA),
        Some(20)
    );

    assert_eq!(
        PdfBitmap::preferred_stride_bytes(1 << 30, PdfBitmapFormat::BGRA),
        None
    );
}

#[test]
fn test_bytes_required_for_size_and_format() {
    // simple case: stride * height
    assert_eq!(
        PdfBitmap::bytes_required_for_size_and_format(1, 100, PdfBitmapFormat::Gray),
        400
    );
    assert_eq!(
        PdfBitmap::bytes_required_for_size_and_format(4, 100, PdfBitmapFormat::Gray),
        400
    );
    assert_eq!(
        PdfBitmap::bytes_required_for_size_and_format(256, 256, PdfBitmapFormat::BGR),
        256 * 256 * 3
    );

    // if stride estimation fails, fall back to 4 bytes per pixel
    assert_eq!(
        PdfBitmap::bytes_required_for_size_and_format(1 << 30, 50, PdfBitmapFormat::BGRA),
        (1 << 30 as usize) * 50 * 4,
    );
}
