// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
struct Availability {
    ranges: RefCell<Vec<(i64, i64)>>,
}
impl FileAvail for Availability {
    fn is_data_avail(&self, offset: i64, size: usize) -> bool {
        {
            let mut ranges = self.ranges.borrow().clone();
            ranges.sort_unstable();
            let mut end = offset;
            for (start, stop) in ranges {
                if start > end {
                    break;
                }
                if stop > end {
                    end = stop;
                }
                if end >= offset + size as i64 {
                    return true;
                }
            }
            false
        }
    }
}
struct Hints(Rc<RefCell<Vec<(i64, usize)>>>);
impl DownloadHints for Hints {
    fn add_segment(&mut self, offset: i64, size: usize) {
        self.0.borrow_mut().push((offset, size));
    }
}
fn fixture() -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../upstream/pdfium/testing/resources/feature_linearized_loading.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
#[test]
fn progressive_linearized_document_pages_and_weak_document_teardown() {
    let bytes = fixture();
    let length = bytes.len() as i64;
    let available = Rc::new(Availability {
        ranges: RefCell::new(Vec::new()),
    });
    let callback: Rc<dyn FileAvail> = available.clone();
    let mut avail = CpdfDataAvail::new(
        Some(&callback),
        Rc::new(CfxReadOnlyContainerStream::new(bytes)),
    );
    let requests = Rc::new(RefCell::new(Vec::new()));
    let hints: Rc<RefCell<dyn DownloadHints>> = Rc::new(RefCell::new(Hints(requests.clone())));
    assert_eq!(
        avail.is_linearized_pdf(),
        DocLinearizationStatus::LinearizationUnknown
    );
    let mut polls = 0;
    loop {
        let result = avail.is_doc_avail(Some(&hints));
        if result == DataAvailable {
            break;
        }
        assert_eq!(result, DataNotAvailable);
        let pending = std::mem::take(&mut *requests.borrow_mut());
        assert!(!pending.is_empty());
        // Loading the linearized document requires its first section, not a
        // synthetic entire-file fallback.
        for (offset, size) in pending {
            assert!(!(offset == 0 && size as i64 == length));
            available
                .ranges
                .borrow_mut()
                .push((offset, offset + size as i64));
        }
        polls += 1;
        assert!(polls < 20);
    }
    assert_eq!(
        avail.is_linearized_pdf(),
        DocLinearizationStatus::Linearized
    );
    let document = avail.parse_document(b"").unwrap();
    assert_eq!(document.borrow().page_count(), 2);
    assert!(avail.parse_document(b"").is_err());
    assert_eq!(avail.is_page_avail(0, Some(&hints)), DataAvailable);
    let mut polls = 0;
    loop {
        let result = avail.is_page_avail(1, Some(&hints));
        if result == DataAvailable {
            break;
        }
        assert_eq!(result, DataNotAvailable);
        let pending = std::mem::take(&mut *requests.borrow_mut());
        assert!(!pending.is_empty());
        for (offset, size) in pending {
            available
                .ranges
                .borrow_mut()
                .push((offset, offset + size as i64));
        }
        polls += 1;
        assert!(polls < 20);
    }
    assert!(document.borrow_mut().page_dictionary(1).is_some());
    assert_eq!(avail.get_page_count(), 2);
    let weak = Rc::downgrade(&document);
    drop(document);
    assert!(weak.upgrade().is_none());
    assert!(!avail.pages_obj_avail.is_empty());
    let reloaded = avail.parse_document(b"").unwrap();
    assert!(avail.pages_obj_avail.is_empty());
    assert!(avail.pages_resources_avail.is_empty());
    assert!(avail.pages_array.is_empty());
    assert!(avail.form_avail.is_none());
    drop(reloaded);
    assert_eq!(avail.is_page_avail(0, None), DataError);
    assert_eq!(avail.is_form_avail(None), DocFormStatus::FormAvailable);
}
#[test]
fn empty_and_malformed_files_follow_native_errors_and_fallback() {
    let mut empty = CpdfDataAvail::new(None, Rc::new(CfxReadOnlyContainerStream::new(Vec::new())));
    assert_eq!(empty.is_doc_avail(None), DataError);
    let mut malformed = CpdfDataAvail::new(
        None,
        Rc::new(CfxReadOnlyContainerStream::new(b"invalid".to_vec())),
    );
    assert_eq!(malformed.is_doc_avail(None), DataAvailable);
    assert_eq!(
        malformed.parse_document(b"").err(),
        Some(ParserError::FormatError)
    );
}

fn ordinary_pdf(count: bool) -> Vec<u8> {
    let mut data = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R /AcroForm 10 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages {} /Kids [3 0 R 4 0 R] >>",
            if count { "/Count 2" } else { "" }
        )
        .into_bytes(),
        b"<< /Type /Pages /Count 1 /Parent 2 0 R /Kids [5 0 R] >>".to_vec(),
        b"<< /Type /Pages /Count 1 /Parent 2 0 R /Kids [6 0 R] >>".to_vec(),
        b"<< /Type /Page /Parent 3 0 R /Resources 7 0 R /Contents 8 0 R /MediaBox [0 0 20 20] >>"
            .to_vec(),
        b"<< /Type /Page /Parent 4 0 R /Resources 7 0 R /Contents 9 0 R /MediaBox [0 0 20 20] >>"
            .to_vec(),
        b"<< /XObject << /Im0 11 0 R >> >>".to_vec(),
    ];
    for _ in 0..2 {
        let mut stream = b"<< /Length 18000 >>\nstream\n".to_vec();
        stream.extend(std::iter::repeat_n(b' ', 18000));
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    objects.push(b"<< /Fields [] >>".to_vec());
    objects.push(b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\nx\nendstream".to_vec());
    for (index, body) in objects.into_iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        data.extend(body);
        data.extend_from_slice(b"\nendobj\n");
    }
    let xref = data.len();
    data.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for &offset in &offsets[1..] {
        data.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    data.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    data
}
#[test]
fn ordinary_nested_page_tree_and_missing_count_fallback() {
    for count in [true, false] {
        let bytes = ordinary_pdf(count);
        let available = Rc::new(Availability {
            ranges: RefCell::new(Vec::new()),
        });
        let callback: Rc<dyn FileAvail> = available.clone();
        let mut avail = CpdfDataAvail::new(
            Some(&callback),
            Rc::new(CfxReadOnlyContainerStream::new(bytes)),
        );
        let requests = Rc::new(RefCell::new(Vec::new()));
        let hints: Rc<RefCell<dyn DownloadHints>> = Rc::new(RefCell::new(Hints(requests.clone())));
        let mut polls = 0;
        loop {
            if avail.is_doc_avail(Some(&hints)) == DataAvailable {
                break;
            }
            let pending = std::mem::take(&mut *requests.borrow_mut());
            // The source yields once when a missing count selects the whole
            // tree walk, even when no further download is necessary.
            assert!(
                !pending.is_empty()
                    || (avail.total_load_page_tree && avail.status == InternalStatus::PageTree)
            );
            for (offset, size) in pending {
                available
                    .ranges
                    .borrow_mut()
                    .push((offset, offset + size as i64));
            }
            polls += 1;
            assert!(polls < 30);
        }
        assert_eq!(
            avail.is_linearized_pdf(),
            DocLinearizationStatus::NotLinearized
        );
        let document = avail.parse_document(b"").unwrap();
        assert_eq!(document.borrow().page_count(), 2);
        for page in 0..2 {
            let mut polls = 0;
            loop {
                let result = avail.is_page_avail(page, Some(&hints));
                if result == DataAvailable {
                    break;
                }
                assert_eq!(result, DataNotAvailable);
                let pending = std::mem::take(&mut *requests.borrow_mut());
                assert!(!pending.is_empty());
                for (offset, size) in pending {
                    available
                        .ranges
                        .borrow_mut()
                        .push((offset, offset + size as i64));
                }
                polls += 1;
                assert!(polls < 30);
            }
            assert_eq!(
                document
                    .borrow_mut()
                    .page_dictionary(page as usize)
                    .unwrap()
                    .borrow()
                    .obj_num(),
                5 + page
            );
        }
        assert_eq!(
            avail.is_form_avail(Some(&hints)),
            DocFormStatus::FormAvailable
        );
    }
}

#[test]
fn hybrid_full_file_fallback_resolves_unloaded_pages_through_the_document() {
    // Valid hybrid-reference PDF: the classic table marks compressed objects
    // free, while /XRefStm gives their positions inside the object stream.
    let pages = b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>";
    let page = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 20 20] /Resources << >> >>";
    let header = format!("2 0 3 {} ", pages.len() + 1);
    let mut objects = header.as_bytes().to_vec();
    objects.extend_from_slice(pages);
    objects.push(b' ');
    objects.extend_from_slice(page);
    let mut object_stream = format!(
        "<< /Type /ObjStm /N 2 /First {} /Length {} >>\nstream\n",
        header.len(),
        objects.len()
    )
    .into_bytes();
    object_stream.extend(objects);
    object_stream.extend_from_slice(b"\nendstream");
    let records = [2, 0, 0, 0, 5, 0, 0, 2, 0, 0, 0, 5, 0, 1];
    let mut xref_stream =
        b"<< /Type /XRef /Root 1 0 R /Size 7 /W [1 4 2] /Index [2 2] /Length 14 >>\nstream\n"
            .to_vec();
    xref_stream.extend_from_slice(&records);
    xref_stream.extend_from_slice(b"\nendstream");
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = [0usize; 7];
    for (number, object) in [
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (5, object_stream),
        (6, xref_stream),
    ] {
        offsets[number] = bytes.len();
        bytes.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        bytes.extend(object);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(b"xref\n0 7\n");
    for (number, &offset) in offsets.iter().enumerate() {
        let used = matches!(number, 1 | 5 | 6);
        bytes.extend_from_slice(
            format!(
                "{offset:010} {:05} {} \n",
                if used { 0 } else { 65535 },
                if used { "n" } else { "f" }
            )
            .as_bytes(),
        );
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size 7 /Root 1 0 R /XRefStm {} >>\nstartxref\n{xref}\n%%EOF\n",
            offsets[6]
        )
        .as_bytes(),
    );
    let mut availability =
        CpdfDataAvail::new(None, Rc::new(CfxReadOnlyContainerStream::new(bytes)));
    let mut status = DataNotAvailable;
    for _ in 0..10 {
        status = availability.is_doc_avail(None);
        if status == DataAvailable {
            break;
        }
        assert_eq!(status, DataNotAvailable);
    }
    assert_eq!(status, DataAvailable);
    let document = availability.parse_document(b"").unwrap();
    assert_eq!(document.borrow().page_count(), 1);
    assert!(!document.borrow().is_page_loaded(0));
    assert_eq!(availability.is_page_avail(0, None), DataAvailable);
    assert_eq!(
        document
            .borrow_mut()
            .page_dictionary(0)
            .unwrap()
            .borrow()
            .obj_num(),
        3
    );
}

#[test]
fn nonpositive_linearized_page_count_hands_off_to_the_complete_tree_walk() {
    let original = std::fs::read(format!(
        "{}/../../upstream/pdfium/testing/resources/linearized.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let mut bytes = original.clone();
    for (from, to) in [
        (b"/Count 3".as_slice(), b"/Count 0".as_slice()),
        (b"/H [".as_slice(), b"/J [".as_slice()),
    ] {
        let at = bytes
            .windows(from.len())
            .position(|part| part == from)
            .unwrap();
        bytes[at..at + from.len()].copy_from_slice(to);
    }
    let available = Rc::new(Availability {
        ranges: RefCell::new(vec![(0, bytes.len() as i64)]),
    });
    let callback: Rc<dyn FileAvail> = available;
    let mut avail = CpdfDataAvail::new(
        Some(&callback),
        Rc::new(CfxReadOnlyContainerStream::new(bytes)),
    );
    assert_eq!(avail.is_doc_avail(None), DataAvailable);
    let document = avail.parse_document(b"").unwrap();
    assert_eq!(document.borrow().page_count(), 3);
    assert_eq!(avail.check_linearized_data(), DataAvailable);
    assert_eq!(avail.status, InternalStatus::PageTree);
    // Native LoadDocPages returns false while selecting the complete walk.
    // Returning true here makes CheckPage's PageTree loop spin indefinitely.
    assert!(!avail.load_doc_pages());
    assert!(avail.total_load_page_tree);
    assert_eq!(avail.status, InternalStatus::PageTree);
    let mut ready = false;
    for _ in 0..8 {
        match avail.is_page_avail(2, None) {
            DataAvailable => {
                ready = true;
                break;
            }
            DataNotAvailable => {}
            DataError => panic!("Valid leaf tree was rejected"),
        }
    }
    assert!(
        ready,
        "Complete page-tree handoff must make bounded progress"
    );
    for page in 0..3 {
        assert_eq!(avail.is_page_avail(page, None), DataAvailable);
        assert!(document
            .borrow_mut()
            .page_dictionary(page as usize)
            .is_some());
    }
}
