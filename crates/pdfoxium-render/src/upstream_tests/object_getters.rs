// Upstream pdfium-render 0.9.4 tests; MIT.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_object_get_translation() -> Result<(), PdfiumError> {
    // Tests to make sure we can retrieve the correct horizontal and vertical translation deltas
    // from an object after applying a translation transformation.

    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document
        .pages_mut()
        .create_page_at_start(PdfPagePaperSize::a4())?;

    let mut object = PdfPagePathObject::new_rect(
        &document,
        PdfRect::new_from_values(100.0, 100.0, 400.0, 400.0),
        Some(PdfColor::RED),
        Some(PdfPoints::new(1.0)),
        None,
    )?;

    object.translate(PdfPoints::new(250.0), PdfPoints::new(350.0))?;

    let object = page.objects_mut().add_path_object(object)?;

    assert_eq!(object.get_horizontal_translation().value, 250.0);
    assert_eq!(object.get_vertical_translation().value, 350.0);
    assert_eq!(object.get_horizontal_scale(), 1.0);
    assert_eq!(object.get_vertical_scale(), 1.0);
    assert_eq!(object.get_rotation_clockwise_degrees(), 0.0);

    Ok(())
}

#[test]
fn test_object_get_scale() -> Result<(), PdfiumError> {
    // Tests to make sure we can retrieve the correct horizontal and vertical scale factors
    // from an object after applying a scale transformation.

    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document
        .pages_mut()
        .create_page_at_start(PdfPagePaperSize::a4())?;

    let mut object = PdfPagePathObject::new_rect(
        &document,
        PdfRect::new_from_values(100.0, 100.0, 400.0, 400.0),
        Some(PdfColor::RED),
        Some(PdfPoints::new(1.0)),
        None,
    )?;

    object.scale(1.75, 2.25)?;

    let object = page.objects_mut().add_path_object(object)?;

    assert_eq!(object.get_horizontal_scale(), 1.75);
    assert_eq!(object.get_vertical_scale(), 2.25);
    assert_eq!(object.get_horizontal_translation().value, 0.0);
    assert_eq!(object.get_vertical_translation().value, 0.0);
    assert_eq!(object.get_rotation_clockwise_degrees(), 0.0);

    Ok(())
}

#[test]
fn test_object_get_rotation() -> Result<(), PdfiumError> {
    // Tests to make sure we can retrieve the correct clockwise rotation angle from an object
    // after applying a rotation transformation.

    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document
        .pages_mut()
        .create_page_at_start(PdfPagePaperSize::a4())?;

    let mut object = PdfPagePathObject::new_rect(
        &document,
        PdfRect::new_from_values(100.0, 100.0, 400.0, 400.0),
        Some(PdfColor::RED),
        Some(PdfPoints::new(1.0)),
        None,
    )?;

    object.rotate_clockwise_degrees(35.0)?;

    let object = page.objects_mut().add_path_object(object)?;

    assert_eq!(object.get_rotation_clockwise_degrees(), 35.0);
    assert_eq!(object.get_horizontal_translation().value, 0.0);
    assert_eq!(object.get_vertical_translation().value, 0.0);
    assert_eq!(object.get_horizontal_scale(), 0.8191520571708679); // Rotating affects the scale factors
    assert_eq!(object.get_vertical_scale(), 0.8191520571708679); // Rotating affects the scale factors

    Ok(())
}

#[test]
fn test_object_get_skew() -> Result<(), PdfiumError> {
    // Tests to make sure we can retrieve the correct skew axes values from an object
    // after applying a skew transformation.

    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document
        .pages_mut()
        .create_page_at_start(PdfPagePaperSize::a4())?;

    let mut object = PdfPagePathObject::new_rect(
        &document,
        PdfRect::new_from_values(100.0, 100.0, 400.0, 400.0),
        Some(PdfColor::RED),
        Some(PdfPoints::new(1.0)),
        None,
    )?;

    object.skew_degrees(15.5, 25.5)?;

    let object = page.objects_mut().add_path_object(object)?;

    assert_eq!(
        (object.get_x_axis_skew_degrees() * 10.0).round() / 10.0,
        15.5
    ); // Handles the returned value being a tiny bit off, e.g. 15.4999 instead of 15.5
    assert_eq!(
        (object.get_y_axis_skew_degrees() * 10.0).round() / 10.0,
        25.5
    ); // Handles the returned value being a tiny bit off, e.g. 25.4999 instead of 25.5
    assert_eq!(object.get_horizontal_translation().value, 0.0);
    assert_eq!(object.get_vertical_translation().value, 0.0);
    assert_eq!(object.get_horizontal_scale(), 1.0);
    assert_eq!(object.get_vertical_scale(), 1.0);
    assert_eq!(
        (object.get_rotation_counter_clockwise_degrees() * 10.0).round() / 10.0,
        15.5
    ); // Rotation angle will be the same as the x axis skew angle.

    Ok(())
}
