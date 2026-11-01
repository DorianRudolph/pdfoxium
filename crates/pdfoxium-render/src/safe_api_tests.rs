use crate::prelude::*;

fn document(engine: &Pdfium) -> PdfDocument<'_> {
    // Native handles retain their document internally; the public lifetime is tied to Pdfium.
    engine.create_new_pdf().unwrap()
}

#[test]
fn typed_objects_copy_resources_and_survive_source_drop() -> Result<(), PdfiumError> {
    let engine = Pdfium::default();
    let mut source = engine.create_new_pdf()?;
    let mut destination = engine.create_new_pdf()?;
    let font = source.fonts_mut().load_true_type_from_bytes(
        include_bytes!("../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"),
        false,
    )?;
    let mut source_page = source
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    let text = PdfPageTextObject::new(&source, "copied resources", font, PdfPoints::new(18.))?;
    let mut text = source_page.objects_mut().add_text_object(text)?;
    text.translate(PdfPoints::new(40.), PdfPoints::new(80.))?;
    let mut target = destination
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    assert_eq!(
        source_page.text()?.all().trim(),
        "copied resources",
        "source extraction"
    );
    let copied = text.copy_to_page(&mut target)?;
    assert!(copied
        .as_x_object_form_object()
        .unwrap()
        .first()?
        .as_text_object()
        .is_some());
    fn collection_len<'a>(objects: &impl PdfPageObjectsCommon<'a>) -> usize {
        objects.len()
    }
    assert_eq!(collection_len(copied.as_x_object_form_object().unwrap()), 1);
    assert_eq!(target.text()?.all().trim(), "copied resources");
    let source_state = std::rc::Rc::downgrade(&source.inner.inner);
    drop(text);
    drop(source_page);
    drop(source);
    assert!(source_state.upgrade().is_none());
    let bytes = destination.save_to_bytes()?;
    let reopened = engine.load_pdf_from_byte_vec(bytes, None)?;
    assert!(reopened
        .pages()
        .first()?
        .text()?
        .all()
        .contains("copied resources"));
    Ok(())
}

#[test]
fn independently_retrieved_objects_share_removal_and_relocation() -> Result<(), PdfiumError> {
    let engine = Pdfium::default();
    let mut document = document(&engine);
    let mut first = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    let mut second = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    first.objects_mut().create_path_object_rect(
        PdfRect::new_from_values(10., 20., 30., 40.),
        None,
        None,
        Some(PdfColor::RED),
    )?;
    let mut alias = first.objects().first()?;
    let removed = first.objects_mut().remove_object_at_index(0)?;
    assert!(first.objects().is_empty());
    alias.translate(PdfPoints::new(5.), PdfPoints::ZERO)?;
    second.objects_mut().add_object(removed)?;
    alias.set_fill_color(PdfColor::BLUE)?;
    assert_eq!(second.objects().first()?.fill_color()?.blue(), 255);
    assert_eq!(
        second.objects().first()?.bounds()?.left(),
        PdfPoints::new(25.)
    );
    Ok(())
}

#[test]
fn page_deletion_invalidates_all_existing_object_wrappers() -> Result<(), PdfiumError> {
    let engine = Pdfium::default();
    let mut document = document(&engine);
    let mut page = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    let mut object = page.objects_mut().create_path_object_rect(
        PdfRect::new_from_values(0., 0., 10., 10.),
        None,
        None,
        Some(PdfColor::RED),
    )?;
    document.pages().first()?.delete()?;
    document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    assert!(matches!(
        object.translate(PdfPoints::ZERO, PdfPoints::ZERO),
        Err(PdfiumError::OwnershipNotAttachedToPage)
    ));
    assert!(matches!(
        page.text(),
        Err(PdfiumError::OwnershipNotAttachedToPage)
    ));
    Ok(())
}

#[test]
fn path_construction_boundaries_and_regeneration_survive_reopen() -> Result<(), PdfiumError> {
    let engine = Pdfium::default();
    let mut document = engine.create_new_pdf()?;
    let mut page = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
    let path = PdfPagePathObject::new_ellipse_at(
        &document,
        PdfPoints::new(50.),
        PdfPoints::new(60.),
        PdfPoints::new(20.),
        PdfPoints::new(10.),
        None,
        None,
        Some(PdfColor::RED),
    )?;
    assert!(path.segments().len() >= 13);
    page.objects_mut().add_path_object(path)?;
    page.boundaries_mut()
        .set_crop(PdfRect::new_from_values(10., 20., 200., 300.))?;
    page.regenerate_content()?;
    let reopened = engine.load_pdf_from_byte_vec(document.save_to_bytes()?, None)?;
    let page = reopened.pages().first()?;
    assert_eq!(page.objects().len(), 1);
    assert_eq!(
        page.boundaries().crop()?.bounds,
        PdfRect::new_from_values(10., 20., 200., 300.)
    );
    assert_eq!(page.objects().first()?.fill_color()?.red(), 255);
    Ok(())
}

#[test]
fn created_annotations_links_quads_and_deletion_round_trip() -> Result<(), PdfiumError> {
    let engine = Pdfium::default();
    let mut document = engine.create_new_pdf()?;
    let mut page = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    let mut link = page
        .annotations_mut()
        .create_link_annotation("https://example.com/")?;
    link.set_bounds(PdfRect::new_from_values(10., 20., 30., 80.))?;
    assert_eq!(
        link.link()?
            .action()
            .unwrap()
            .as_uri_action()
            .unwrap()
            .uri()?,
        "https://example.com/"
    );
    let mut highlight = page.annotations_mut().create_highlight_annotation()?;
    highlight.set_contents("review")?;
    highlight
        .attachment_points_mut()
        .create_attachment_point_at_end(
            PdfRect::new_from_values(40., 20., 60., 80.).to_quad_points(),
        )?;
    assert_eq!(highlight.attachment_points().len(), 1);
    let reopened = engine.load_pdf_from_byte_vec(document.save_to_bytes()?, None)?;
    let mut page = reopened.pages().first()?;
    assert_eq!(page.links().len(), 1);
    assert_eq!(page.annotations().len(), 2);
    let annotations = page.annotations_mut();
    let annotation = annotations.get(1)?;
    assert_eq!(annotation.contents().as_deref(), Some("review"));
    annotations.delete_annotation(annotation)?;
    assert_eq!(page.annotations().len(), 1);
    Ok(())
}

#[test]
fn spell_check_and_scrolling_flags_are_independent() -> Result<(), PdfiumError> {
    let engine = Pdfium::default();
    let document = engine.load_pdf_from_file("test/form-test.pdf", None)?;
    let page = document.pages().first()?;
    let mut annotation = page
        .annotations()
        .iter()
        .find(|a| a.as_form_field().is_some())
        .unwrap();
    let field = annotation
        .as_form_field_mut()
        .unwrap()
        .as_text_field_mut()
        .unwrap();
    assert!(field.is_spell_checked());
    assert!(!field.is_scrollable());
    field.set_is_spell_checked(false)?;
    assert!(!field.is_spell_checked());
    assert!(!field.is_scrollable());
    field.set_is_scrollable(true)?;
    assert!(!field.is_spell_checked());
    assert!(field.is_scrollable());
    Ok(())
}

#[test]
fn custom_font_provider_reentry_replacement_and_clear() -> Result<(), PdfiumError> {
    use std::sync::{Arc, Mutex};
    struct Provider {
        calls: Arc<Mutex<Vec<(bool, bool)>>>,
        replacement_calls: Arc<Mutex<Vec<(bool, bool)>>>,
        replace: bool,
    }
    impl PdfiumCustomFontProvider for Provider {
        fn provide(
            &mut self,
            request: PdfiumCustomFontProviderRequest,
        ) -> Option<PdfiumCustomFontProviderResponse> {
            self.calls
                .lock()
                .unwrap()
                .push((request.is_serif, request.is_cursive));
            let mut engine = Pdfium::default();
            // Re-enter core while the outer mapper is executing this callback.
            let mut document = engine.create_new_pdf().unwrap();
            document
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::a4())
                .unwrap();
            assert!(!document.save_to_bytes().unwrap().is_empty());
            if self.replace {
                engine.set_custom_font_provider(Box::new(Provider {
                    calls: self.replacement_calls.clone(),
                    replacement_calls: self.replacement_calls.clone(),
                    replace: false,
                }));
            }
            Some(PdfiumCustomFontProviderResponse {
                id: 17,
                font_face: request.font_face,
                character_set: request.character_set,
                data: include_bytes!(
                    "../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
                )
                .to_vec(),
            })
        }
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let replacement_calls = Arc::new(Mutex::new(Vec::new()));
    let mut engine = Pdfium::default();
    engine.set_custom_font_provider(Box::new(Provider {
        calls: calls.clone(),
        replacement_calls: replacement_calls.clone(),
        replace: true,
    }));
    use pdfoxium_core::fxge::{cfx_gemodule::with_ge_module, cfx_substfont::CfxSubstFont};
    let lookup = |name: &[u8]| {
        with_ge_module(|m| {
            m.font_mgr().builtin_mapper_mut().find_subst_face(
                name,
                true,
                0,
                400,
                0,
                pdfoxium_core::fxcrt::fx_codepage::FxCodePage::DefAnsi,
                &mut CfxSubstFont::default(),
            )
        })
    };
    let _ = lookup(b"AdapterReentrySerif");
    let _ = lookup(b"AdapterReplacementSerif");
    assert!(!calls.lock().unwrap().is_empty());
    assert!(!replacement_calls.lock().unwrap().is_empty());
    engine.clear_custom_font_provider();
    assert!(!with_ge_module(|m| m
        .font_mgr()
        .builtin_mapper()
        .has_system_font_info()));
    engine.use_platform_default_font_provider()?;
    assert!(with_ge_module(|m| m
        .font_mgr()
        .builtin_mapper()
        .has_system_font_info()));
    Ok(())
}

#[test]
fn annotation_appearance_objects_round_trip_and_invalidate_on_removal() -> Result<(), PdfiumError> {
    let engine = Pdfium::default();
    let mut document = engine.create_new_pdf()?;
    let mut page = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    let mut stamp = page.annotations_mut().create_stamp_annotation()?;
    stamp.set_bounds(PdfRect::new_from_values(10., 20., 80., 120.))?;
    let mut object = stamp.objects_mut().create_path_object_rect(
        PdfRect::new_from_values(10., 20., 40., 60.),
        None,
        None,
        Some(PdfColor::RED),
    )?;
    assert_eq!(stamp.objects().len(), 1);
    drop(stamp);
    let stamp = page.annotations_mut().first()?;
    let alias = stamp.objects().first()?;
    object.set_fill_color(PdfColor::BLUE)?;
    assert_eq!(alias.fill_color()?.blue(), 255);
    let reopened = engine.load_pdf_from_byte_vec(document.save_to_bytes()?, None)?;
    assert_eq!(
        reopened
            .pages()
            .first()?
            .annotations()
            .first()?
            .objects()
            .len(),
        1
    );
    page.annotations_mut().delete_annotation(stamp.into())?;
    assert!(matches!(
        object.set_fill_color(PdfColor::RED),
        Err(PdfiumError::OwnershipNotAttachedToAnnotation)
    ));
    Ok(())
}

#[test]
fn font_queries_reuse_the_existing_token() -> Result<(), PdfiumError> {
    let engine = Pdfium::default();
    let mut document = engine.create_new_pdf()?;
    let font = document.fonts_mut().courier();
    let mut page = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;
    let object = page.objects_mut().create_text_object(
        PdfPoints::new(10.),
        PdfPoints::new(20.),
        "stable",
        font,
        PdfPoints::new(12.),
    )?;
    let text = object.as_text_object().unwrap();
    let count = document.inner.inner.fonts.borrow().len();
    for _ in 0..20 {
        assert_eq!(text.font().token(), font);
    }
    assert_eq!(document.inner.inner.fonts.borrow().len(), count);
    Ok(())
}

#[cfg(feature = "image_api")]
#[test]
fn gray_image_removes_stride_padding_and_preserves_pixel_format() -> Result<(), PdfiumError> {
    let mut bytes = [1, 2, 3, 99, 4, 5, 6, 99];
    let bitmap = PdfBitmap::from_bytes(3, 2, PdfBitmapFormat::Gray, &mut bytes)?;
    let image = bitmap.as_image()?;
    assert!(matches!(image, image::DynamicImage::ImageLuma8(_)));
    assert_eq!(image.as_bytes(), &[1, 2, 3, 4, 5, 6]);
    assert_eq!(bitmap.as_rgba_bytes().len(), 24);
    Ok(())
}
