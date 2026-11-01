use pdfoxium::{DestinationView, Document, FormType, LinkTarget, Rotation};
use std::io::{self, Write};

fn fixture() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R /Names << /EmbeddedFiles << /Names [(payload.txt) 10 0 R] >> /Dests << /Names [(named) [3 0 R /FitV null]] >> >> /PageLabels << /Nums [0 << /S /r /P (Intro-) >>] >> /AcroForm << /Fields [] >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",
        "<< /Type /Page /Parent 2 0 R /CropBox [10 20 110 220] /Rotate 90 /UserUnit 2 /Annots [null 4 0 R 5 0 R 12 0 R 13 0 R 14 0 R] >>",
        "<< /Type /Annot /Subtype /Link /Rect [10 20 30 40] /A << /S /URI /URI (https://example.org/) >> /Contents (web) >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 20 60 40] /Dest [3 0 R /XYZ null 42 0] >>",
        "<< /Type /Outlines /First 7 0 R /Last 8 0 R >>",
        "<< /Title (First) /Parent 6 0 R /Next 8 0 R /First 9 0 R /Count 1 /F 3 /C [1 0 0] /Dest [3 0 R /FitH null] >>",
        "<< /Title (Second) /Parent 6 0 R /Next 7 0 R /A << /S /Named /N /NextPage >> >>",
        "<< /Title (Child) /Parent 7 0 R /First 7 0 R >>",
        "<< /Type /Filespec /F (payload.txt) /UF (payload.txt) /Desc (An attachment) /EF << /F 11 0 R >> >>",
        "<< /Type /EmbeddedFile /Length 11 >>\nstream\nhello world\nendstream",
        "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /Dest (named) >>",
        "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /A << /S /Launch /F (other.pdf) >> >>",
        "<< /Type /Annot /Subtype /Text /Rect [1 2 3 4] /F 4 /Contents <FEFFD83DDE00> >>",
        "<< /Title (Title) /Author (Author) /CreationDate (D:20261026+01'00') /Subject <FEFFD83DDE00> >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 15 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    bytes
}

#[test]
fn read_metadata_layout_navigation_and_lazy_attachment() {
    let doc = Document::from_bytes(fixture()).unwrap();
    let metadata = doc.metadata().unwrap();
    assert_eq!(metadata.title.as_deref(), Some("Title"));
    assert_eq!(metadata.subject.as_deref(), Some("😀"));
    assert_eq!(metadata.creation_date.as_deref(), Some("D:20261026+01'00'"));
    assert_eq!(metadata.producer, None);
    assert_eq!(doc.form_type().unwrap(), FormType::AcroForm);
    assert!(doc.permissions().unwrap().can_copy());
    let page = doc.page(0).unwrap();
    let layout = page.layout().unwrap();
    assert_eq!((layout.size.width, layout.size.height), (200.0, 100.0));
    assert_eq!(layout.rotation, Rotation::Cw90);
    assert_eq!(
        (
            layout.crop.left,
            layout.crop.bottom,
            layout.crop.right,
            layout.crop.top
        ),
        (10.0, 20.0, 110.0, 220.0)
    );
    assert_eq!(layout.label.as_deref(), Some("Intro-i"));
    assert_eq!(doc.page_layouts().unwrap(), vec![layout]);
    let links = page.links().unwrap();
    assert_eq!(links.len(), 4);
    assert_eq!(links[0].annotation_index, 1);
    assert_eq!(
        links[0].target,
        Some(LinkTarget::Uri(b"https://example.org/".to_vec()))
    );
    let Some(LinkTarget::Destination(dest)) = &links[1].target else {
        panic!("missing destination")
    };
    assert_eq!(dest.page_index, 0);
    assert_eq!(
        dest.view,
        DestinationView::Xyz {
            left: None,
            top: Some(42.0),
            zoom: None
        }
    );
    let Some(LinkTarget::NamedDestination { name, resolved }) = &links[2].target else {
        panic!("missing named destination")
    };
    assert_eq!(name, b"named");
    assert_eq!(
        resolved.as_ref().unwrap().view,
        DestinationView::FitV { left: None }
    );
    assert_eq!(
        links[3].target,
        Some(LinkTarget::UnsupportedAction {
            kind: b"Launch".to_vec(),
            file: Some("other.pdf".into())
        })
    );
    let annots = page.annotations().unwrap();
    assert_eq!(annots.last().unwrap().annotation_index, 5);
    assert_eq!(annots.last().unwrap().contents.as_deref(), Some("😀"));
    let model = page.model().unwrap();
    assert_eq!(model.links, links);
    assert_eq!(model.annotations, annots);
    assert_eq!(model.id, page.id());
    let outline = doc.outline().unwrap();
    assert_eq!(
        outline
            .iter()
            .map(|i| (i.depth, i.title.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "First"), (1, "Child"), (0, "Second")]
    );
    assert!(outline[0].bold && outline[0].italic);
    assert_eq!(outline[0].color, Some([1.0, 0.0, 0.0]));
    let Some(LinkTarget::Destination(dest)) = &outline[0].target else {
        panic!("missing destination")
    };
    assert_eq!(dest.view, DestinationView::FitH { top: None });
    let mut attachments = doc.attachments().unwrap();
    assert_eq!(attachments.len(), 1);
    let attachment = attachments.pop().unwrap();
    assert_eq!(attachment.name(), "payload.txt");
    assert_eq!(attachment.description(), "An attachment");
    page.unload().unwrap();
    drop(page);
    drop(doc);
    assert_eq!(attachment.read().unwrap(), b"hello world");
    let mut output = Vec::new();
    attachment.save_to(&mut output).unwrap();
    assert_eq!(output, b"hello world");
    assert_eq!(links[0].annotation_index, 1); // owned results survive page/document drop
}

#[test]
fn attachment_writer_reentry_and_errors_are_reported() {
    let doc = Document::from_bytes(fixture()).unwrap();
    let attachment = doc.attachments().unwrap().remove(0);
    struct Writer(Document);
    impl Write for Writer {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            assert!(matches!(self.0.metadata(), Err(pdfoxium::Error::Busy)));
            Err(io::Error::other("writer failed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let error = attachment.save_to(&mut Writer(doc.clone())).unwrap_err();
    assert_eq!(error.to_string(), "writer failed");
    assert_eq!(doc.metadata().unwrap().title.as_deref(), Some("Title"));
}
