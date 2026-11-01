// Upstream pdfium-render 0.9.4 tests; MIT.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_point_transform() {
    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf().unwrap();

    let mut page = document
        .pages_mut()
        .create_page_at_start(PdfPagePaperSize::a4())
        .unwrap();

    let object = page
        .objects_mut()
        .create_path_object_line(
            PdfPoints::new(100.0),
            PdfPoints::new(200.0),
            PdfPoints::new(300.0),
            PdfPoints::new(400.0),
            PdfColor::BEIGE,
            PdfPoints::new(1.0),
        )
        .unwrap();

    let delta_x = PdfPoints::new(50.0);
    let delta_y = PdfPoints::new(-25.0);

    let matrix = PdfMatrix::identity().translate(delta_x, delta_y).unwrap();

    let raw_segment_0 = object.as_path_object().unwrap().segments().get(0).unwrap();
    let raw_segment_1 = object.as_path_object().unwrap().segments().get(1).unwrap();

    let transformed_segment_0 = object
        .as_path_object()
        .unwrap()
        .segments()
        .transform(matrix)
        .get(0)
        .unwrap();

    let transformed_segment_1 = object
        .as_path_object()
        .unwrap()
        .segments()
        .transform(matrix)
        .get(1)
        .unwrap();

    assert_eq!(transformed_segment_0.x(), raw_segment_0.x() + delta_x);
    assert_eq!(transformed_segment_0.y(), raw_segment_0.y() + delta_y);
    assert_eq!(transformed_segment_1.x(), raw_segment_1.x() + delta_x);
    assert_eq!(transformed_segment_1.y(), raw_segment_1.y() + delta_y);
}

#[test]
fn test_point_transform_during_iteration() {
    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf().unwrap();

    let mut page = document
        .pages_mut()
        .create_page_at_start(PdfPagePaperSize::a4())
        .unwrap();

    let object = page
        .objects_mut()
        .create_path_object_line(
            PdfPoints::new(100.0),
            PdfPoints::new(200.0),
            PdfPoints::new(300.0),
            PdfPoints::new(400.0),
            PdfColor::BEIGE,
            PdfPoints::new(1.0),
        )
        .unwrap();

    let raw_points: Vec<(PdfPoints, PdfPoints)> = object
        .as_path_object()
        .unwrap()
        .segments()
        .iter()
        .map(|segment| segment.point())
        .collect();

    let delta_x = PdfPoints::new(50.0);
    let delta_y = PdfPoints::new(-25.0);

    let matrix = PdfMatrix::identity().translate(delta_x, delta_y).unwrap();

    let transformed_points: Vec<(PdfPoints, PdfPoints)> = object
        .as_path_object()
        .unwrap()
        .segments()
        .transform(matrix)
        .iter()
        .map(|segment| segment.point())
        .collect();

    for (raw, transformed) in raw_points.iter().zip(transformed_points) {
        assert_eq!(transformed.0, raw.0 + delta_x);
        assert_eq!(transformed.1, raw.1 + delta_y);
    }
}
