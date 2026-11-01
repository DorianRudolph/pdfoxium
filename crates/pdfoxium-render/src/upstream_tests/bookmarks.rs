// Upstream pdfium-render 0.9.4 tests; MIT.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;
use std::hash::{DefaultHasher, Hash, Hasher};

#[test]
fn test_bookmarks() -> Result<(), PdfiumError> {
    fn title(bookmark: PdfBookmark) -> String {
        bookmark.title().expect("Bookmark Title")
    }

    fn hash(b: &PdfBookmark) -> u64 {
        let mut s = DefaultHasher::new();
        b.hash(&mut s);
        s.finish()
    }

    let pdfium = test_bind_to_pdfium();
    let document = pdfium.load_pdf_from_file("./test/test-toc.pdf", None)?;

    // Should be able to find Sections 3 and 4
    let section3 = document.bookmarks().find_first_by_title("Section 3")?;
    let section4 = document.bookmarks().find_first_by_title("Section 4")?;

    // Section 3 should have five direct children, sections 3.1 through 3.5,
    // each of which should show up exactly once.
    let direct_children: Vec<String> = section3.iter_direct_children().map(title).collect();
    let expected: Vec<String> = (1..6).map(|i| format!("Section 3.{i}")).collect();
    assert_eq!(direct_children, expected);
    assert_eq!(section3.children_len(), 5);

    // Section 4 has no children
    assert_eq!(section4.iter_direct_children().count(), 0);
    assert_eq!(section4.children_len(), 0);

    // Section 3 has many descendents
    let all_children: Vec<String> = section3.iter_all_descendants().map(title).collect();
    let expected = [
        "Section 3.1",
        "Section 3.2",
        "Section 3.2.1",
        "Section 3.2.2",
        "Section 3.2.3",
        "Section 3.2.4",
        "Section 3.2.5",
        "Section 3.2.6",
        "Section 3.2.7",
        "Section 3.2.8",
        "Section 3.2.9",
        "Section 3.2.10",
        "Section 3.2.11",
        "Section 3.2.12",
        "Section 3.3",
        "Section 3.3.1",
        "Section 3.3.2",
        "Section 3.3.2.1",
        "Section 3.3.2.2",
        "Section 3.3.2.3",
        "Section 3.3.3",
        "Section 3.3.4",
        "Section 3.3.5",
        "Section 3.3.6",
        "Section 3.4",
        "Section 3.4.1",
        "Section 3.4.2",
        "Section 3.5",
        "Section 3.5.1",
        "Section 3.5.2",
    ];
    assert_eq!(all_children, expected);

    // Section 4 has no childern
    assert_eq!(section4.iter_all_descendants().count(), 0);

    // Section 3 has no parents, so when iterating siblings, we expect only
    // sections 4 and 5.
    let siblings: Vec<String> = section3.iter_siblings().map(title).collect();
    assert_eq!(siblings, ["Section 4", "Section 5"]);

    // Find section 3.2.6 in a way that we'll know its parent.
    let section3_2_6 = section3
        .iter_all_descendants()
        .find(|bookmark| bookmark.title() == Some("Section 3.2.6".into()))
        .expect("§3.2.6");
    assert!(section3_2_6.parent().is_some());
    // Then we expect the siblings to be sections 3.2.1 .. 3.2.12, excluding
    // ourselves.
    let siblings: Vec<String> = section3_2_6.iter_siblings().map(title).collect();
    let expected = [
        "Section 3.2.1",
        "Section 3.2.2",
        "Section 3.2.3",
        "Section 3.2.4",
        "Section 3.2.5",
        "Section 3.2.7",
        "Section 3.2.8",
        "Section 3.2.9",
        "Section 3.2.10",
        "Section 3.2.11",
        "Section 3.2.12",
    ];
    assert_eq!(siblings, expected);

    // Section 5.3.3.1 is an only child.
    let section5_3_3_1 = document
        .bookmarks()
        .find_first_by_title("Section 5.3.3.1")?;
    assert!(section5_3_3_1.parent().is_none());
    assert_eq!(section5_3_3_1.iter_siblings().count(), 0);
    // Even if we know its parent.
    let section5_3_3_1 = document
        .bookmarks()
        .iter()
        .find(|bookmark| bookmark.title() == Some("Section 5.3.3.1".into()))
        .expect("§5.3.3.1");
    assert!(section5_3_3_1.parent().is_some());
    assert_eq!(section5_3_3_1.iter_siblings().count(), 0);

    // Check that the parent is set right
    for bookmark in document.bookmarks().iter() {
        match bookmark.parent() {
            // If there is no parent, then this should be a top-level
            // section, which doesn't have a . in its name.
            None => assert!(!title(bookmark).contains('.')),
            Some(parent) => {
                // If you take this section's title, and lop off the last
                // dot and what comes after it, you should have the parent
                // section's title.
                let this_title = title(bookmark);
                let last_dot = this_title.rfind('.').unwrap();
                assert_eq!(title(parent), this_title[..last_dot]);
            }
        }
    }

    let all_bookmarks: Vec<_> = document.bookmarks().iter().map(title).collect();
    let expected = [
        "Section 1",
        "Section 2",
        "Section 3",
        "Section 3.1",
        "Section 3.2",
        "Section 3.2.1",
        "Section 3.2.2",
        "Section 3.2.3",
        "Section 3.2.4",
        "Section 3.2.5",
        "Section 3.2.6",
        "Section 3.2.7",
        "Section 3.2.8",
        "Section 3.2.9",
        "Section 3.2.10",
        "Section 3.2.11",
        "Section 3.2.12",
        "Section 3.3",
        "Section 3.3.1",
        "Section 3.3.2",
        "Section 3.3.2.1",
        "Section 3.3.2.2",
        "Section 3.3.2.3",
        "Section 3.3.3",
        "Section 3.3.4",
        "Section 3.3.5",
        "Section 3.3.6",
        "Section 3.4",
        "Section 3.4.1",
        "Section 3.4.2",
        "Section 3.5",
        "Section 3.5.1",
        "Section 3.5.2",
        "Section 4",
        "Section 5",
        "Section 5.1",
        "Section 5.2",
        "Section 5.3",
        "Section 5.3.1",
        "Section 5.3.2",
        "Section 5.3.3",
        "Section 5.3.3.1",
        "Section 5.3.4",
        "Section 5.4",
        "Section 5.4.1",
        "Section 5.4.1.1",
        "Section 5.4.1.2",
        "Section 5.4.2",
        "Section 5.4.2.1",
        "Section 5.4.3",
        "Section 5.4.4",
        "Section 5.4.5",
        "Section 5.4.6",
        "Section 5.4.7",
        "Section 5.4.8",
        "Section 5.4.8.1",
        "Section 5.5",
        "Section 5.5.1",
        "Section 5.5.2",
        "Section 5.5.3",
        "Section 5.5.4",
        "Section 5.5.5",
        "Section 5.5.5.1",
        "Section 5.5.5.2",
        "Section 5.5.5.3",
        "Section 5.5.6",
        "Section 5.5.6.1",
        "Section 5.5.6.2",
        "Section 5.5.6.3",
        "Section 5.5.7",
        "Section 5.5.7.1",
        "Section 5.5.7.2",
        "Section 5.5.7.3",
        "Section 5.5.7.4",
        "Section 5.5.7.5",
        "Section 5.5.7.6",
        "Section 5.5.8",
        "Section 5.5.8.1",
        "Section 5.5.8.2",
        "Section 5.5.8.3",
        "Section 5.5.8.4",
        "Section 5.5.8.5",
        "Section 5.5.8.6",
        "Section 5.5.8.7",
        "Section 5.5.8.8",
        "Section 5.5.8.9",
        "Section 5.5.9",
        "Section 5.5.10",
        "Section 5.6",
        "Section 5.7",
        "Section 5.7.1",
        "Section 5.7.2",
        "Section 5.7.3",
        "Section 5.7.3.1",
        "Section 5.7.3.2",
        "Section 5.7.3.3",
        "Section 5.7.3.4",
        "Section 5.7.4",
        "Section 5.7.4.1",
        "Section 5.7.4.2",
        "Section 5.7.4.3",
    ];
    assert_eq!(all_bookmarks, expected);

    // Test that each bookmark is equal it itself but no two bookmarks are equal to each other.
    let all_bookmarks: Vec<_> = document.bookmarks().iter().collect();
    for i in 0..all_bookmarks.len() {
        assert!(all_bookmarks[i] == all_bookmarks[i]);
        assert_eq!(hash(&all_bookmarks[i]), hash(&all_bookmarks[i]));
        for j in (i + 1)..all_bookmarks.len() {
            assert!(all_bookmarks[i] != all_bookmarks[j]);
        }
    }

    // Get all the bookmarks again, and test that the second iteration returned an equal set of
    // bookmarks
    let all_bookmarks2: Vec<_> = document.bookmarks().iter().collect();
    assert!(all_bookmarks == all_bookmarks2);

    // Bookmarks should be equal to their clone
    let the_clone = all_bookmarks[0].clone();
    assert!(all_bookmarks[0] == the_clone);
    assert_eq!(hash(&all_bookmarks[0]), hash(&the_clone));

    // Load the document a second time, and assert that the bookmarks are different.
    let document2 = pdfium.load_pdf_from_file("./test/test-toc.pdf", None)?;
    let all_bookmarks2: Vec<_> = document2.bookmarks().iter().collect();
    assert_eq!(all_bookmarks.len(), all_bookmarks2.len());
    for i in 0..all_bookmarks.len() {
        for j in 0..all_bookmarks.len() {
            assert!(all_bookmarks[i] != all_bookmarks2[j]);
        }
    }

    Ok(())
}
